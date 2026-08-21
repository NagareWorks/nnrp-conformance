use nnrp_conformance::wire_endpoint::{
    ReferenceTransport, WireEndpointSecurity, WireReferenceEndpoint,
};
use nnrp_conformance::wire_external::{
    WireExternalCase, WireExternalCaseReport, WireExternalDirection, WireExternalFrame,
    WireExternalMode, WireExternalTerminal, run_wire_external_case,
};
use nnrp_conformance_fixtures::{
    AdapterArtifactContext, AdapterExecutionCase, AdapterExecutionPlan,
    ApiProfileCapabilityManifest, ApiProfileCaseOutcome, ApiProfileCaseResultReport,
    ApiProfileExecutionCase, ApiProfileExecutionPlan, ApiProfileExpectedEvent, ApiProfileRecipe,
    BenchmarkArtifactContext, BenchmarkCategory, BenchmarkExecutionPlan, BenchmarkScenario,
    BenchmarkWorkload, CapabilityManifest, CaseDefinition, CaseManifest, CaseStatus,
    CompatibilityMatrixEntry, ConformanceReport, FixtureError, ProtocolManifest, ReportCase,
    ReportStatusSummary, ReportSummary, WireConformanceCaseResult, WireConformanceCaseResultReport,
    WireConformanceExecutionPlan, WireConformanceFrameDirection, WireConformanceObservedFrame,
    WireConformanceScenario, WireConformanceTargetManifest, WireConformanceTerminal, WireHostRole,
    WireHostRouteRejectionReason, validate_protocol_alignment,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::Duration;

pub mod openai_profile_wire;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum CaseSelection {
    Selected,
    NotClaimed,
    Informational,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ApiProfileValidationSummary {
    pub selected_cases: usize,
    pub passed_cases: usize,
    pub failed_cases: usize,
    pub skipped_cases: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct WireConformanceValidationSummary {
    pub selected_scenarios: usize,
    pub passed_scenarios: usize,
    pub failed_scenarios: usize,
    pub skipped_scenarios: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct WireExternalExecutionSummary {
    pub selected_scenarios: usize,
    pub passed_scenarios: usize,
    pub failed_scenarios: usize,
}

impl CaseSelection {
    fn as_str(self) -> &'static str {
        match self {
            Self::Selected => "selected",
            Self::NotClaimed => "not_claimed",
            Self::Informational => "informational",
        }
    }
}

#[derive(Default)]
struct CompatibilityMatrixAccumulator {
    required_capabilities: BTreeSet<String>,
    summary: ReportSummary,
    statuses: ReportStatusSummary,
    case_ids: BTreeSet<String>,
}

fn select_case(case: &CaseDefinition, declared_capabilities: &BTreeSet<String>) -> CaseSelection {
    let capabilities_satisfied = case
        .required_capabilities
        .iter()
        .all(|capability| declared_capabilities.contains(capability));

    match case.status {
        CaseStatus::Mandatory | CaseStatus::Optional => {
            if capabilities_satisfied {
                CaseSelection::Selected
            } else {
                CaseSelection::NotClaimed
            }
        }
        CaseStatus::Experimental | CaseStatus::Deprecated => CaseSelection::Informational,
    }
}

fn build_execution_plan_from_cases<'a>(
    protocol_manifest: &ProtocolManifest,
    cases: impl Iterator<Item = &'a CaseDefinition>,
    capability_manifest: Option<&CapabilityManifest>,
) -> ConformanceReport {
    let declared_capabilities = capability_manifest
        .map(|manifest| manifest.supports.iter().cloned().collect::<BTreeSet<_>>())
        .unwrap_or_default();
    let implementation_name = capability_manifest
        .map(|manifest| manifest.implementation_name.clone())
        .unwrap_or_else(|| "unclaimed".to_string());

    let mut selected_cases = 0;
    let mut not_claimed_cases = 0;
    let mut informational_cases = 0;
    let mut compatibility_matrix = BTreeMap::<String, CompatibilityMatrixAccumulator>::new();
    let mut report_cases = Vec::new();

    for case in cases {
        let selection = select_case(case, &declared_capabilities);

        match selection {
            CaseSelection::Selected => selected_cases += 1,
            CaseSelection::NotClaimed => not_claimed_cases += 1,
            CaseSelection::Informational => informational_cases += 1,
        }

        let matrix_entry = compatibility_matrix
            .entry(case.feature.clone())
            .or_default();
        matrix_entry
            .required_capabilities
            .extend(case.required_capabilities.iter().cloned());
        matrix_entry.case_ids.insert(case.id.clone());
        match selection {
            CaseSelection::Selected => matrix_entry.summary.selected_cases += 1,
            CaseSelection::NotClaimed => matrix_entry.summary.not_claimed_cases += 1,
            CaseSelection::Informational => matrix_entry.summary.informational_cases += 1,
        }
        match case.status {
            CaseStatus::Mandatory => matrix_entry.statuses.mandatory_cases += 1,
            CaseStatus::Optional => matrix_entry.statuses.optional_cases += 1,
            CaseStatus::Experimental => matrix_entry.statuses.experimental_cases += 1,
            CaseStatus::Deprecated => matrix_entry.statuses.deprecated_cases += 1,
        }

        report_cases.push(ReportCase {
            id: case.id.clone(),
            feature: Some(case.feature.clone()),
            status: Some(case.status),
            selection: selection.as_str().to_string(),
        });
    }

    let compatibility_matrix = compatibility_matrix
        .into_iter()
        .map(|(feature, entry)| CompatibilityMatrixEntry {
            feature,
            required_capabilities: entry.required_capabilities.into_iter().collect(),
            summary: entry.summary,
            statuses: entry.statuses,
            case_ids: entry.case_ids.into_iter().collect(),
        })
        .collect();

    ConformanceReport {
        protocol_version: protocol_manifest.protocol_version.clone(),
        implementation_name,
        summary: ReportSummary {
            selected_cases,
            not_claimed_cases,
            informational_cases,
        },
        compatibility_matrix,
        cases: report_cases,
    }
}

fn build_adapter_execution_plan_from_cases<'a>(
    protocol_manifest: &ProtocolManifest,
    cases: impl Iterator<Item = &'a CaseDefinition>,
    capability_manifest: &CapabilityManifest,
    artifacts: AdapterArtifactContext,
) -> AdapterExecutionPlan {
    let declared_capabilities = capability_manifest
        .supports
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let selected_cases = cases
        .filter(|case| select_case(case, &declared_capabilities) == CaseSelection::Selected)
        .map(|case| AdapterExecutionCase {
            id: case.id.clone(),
            layer: case.layer,
            status: case.status,
            feature: case.feature.clone(),
            required_capabilities: case.required_capabilities.clone(),
            description: case.description.clone(),
            parameters: case.parameters.clone(),
        })
        .collect();

    AdapterExecutionPlan {
        schema: Some(
            "https://github.com/NagareWorks/nnrp-conformance/schemas/adapter-execution-plan.schema.json"
                .to_string(),
        ),
        protocol_version: protocol_manifest.protocol_version.clone(),
        suite_version: protocol_manifest.suite_version.clone(),
        implementation_name: capability_manifest.implementation_name.clone(),
        artifacts,
        cases: selected_cases,
    }
}

fn validate_declared_capabilities<'a>(
    capability_manifest: Option<&CapabilityManifest>,
    cases: impl Iterator<Item = &'a CaseDefinition>,
) -> Result<(), FixtureError> {
    let Some(capability_manifest) = capability_manifest else {
        return Ok(());
    };

    let allowed_capabilities = cases
        .flat_map(|case| case.required_capabilities.iter().cloned())
        .collect::<BTreeSet<_>>();
    let unknown_capabilities = capability_manifest
        .supports
        .iter()
        .filter(|capability| !allowed_capabilities.contains(*capability))
        .cloned()
        .collect::<Vec<_>>();

    if unknown_capabilities.is_empty() {
        return Ok(());
    }

    Err(FixtureError::Validation {
        message: format!(
            "capability manifest {} declares unknown capability token(s): {}",
            capability_manifest.implementation_name,
            unknown_capabilities.join(", ")
        ),
    })
}

pub fn validate_complete_capability_coverage<'a>(
    capability_manifest: &CapabilityManifest,
    cases: impl Iterator<Item = &'a CaseDefinition>,
) -> Result<(), FixtureError> {
    let required_capabilities = cases
        .filter(|case| matches!(case.status, CaseStatus::Mandatory | CaseStatus::Optional))
        .flat_map(|case| case.required_capabilities.iter().cloned())
        .collect::<BTreeSet<_>>();
    let declared_capabilities = capability_manifest
        .supports
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let missing_capabilities = required_capabilities
        .difference(&declared_capabilities)
        .cloned()
        .collect::<Vec<_>>();

    if missing_capabilities.is_empty() {
        return Ok(());
    }

    Err(FixtureError::Validation {
        message: format!(
            "capability manifest {} does not cover the complete adapter case scope; missing capability token(s): {}",
            capability_manifest.implementation_name,
            missing_capabilities.join(", ")
        ),
    })
}

pub fn build_benchmark_execution_plan(
    protocol_manifest: &ProtocolManifest,
    capability_manifest: &CapabilityManifest,
    artifacts: BenchmarkArtifactContext,
) -> BenchmarkExecutionPlan {
    BenchmarkExecutionPlan {
        schema: Some(
            "https://github.com/NagareWorks/nnrp-conformance/schemas/benchmark-execution-plan.schema.json"
                .to_string(),
        ),
        protocol_version: protocol_manifest.protocol_version.clone(),
        suite_version: protocol_manifest.suite_version.clone(),
        implementation_name: capability_manifest.implementation_name.clone(),
        artifacts,
        scenarios: default_benchmark_scenarios(&capability_manifest.supports),
    }
}

pub fn build_api_profile_execution_plan(
    capability_manifest: &ApiProfileCapabilityManifest,
    recipes: &[ApiProfileRecipe],
    artifacts: AdapterArtifactContext,
) -> Result<ApiProfileExecutionPlan, FixtureError> {
    validate_api_profile_alignment(capability_manifest, recipes)?;

    let declared_capabilities = api_profile_declared_capabilities(capability_manifest);
    let coverage_matrix = build_api_profile_coverage_matrix(recipes, &declared_capabilities);
    let selected_cases = recipes
        .iter()
        .filter(|recipe| {
            api_recipe_selection(recipe, &declared_capabilities) == CaseSelection::Selected
        })
        .map(|recipe| ApiProfileExecutionCase {
            id: recipe.id.clone(),
            operation: recipe.operation.clone(),
            status: recipe.status,
            required_capabilities: required_api_capabilities(recipe),
            request: substitute_api_profile_request_parameters(recipe),
            expect: recipe.expect.clone(),
        })
        .collect();

    Ok(ApiProfileExecutionPlan {
        schema: Some(
            "https://github.com/NagareWorks/nnrp-conformance/schemas/api-profile-execution-plan.schema.json"
                .to_string(),
        ),
        profile: capability_manifest.profile.clone(),
        schema_version: capability_manifest.schema_version.clone(),
        adapter: capability_manifest.adapter.clone(),
        artifacts,
        coverage_matrix,
        cases: selected_cases,
    })
}

pub fn build_wire_conformance_execution_plan(
    target_manifest: &WireConformanceTargetManifest,
    scenarios: &[WireConformanceScenario],
    artifacts: AdapterArtifactContext,
) -> Result<WireConformanceExecutionPlan, FixtureError> {
    for endpoint in &target_manifest.wire_conformance.transports {
        validate_wire_transport_endpoint(endpoint)?;
    }
    let declares_host_routes = target_manifest
        .wire_conformance
        .capabilities
        .iter()
        .any(|capability| capability == "host.routes");
    let has_host_route_providers = !target_manifest
        .wire_conformance
        .host_route_providers
        .is_empty();
    if declares_host_routes != has_host_route_providers {
        return Err(FixtureError::Validation {
            message: if declares_host_routes {
                "target claims host.routes without declaring host-route providers".to_string()
            } else {
                "target declares host-route providers without claiming host.routes".to_string()
            },
        });
    }
    let mut declared_host_transports = BTreeSet::new();
    let mut declared_host_provider_ids = BTreeSet::new();
    for provider in &target_manifest.wire_conformance.host_route_providers {
        if provider.provider_id.is_empty() {
            return Err(FixtureError::Validation {
                message: "host-route provider id must not be empty".to_string(),
            });
        }
        if !declared_host_transports.insert(provider.transport) {
            return Err(FixtureError::Validation {
                message: format!(
                    "target declares more than one host-route provider for {:?}",
                    provider.transport
                ),
            });
        }
        if !declared_host_provider_ids.insert(provider.provider_id.as_str()) {
            return Err(FixtureError::Validation {
                message: format!(
                    "target repeats host-route provider id {}",
                    provider.provider_id
                ),
            });
        }
    }
    let target_modes = target_manifest
        .wire_conformance
        .modes
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let target_transports = target_manifest
        .wire_conformance
        .transports
        .iter()
        .map(|transport| transport.name)
        .collect::<BTreeSet<_>>();
    let target_host_providers = target_manifest
        .wire_conformance
        .host_route_providers
        .iter()
        .map(|provider| (provider.transport, provider.provider_id.as_str()))
        .collect::<BTreeSet<_>>();
    let target_capabilities = target_manifest
        .wire_conformance
        .capabilities
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();

    let selected_scenarios = scenarios
        .iter()
        .filter(|scenario| {
            target_modes.contains(&scenario.mode)
                && scenario
                    .transport
                    .is_none_or(|transport| target_transports.contains(&transport))
                && scenario.host_route.as_ref().is_none_or(|fixture| {
                    fixture.routes.iter().all(|route| {
                        target_host_providers
                            .contains(&(route.transport, route.provider_id.as_str()))
                    })
                })
                && scenario
                    .required_capabilities
                    .iter()
                    .all(|capability| target_capabilities.contains(capability))
        })
        .cloned()
        .collect::<Vec<_>>();

    for scenario in &selected_scenarios {
        validate_wire_scenario_shape(scenario)?;
        if scenario.host_route.is_some() {
            validate_wire_host_route_fixture(scenario, target_manifest)?;
        }
    }

    Ok(WireConformanceExecutionPlan {
        schema: Some(
            "https://github.com/NagareWorks/nnrp-conformance/schemas/wire-conformance-execution-plan.schema.json"
                .to_string(),
        ),
        protocol_version: target_manifest.protocol_version.clone(),
        suite_version: target_manifest.suite_version.clone(),
        target_name: target_manifest.target_name.clone(),
        host_route_providers: target_manifest
            .wire_conformance
            .host_route_providers
            .clone(),
        artifacts,
        scenarios: selected_scenarios,
    })
}

fn validate_wire_scenario_shape(scenario: &WireConformanceScenario) -> Result<(), FixtureError> {
    if scenario.transport.is_some() == scenario.host_route.is_some() {
        return Err(FixtureError::Validation {
            message: format!(
                "wire scenario {} must declare exactly one of transport or host_route",
                scenario.id
            ),
        });
    }
    if scenario.host_route.is_some() != scenario.expect.route.is_some() {
        return Err(FixtureError::Validation {
            message: format!(
                "wire scenario {} must declare route expectations exactly when it declares host_route",
                scenario.id
            ),
        });
    }
    if scenario.host_route.is_some() && scenario.expect.result_drop_reason_code.is_some() {
        return Err(FixtureError::Validation {
            message: format!(
                "wire host-route scenario {} must not declare result_drop_reason_code",
                scenario.id
            ),
        });
    }
    if scenario.host_route.is_some() && !scenario.expect.frame_payload_invariants.is_empty() {
        return Err(FixtureError::Validation {
            message: format!(
                "wire host-route scenario {} must not declare frame_payload_invariants",
                scenario.id
            ),
        });
    }
    if scenario.expect.result_drop_reason_code == Some(0) {
        return Err(FixtureError::Validation {
            message: format!(
                "wire scenario {} result_drop_reason_code must be non-zero",
                scenario.id
            ),
        });
    }
    if scenario.expect.result_drop_reason_code.is_some()
        && !scenario
            .expect
            .frames
            .iter()
            .any(|frame| frame == "RESULT_DROP_REASON")
    {
        return Err(FixtureError::Validation {
            message: format!(
                "wire scenario {} declares result_drop_reason_code without RESULT_DROP_REASON",
                scenario.id
            ),
        });
    }
    if scenario.transport.is_some() {
        if scenario.expect.frames.is_empty() || scenario.expect.allowed_frames.is_empty() {
            return Err(FixtureError::Validation {
                message: format!(
                    "wire frame scenario {} must declare ordered frames and allowed_frames",
                    scenario.id
                ),
            });
        }
        if let Some(frame) = scenario
            .expect
            .frames
            .iter()
            .find(|frame| !scenario.expect.allowed_frames.contains(frame))
        {
            return Err(FixtureError::Validation {
                message: format!(
                    "wire scenario {} requires frame {} outside allowed_frames",
                    scenario.id, frame
                ),
            });
        }
        validate_frame_payload_invariant_contract(scenario)?;
    } else if !scenario.expect.frames.is_empty() || !scenario.expect.allowed_frames.is_empty() {
        return Err(FixtureError::Validation {
            message: format!(
                "wire host-route scenario {} must use route evidence instead of frame expectations",
                scenario.id
            ),
        });
    }
    if let Some(fixture) = &scenario.host_route {
        let expected_mode = match fixture.role {
            WireHostRole::Client => nnrp_conformance_fixtures::WireConformanceMode::SuiteAsServer,
            WireHostRole::Server => nnrp_conformance_fixtures::WireConformanceMode::SuiteAsClient,
        };
        if scenario.mode != expected_mode {
            return Err(FixtureError::Validation {
                message: format!(
                    "wire scenario {} uses {:?} for a {:?} host route; expected {:?}",
                    scenario.id, scenario.mode, fixture.role, expected_mode
                ),
            });
        }
    }
    Ok(())
}

fn validate_wire_host_route_fixture(
    scenario: &WireConformanceScenario,
    target_manifest: &WireConformanceTargetManifest,
) -> Result<(), FixtureError> {
    use nnrp_conformance_fixtures::{
        WireHostCredentialOwner, WireHostPlatform, WireHostRole, WireHostRouteSecurityMode,
    };

    let fixture = scenario
        .host_route
        .as_ref()
        .expect("host-route fixture validation requires a host-route scenario");
    let expected_route =
        scenario
            .expect
            .route
            .as_ref()
            .ok_or_else(|| FixtureError::Validation {
                message: format!(
                    "wire scenario {} has a host-route fixture without route expectations",
                    scenario.id
                ),
            })?;

    if !fixture.application_endpoint.starts_with("nnrp://")
        && !fixture.application_endpoint.starts_with("nnrps://")
    {
        return Err(FixtureError::Validation {
            message: "host-route application endpoint must use nnrp:// or nnrps://".to_string(),
        });
    }
    if fixture.routes.is_empty() {
        return Err(FixtureError::Validation {
            message: "host-route fixture must declare at least one provider route".to_string(),
        });
    }

    let mut transports = BTreeSet::new();
    let mut provider_ids = BTreeSet::new();
    for route in &fixture.routes {
        if route.provider_id.is_empty() || route.locator.is_empty() {
            return Err(FixtureError::Validation {
                message: "host-route provider id and locator must not be empty".to_string(),
            });
        }
        if !transports.insert(route.transport) {
            return Err(FixtureError::Validation {
                message: format!(
                    "host-route fixture declares more than one route for {:?}",
                    route.transport
                ),
            });
        }
        if !provider_ids.insert(route.provider_id.as_str()) {
            return Err(FixtureError::Validation {
                message: format!(
                    "host-route fixture repeats provider id {}",
                    route.provider_id
                ),
            });
        }
        let provider = target_manifest
            .wire_conformance
            .host_route_providers
            .iter()
            .find(|provider| {
                provider.transport == route.transport && provider.provider_id == route.provider_id
            })
            .ok_or_else(|| FixtureError::Validation {
                message: format!(
                    "target does not declare host-route provider {} for {:?}",
                    route.provider_id, route.transport
                ),
            })?;
        if !provider.platforms.contains(&fixture.platform) {
            return Err(FixtureError::Validation {
                message: format!(
                    "host-route provider {} does not support {:?}",
                    route.provider_id, fixture.platform
                ),
            });
        }
        if !provider.security_modes.contains(&route.security.mode) {
            return Err(FixtureError::Validation {
                message: format!(
                    "host-route provider {} does not support {:?}",
                    route.provider_id, route.security.mode
                ),
            });
        }
        if !provider.installed
            && !expected_route
                .rejection_reasons
                .iter()
                .any(|reason| *reason == WireHostRouteRejectionReason::LocalUnavailable)
        {
            return Err(FixtureError::Validation {
                message: format!(
                    "wire scenario {} uses uninstalled provider {} without expecting local-unavailable",
                    scenario.id, route.provider_id
                ),
            });
        }
        let transport_security_compatible = matches!(
            (route.transport, route.security.mode),
            (
                nnrp_conformance_fixtures::WireConformanceTransport::Tcp,
                WireHostRouteSecurityMode::Plain
                    | WireHostRouteSecurityMode::TlsServerAuth
                    | WireHostRouteSecurityMode::MutualTls
            ) | (
                nnrp_conformance_fixtures::WireConformanceTransport::Quic,
                WireHostRouteSecurityMode::TlsServerAuth | WireHostRouteSecurityMode::MutualTls
            ) | (
                nnrp_conformance_fixtures::WireConformanceTransport::Ipc,
                WireHostRouteSecurityMode::Plain
            ) | (
                nnrp_conformance_fixtures::WireConformanceTransport::Websocket,
                WireHostRouteSecurityMode::Plain
                    | WireHostRouteSecurityMode::Wss
                    | WireHostRouteSecurityMode::BrowserHost
            )
        );
        if !transport_security_compatible
            || (fixture.platform == WireHostPlatform::Browser
                && route.transport
                    != nnrp_conformance_fixtures::WireConformanceTransport::Websocket)
        {
            return Err(FixtureError::Validation {
                message: format!(
                    "host-route provider {} uses incompatible {:?} security on {:?}",
                    route.provider_id, route.security.mode, route.transport
                ),
            });
        }
        match (route.security.mode, route.security.credential_owner) {
            (WireHostRouteSecurityMode::Plain, WireHostCredentialOwner::None) => {}
            (WireHostRouteSecurityMode::BrowserHost, WireHostCredentialOwner::Host)
                if fixture.platform == WireHostPlatform::Browser => {}
            (WireHostRouteSecurityMode::TlsServerAuth, WireHostCredentialOwner::Suite)
            | (WireHostRouteSecurityMode::TlsServerAuth, WireHostCredentialOwner::Target)
            | (WireHostRouteSecurityMode::MutualTls, WireHostCredentialOwner::Suite)
            | (WireHostRouteSecurityMode::MutualTls, WireHostCredentialOwner::Target)
            | (WireHostRouteSecurityMode::Wss, WireHostCredentialOwner::Suite)
            | (WireHostRouteSecurityMode::Wss, WireHostCredentialOwner::Target)
                if fixture.platform == WireHostPlatform::Native => {}
            _ => {
                return Err(FixtureError::Validation {
                    message: format!(
                        "host-route provider {} has incompatible security ownership",
                        route.provider_id
                    ),
                });
            }
        }
    }

    let expected_mode = match fixture.role {
        WireHostRole::Client => nnrp_conformance_fixtures::WireConformanceMode::SuiteAsServer,
        WireHostRole::Server => nnrp_conformance_fixtures::WireConformanceMode::SuiteAsClient,
    };
    if !target_manifest
        .wire_conformance
        .modes
        .contains(&expected_mode)
    {
        return Err(FixtureError::Validation {
            message: format!(
                "target does not declare {:?} required by the host-route {:?} role",
                expected_mode, fixture.role
            ),
        });
    }
    Ok(())
}

fn validate_wire_transport_endpoint(
    endpoint: &nnrp_conformance_fixtures::WireConformanceTransportEndpoint,
) -> Result<(), FixtureError> {
    use nnrp_conformance_fixtures::WireConformanceTransport;

    if endpoint.endpoint.is_empty() {
        return Err(FixtureError::Validation {
            message: format!("{:?} wire endpoint must not be empty", endpoint.name),
        });
    }
    let (allows_security, requires_security) = match endpoint.name {
        WireConformanceTransport::Tcp => (true, false),
        WireConformanceTransport::Ipc => (false, false),
        WireConformanceTransport::Quic => (true, true),
        WireConformanceTransport::Websocket => {
            if endpoint.endpoint.starts_with("wss://") {
                (true, true)
            } else if endpoint.endpoint.starts_with("ws://") {
                (false, false)
            } else {
                return Err(FixtureError::Validation {
                    message: "WebSocket wire endpoint must use ws:// or wss://".to_string(),
                });
            }
        }
    };
    if endpoint.tls != endpoint.security.is_some()
        || (requires_security && !endpoint.tls)
        || (!allows_security && endpoint.tls)
    {
        return Err(FixtureError::Validation {
            message: format!(
                "{:?} wire endpoint TLS flag and security material do not match its transport contract",
                endpoint.name
            ),
        });
    }
    if let Some(security) = &endpoint.security {
        if security.server_name.is_empty()
            || security.trusted_certificate_der_path.is_empty()
            || security.certificate_der_path.is_empty()
            || security.private_key_pkcs8_der_path.is_empty()
        {
            return Err(FixtureError::Validation {
                message: format!(
                    "{:?} wire endpoint security fields must not be empty",
                    endpoint.name
                ),
            });
        }
    }
    Ok(())
}

pub fn validate_wire_conformance_results(
    expected_plan: &WireConformanceExecutionPlan,
    actual_report: &WireConformanceCaseResultReport,
) -> Result<WireConformanceValidationSummary, FixtureError> {
    if expected_plan.protocol_version != actual_report.protocol_version {
        return Err(FixtureError::Validation {
            message: format!(
                "wire protocol version mismatch: expected {}, got {}",
                expected_plan.protocol_version, actual_report.protocol_version
            ),
        });
    }
    if expected_plan.suite_version != actual_report.suite_version {
        return Err(FixtureError::Validation {
            message: format!(
                "wire suite version mismatch: expected {}, got {}",
                expected_plan.suite_version, actual_report.suite_version
            ),
        });
    }
    if expected_plan.target_name != actual_report.target_name {
        return Err(FixtureError::Validation {
            message: format!(
                "wire target name mismatch: expected {}, got {}",
                expected_plan.target_name, actual_report.target_name
            ),
        });
    }

    let expected_scenarios = expected_plan
        .scenarios
        .iter()
        .map(|scenario| (scenario.id.as_str(), scenario))
        .collect::<BTreeMap<_, _>>();
    let mut actual_ids = BTreeSet::new();
    let mut summary = WireConformanceValidationSummary {
        selected_scenarios: expected_scenarios.len(),
        passed_scenarios: 0,
        failed_scenarios: 0,
        skipped_scenarios: 0,
    };

    for result in &actual_report.results {
        let expected_scenario =
            expected_scenarios
                .get(result.id.as_str())
                .ok_or_else(|| FixtureError::Validation {
                    message: format!(
                        "wire results contain an unexpected scenario id: {}",
                        result.id
                    ),
                })?;
        if !actual_ids.insert(result.id.as_str()) {
            return Err(FixtureError::Validation {
                message: format!(
                    "wire results contain a duplicate scenario id: {}",
                    result.id
                ),
            });
        }

        match result.outcome {
            ApiProfileCaseOutcome::Passed => {
                if expected_scenario.expect.terminal != result.terminal {
                    return Err(FixtureError::Validation {
                        message: format!(
                            "wire scenario {} terminal mismatch: expected {:?}, got {:?}",
                            result.id, expected_scenario.expect.terminal, result.terminal
                        ),
                    });
                }
                if let Some(unexpected_frame) = result.observed_frames.iter().find(|frame| {
                    !expected_scenario
                        .expect
                        .allowed_frames
                        .contains(&frame.frame)
                }) {
                    return Err(FixtureError::Validation {
                        message: format!(
                            "wire scenario {} observed unexpected frame {}",
                            result.id, unexpected_frame.frame
                        ),
                    });
                }
                let mut observed_cursor = 0;
                for (required_index, expected_frame) in
                    expected_scenario.expect.frames.iter().enumerate()
                {
                    let Some(relative_index) = result.observed_frames[observed_cursor..]
                        .iter()
                        .position(|frame| &frame.frame == expected_frame)
                    else {
                        return Err(FixtureError::Validation {
                            message: format!(
                                "wire scenario {} missing or reordered expected frame {} at required position {}",
                                result.id,
                                expected_frame,
                                required_index + 1
                            ),
                        });
                    };
                    observed_cursor += relative_index + 1;
                }
                validate_frame_payload_invariants(expected_scenario, result)?;
                validate_result_drop_reason(expected_scenario, result)?;
                validate_wire_route_evidence(expected_plan, expected_scenario, result)?;
                summary.passed_scenarios += 1;
            }
            ApiProfileCaseOutcome::Failed => summary.failed_scenarios += 1,
            ApiProfileCaseOutcome::Skipped => summary.skipped_scenarios += 1,
        }
    }

    if actual_ids.len() != expected_scenarios.len() {
        return Err(FixtureError::Validation {
            message: format!(
                "wire results are missing {} selected scenario(s)",
                expected_scenarios.len().saturating_sub(actual_ids.len())
            ),
        });
    }

    Ok(summary)
}

fn validate_frame_payload_invariant_contract(
    scenario: &WireConformanceScenario,
) -> Result<(), FixtureError> {
    let mut identities = BTreeSet::new();
    for invariant in &scenario.expect.frame_payload_invariants {
        if invariant.frame.is_empty() || invariant.fields.is_empty() {
            return Err(FixtureError::Validation {
                message: format!(
                    "wire scenario {} frame payload invariants require a frame and at least one field",
                    scenario.id
                ),
            });
        }
        if !scenario.expect.allowed_frames.contains(&invariant.frame) {
            return Err(FixtureError::Validation {
                message: format!(
                    "wire scenario {} payload invariant references frame {} outside allowed_frames",
                    scenario.id, invariant.frame
                ),
            });
        }
        if !identities.insert((invariant.frame.clone(), invariant.direction)) {
            return Err(FixtureError::Validation {
                message: format!(
                    "wire scenario {} declares duplicate payload invariants for frame {} and direction {:?}",
                    scenario.id, invariant.frame, invariant.direction
                ),
            });
        }
    }
    Ok(())
}

fn validate_frame_payload_invariants(
    scenario: &WireConformanceScenario,
    result: &WireConformanceCaseResult,
) -> Result<(), FixtureError> {
    for invariant in &scenario.expect.frame_payload_invariants {
        let observed = result
            .observed_frames
            .iter()
            .filter(|frame| {
                frame.frame == invariant.frame
                    && invariant
                        .direction
                        .is_none_or(|direction| frame.direction == direction)
            })
            .collect::<Vec<_>>();
        if observed.is_empty() {
            return Err(FixtureError::Validation {
                message: format!(
                    "wire scenario {} did not observe frame {} for payload validation",
                    result.id, invariant.frame
                ),
            });
        }
        for frame in observed {
            let payload = frame
                .payload
                .as_ref()
                .and_then(serde_json::Value::as_object)
                .ok_or_else(|| FixtureError::Validation {
                    message: format!(
                        "wire scenario {} frame {} has no object payload",
                        result.id, invariant.frame
                    ),
                })?;
            for (field, expected) in &invariant.fields {
                if payload.get(field) != Some(expected) {
                    return Err(FixtureError::Validation {
                        message: format!(
                            "wire scenario {} frame {} payload field {} mismatch: expected {}, got {:?}",
                            result.id,
                            invariant.frame,
                            field,
                            expected,
                            payload.get(field)
                        ),
                    });
                }
            }
        }
    }
    Ok(())
}

fn validate_result_drop_reason(
    scenario: &WireConformanceScenario,
    result: &WireConformanceCaseResult,
) -> Result<(), FixtureError> {
    let Some(expected_code) = scenario.expect.result_drop_reason_code else {
        return Ok(());
    };
    let observed_codes = result
        .observed_frames
        .iter()
        .filter(|frame| frame.frame == "RESULT_DROP_REASON")
        .map(|frame| {
            frame
                .payload
                .as_ref()
                .and_then(|payload| payload.get("drop_reason_code"))
                .and_then(serde_json::Value::as_u64)
        })
        .collect::<Vec<_>>();
    if observed_codes.is_empty()
        || observed_codes
            .iter()
            .any(|code| *code != Some(u64::from(expected_code)))
    {
        return Err(FixtureError::Validation {
            message: format!(
                "wire scenario {} result drop reason mismatch: expected {}, got {:?}",
                result.id, expected_code, observed_codes
            ),
        });
    }
    Ok(())
}

fn validate_wire_route_evidence(
    plan: &WireConformanceExecutionPlan,
    scenario: &WireConformanceScenario,
    result: &WireConformanceCaseResult,
) -> Result<(), FixtureError> {
    let Some(expected) = scenario.expect.route.as_ref() else {
        if result.route_evidence.is_some() {
            return Err(FixtureError::Validation {
                message: format!(
                    "wire result {} reports route evidence for a frame-only scenario",
                    result.id
                ),
            });
        }
        return Ok(());
    };
    let evidence = result
        .route_evidence
        .as_ref()
        .ok_or_else(|| FixtureError::Validation {
            message: format!("wire result {} is missing route evidence", result.id),
        })?;
    let fixture = scenario
        .host_route
        .as_ref()
        .ok_or_else(|| FixtureError::Validation {
            message: format!(
                "wire scenario {} expects route evidence without a host-route fixture",
                scenario.id
            ),
        })?;

    if evidence.application_endpoint != fixture.application_endpoint {
        return Err(FixtureError::Validation {
            message: format!(
                "wire result {} reports application endpoint {}, expected {}",
                result.id, evidence.application_endpoint, fixture.application_endpoint
            ),
        });
    }
    if evidence.candidates.len() != fixture.routes.len() {
        return Err(FixtureError::Validation {
            message: format!(
                "wire result {} reports {} route candidate(s), expected {}",
                result.id,
                evidence.candidates.len(),
                fixture.routes.len()
            ),
        });
    }
    let expected_candidate_identities = fixture
        .routes
        .iter()
        .map(|route| {
            (
                route.transport,
                route.provider_id.as_str(),
                route.locator.as_str(),
            )
        })
        .collect::<BTreeSet<_>>();
    let actual_candidate_identities = evidence
        .candidates
        .iter()
        .map(|candidate| {
            (
                candidate.transport,
                candidate.provider_id.as_str(),
                candidate.requested_locator.as_str(),
            )
        })
        .collect::<BTreeSet<_>>();
    if actual_candidate_identities != expected_candidate_identities
        || actual_candidate_identities.len() != evidence.candidates.len()
    {
        return Err(FixtureError::Validation {
            message: format!(
                "wire result {} candidate identities do not match the suite-owned route set",
                result.id
            ),
        });
    }
    for candidate in &evidence.candidates {
        let provider = plan
            .host_route_providers
            .iter()
            .find(|provider| {
                provider.transport == candidate.transport
                    && provider.provider_id == candidate.provider_id
            })
            .ok_or_else(|| FixtureError::Validation {
                message: format!(
                    "wire result {} reports undeclared host-route provider {} for {:?}",
                    result.id, candidate.provider_id, candidate.transport
                ),
            })?;
        if candidate.selected && candidate.rejection_reason.is_some() {
            return Err(FixtureError::Validation {
                message: format!(
                    "wire result {} marks selected {:?} route as rejected",
                    result.id, candidate.transport
                ),
            });
        }
        if !provider.installed
            && (candidate.selected
                || candidate.rejection_reason
                    != Some(WireHostRouteRejectionReason::LocalUnavailable))
        {
            return Err(FixtureError::Validation {
                message: format!(
                    "wire result {} must reject uninstalled provider {} as local-unavailable",
                    result.id, candidate.provider_id
                ),
            });
        }
        if candidate.selected && (!candidate.locator_resolved || !candidate.security_satisfied) {
            return Err(FixtureError::Validation {
                message: format!(
                    "wire result {} selected an unresolved or security-incompatible {:?} route",
                    result.id, candidate.transport
                ),
            });
        }
    }

    let selected_count = evidence
        .candidates
        .iter()
        .filter(|candidate| candidate.selected)
        .count();
    if fixture.role == WireHostRole::Client && selected_count > 1 {
        return Err(FixtureError::Validation {
            message: format!(
                "wire result {} selected {} client carriers; at most one is permitted",
                result.id, selected_count
            ),
        });
    }

    if let Some(count) = expected.selected_count {
        if selected_count != count as usize {
            return Err(FixtureError::Validation {
                message: format!(
                    "wire result {} selected {} route(s), expected {}",
                    result.id, selected_count, count
                ),
            });
        }
    }
    if let Some(transport) = expected.selected_transport {
        let selected = evidence
            .candidates
            .iter()
            .any(|candidate| candidate.selected && candidate.transport == transport);
        if !selected {
            return Err(FixtureError::Validation {
                message: format!(
                    "wire result {} did not select expected {:?} route",
                    result.id, transport
                ),
            });
        }
    }

    let actual_rejections = evidence
        .candidates
        .iter()
        .filter_map(|candidate| candidate.rejection_reason)
        .fold(BTreeMap::new(), |mut counts, reason| {
            *counts.entry(reason).or_insert(0usize) += 1;
            counts
        });
    let expected_rejections =
        expected
            .rejection_reasons
            .iter()
            .copied()
            .fold(BTreeMap::new(), |mut counts, reason| {
                *counts.entry(reason).or_insert(0usize) += 1;
                counts
            });
    if actual_rejections != expected_rejections {
        return Err(FixtureError::Validation {
            message: format!(
                "wire result {} reports rejection reasons {:?}, expected {:?}",
                result.id, actual_rejections, expected_rejections
            ),
        });
    }

    for listener in &evidence.listeners {
        let known_route = fixture.routes.iter().any(|route| {
            listener.transport == route.transport
                && listener.provider_id == route.provider_id
                && listener.requested_locator == route.locator
        });
        if !known_route {
            return Err(FixtureError::Validation {
                message: format!(
                    "wire result {} reports listener evidence outside the suite-owned route set",
                    result.id
                ),
            });
        }
    }

    let bound = evidence
        .listeners
        .iter()
        .filter(|listener| listener.bound_endpoint.is_some())
        .map(|listener| listener.transport)
        .collect::<BTreeSet<_>>();
    for transport in &expected.bound_transports {
        if !bound.contains(transport) {
            return Err(FixtureError::Validation {
                message: format!(
                    "wire result {} is missing bound {:?} listener",
                    result.id, transport
                ),
            });
        }
    }

    for session in &evidence.accepted_sessions {
        if session.transport != session.active_transport {
            return Err(FixtureError::Validation {
                message: format!(
                    "wire result {} accepted a {:?} listener but reports {:?} as active",
                    result.id, session.transport, session.active_transport
                ),
            });
        }
        if !fixture.routes.iter().any(|route| {
            route.transport == session.transport && route.provider_id == session.provider_id
        }) {
            return Err(FixtureError::Validation {
                message: format!(
                    "wire result {} reports an accepted session outside the suite-owned route set",
                    result.id
                ),
            });
        }
    }

    let accepted_identities = evidence
        .accepted_sessions
        .iter()
        .map(|session| (session.transport, session.provider_id.as_str()))
        .collect::<BTreeSet<_>>();
    if accepted_identities.len() != evidence.accepted_sessions.len() {
        return Err(FixtureError::Validation {
            message: format!(
                "wire result {} reports duplicate accepted-session evidence",
                result.id
            ),
        });
    }
    if fixture.role == WireHostRole::Client {
        let selected_identities = evidence
            .candidates
            .iter()
            .filter(|candidate| candidate.selected)
            .map(|candidate| (candidate.transport, candidate.provider_id.as_str()))
            .collect::<BTreeSet<_>>();
        if accepted_identities != selected_identities {
            return Err(FixtureError::Validation {
                message: format!(
                    "wire result {} client accepted-session evidence does not match the selected carrier",
                    result.id
                ),
            });
        }
    }

    let accepted = evidence
        .accepted_sessions
        .iter()
        .map(|session| session.active_transport)
        .collect::<BTreeSet<_>>();
    for transport in &expected.accepted_transports {
        if !accepted.contains(transport) {
            return Err(FixtureError::Validation {
                message: format!(
                    "wire result {} is missing accepted {:?} session",
                    result.id, transport
                ),
            });
        }
    }

    if expected
        .atomic_rollback
        .is_some_and(|value| value != evidence.atomic_rollback)
    {
        return Err(FixtureError::Validation {
            message: format!("wire result {} atomic rollback mismatch", result.id),
        });
    }
    if expected
        .logical_set_closed
        .is_some_and(|value| value != evidence.logical_set_closed)
    {
        return Err(FixtureError::Validation {
            message: format!(
                "wire result {} logical listener-set state mismatch",
                result.id
            ),
        });
    }
    if expected.terminal_failure.as_deref() != evidence.terminal_failure.as_deref()
        && expected.terminal_failure.is_some()
    {
        return Err(FixtureError::Validation {
            message: format!(
                "wire result {} terminal listener failure mismatch",
                result.id
            ),
        });
    }
    Ok(())
}

pub async fn run_wire_conformance_external(
    plan: &WireConformanceExecutionPlan,
    target_manifest: &WireConformanceTargetManifest,
    target_manifest_path: &Path,
) -> Result<WireConformanceCaseResultReport, FixtureError> {
    run_wire_conformance_external_with_host_target(
        plan,
        target_manifest,
        target_manifest_path,
        None,
    )
    .await
}

pub async fn run_wire_conformance_external_with_host_target(
    plan: &WireConformanceExecutionPlan,
    target_manifest: &WireConformanceTargetManifest,
    target_manifest_path: &Path,
    host_route_target: Option<&Path>,
) -> Result<WireConformanceCaseResultReport, FixtureError> {
    validate_wire_plan_target_alignment(plan, target_manifest)?;

    let mut results = Vec::with_capacity(plan.scenarios.len());
    for scenario in &plan.scenarios {
        let result = if scenario.host_route.is_some() {
            let executable = host_route_target.ok_or_else(|| FixtureError::Validation {
                message: format!(
                    "host-route scenario {} requires --host-route-target",
                    scenario.id
                ),
            })?;
            host_route::run_host_route_scenario(
                scenario,
                executable,
                Path::new(&plan.artifacts.evidence_dir),
                &plan.suite_version,
                &plan.target_name,
            )
            .await?
        } else {
            run_wire_external_scenario(plan, target_manifest, target_manifest_path, scenario)
                .await?
        };
        results.push(result);
    }

    Ok(WireConformanceCaseResultReport {
        schema: Some(
            "https://github.com/NagareWorks/nnrp-conformance/schemas/wire-conformance-case-results.schema.json"
                .to_string(),
        ),
        protocol_version: plan.protocol_version.clone(),
        suite_version: plan.suite_version.clone(),
        target_name: plan.target_name.clone(),
        results,
    })
}

pub fn summarize_wire_external_report(
    report: &WireConformanceCaseResultReport,
) -> WireExternalExecutionSummary {
    let mut summary = WireExternalExecutionSummary {
        selected_scenarios: report.results.len(),
        passed_scenarios: 0,
        failed_scenarios: 0,
    };
    for result in &report.results {
        match result.outcome {
            ApiProfileCaseOutcome::Passed => summary.passed_scenarios += 1,
            ApiProfileCaseOutcome::Failed | ApiProfileCaseOutcome::Skipped => {
                summary.failed_scenarios += 1
            }
        }
    }
    summary
}

fn validate_wire_plan_target_alignment(
    plan: &WireConformanceExecutionPlan,
    target_manifest: &WireConformanceTargetManifest,
) -> Result<(), FixtureError> {
    if plan.protocol_version != target_manifest.protocol_version {
        return Err(FixtureError::Validation {
            message: format!(
                "wire target protocol version mismatch: expected {}, got {}",
                plan.protocol_version, target_manifest.protocol_version
            ),
        });
    }
    if plan.suite_version != target_manifest.suite_version {
        return Err(FixtureError::Validation {
            message: format!(
                "wire target suite version mismatch: expected {}, got {}",
                plan.suite_version, target_manifest.suite_version
            ),
        });
    }
    if plan.target_name != target_manifest.target_name {
        return Err(FixtureError::Validation {
            message: format!(
                "wire target name mismatch: expected {}, got {}",
                plan.target_name, target_manifest.target_name
            ),
        });
    }
    Ok(())
}

async fn run_wire_external_scenario(
    plan: &WireConformanceExecutionPlan,
    target_manifest: &WireConformanceTargetManifest,
    target_manifest_path: &Path,
    scenario: &WireConformanceScenario,
) -> Result<WireConformanceCaseResult, FixtureError> {
    let transport = scenario.transport.ok_or_else(|| FixtureError::Validation {
        message: format!(
            "host-route scenario {} requires the host-route executor",
            scenario.id
        ),
    })?;
    let endpoint_manifest = target_manifest
        .wire_conformance
        .transports
        .iter()
        .find(|endpoint| endpoint.name == transport)
        .ok_or_else(|| FixtureError::Validation {
            message: format!(
                "target manifest does not declare {:?} transport endpoint",
                transport
            ),
        })?;
    let endpoint = wire_reference_endpoint(endpoint_manifest, target_manifest_path)?;
    let timeout = wire_scenario_timeout(scenario);

    let execution = if scenario.id == openai_profile_wire::SCENARIO_ID {
        tokio::time::timeout(timeout, openai_profile_wire::run_client(&endpoint)).await
    } else {
        let case = wire_external_case_for_scenario(scenario)?;
        tokio::time::timeout(timeout, async {
            Ok(run_wire_external_case(case, &endpoint).await?)
        })
        .await
    };
    let evidence_paths = vec![wire_evidence_path(plan, &scenario.id)];
    match execution {
        Ok(Ok(report)) => Ok(wire_external_case_result(scenario, report, evidence_paths)),
        Ok(Err(error)) => Ok(WireConformanceCaseResult {
            id: scenario.id.clone(),
            outcome: ApiProfileCaseOutcome::Failed,
            terminal: WireConformanceTerminal::Error,
            observed_frames: Vec::new(),
            route_evidence: None,
            message: Some(format!("external wire target failed: {error}")),
            evidence_paths,
        }),
        Err(_) => Ok(WireConformanceCaseResult {
            id: scenario.id.clone(),
            outcome: ApiProfileCaseOutcome::Failed,
            terminal: WireConformanceTerminal::Error,
            observed_frames: Vec::new(),
            route_evidence: None,
            message: Some(format!(
                "external wire target exceeded {} ms execution timeout",
                timeout.as_millis()
            )),
            evidence_paths,
        }),
    }
}

fn wire_external_case_for_scenario(
    scenario: &WireConformanceScenario,
) -> Result<WireExternalCase, FixtureError> {
    let case = match scenario.id.as_str() {
        "wire.control.cancel-abort.client" => WireExternalCase::CancelAbortClient,
        "wire.control.deadline-before-submit.client" => {
            WireExternalCase::DeadlineBeforeSubmitClient
        }
        "wire.control.priority-deadline.proxy" => WireExternalCase::PriorityDeadlineProxy,
        "wire.control.progress-backpressure.server" => WireExternalCase::ProgressBackpressureServer,
        "wire.control.capability-route-cache.client" => {
            WireExternalCase::CapabilityRouteCacheClient
        }
        "wire.control.cancel-abort.ipc-client" => WireExternalCase::CancelAbortIpcClient,
        "wire.control.progress-backpressure.websocket-server" => {
            WireExternalCase::ProgressBackpressureWebSocketServer
        }
        _ => {
            return Err(FixtureError::Validation {
                message: format!(
                    "wire scenario {} has no typed external executor",
                    scenario.id
                ),
            });
        }
    };
    if wire_mode(case.mode()) != scenario.mode
        || Some(wire_transport(case.transport())) != scenario.transport
    {
        return Err(FixtureError::Validation {
            message: format!(
                "wire scenario {} mode or transport does not match its typed executor",
                scenario.id
            ),
        });
    }
    Ok(case)
}

fn wire_reference_endpoint(
    endpoint: &nnrp_conformance_fixtures::WireConformanceTransportEndpoint,
    target_manifest_path: &Path,
) -> Result<WireReferenceEndpoint, FixtureError> {
    let transport = match endpoint.name {
        nnrp_conformance_fixtures::WireConformanceTransport::Tcp => ReferenceTransport::Tcp,
        nnrp_conformance_fixtures::WireConformanceTransport::Quic => ReferenceTransport::Quic,
        nnrp_conformance_fixtures::WireConformanceTransport::Ipc => ReferenceTransport::Ipc,
        nnrp_conformance_fixtures::WireConformanceTransport::Websocket => {
            ReferenceTransport::WebSocket
        }
    };
    let Some(security) = &endpoint.security else {
        return Ok(WireReferenceEndpoint::plain(
            transport,
            endpoint.endpoint.clone(),
        ));
    };
    let base = target_manifest_path.parent().unwrap_or(Path::new("."));
    Ok(WireReferenceEndpoint::secure(
        transport,
        endpoint.endpoint.clone(),
        WireEndpointSecurity {
            server_name: security.server_name.clone(),
            trusted_certificate_der: read_wire_security_file(
                base,
                &security.trusted_certificate_der_path,
            )?,
            certificate_der: read_wire_security_file(base, &security.certificate_der_path)?,
            private_key_pkcs8_der: read_wire_security_file(
                base,
                &security.private_key_pkcs8_der_path,
            )?,
        },
    ))
}

fn read_wire_security_file(base: &Path, relative_path: &str) -> Result<Vec<u8>, FixtureError> {
    let path = base.join(relative_path);
    std::fs::read(&path).map_err(|source| FixtureError::Read {
        path: path.display().to_string(),
        source,
    })
}

fn wire_scenario_timeout(scenario: &WireConformanceScenario) -> Duration {
    let declared_ms = scenario
        .steps
        .iter()
        .filter_map(|step| step.timeout_ms)
        .max()
        .unwrap_or(0);
    Duration::from_millis(declared_ms.saturating_add(5_000))
}

fn wire_external_case_result(
    scenario: &WireConformanceScenario,
    report: WireExternalCaseReport,
    evidence_paths: Vec<String>,
) -> WireConformanceCaseResult {
    WireConformanceCaseResult {
        id: scenario.id.clone(),
        outcome: ApiProfileCaseOutcome::Passed,
        terminal: wire_terminal(report.terminal),
        observed_frames: report
            .observed_frames
            .into_iter()
            .map(|frame| WireConformanceObservedFrame {
                direction: wire_direction(frame.direction),
                frame: wire_frame(frame.frame).to_string(),
                payload: Some(frame.detail),
                timestamp_us: Some(u64::try_from(frame.timestamp_us).unwrap_or(u64::MAX)),
            })
            .collect(),
        route_evidence: None,
        message: Some(format!(
            "external wire target executed {:?} over {:?} in {} us",
            report.mode, report.transport, report.elapsed_us
        )),
        evidence_paths,
    }
}

fn wire_mode(mode: WireExternalMode) -> nnrp_conformance_fixtures::WireConformanceMode {
    match mode {
        WireExternalMode::SuiteAsClient => {
            nnrp_conformance_fixtures::WireConformanceMode::SuiteAsClient
        }
        WireExternalMode::SuiteAsServer => {
            nnrp_conformance_fixtures::WireConformanceMode::SuiteAsServer
        }
        WireExternalMode::SuiteAsProxy => {
            nnrp_conformance_fixtures::WireConformanceMode::SuiteAsProxy
        }
    }
}

fn wire_transport(
    transport: ReferenceTransport,
) -> nnrp_conformance_fixtures::WireConformanceTransport {
    match transport {
        ReferenceTransport::Tcp => nnrp_conformance_fixtures::WireConformanceTransport::Tcp,
        ReferenceTransport::Ipc => nnrp_conformance_fixtures::WireConformanceTransport::Ipc,
        ReferenceTransport::Quic => nnrp_conformance_fixtures::WireConformanceTransport::Quic,
        ReferenceTransport::WebSocket => {
            nnrp_conformance_fixtures::WireConformanceTransport::Websocket
        }
    }
}

fn wire_terminal(terminal: WireExternalTerminal) -> WireConformanceTerminal {
    match terminal {
        WireExternalTerminal::Success => WireConformanceTerminal::Success,
        WireExternalTerminal::Cancelled => WireConformanceTerminal::Cancelled,
        WireExternalTerminal::Dropped => WireConformanceTerminal::Dropped,
    }
}

fn wire_direction(direction: WireExternalDirection) -> WireConformanceFrameDirection {
    match direction {
        WireExternalDirection::SuiteToTarget | WireExternalDirection::SuiteProxyToTarget => {
            WireConformanceFrameDirection::Sent
        }
        WireExternalDirection::TargetToSuite
        | WireExternalDirection::ProbeToSuiteProxy
        | WireExternalDirection::TargetThroughSuiteProxyToProbe => {
            WireConformanceFrameDirection::Received
        }
    }
}

fn wire_frame(frame: WireExternalFrame) -> &'static str {
    match frame {
        WireExternalFrame::Request => "REQUEST",
        WireExternalFrame::Deadline => "DEADLINE",
        WireExternalFrame::Cancel => "CANCEL",
        WireExternalFrame::PriorityUpdate => "PRIORITY_UPDATE",
        WireExternalFrame::ExpireAt => "EXPIRE_AT",
        WireExternalFrame::Progress => "PROGRESS",
        WireExternalFrame::CreditUpdate => "CREDIT_UPDATE",
        WireExternalFrame::PartialResult => "PARTIAL_RESULT",
        WireExternalFrame::CapabilityNegotiation => "CAPABILITY_NEGOTIATION",
        WireExternalFrame::RouteHint => "ROUTE_HINT",
        WireExternalFrame::CacheReference => "CACHE_REFERENCE",
        WireExternalFrame::CacheMiss => "CACHE_MISS",
        WireExternalFrame::TraceContext => "TRACE_CONTEXT",
        WireExternalFrame::ResultPush => "RESULT_PUSH",
        WireExternalFrame::ResultDropReason => "RESULT_DROP_REASON",
    }
}

fn wire_evidence_path(plan: &WireConformanceExecutionPlan, scenario_id: &str) -> String {
    let safe_id = scenario_id.replace(['/', '\\', ':'], "_");
    format!("{}/{}.jsonl", plan.artifacts.evidence_dir, safe_id)
}

fn substitute_api_profile_request_parameters(
    recipe: &ApiProfileRecipe,
) -> nnrp_conformance_fixtures::ApiProfileRecipeRequest {
    let mut request = recipe.request.clone();
    substitute_json_parameters(&mut request.body, &recipe.parameters);
    if let Some(nnrp) = &mut request.nnrp {
        substitute_json_parameters(nnrp, &recipe.parameters);
    }
    request
}

fn substitute_json_parameters(
    value: &mut serde_json::Value,
    parameters: &BTreeMap<String, String>,
) {
    match value {
        serde_json::Value::String(text) => {
            if let Some(parameter_name) = text
                .strip_prefix("${")
                .and_then(|rest| rest.strip_suffix('}'))
            {
                if let Some(parameter_value) = parameters.get(parameter_name) {
                    *text = parameter_value.clone();
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                substitute_json_parameters(item, parameters);
            }
        }
        serde_json::Value::Object(fields) => {
            for field in fields.values_mut() {
                substitute_json_parameters(field, parameters);
            }
        }
        serde_json::Value::Null | serde_json::Value::Bool(_) | serde_json::Value::Number(_) => {}
    }
}

pub fn validate_api_profile_results(
    expected_plan: &ApiProfileExecutionPlan,
    actual_report: &ApiProfileCaseResultReport,
) -> Result<ApiProfileValidationSummary, FixtureError> {
    if expected_plan.profile != actual_report.profile {
        return Err(FixtureError::Validation {
            message: format!(
                "api profile mismatch: expected {}, got {}",
                expected_plan.profile, actual_report.profile
            ),
        });
    }
    if expected_plan.schema_version != actual_report.schema_version {
        return Err(FixtureError::Validation {
            message: format!(
                "api profile schema version mismatch: expected {}, got {}",
                expected_plan.schema_version, actual_report.schema_version
            ),
        });
    }
    if expected_plan.adapter != actual_report.adapter {
        return Err(FixtureError::Validation {
            message: format!(
                "api profile adapter mismatch: expected {}, got {}",
                expected_plan.adapter, actual_report.adapter
            ),
        });
    }

    let expected_cases = expected_plan
        .cases
        .iter()
        .map(|case| (case.id.as_str(), case))
        .collect::<BTreeMap<_, _>>();
    let mut actual_ids = BTreeSet::new();
    let mut summary = ApiProfileValidationSummary {
        selected_cases: expected_cases.len(),
        passed_cases: 0,
        failed_cases: 0,
        skipped_cases: 0,
    };

    for result in &actual_report.results {
        let expected_case =
            expected_cases
                .get(result.id.as_str())
                .ok_or_else(|| FixtureError::Validation {
                    message: format!(
                        "api profile results contain an unexpected case id: {}",
                        result.id
                    ),
                })?;
        if !actual_ids.insert(result.id.as_str()) {
            return Err(FixtureError::Validation {
                message: format!(
                    "api profile results contain a duplicate case id: {}",
                    result.id
                ),
            });
        }

        match result.outcome {
            ApiProfileCaseOutcome::Passed => {
                if expected_case.expect.terminal != result.terminal {
                    return Err(FixtureError::Validation {
                        message: format!(
                            "api profile case {} terminal mismatch: expected {:?}, got {:?}",
                            result.id, expected_case.expect.terminal, result.terminal
                        ),
                    });
                }
                validate_expected_api_events(
                    &expected_case.expect.events,
                    &result.events,
                    result.id.as_str(),
                )?;
                validate_api_terminal_event(&result.terminal, &result.events, result.id.as_str())?;
                summary.passed_cases += 1;
            }
            ApiProfileCaseOutcome::Failed => summary.failed_cases += 1,
            ApiProfileCaseOutcome::Skipped => summary.skipped_cases += 1,
        }
    }

    if actual_ids.len() != expected_cases.len() {
        return Err(FixtureError::Validation {
            message: format!(
                "api profile results are missing {} selected case(s)",
                expected_cases.len().saturating_sub(actual_ids.len())
            ),
        });
    }

    Ok(summary)
}

fn validate_api_terminal_event(
    terminal: &nnrp_conformance_fixtures::ApiProfileTerminal,
    events: &[nnrp_conformance_fixtures::ApiProfileObservedEvent],
    case_id: &str,
) -> Result<(), FixtureError> {
    let required_event = match terminal {
        nnrp_conformance_fixtures::ApiProfileTerminal::Success => None,
        nnrp_conformance_fixtures::ApiProfileTerminal::Error => Some("response.error"),
        nnrp_conformance_fixtures::ApiProfileTerminal::Cancelled => Some("response.cancelled"),
    };

    if let Some(required_event) = required_event {
        if !events
            .iter()
            .any(|event| event.event_type == required_event)
        {
            return Err(FixtureError::Validation {
                message: format!(
                    "api profile case {case_id} terminal {:?} must include {required_event}",
                    terminal
                ),
            });
        }
    }

    Ok(())
}

fn validate_api_profile_alignment(
    capability_manifest: &ApiProfileCapabilityManifest,
    recipes: &[ApiProfileRecipe],
) -> Result<(), FixtureError> {
    if capability_manifest.profile != "openai-compatible" {
        return Err(FixtureError::Validation {
            message: format!("unsupported api profile: {}", capability_manifest.profile),
        });
    }
    if capability_manifest.schema_version != "openai-compatible/1" {
        return Err(FixtureError::Validation {
            message: format!(
                "unsupported api profile schema version: {}",
                capability_manifest.schema_version
            ),
        });
    }

    for recipe in recipes {
        if recipe.profile != capability_manifest.profile {
            return Err(FixtureError::Validation {
                message: format!(
                    "api recipe {} profile mismatch: expected {}, got {}",
                    recipe.id, capability_manifest.profile, recipe.profile
                ),
            });
        }
        if recipe.schema_version != capability_manifest.schema_version {
            return Err(FixtureError::Validation {
                message: format!(
                    "api recipe {} schema version mismatch: expected {}, got {}",
                    recipe.id, capability_manifest.schema_version, recipe.schema_version
                ),
            });
        }
    }

    Ok(())
}

fn recipe_is_claimed(declared_capabilities: &BTreeSet<String>, recipe: &ApiProfileRecipe) -> bool {
    required_api_capabilities(recipe)
        .iter()
        .all(|capability| declared_capabilities.contains(capability))
}

fn api_recipe_selection(
    recipe: &ApiProfileRecipe,
    declared_capabilities: &BTreeSet<String>,
) -> CaseSelection {
    match recipe.status {
        CaseStatus::Mandatory | CaseStatus::Optional => {
            if recipe_is_claimed(declared_capabilities, recipe) {
                CaseSelection::Selected
            } else {
                CaseSelection::NotClaimed
            }
        }
        CaseStatus::Experimental | CaseStatus::Deprecated => CaseSelection::Informational,
    }
}

fn api_profile_declared_capabilities(
    capability_manifest: &ApiProfileCapabilityManifest,
) -> BTreeSet<String> {
    let mut capabilities = BTreeSet::new();
    for level in &capability_manifest.compatibility_levels {
        capabilities.insert(format!("api.level{level}"));
    }
    for operation in &capability_manifest.operations {
        capabilities.insert(format!("api.{}", operation.name));
        if operation.streaming {
            capabilities.insert("api.streaming".to_string());
        }
        if operation.non_streaming {
            capabilities.insert("api.non_streaming".to_string());
        }
        if operation.tool_calls {
            capabilities.insert("api.tool_calls".to_string());
        }
        if operation.cancellation {
            capabilities.insert("api.cancellation".to_string());
        }
    }
    for extension in &capability_manifest.extensions {
        capabilities.insert(format!("api.extension.{}", extension.name));
        if extension.critical {
            capabilities.insert(format!("api.extension.{}.critical", extension.name));
        }
    }
    capabilities
}

fn build_api_profile_coverage_matrix(
    recipes: &[ApiProfileRecipe],
    declared_capabilities: &BTreeSet<String>,
) -> Vec<CompatibilityMatrixEntry> {
    let mut compatibility_matrix = BTreeMap::<String, CompatibilityMatrixAccumulator>::new();

    for recipe in recipes {
        let selection = api_recipe_selection(recipe, declared_capabilities);
        let entry = compatibility_matrix
            .entry(recipe.operation.clone())
            .or_default();
        entry
            .required_capabilities
            .extend(required_api_capabilities(recipe));
        entry.case_ids.insert(recipe.id.clone());
        match selection {
            CaseSelection::Selected => entry.summary.selected_cases += 1,
            CaseSelection::NotClaimed => entry.summary.not_claimed_cases += 1,
            CaseSelection::Informational => entry.summary.informational_cases += 1,
        }
        match recipe.status {
            CaseStatus::Mandatory => entry.statuses.mandatory_cases += 1,
            CaseStatus::Optional => entry.statuses.optional_cases += 1,
            CaseStatus::Experimental => entry.statuses.experimental_cases += 1,
            CaseStatus::Deprecated => entry.statuses.deprecated_cases += 1,
        }
    }

    compatibility_matrix
        .into_iter()
        .map(|(feature, entry)| CompatibilityMatrixEntry {
            feature,
            required_capabilities: entry.required_capabilities.into_iter().collect(),
            summary: entry.summary,
            statuses: entry.statuses,
            case_ids: entry.case_ids.into_iter().collect(),
        })
        .collect()
}

fn required_api_capabilities(recipe: &ApiProfileRecipe) -> Vec<String> {
    if !recipe.required_capabilities.is_empty() {
        let mut capabilities = recipe.required_capabilities.clone();
        capabilities.sort();
        capabilities.dedup();
        return capabilities;
    }

    let mut capabilities = vec![format!("api.{}", recipe.operation)];
    capabilities.push(
        if recipe_requires_streaming(recipe) {
            "api.streaming"
        } else {
            "api.non_streaming"
        }
        .to_string(),
    );

    if recipe_requires_tool_calls(recipe) {
        capabilities.push("api.tool_calls".to_string());
    }
    if recipe_requires_cancellation(recipe) {
        capabilities.push("api.cancellation".to_string());
    }
    if let Some(extensions) = recipe
        .request
        .nnrp
        .as_ref()
        .and_then(|nnrp| nnrp.get("extensions"))
        .and_then(|extensions| extensions.as_array())
    {
        capabilities.extend(
            extensions
                .iter()
                .filter_map(|extension| extension.as_str())
                .map(|extension| format!("api.extension.{extension}")),
        );
    }

    capabilities.sort();
    capabilities.dedup();
    capabilities
}

fn recipe_requires_streaming(recipe: &ApiProfileRecipe) -> bool {
    recipe
        .request
        .body
        .get("stream")
        .and_then(|stream| stream.as_bool())
        .unwrap_or(false)
}

fn recipe_requires_tool_calls(recipe: &ApiProfileRecipe) -> bool {
    recipe
        .request
        .body
        .get("tools")
        .and_then(|tools| tools.as_array())
        .is_some_and(|tools| !tools.is_empty())
}

fn recipe_requires_cancellation(recipe: &ApiProfileRecipe) -> bool {
    recipe
        .request
        .nnrp
        .as_ref()
        .and_then(|nnrp| nnrp.get("cancel_after_events"))
        .and_then(|count| count.as_u64())
        .is_some()
}

fn validate_expected_api_events(
    expected_events: &[ApiProfileExpectedEvent],
    actual_events: &[nnrp_conformance_fixtures::ApiProfileObservedEvent],
    case_id: &str,
) -> Result<(), FixtureError> {
    let mut search_from = 0usize;

    for expected in expected_events {
        let min_count = expected.min_count.unwrap_or(u64::from(!expected.optional));
        let observed_count = actual_events
            .iter()
            .filter(|event| event.event_type == expected.event_type)
            .count() as u64;
        if observed_count < min_count {
            return Err(FixtureError::Validation {
                message: format!(
                    "api profile case {case_id} expected event {} at least {} time(s), got {}",
                    expected.event_type, min_count, observed_count
                ),
            });
        }

        if expected.optional && observed_count == 0 {
            continue;
        }

        if min_count > 0 {
            let Some((relative_index, event)) = actual_events
                .iter()
                .skip(search_from)
                .enumerate()
                .find(|(_, event)| event.event_type == expected.event_type)
            else {
                return Err(FixtureError::Validation {
                    message: format!(
                        "api profile case {case_id} did not observe event {} in expected order",
                        expected.event_type
                    ),
                });
            };
            validate_expected_api_event_fields(expected, event, case_id)?;
            search_from += relative_index + 1;
        }
    }

    Ok(())
}

fn validate_expected_api_event_fields(
    expected: &ApiProfileExpectedEvent,
    actual: &nnrp_conformance_fixtures::ApiProfileObservedEvent,
    case_id: &str,
) -> Result<(), FixtureError> {
    let Some(expected_fields) = expected
        .fields
        .as_ref()
        .and_then(|fields| fields.as_object())
    else {
        return Ok(());
    };

    for (field, expected_value) in expected_fields {
        let Some(actual_value) = actual.fields.get(field) else {
            return Err(FixtureError::Validation {
                message: format!(
                    "api profile case {case_id} event {} missing expected field {field}",
                    expected.event_type
                ),
            });
        };
        if !json_contains(actual_value, expected_value) {
            return Err(FixtureError::Validation {
                message: format!(
                    "api profile case {case_id} event {} field {field} mismatch",
                    expected.event_type
                ),
            });
        }
    }

    Ok(())
}

fn json_contains(actual: &serde_json::Value, expected: &serde_json::Value) -> bool {
    match (actual, expected) {
        (serde_json::Value::Object(actual), serde_json::Value::Object(expected)) => {
            expected.iter().all(|(key, expected_value)| {
                actual
                    .get(key)
                    .is_some_and(|actual_value| json_contains(actual_value, expected_value))
            })
        }
        (serde_json::Value::Array(actual), serde_json::Value::Array(expected)) => {
            expected.len() <= actual.len()
                && expected
                    .iter()
                    .zip(actual.iter())
                    .all(|(expected_value, actual_value)| {
                        json_contains(actual_value, expected_value)
                    })
        }
        _ => actual == expected,
    }
}

fn default_benchmark_scenarios(supports: &[String]) -> Vec<BenchmarkScenario> {
    let declared_capabilities = supports.iter().cloned().collect::<BTreeSet<_>>();

    let mut scenarios = vec![
        BenchmarkScenario {
            id: "l4.header.encode_decode.latency".to_string(),
            category: BenchmarkCategory::Latency,
            feature: "benchmark.header".to_string(),
            required_capabilities: vec![],
            description:
                "Measure L0 header encode/decode latency for the minimum fixed header shape."
                    .to_string(),
            workload: BenchmarkWorkload {
                operation: "header_encode_decode".to_string(),
                payload: "l0_header".to_string(),
                transport: None,
                iterations: Some(100_000),
                warmup_iterations: Some(10_000),
                duration_seconds: None,
            },
        },
        BenchmarkScenario {
            id: "l4.metadata.session_open_ack.latency".to_string(),
            category: BenchmarkCategory::Latency,
            feature: "benchmark.metadata".to_string(),
            required_capabilities: vec!["session.open_close".to_string()],
            description:
                "Measure SESSION_OPEN plus SESSION_OPEN_ACK metadata encode/decode latency."
                    .to_string(),
            workload: BenchmarkWorkload {
                operation: "metadata_encode_decode".to_string(),
                payload: "session_open_ack".to_string(),
                transport: None,
                iterations: Some(100_000),
                warmup_iterations: Some(10_000),
                duration_seconds: None,
            },
        },
        BenchmarkScenario {
            id: "l4.metadata.submit_result.latency".to_string(),
            category: BenchmarkCategory::Latency,
            feature: "benchmark.metadata.submit_result".to_string(),
            required_capabilities: vec![
                "frame_submit.tensor.inline".to_string(),
                "result_push.basic".to_string(),
            ],
            description: "Measure FRAME_SUBMIT plus RESULT_PUSH metadata encode/decode latency."
                .to_string(),
            workload: BenchmarkWorkload {
                operation: "submit_result_metadata_encode_decode".to_string(),
                payload: "frame_submit_result_push".to_string(),
                transport: None,
                iterations: Some(100_000),
                warmup_iterations: Some(10_000),
                duration_seconds: None,
            },
        },
        BenchmarkScenario {
            id: "l4.typed_payload.tensor_pack_unpack.latency".to_string(),
            category: BenchmarkCategory::Latency,
            feature: "benchmark.typed_payload.tensor".to_string(),
            required_capabilities: vec!["frame_submit.tensor.inline".to_string()],
            description: "Measure tensor descriptor, tile index, and payload pack/unpack latency."
                .to_string(),
            workload: BenchmarkWorkload {
                operation: "typed_payload_pack_unpack".to_string(),
                payload: "tensor_descriptor_plus_payload".to_string(),
                transport: None,
                iterations: Some(100_000),
                warmup_iterations: Some(10_000),
                duration_seconds: None,
            },
        },
        BenchmarkScenario {
            id: "l4.runtime.probe.latency".to_string(),
            category: BenchmarkCategory::Latency,
            feature: "benchmark.runtime_probe".to_string(),
            required_capabilities: vec![],
            description: "Measure SDK runtime version and capability probe latency.".to_string(),
            workload: BenchmarkWorkload {
                operation: "runtime_probe".to_string(),
                payload: "version_capability_query".to_string(),
                transport: None,
                iterations: Some(100_000),
                warmup_iterations: Some(10_000),
                duration_seconds: None,
            },
        },
        BenchmarkScenario {
            id: "l4.session.lifecycle.latency".to_string(),
            category: BenchmarkCategory::Latency,
            feature: "benchmark.session_lifecycle".to_string(),
            required_capabilities: vec!["session.open_close".to_string()],
            description: "Measure SDK-local session open plus close lifecycle latency.".to_string(),
            workload: BenchmarkWorkload {
                operation: "session_lifecycle".to_string(),
                payload: "open_close_loop".to_string(),
                transport: None,
                iterations: Some(100_000),
                warmup_iterations: Some(10_000),
                duration_seconds: None,
            },
        },
        BenchmarkScenario {
            id: "l4.submit_result.inline_tensor.throughput".to_string(),
            category: BenchmarkCategory::Throughput,
            feature: "benchmark.submit_result".to_string(),
            required_capabilities: vec![
                "frame_submit.tensor.inline".to_string(),
                "result_push.basic".to_string(),
            ],
            description:
                "Measure inline tensor submit/result throughput through the SDK runtime path."
                    .to_string(),
            workload: BenchmarkWorkload {
                operation: "submit_result_loop".to_string(),
                payload: "inline_tensor_4k".to_string(),
                transport: None,
                iterations: None,
                warmup_iterations: Some(1_000),
                duration_seconds: Some(10),
            },
        },
        BenchmarkScenario {
            id: "l4.transport.tcp.loopback.throughput".to_string(),
            category: BenchmarkCategory::Throughput,
            feature: "benchmark.transport.tcp".to_string(),
            required_capabilities: vec!["transport.tcp".to_string()],
            description: "Measure request/result throughput over a local TCP loopback transport."
                .to_string(),
            workload: BenchmarkWorkload {
                operation: "transport_loopback".to_string(),
                payload: "request_result_stream".to_string(),
                transport: Some("tcp".to_string()),
                iterations: None,
                warmup_iterations: Some(1_000),
                duration_seconds: Some(10),
            },
        },
        BenchmarkScenario {
            id: "l4.transport.quic.loopback.throughput".to_string(),
            category: BenchmarkCategory::Throughput,
            feature: "benchmark.transport.quic".to_string(),
            required_capabilities: vec!["transport.quic".to_string()],
            description:
                "Measure request/result throughput over a local QUIC loopback transport slot."
                    .to_string(),
            workload: BenchmarkWorkload {
                operation: "transport_loopback".to_string(),
                payload: "request_result_stream".to_string(),
                transport: Some("quic".to_string()),
                iterations: None,
                warmup_iterations: Some(1_000),
                duration_seconds: Some(10),
            },
        },
    ];

    scenarios.retain(|scenario| {
        scenario
            .required_capabilities
            .iter()
            .all(|capability| declared_capabilities.contains(capability))
    });
    scenarios
}

pub fn build_execution_plan(
    protocol_manifest: &ProtocolManifest,
    case_manifest: &CaseManifest,
    capability_manifest: Option<&CapabilityManifest>,
    case_manifest_path: &std::path::Path,
    capability_manifest_path: Option<&std::path::Path>,
) -> Result<ConformanceReport, FixtureError> {
    validate_protocol_alignment(
        protocol_manifest,
        case_manifest,
        capability_manifest,
        case_manifest_path,
        capability_manifest_path,
    )?;

    Ok(build_execution_plan_from_cases(
        protocol_manifest,
        case_manifest.cases.iter(),
        capability_manifest,
    ))
}

pub fn build_execution_plan_for_manifests<'a>(
    protocol_manifest: &ProtocolManifest,
    case_manifests: impl IntoIterator<Item = (&'a CaseManifest, &'a Path)>,
    capability_manifest: Option<&CapabilityManifest>,
    capability_manifest_path: Option<&Path>,
) -> Result<ConformanceReport, FixtureError> {
    let case_manifests = case_manifests.into_iter().collect::<Vec<_>>();

    for (case_manifest, case_manifest_path) in &case_manifests {
        validate_protocol_alignment(
            protocol_manifest,
            case_manifest,
            capability_manifest,
            case_manifest_path,
            capability_manifest_path,
        )?;
    }
    validate_declared_capabilities(
        capability_manifest,
        case_manifests
            .iter()
            .flat_map(|(case_manifest, _)| case_manifest.cases.iter()),
    )?;

    Ok(build_execution_plan_from_cases(
        protocol_manifest,
        case_manifests
            .into_iter()
            .flat_map(|(case_manifest, _)| case_manifest.cases.iter()),
        capability_manifest,
    ))
}

pub fn build_adapter_execution_plan(
    protocol_manifest: &ProtocolManifest,
    case_manifest: &CaseManifest,
    capability_manifest: &CapabilityManifest,
    case_manifest_path: &std::path::Path,
    capability_manifest_path: &std::path::Path,
    artifacts: AdapterArtifactContext,
) -> Result<AdapterExecutionPlan, FixtureError> {
    validate_protocol_alignment(
        protocol_manifest,
        case_manifest,
        Some(capability_manifest),
        case_manifest_path,
        Some(capability_manifest_path),
    )?;

    Ok(build_adapter_execution_plan_from_cases(
        protocol_manifest,
        case_manifest.cases.iter(),
        capability_manifest,
        artifacts,
    ))
}

pub fn build_adapter_execution_plan_for_manifests<'a>(
    protocol_manifest: &ProtocolManifest,
    case_manifests: impl IntoIterator<Item = (&'a CaseManifest, &'a Path)>,
    capability_manifest: &CapabilityManifest,
    capability_manifest_path: &Path,
    artifacts: AdapterArtifactContext,
) -> Result<AdapterExecutionPlan, FixtureError> {
    let case_manifests = case_manifests.into_iter().collect::<Vec<_>>();

    for (case_manifest, case_manifest_path) in &case_manifests {
        validate_protocol_alignment(
            protocol_manifest,
            case_manifest,
            Some(capability_manifest),
            case_manifest_path,
            Some(capability_manifest_path),
        )?;
    }
    validate_declared_capabilities(
        Some(capability_manifest),
        case_manifests
            .iter()
            .flat_map(|(case_manifest, _)| case_manifest.cases.iter()),
    )?;

    Ok(build_adapter_execution_plan_from_cases(
        protocol_manifest,
        case_manifests
            .into_iter()
            .flat_map(|(case_manifest, _)| case_manifest.cases.iter()),
        capability_manifest,
        artifacts,
    ))
}

#[cfg(test)]
mod tests {
    use super::{
        build_adapter_execution_plan, build_adapter_execution_plan_for_manifests,
        build_api_profile_execution_plan, build_benchmark_execution_plan, build_execution_plan,
        build_execution_plan_for_manifests, build_wire_conformance_execution_plan,
        run_wire_conformance_external, validate_api_profile_results,
        validate_complete_capability_coverage, validate_wire_conformance_results,
        wire_external_case_for_scenario,
    };
    use nnrp_conformance_fixtures::{
        AdapterArtifactContext, ApiProfileCapabilityManifest, ApiProfileCaseOutcome,
        ApiProfileCaseResult, ApiProfileCaseResultReport, ApiProfileExpectation,
        ApiProfileExpectedEvent, ApiProfileExtensionCapability, ApiProfileObservedEvent,
        ApiProfileOperationCapability, ApiProfileRecipe, ApiProfileRecipeRequest,
        ApiProfileTerminal, BenchmarkArtifactContext, CapabilityManifest, CaseDefinition,
        CaseLayer, CaseManifest, CaseStatus, ProtocolManifest, WireConformanceCaseResult,
        WireConformanceCaseResultReport, WireConformanceExpectation, WireConformanceFrameDirection,
        WireConformanceFramePayloadInvariant, WireConformanceLimits, WireConformanceMode,
        WireConformanceObservedFrame, WireConformanceScenario, WireConformanceStep,
        WireConformanceTarget, WireConformanceTargetManifest, WireConformanceTerminal,
        WireConformanceTransport, WireConformanceTransportEndpoint,
        WireConformanceTransportSecurity, WireHostAcceptedSessionEvidence, WireHostCredentialOwner,
        WireHostPlatform, WireHostProviderRoute, WireHostRole, WireHostRouteCandidateEvidence,
        WireHostRouteEvidence, WireHostRouteExpectation, WireHostRouteFixture,
        WireHostRouteProviderCapability, WireHostRouteRejectionReason, WireHostRouteSecurity,
        WireHostRouteSecurityMode, load_json_file,
    };
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};

    #[test]
    fn complete_capability_coverage_reports_every_missing_token() {
        let cases = [
            CaseDefinition {
                id: "l1.handshake.basic".to_string(),
                layer: CaseLayer::L1,
                status: CaseStatus::Mandatory,
                feature: "handshake.basic".to_string(),
                required_capabilities: vec!["handshake.basic".to_string()],
                description: "handshake".to_string(),
                parameters: BTreeMap::new(),
            },
            CaseDefinition {
                id: "l3.transport.probe".to_string(),
                layer: CaseLayer::L3,
                status: CaseStatus::Optional,
                feature: "transport.probe".to_string(),
                required_capabilities: vec![
                    "transport.quic".to_string(),
                    "transport.tcp".to_string(),
                ],
                description: "transport probe".to_string(),
                parameters: BTreeMap::new(),
            },
            CaseDefinition {
                id: "l4.control.supersede".to_string(),
                layer: CaseLayer::L4,
                status: CaseStatus::Experimental,
                feature: "control.supersede".to_string(),
                required_capabilities: vec!["control.supersede".to_string()],
                description: "experimental supersede control".to_string(),
                parameters: BTreeMap::new(),
            },
            CaseDefinition {
                id: "l4.control.legacy".to_string(),
                layer: CaseLayer::L4,
                status: CaseStatus::Deprecated,
                feature: "control.legacy".to_string(),
                required_capabilities: vec!["control.legacy".to_string()],
                description: "deprecated legacy control".to_string(),
                parameters: BTreeMap::new(),
            },
        ];
        let capability_manifest = CapabilityManifest {
            schema: None,
            implementation_name: "partial-sdk".to_string(),
            protocol_version: "nnrp-1-preview4".to_string(),
            supports: vec!["handshake.basic".to_string()],
        };

        let error = validate_complete_capability_coverage(&capability_manifest, cases.iter())
            .expect_err("partial SDK coverage must be rejected");

        assert_eq!(
            error.to_string(),
            "fixture validation failed: capability manifest partial-sdk does not cover the complete adapter case scope; missing capability token(s): transport.quic, transport.tcp"
        );
    }

    #[test]
    fn complete_capability_coverage_accepts_the_full_case_scope() {
        let cases = [CaseDefinition {
            id: "l3.transport.probe".to_string(),
            layer: CaseLayer::L3,
            status: CaseStatus::Optional,
            feature: "transport.probe".to_string(),
            required_capabilities: vec!["transport.quic".to_string(), "transport.tcp".to_string()],
            description: "transport probe".to_string(),
            parameters: BTreeMap::new(),
        }];
        let capability_manifest = CapabilityManifest {
            schema: None,
            implementation_name: "complete-sdk".to_string(),
            protocol_version: "nnrp-1-preview4".to_string(),
            supports: vec!["transport.tcp".to_string(), "transport.quic".to_string()],
        };

        validate_complete_capability_coverage(&capability_manifest, cases.iter())
            .expect("complete SDK coverage should pass");
    }

    #[test]
    fn marks_unclaimed_capabilities_as_not_claimed() {
        let protocol_manifest = ProtocolManifest {
            schema: None,
            protocol_version: "nnrp-1-preview3".to_string(),
            suite_version: "0.1.0".to_string(),
            status: "draft".to_string(),
            case_manifests: vec![],
            vector_recipe_manifests: vec![],
            vector_manifests: vec![],
            report_schema: "report.schema.json".to_string(),
        };
        let case_manifest = CaseManifest {
            schema: None,
            protocol_version: "nnrp-1-preview3".to_string(),
            manifest_name: "mandatory-core".to_string(),
            cases: vec![CaseDefinition {
                id: "l1.flow_update.preview3".to_string(),
                layer: CaseLayer::L1,
                status: CaseStatus::Mandatory,
                feature: "flow_update".to_string(),
                required_capabilities: vec!["flow_update".to_string()],
                description: "test".to_string(),
                parameters: BTreeMap::new(),
            }],
        };
        let capability_manifest = CapabilityManifest {
            schema: None,
            implementation_name: "sample".to_string(),
            protocol_version: "nnrp-1-preview3".to_string(),
            supports: vec![],
        };

        let summary = build_execution_plan(
            &protocol_manifest,
            &case_manifest,
            Some(&capability_manifest),
            Path::new("cases/mandatory-core.json"),
            Some(Path::new("example-capabilities.json")),
        )
        .expect("execution plan should build");

        assert_eq!(summary.summary.selected_cases, 0);
        assert_eq!(summary.summary.not_claimed_cases, 1);
        assert_eq!(summary.cases[0].selection, "not_claimed");
    }

    #[test]
    fn rejects_unknown_capability_tokens() {
        let protocol_manifest = ProtocolManifest {
            schema: None,
            protocol_version: "nnrp-1-preview3".to_string(),
            suite_version: "0.1.0".to_string(),
            status: "draft".to_string(),
            case_manifests: vec![],
            vector_recipe_manifests: vec![],
            vector_manifests: vec![],
            report_schema: "report.schema.json".to_string(),
        };
        let case_manifest = CaseManifest {
            schema: None,
            protocol_version: "nnrp-1-preview3".to_string(),
            manifest_name: "mandatory-core".to_string(),
            cases: vec![CaseDefinition {
                id: "l1.flow_update.preview3".to_string(),
                layer: CaseLayer::L1,
                status: CaseStatus::Mandatory,
                feature: "flow_update".to_string(),
                required_capabilities: vec!["flow_update".to_string()],
                description: "test".to_string(),
                parameters: BTreeMap::new(),
            }],
        };
        let capability_manifest = CapabilityManifest {
            schema: None,
            implementation_name: "sample".to_string(),
            protocol_version: "nnrp-1-preview3".to_string(),
            supports: vec!["flow_update_typo".to_string()],
        };

        let error = build_execution_plan_for_manifests(
            &protocol_manifest,
            [(&case_manifest, Path::new("cases/mandatory-core.json"))],
            Some(&capability_manifest),
            Some(Path::new("example-capabilities.json")),
        )
        .expect_err("unknown capability token should be rejected");

        assert!(error.to_string().contains("unknown capability token"));
        assert!(error.to_string().contains("flow_update_typo"));
    }

    #[test]
    fn keeps_experimental_cases_informational() {
        let protocol_manifest = ProtocolManifest {
            schema: None,
            protocol_version: "nnrp-1-preview3".to_string(),
            suite_version: "0.1.0".to_string(),
            status: "draft".to_string(),
            case_manifests: vec![],
            vector_recipe_manifests: vec![],
            vector_manifests: vec![],
            report_schema: "report.schema.json".to_string(),
        };
        let case_manifest = CaseManifest {
            schema: None,
            protocol_version: "nnrp-1-preview3".to_string(),
            manifest_name: "mandatory-core".to_string(),
            cases: vec![CaseDefinition {
                id: "l1.flow_update.preview3".to_string(),
                layer: CaseLayer::L1,
                status: CaseStatus::Experimental,
                feature: "flow_update".to_string(),
                required_capabilities: vec!["flow_update".to_string()],
                description: "test".to_string(),
                parameters: BTreeMap::new(),
            }],
        };

        let summary = build_execution_plan(
            &protocol_manifest,
            &case_manifest,
            None,
            Path::new("cases/mandatory-core.json"),
            Option::<&Path>::None,
        )
        .expect("execution plan should build");

        assert_eq!(summary.summary.informational_cases, 1);
        assert_eq!(summary.cases[0].selection, "informational");
    }

    #[test]
    fn aggregates_multiple_case_manifests() {
        let protocol_manifest = ProtocolManifest {
            schema: None,
            protocol_version: "nnrp-1-preview2".to_string(),
            suite_version: "0.1.0".to_string(),
            status: "draft".to_string(),
            case_manifests: vec![
                "cases/l0-wire-vectors.json".to_string(),
                "cases/l3-transport-smoke.json".to_string(),
            ],
            vector_recipe_manifests: vec![],
            vector_manifests: vec![],
            report_schema: "../../schemas/report.schema.json".to_string(),
        };
        let case_manifest_a = CaseManifest {
            schema: None,
            protocol_version: "nnrp-1-preview2".to_string(),
            manifest_name: "l0-wire-vectors".to_string(),
            cases: vec![CaseDefinition {
                id: "l0.header.fixed_shape.golden".to_string(),
                layer: CaseLayer::L0,
                status: CaseStatus::Mandatory,
                feature: "header.fixed_shape".to_string(),
                required_capabilities: vec![],
                description: "test".to_string(),
                parameters: BTreeMap::new(),
            }],
        };
        let case_manifest_b = CaseManifest {
            schema: None,
            protocol_version: "nnrp-1-preview2".to_string(),
            manifest_name: "l3-transport-smoke".to_string(),
            cases: vec![CaseDefinition {
                id: "l3.transport.tcp.session_smoke".to_string(),
                layer: CaseLayer::L3,
                status: CaseStatus::Optional,
                feature: "transport.tcp".to_string(),
                required_capabilities: vec!["transport.tcp".to_string()],
                description: "test".to_string(),
                parameters: BTreeMap::new(),
            }],
        };
        let capability_manifest = CapabilityManifest {
            schema: None,
            implementation_name: "sample".to_string(),
            protocol_version: "nnrp-1-preview2".to_string(),
            supports: vec!["transport.tcp".to_string()],
        };

        let summary = build_execution_plan_for_manifests(
            &protocol_manifest,
            [
                (&case_manifest_a, Path::new("cases/l0-wire-vectors.json")),
                (&case_manifest_b, Path::new("cases/l3-transport-smoke.json")),
            ],
            Some(&capability_manifest),
            Some(Path::new("nnrp-preview2.capabilities.json")),
        )
        .expect("execution plan should build");

        assert_eq!(summary.summary.selected_cases, 2);
        assert_eq!(summary.summary.not_claimed_cases, 0);
        assert_eq!(summary.cases.len(), 2);
        assert_eq!(summary.compatibility_matrix.len(), 2);
    }

    #[test]
    fn builds_feature_compatibility_matrix() {
        let protocol_manifest = ProtocolManifest {
            schema: None,
            protocol_version: "nnrp-1-preview3".to_string(),
            suite_version: "0.1.0".to_string(),
            status: "draft".to_string(),
            case_manifests: vec![],
            vector_recipe_manifests: vec![],
            vector_manifests: vec![],
            report_schema: "report.schema.json".to_string(),
        };
        let case_manifest = CaseManifest {
            schema: None,
            protocol_version: "nnrp-1-preview3".to_string(),
            manifest_name: "matrix".to_string(),
            cases: vec![
                CaseDefinition {
                    id: "l1.flow_update.connection.scope.validation".to_string(),
                    layer: CaseLayer::L1,
                    status: CaseStatus::Experimental,
                    feature: "flow_update".to_string(),
                    required_capabilities: vec!["flow_update".to_string()],
                    description: "test".to_string(),
                    parameters: BTreeMap::new(),
                },
                CaseDefinition {
                    id: "l1.transport.tcp.minimum".to_string(),
                    layer: CaseLayer::L3,
                    status: CaseStatus::Optional,
                    feature: "transport.tcp".to_string(),
                    required_capabilities: vec!["transport.tcp".to_string()],
                    description: "test".to_string(),
                    parameters: BTreeMap::new(),
                },
                CaseDefinition {
                    id: "l1.transport.tcp.fallback".to_string(),
                    layer: CaseLayer::L3,
                    status: CaseStatus::Optional,
                    feature: "transport.tcp".to_string(),
                    required_capabilities: vec![
                        "transport.tcp".to_string(),
                        "transport.common".to_string(),
                    ],
                    description: "test".to_string(),
                    parameters: BTreeMap::new(),
                },
            ],
        };
        let capability_manifest = CapabilityManifest {
            schema: None,
            implementation_name: "sample".to_string(),
            protocol_version: "nnrp-1-preview3".to_string(),
            supports: vec!["transport.tcp".to_string()],
        };

        let summary = build_execution_plan(
            &protocol_manifest,
            &case_manifest,
            Some(&capability_manifest),
            Path::new("cases/matrix.json"),
            Some(Path::new("example-capabilities.json")),
        )
        .expect("execution plan should build");

        assert_eq!(summary.compatibility_matrix.len(), 2);

        let flow_update = summary
            .compatibility_matrix
            .iter()
            .find(|entry| entry.feature == "flow_update")
            .expect("flow_update feature entry should exist");
        assert_eq!(flow_update.summary.informational_cases, 1);
        assert_eq!(flow_update.statuses.experimental_cases, 1);
        assert_eq!(flow_update.required_capabilities, vec!["flow_update"]);

        let transport_tcp = summary
            .compatibility_matrix
            .iter()
            .find(|entry| entry.feature == "transport.tcp")
            .expect("transport.tcp feature entry should exist");
        assert_eq!(transport_tcp.summary.selected_cases, 1);
        assert_eq!(transport_tcp.summary.not_claimed_cases, 1);
        assert_eq!(transport_tcp.statuses.optional_cases, 2);
        assert_eq!(
            transport_tcp.required_capabilities,
            vec!["transport.common", "transport.tcp"]
        );
        assert_eq!(
            transport_tcp.case_ids,
            vec!["l1.transport.tcp.fallback", "l1.transport.tcp.minimum"]
        );
    }

    #[test]
    fn adapter_execution_plan_keeps_only_selected_cases() {
        let protocol_manifest = ProtocolManifest {
            schema: None,
            protocol_version: "nnrp-1-preview3".to_string(),
            suite_version: "0.1.0".to_string(),
            status: "draft".to_string(),
            case_manifests: vec![],
            vector_recipe_manifests: vec![],
            vector_manifests: vec![],
            report_schema: "report.schema.json".to_string(),
        };
        let case_manifest = CaseManifest {
            schema: None,
            protocol_version: "nnrp-1-preview3".to_string(),
            manifest_name: "adapter-plan".to_string(),
            cases: vec![
                CaseDefinition {
                    id: "l1.handshake.basic".to_string(),
                    layer: CaseLayer::L1,
                    status: CaseStatus::Mandatory,
                    feature: "handshake.basic".to_string(),
                    required_capabilities: vec!["handshake.basic".to_string()],
                    description: "selected".to_string(),
                    parameters: BTreeMap::from([(
                        "metadata_hex".to_string(),
                        serde_json::json!("0102"),
                    )]),
                },
                CaseDefinition {
                    id: "l3.transport.quic.minimum".to_string(),
                    layer: CaseLayer::L3,
                    status: CaseStatus::Optional,
                    feature: "transport.quic".to_string(),
                    required_capabilities: vec!["transport.quic".to_string()],
                    description: "not claimed".to_string(),
                    parameters: BTreeMap::new(),
                },
                CaseDefinition {
                    id: "l1.flow_update.connection.scope.validation".to_string(),
                    layer: CaseLayer::L1,
                    status: CaseStatus::Experimental,
                    feature: "flow_update".to_string(),
                    required_capabilities: vec!["flow_update".to_string()],
                    description: "informational".to_string(),
                    parameters: BTreeMap::new(),
                },
            ],
        };
        let capability_manifest = CapabilityManifest {
            schema: None,
            implementation_name: "sample".to_string(),
            protocol_version: "nnrp-1-preview3".to_string(),
            supports: vec!["handshake.basic".to_string()],
        };

        let plan = build_adapter_execution_plan(
            &protocol_manifest,
            &case_manifest,
            &capability_manifest,
            Path::new("cases/adapter-plan.json"),
            Path::new("example-capabilities.json"),
            AdapterArtifactContext {
                results_path: "artifacts/adapter-results.json".to_string(),
                evidence_dir: "artifacts/evidence".to_string(),
            },
        )
        .expect("adapter execution plan should build");

        assert_eq!(plan.implementation_name, "sample");
        assert_eq!(plan.cases.len(), 1);
        assert_eq!(plan.cases[0].id, "l1.handshake.basic");
        assert_eq!(
            plan.cases[0].parameters.get("metadata_hex"),
            Some(&serde_json::json!("0102"))
        );
    }

    #[test]
    fn builds_preview3_execution_plan_from_repo_fixtures() {
        let protocol_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("protocol")
            .join("nnrp-1-preview3");
        let protocol_manifest: ProtocolManifest =
            load_json_file(protocol_root.join("manifest.json"))
                .expect("protocol manifest should load");
        let capability_manifest: CapabilityManifest =
            load_json_file(protocol_root.join("example-capabilities.json"))
                .expect("example capability manifest should load");

        let loaded_manifests = protocol_manifest
            .case_manifests
            .iter()
            .map(|relative_path| {
                let case_manifest: CaseManifest = load_json_file(protocol_root.join(relative_path))
                    .unwrap_or_else(|error| {
                        panic!("case manifest {relative_path} should load: {error}")
                    });
                (PathBuf::from(relative_path), case_manifest)
            })
            .collect::<Vec<_>>();

        let summary = build_execution_plan_for_manifests(
            &protocol_manifest,
            loaded_manifests
                .iter()
                .map(|(path, manifest)| (manifest, path.as_path())),
            Some(&capability_manifest),
            Some(Path::new("example-capabilities.json")),
        )
        .expect("execution plan should build from repo fixtures");

        assert_eq!(summary.summary.selected_cases, 20);
        assert_eq!(summary.summary.not_claimed_cases, 37);
        assert_eq!(summary.summary.informational_cases, 9);
        assert_eq!(summary.cases.len(), 66);
        assert!(
            summary
                .cases
                .iter()
                .any(|case| case.id == "l2.profile.token.partial.callback_polling.validation")
        );
    }

    #[test]
    fn builds_preview3_adapter_execution_plan_from_repo_fixtures() {
        let protocol_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("protocol")
            .join("nnrp-1-preview3");
        let protocol_manifest: ProtocolManifest =
            load_json_file(protocol_root.join("manifest.json"))
                .expect("protocol manifest should load");
        let capability_manifest: CapabilityManifest =
            load_json_file(protocol_root.join("example-capabilities.json"))
                .expect("example capability manifest should load");

        let loaded_manifests = protocol_manifest
            .case_manifests
            .iter()
            .map(|relative_path| {
                let case_manifest: CaseManifest = load_json_file(protocol_root.join(relative_path))
                    .unwrap_or_else(|error| {
                        panic!("case manifest {relative_path} should load: {error}")
                    });
                (PathBuf::from(relative_path), case_manifest)
            })
            .collect::<Vec<_>>();

        let plan = build_adapter_execution_plan_for_manifests(
            &protocol_manifest,
            loaded_manifests
                .iter()
                .map(|(path, manifest)| (manifest, path.as_path())),
            &capability_manifest,
            Path::new("example-capabilities.json"),
            AdapterArtifactContext {
                results_path: "artifacts/adapter-results.json".to_string(),
                evidence_dir: "artifacts/evidence".to_string(),
            },
        )
        .expect("adapter execution plan should build from repo fixtures");

        assert_eq!(plan.cases.len(), 20);
        assert!(
            plan.cases
                .iter()
                .any(|case| case.id == "l0.header.roundtrip.basic")
        );
        assert!(
            plan.cases
                .iter()
                .any(|case| case.id == "l1.session.open_close")
        );
        assert!(plan
            .cases
            .iter()
            .any(|case| case.id == "l2.result_push.basic.event_pump.single_terminal.validation"));
    }

    #[test]
    fn benchmark_plan_includes_optional_transport_slots_only_when_claimed() {
        let protocol_manifest = ProtocolManifest {
            schema: None,
            protocol_version: "nnrp-1-preview3".to_string(),
            suite_version: "0.1.0".to_string(),
            status: "draft".to_string(),
            case_manifests: vec![],
            vector_recipe_manifests: vec![],
            vector_manifests: vec![],
            report_schema: "report.schema.json".to_string(),
        };
        let capability_manifest = CapabilityManifest {
            schema: None,
            implementation_name: "sample".to_string(),
            protocol_version: "nnrp-1-preview3".to_string(),
            supports: vec!["transport.tcp".to_string()],
        };

        let plan = build_benchmark_execution_plan(
            &protocol_manifest,
            &capability_manifest,
            BenchmarkArtifactContext {
                results_path: "artifacts/benchmark-results.json".to_string(),
                evidence_dir: "artifacts/benchmark-evidence".to_string(),
            },
        );

        assert_eq!(plan.implementation_name, "sample");
        assert!(
            plan.scenarios
                .iter()
                .any(|scenario| scenario.id == "l4.transport.tcp.loopback.throughput")
        );
        assert!(
            !plan
                .scenarios
                .iter()
                .any(|scenario| scenario.id == "l4.metadata.session_open_ack.latency")
        );
        assert!(
            !plan
                .scenarios
                .iter()
                .any(|scenario| scenario.id == "l4.submit_result.inline_tensor.throughput")
        );
        assert!(
            !plan
                .scenarios
                .iter()
                .any(|scenario| scenario.id == "l4.transport.quic.loopback.throughput")
        );
    }

    fn sample_api_capabilities() -> ApiProfileCapabilityManifest {
        ApiProfileCapabilityManifest {
            schema: None,
            adapter: "vllm-nnrp-adapter".to_string(),
            profile: "openai-compatible".to_string(),
            schema_version: "openai-compatible/1".to_string(),
            compatibility_levels: vec![1],
            operations: vec![ApiProfileOperationCapability {
                name: "chat.completions.create".to_string(),
                streaming: true,
                non_streaming: true,
                tool_calls: false,
                cancellation: true,
            }],
            extensions: vec![ApiProfileExtensionCapability {
                name: "diagnostics".to_string(),
                critical: false,
                description: None,
            }],
        }
    }

    fn sample_api_recipe(id: &str, stream: bool) -> ApiProfileRecipe {
        ApiProfileRecipe {
            schema: None,
            id: id.to_string(),
            profile: "openai-compatible".to_string(),
            schema_version: "openai-compatible/1".to_string(),
            operation: "chat.completions.create".to_string(),
            required_capabilities: vec![],
            status: CaseStatus::Mandatory,
            parameters: BTreeMap::new(),
            request: ApiProfileRecipeRequest {
                body: serde_json::json!({
                    "model": "example-model",
                    "messages": [{"role": "user", "content": "Say hello."}],
                    "stream": stream
                }),
                nnrp: None,
            },
            expect: ApiProfileExpectation {
                events: vec![
                    ApiProfileExpectedEvent {
                        event_type: "response.output_text.delta".to_string(),
                        optional: false,
                        min_count: Some(1),
                        fields: None,
                    },
                    ApiProfileExpectedEvent {
                        event_type: "response.completed".to_string(),
                        optional: true,
                        min_count: None,
                        fields: None,
                    },
                ],
                terminal: ApiProfileTerminal::Success,
            },
        }
    }

    #[test]
    fn api_profile_plan_selects_recipes_claimed_by_capabilities() {
        let mut unsupported_tool_recipe = sample_api_recipe("tool-case", true);
        unsupported_tool_recipe.request.body["tools"] = serde_json::json!([
            {"type": "function", "function": {"name": "lookup"}}
        ]);

        let plan = build_api_profile_execution_plan(
            &sample_api_capabilities(),
            &[
                sample_api_recipe("streaming-case", true),
                sample_api_recipe("non-streaming-case", false),
                unsupported_tool_recipe,
            ],
            AdapterArtifactContext {
                results_path: "artifacts/api-profile-results.json".to_string(),
                evidence_dir: "artifacts/api-profile-evidence".to_string(),
            },
        )
        .expect("api profile plan should build");

        assert_eq!(plan.adapter, "vllm-nnrp-adapter");
        assert_eq!(plan.cases.len(), 2);
        assert!(plan.cases.iter().any(|case| {
            case.id == "streaming-case"
                && case
                    .required_capabilities
                    .contains(&"api.streaming".to_string())
        }));
        assert!(plan.cases.iter().any(|case| {
            case.id == "non-streaming-case"
                && case
                    .required_capabilities
                    .contains(&"api.non_streaming".to_string())
        }));
        assert_eq!(plan.coverage_matrix.len(), 1);
        assert_eq!(plan.coverage_matrix[0].summary.selected_cases, 2);
        assert_eq!(plan.coverage_matrix[0].summary.not_claimed_cases, 1);
    }

    #[test]
    fn api_profile_plan_substitutes_recipe_parameters() {
        let mut recipe = sample_api_recipe("parameterized-case", false);
        recipe
            .parameters
            .insert("MODEL_ID".to_string(), "backend-error".to_string());
        recipe.request.body["model"] = serde_json::json!("${MODEL_ID}");
        recipe.request.nnrp = Some(serde_json::json!({"trace": "${MODEL_ID}"}));

        let plan = build_api_profile_execution_plan(
            &sample_api_capabilities(),
            &[recipe],
            AdapterArtifactContext {
                results_path: "artifacts/api-profile-results.json".to_string(),
                evidence_dir: "artifacts/api-profile-evidence".to_string(),
            },
        )
        .expect("api profile plan should build");

        assert_eq!(plan.cases[0].request.body["model"], "backend-error");
        assert_eq!(
            plan.cases[0].request.nnrp.as_ref().unwrap()["trace"],
            "backend-error"
        );
    }

    #[test]
    fn api_profile_plan_builds_from_frozen_openai_recipe_catalog() {
        let profile_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("profiles")
            .join("openai-compatible")
            .join("1");
        let manifest: nnrp_conformance_fixtures::ApiProfileSuiteManifest =
            load_json_file(profile_root.join("manifest.json"))
                .expect("api profile manifest should load");
        let recipes = manifest
            .recipe_manifests
            .iter()
            .map(|recipe_path| {
                load_json_file::<ApiProfileRecipe>(profile_root.join(recipe_path))
                    .unwrap_or_else(|error| panic!("recipe {recipe_path} should load: {error}"))
            })
            .collect::<Vec<_>>();
        let mut capabilities = sample_api_capabilities();
        capabilities.operations[0].tool_calls = true;

        let plan = build_api_profile_execution_plan(
            &capabilities,
            &recipes,
            AdapterArtifactContext {
                results_path: "artifacts/api-profile-results.json".to_string(),
                evidence_dir: "artifacts/api-profile-evidence".to_string(),
            },
        )
        .expect("api profile plan should build from frozen catalog");

        assert_eq!(recipes.len(), 8);
        assert_eq!(plan.cases.len(), 8);
        assert!(
            plan.cases
                .iter()
                .any(|case| case.id == "openai-compatible.chat.backend-error"
                    && case.request.body["model"] == "backend-error")
        );
        assert!(
            plan.cases
                .iter()
                .any(|case| case.id == "openai-compatible.chat.unsupported-operation")
        );
        assert!(
            plan.coverage_matrix
                .iter()
                .all(|entry| entry.summary.not_claimed_cases == 0)
        );
    }

    #[test]
    fn api_profile_results_validate_event_order_and_terminal() {
        let plan = build_api_profile_execution_plan(
            &sample_api_capabilities(),
            &[sample_api_recipe("streaming-case", true)],
            AdapterArtifactContext {
                results_path: "artifacts/api-profile-results.json".to_string(),
                evidence_dir: "artifacts/api-profile-evidence".to_string(),
            },
        )
        .expect("api profile plan should build");

        let summary = validate_api_profile_results(
            &plan,
            &ApiProfileCaseResultReport {
                schema: None,
                profile: "openai-compatible".to_string(),
                schema_version: "openai-compatible/1".to_string(),
                adapter: "vllm-nnrp-adapter".to_string(),
                results: vec![ApiProfileCaseResult {
                    id: "streaming-case".to_string(),
                    outcome: ApiProfileCaseOutcome::Passed,
                    terminal: ApiProfileTerminal::Success,
                    events: vec![
                        ApiProfileObservedEvent {
                            event_type: "response.output_text.delta".to_string(),
                            fields: BTreeMap::new(),
                        },
                        ApiProfileObservedEvent {
                            event_type: "response.completed".to_string(),
                            fields: BTreeMap::new(),
                        },
                    ],
                    diagnostics: None,
                    message: None,
                }],
            },
        )
        .expect("api profile results should validate");

        assert_eq!(summary.selected_cases, 1);
        assert_eq!(summary.passed_cases, 1);
    }

    #[test]
    fn api_profile_results_reject_missing_required_event() {
        let plan = build_api_profile_execution_plan(
            &sample_api_capabilities(),
            &[sample_api_recipe("streaming-case", true)],
            AdapterArtifactContext {
                results_path: "artifacts/api-profile-results.json".to_string(),
                evidence_dir: "artifacts/api-profile-evidence".to_string(),
            },
        )
        .expect("api profile plan should build");

        let error = validate_api_profile_results(
            &plan,
            &ApiProfileCaseResultReport {
                schema: None,
                profile: "openai-compatible".to_string(),
                schema_version: "openai-compatible/1".to_string(),
                adapter: "vllm-nnrp-adapter".to_string(),
                results: vec![ApiProfileCaseResult {
                    id: "streaming-case".to_string(),
                    outcome: ApiProfileCaseOutcome::Passed,
                    terminal: ApiProfileTerminal::Success,
                    events: vec![],
                    diagnostics: None,
                    message: None,
                }],
            },
        )
        .expect_err("api profile results should reject missing required event");

        assert!(error.to_string().contains("response.output_text.delta"));
    }

    fn sample_wire_target() -> WireConformanceTargetManifest {
        WireConformanceTargetManifest {
            schema: None,
            target_name: "sample-target".to_string(),
            protocol_version: "nnrp-1-preview4".to_string(),
            suite_version: "0.1.0".to_string(),
            wire_conformance: WireConformanceTarget {
                modes: vec![WireConformanceMode::SuiteAsClient],
                transports: vec![WireConformanceTransportEndpoint {
                    name: WireConformanceTransport::Tcp,
                    endpoint: "127.0.0.1:44001".to_string(),
                    tls: false,
                    security: None,
                }],
                host_route_providers: vec![],
                capabilities: vec![
                    "control.cancel_abort".to_string(),
                    "control.result_drop_reason".to_string(),
                ],
                limits: WireConformanceLimits {
                    max_frame_bytes: 65536,
                    max_in_flight: 16,
                },
            },
        }
    }

    fn sample_wire_scenario(
        id: &str,
        mode: WireConformanceMode,
        transport: WireConformanceTransport,
        capabilities: Vec<&str>,
    ) -> WireConformanceScenario {
        WireConformanceScenario {
            id: id.to_string(),
            mode,
            transport: Some(transport),
            host_route: None,
            status: CaseStatus::Mandatory,
            feature: "wire.control".to_string(),
            required_capabilities: capabilities
                .into_iter()
                .map(std::string::ToString::to_string)
                .collect(),
            description: "Sample wire scenario".to_string(),
            steps: vec![WireConformanceStep {
                action: "send_frame".to_string(),
                frame: Some("CANCEL".to_string()),
                payload: None,
                timeout_ms: Some(100),
            }],
            expect: WireConformanceExpectation {
                terminal: WireConformanceTerminal::Cancelled,
                frames: vec!["CANCEL_ACK".to_string()],
                allowed_frames: vec!["CANCEL_ACK".to_string()],
                frame_payload_invariants: vec![],
                result_drop_reason_code: None,
                route: None,
            },
        }
    }

    fn observed_wire_frame(frame: &str, timestamp_us: u64) -> WireConformanceObservedFrame {
        WireConformanceObservedFrame {
            direction: WireConformanceFrameDirection::Received,
            frame: frame.to_string(),
            payload: None,
            timestamp_us: Some(timestamp_us),
        }
    }

    fn observed_wire_frame_with_payload(
        frame: &str,
        timestamp_us: u64,
        payload: serde_json::Value,
    ) -> WireConformanceObservedFrame {
        WireConformanceObservedFrame {
            direction: WireConformanceFrameDirection::Received,
            frame: frame.to_string(),
            payload: Some(payload),
            timestamp_us: Some(timestamp_us),
        }
    }

    fn sample_host_route_target() -> WireConformanceTargetManifest {
        let mut target = sample_wire_target();
        target
            .wire_conformance
            .modes
            .push(WireConformanceMode::SuiteAsServer);
        target
            .wire_conformance
            .capabilities
            .push("host.routes".to_string());
        target.wire_conformance.host_route_providers = vec![
            WireHostRouteProviderCapability {
                transport: WireConformanceTransport::Tcp,
                provider_id: "nnrp.transport.tcp.native".to_string(),
                installed: true,
                platforms: vec![WireHostPlatform::Native],
                security_modes: vec![WireHostRouteSecurityMode::Plain],
            },
            WireHostRouteProviderCapability {
                transport: WireConformanceTransport::Ipc,
                provider_id: "nnrp.transport.ipc.native".to_string(),
                installed: true,
                platforms: vec![WireHostPlatform::Native],
                security_modes: vec![WireHostRouteSecurityMode::Plain],
            },
        ];
        target
    }

    fn sample_host_route_scenario() -> WireConformanceScenario {
        WireConformanceScenario {
            id: "wire.host-route.client.multi-route".to_string(),
            mode: WireConformanceMode::SuiteAsServer,
            transport: None,
            host_route: Some(WireHostRouteFixture {
                role: WireHostRole::Client,
                platform: WireHostPlatform::Native,
                application_endpoint: "nnrp://host-route.test".to_string(),
                routes: vec![
                    WireHostProviderRoute {
                        transport: WireConformanceTransport::Tcp,
                        provider_id: "nnrp.transport.tcp.native".to_string(),
                        locator: "suite://allocate/tcp/client-primary".to_string(),
                        security: WireHostRouteSecurity {
                            mode: WireHostRouteSecurityMode::Plain,
                            credential_owner: WireHostCredentialOwner::None,
                        },
                        injected_failures: vec![],
                    },
                    WireHostProviderRoute {
                        transport: WireConformanceTransport::Ipc,
                        provider_id: "nnrp.transport.ipc.native".to_string(),
                        locator: "suite://allocate/ipc/client-secondary".to_string(),
                        security: WireHostRouteSecurity {
                            mode: WireHostRouteSecurityMode::Plain,
                            credential_owner: WireHostCredentialOwner::None,
                        },
                        injected_failures: vec![],
                    },
                ],
            }),
            status: CaseStatus::Mandatory,
            feature: "host.routes".to_string(),
            required_capabilities: vec!["host.routes".to_string()],
            description: "Select one runtime carrier from two live routes.".to_string(),
            steps: vec![WireConformanceStep {
                action: "connect_routes".to_string(),
                frame: None,
                payload: None,
                timeout_ms: Some(1000),
            }],
            expect: WireConformanceExpectation {
                terminal: WireConformanceTerminal::Success,
                frames: vec![],
                allowed_frames: vec![],
                frame_payload_invariants: vec![],
                result_drop_reason_code: None,
                route: Some(WireHostRouteExpectation {
                    selected_count: Some(1),
                    selected_transport: None,
                    rejection_reasons: vec![],
                    bound_transports: vec![],
                    accepted_transports: vec![],
                    atomic_rollback: Some(false),
                    logical_set_closed: Some(false),
                    terminal_failure: None,
                }),
            },
        }
    }

    fn sample_host_route_evidence(selected_count: usize) -> WireHostRouteEvidence {
        WireHostRouteEvidence {
            application_endpoint: "nnrp://host-route.test".to_string(),
            candidates: [
                (WireConformanceTransport::Tcp, "nnrp.transport.tcp.native"),
                (WireConformanceTransport::Ipc, "nnrp.transport.ipc.native"),
            ]
            .into_iter()
            .enumerate()
            .map(
                |(index, (transport, provider_id))| WireHostRouteCandidateEvidence {
                    transport,
                    provider_id: provider_id.to_string(),
                    requested_locator: match transport {
                        WireConformanceTransport::Tcp => {
                            "suite://allocate/tcp/client-primary".to_string()
                        }
                        WireConformanceTransport::Ipc => {
                            "suite://allocate/ipc/client-secondary".to_string()
                        }
                        _ => unreachable!("sample fixture only uses TCP and IPC"),
                    },
                    locator_resolved: true,
                    security_satisfied: true,
                    selected: index < selected_count,
                    rejection_reason: None,
                },
            )
            .collect(),
            listeners: vec![],
            accepted_sessions: vec![WireHostAcceptedSessionEvidence {
                transport: WireConformanceTransport::Tcp,
                provider_id: "nnrp.transport.tcp.native".to_string(),
                active_transport: WireConformanceTransport::Tcp,
            }],
            atomic_rollback: false,
            logical_set_closed: false,
            terminal_failure: None,
        }
    }

    #[test]
    fn wire_plan_and_results_preserve_suite_owned_host_route_evidence() {
        let plan = build_wire_conformance_execution_plan(
            &sample_host_route_target(),
            &[sample_host_route_scenario()],
            AdapterArtifactContext {
                results_path: "artifacts/wire-results.json".to_string(),
                evidence_dir: "artifacts/wire-evidence".to_string(),
            },
        )
        .expect("host-route plan should build");
        assert_eq!(plan.scenarios.len(), 1);
        assert!(plan.scenarios[0].host_route.is_some());
        assert_eq!(plan.host_route_providers.len(), 2);
        let mut evidence = sample_host_route_evidence(1);
        evidence.candidates.reverse();

        let report = WireConformanceCaseResultReport {
            schema: None,
            protocol_version: "nnrp-1-preview4".to_string(),
            suite_version: "0.1.0".to_string(),
            target_name: "sample-target".to_string(),
            results: vec![WireConformanceCaseResult {
                id: "wire.host-route.client.multi-route".to_string(),
                outcome: ApiProfileCaseOutcome::Passed,
                terminal: WireConformanceTerminal::Success,
                observed_frames: vec![],
                route_evidence: Some(evidence),
                message: None,
                evidence_paths: vec![],
            }],
        };
        validate_wire_conformance_results(&plan, &report)
            .expect("candidate array order should not affect route identity validation");
    }

    #[test]
    fn wire_plan_and_results_enforce_uninstalled_provider_semantics() {
        let mut target = sample_host_route_target();
        target.wire_conformance.host_route_providers[0].installed = false;
        let mut scenario = sample_host_route_scenario();
        scenario
            .expect
            .route
            .as_mut()
            .expect("sample host route should have route expectations")
            .rejection_reasons = vec![WireHostRouteRejectionReason::LocalUnavailable];
        let plan = build_wire_conformance_execution_plan(
            &target,
            &[scenario],
            AdapterArtifactContext {
                results_path: "artifacts/wire-results.json".to_string(),
                evidence_dir: "artifacts/wire-evidence".to_string(),
            },
        )
        .expect("known uninstalled providers should remain in the execution plan");
        assert!(!plan.host_route_providers[0].installed);

        let mut evidence = sample_host_route_evidence(0);
        evidence.candidates[0].rejection_reason =
            Some(WireHostRouteRejectionReason::LocalUnavailable);
        evidence.candidates[1].selected = true;
        evidence.accepted_sessions[0] = WireHostAcceptedSessionEvidence {
            transport: WireConformanceTransport::Ipc,
            provider_id: "nnrp.transport.ipc.native".to_string(),
            active_transport: WireConformanceTransport::Ipc,
        };
        let report = WireConformanceCaseResultReport {
            schema: None,
            protocol_version: "nnrp-1-preview4".to_string(),
            suite_version: "0.1.0".to_string(),
            target_name: "sample-target".to_string(),
            results: vec![WireConformanceCaseResult {
                id: "wire.host-route.client.multi-route".to_string(),
                outcome: ApiProfileCaseOutcome::Passed,
                terminal: WireConformanceTerminal::Success,
                observed_frames: vec![],
                route_evidence: Some(evidence),
                message: None,
                evidence_paths: vec![],
            }],
        };
        validate_wire_conformance_results(&plan, &report)
            .expect("uninstalled provider must be visible but rejected without invocation");
    }

    #[test]
    fn wire_plan_rejects_uninstalled_provider_without_local_unavailable_expectation() {
        let mut target = sample_host_route_target();
        target.wire_conformance.host_route_providers[0].installed = false;
        let error = build_wire_conformance_execution_plan(
            &target,
            &[sample_host_route_scenario()],
            AdapterArtifactContext {
                results_path: "artifacts/wire-results.json".to_string(),
                evidence_dir: "artifacts/wire-evidence".to_string(),
            },
        )
        .expect_err("uninstalled providers require an explicit local-unavailable oracle");
        assert!(error.to_string().contains("local-unavailable"));
    }

    #[test]
    fn wire_plan_rejects_host_route_capability_without_providers() {
        let mut target = sample_wire_target();
        target
            .wire_conformance
            .capabilities
            .push("host.routes".to_string());
        let error = build_wire_conformance_execution_plan(
            &target,
            &[sample_host_route_scenario()],
            AdapterArtifactContext {
                results_path: "artifacts/wire-results.json".to_string(),
                evidence_dir: "artifacts/wire-evidence".to_string(),
            },
        )
        .expect_err("host.routes without provider declarations must fail");
        assert!(error.to_string().contains("without declaring"));
    }

    #[test]
    fn wire_plan_rejects_host_route_providers_without_capability() {
        let mut target = sample_host_route_target();
        target
            .wire_conformance
            .capabilities
            .retain(|capability| capability != "host.routes");
        let error = build_wire_conformance_execution_plan(
            &target,
            &[sample_host_route_scenario()],
            AdapterArtifactContext {
                results_path: "artifacts/wire-results.json".to_string(),
                evidence_dir: "artifacts/wire-evidence".to_string(),
            },
        )
        .expect_err("provider declarations without host.routes must fail");
        assert!(error.to_string().contains("without claiming"));
    }

    #[test]
    fn wire_plan_rejects_multiple_host_providers_for_one_transport() {
        let mut target = sample_host_route_target();
        let mut duplicate_transport = target.wire_conformance.host_route_providers[0].clone();
        duplicate_transport.provider_id = "example.transport.tcp.alternate".to_string();
        target
            .wire_conformance
            .host_route_providers
            .push(duplicate_transport);
        let error = build_wire_conformance_execution_plan(
            &target,
            &[sample_host_route_scenario()],
            AdapterArtifactContext {
                results_path: "artifacts/wire-results.json".to_string(),
                evidence_dir: "artifacts/wire-evidence".to_string(),
            },
        )
        .expect_err("one host role must not register two providers for one transport");
        assert!(
            error
                .to_string()
                .contains("more than one host-route provider")
        );
    }

    #[test]
    fn wire_plan_rejects_reused_host_provider_id() {
        let mut target = sample_host_route_target();
        let mut duplicate_id = target.wire_conformance.host_route_providers[0].clone();
        duplicate_id.transport = WireConformanceTransport::Websocket;
        target
            .wire_conformance
            .host_route_providers
            .push(duplicate_id);
        let error = build_wire_conformance_execution_plan(
            &target,
            &[sample_host_route_scenario()],
            AdapterArtifactContext {
                results_path: "artifacts/wire-results.json".to_string(),
                evidence_dir: "artifacts/wire-evidence".to_string(),
            },
        )
        .expect_err("provider IDs must remain unique across transports");
        assert!(error.to_string().contains("repeats host-route provider id"));
    }

    #[test]
    fn wire_plan_rejects_multiple_fixture_routes_for_one_transport() {
        let target = sample_host_route_target();
        let mut scenario = sample_host_route_scenario();
        let duplicate = scenario
            .host_route
            .as_ref()
            .expect("sample host route should exist")
            .routes[0]
            .clone();
        scenario
            .host_route
            .as_mut()
            .expect("sample host route should exist")
            .routes
            .push(duplicate);
        let error = build_wire_conformance_execution_plan(
            &target,
            &[scenario],
            AdapterArtifactContext {
                results_path: "artifacts/wire-results.json".to_string(),
                evidence_dir: "artifacts/wire-evidence".to_string(),
            },
        )
        .expect_err("one host route set must not repeat a transport");
        assert!(error.to_string().contains("more than one route"));
    }

    #[test]
    fn wire_plan_rejects_host_route_role_mode_mismatch() {
        let target = sample_host_route_target();
        let mut scenario = sample_host_route_scenario();
        scenario.mode = WireConformanceMode::SuiteAsClient;
        let error = build_wire_conformance_execution_plan(
            &target,
            &[scenario],
            AdapterArtifactContext {
                results_path: "artifacts/wire-results.json".to_string(),
                evidence_dir: "artifacts/wire-evidence".to_string(),
            },
        )
        .expect_err("host-route role and suite mode must agree");
        assert!(error.to_string().contains("host route"));
    }

    #[test]
    fn wire_plan_rejects_host_route_without_route_expectations() {
        let target = sample_host_route_target();
        let mut scenario = sample_host_route_scenario();
        scenario.expect.route = None;
        let error = build_wire_conformance_execution_plan(
            &target,
            &[scenario],
            AdapterArtifactContext {
                results_path: "artifacts/wire-results.json".to_string(),
                evidence_dir: "artifacts/wire-evidence".to_string(),
            },
        )
        .expect_err("host-route scenarios require route expectations");
        assert!(error.to_string().contains("exactly when"));
    }

    #[test]
    fn wire_plan_rejects_transport_scenario_with_route_expectations() {
        let mut scenario = sample_wire_scenario(
            "wire.control.invalid-route-oracle",
            WireConformanceMode::SuiteAsClient,
            WireConformanceTransport::Tcp,
            vec!["control.cancel_abort"],
        );
        scenario.expect.route = Some(WireHostRouteExpectation {
            selected_count: Some(0),
            selected_transport: None,
            rejection_reasons: vec![],
            bound_transports: vec![],
            accepted_transports: vec![],
            atomic_rollback: None,
            logical_set_closed: None,
            terminal_failure: None,
        });
        let error = build_wire_conformance_execution_plan(
            &sample_wire_target(),
            &[scenario],
            AdapterArtifactContext {
                results_path: "artifacts/wire-results.json".to_string(),
                evidence_dir: "artifacts/wire-evidence".to_string(),
            },
        )
        .expect_err("frame-only transport scenarios must not declare route expectations");
        assert!(error.to_string().contains("exactly when"));
    }

    #[test]
    fn wire_plan_rejects_invalid_result_drop_reason_expectations() {
        let target = sample_wire_target();
        let artifacts = AdapterArtifactContext {
            results_path: "artifacts/wire-results.json".to_string(),
            evidence_dir: "artifacts/wire-evidence".to_string(),
        };
        let mut scenario = sample_wire_scenario(
            "wire.control.invalid-drop-reason",
            WireConformanceMode::SuiteAsClient,
            WireConformanceTransport::Tcp,
            vec!["control.cancel_abort"],
        );
        scenario.expect.result_drop_reason_code = Some(0);
        let error =
            build_wire_conformance_execution_plan(&target, &[scenario.clone()], artifacts.clone())
                .expect_err("zero is not a registered result drop reason");
        assert!(error.to_string().contains("must be non-zero"));

        scenario.expect.result_drop_reason_code = Some(3);
        let error = build_wire_conformance_execution_plan(&target, &[scenario], artifacts)
            .expect_err("a typed drop reason requires the corresponding frame");
        assert!(error.to_string().contains("without RESULT_DROP_REASON"));
    }

    #[test]
    fn wire_plan_rejects_result_drop_reason_on_host_route_scenario() {
        let target = sample_host_route_target();
        let mut scenario = sample_host_route_scenario();
        scenario.expect.result_drop_reason_code = Some(3);
        let error = build_wire_conformance_execution_plan(
            &target,
            &[scenario],
            AdapterArtifactContext {
                results_path: "artifacts/wire-results.json".to_string(),
                evidence_dir: "artifacts/wire-evidence".to_string(),
            },
        )
        .expect_err("host-route evidence must not declare frame-level drop reasons");
        assert!(
            error
                .to_string()
                .contains("must not declare result_drop_reason_code")
        );
    }

    #[test]
    fn wire_plan_validates_frame_payload_invariant_contract() {
        let target = sample_wire_target();
        let artifacts = AdapterArtifactContext {
            results_path: "artifacts/wire-results.json".to_string(),
            evidence_dir: "artifacts/wire-evidence".to_string(),
        };
        let invariant = WireConformanceFramePayloadInvariant {
            frame: "TRACE_CONTEXT".to_string(),
            direction: Some(WireConformanceFrameDirection::Received),
            fields: BTreeMap::from([("frame_id".to_string(), serde_json::json!(1))]),
        };
        let mut scenario = sample_wire_scenario(
            "wire.control.trace-correlation",
            WireConformanceMode::SuiteAsClient,
            WireConformanceTransport::Tcp,
            vec!["control.cancel_abort"],
        );
        scenario
            .expect
            .allowed_frames
            .push("TRACE_CONTEXT".to_string());
        scenario.expect.frame_payload_invariants = vec![invariant.clone()];
        build_wire_conformance_execution_plan(&target, &[scenario.clone()], artifacts.clone())
            .expect("a payload invariant over an allowed frame should validate");

        scenario.expect.frame_payload_invariants[0].fields.clear();
        let error =
            build_wire_conformance_execution_plan(&target, &[scenario.clone()], artifacts.clone())
                .expect_err("an invariant without fields must be rejected");
        assert!(error.to_string().contains("at least one field"));

        scenario.expect.frame_payload_invariants = vec![invariant.clone(), invariant.clone()];
        let error =
            build_wire_conformance_execution_plan(&target, &[scenario.clone()], artifacts.clone())
                .expect_err("duplicate frame and direction invariants must be rejected");
        assert!(error.to_string().contains("duplicate payload invariants"));

        scenario.expect.frame_payload_invariants = vec![WireConformanceFramePayloadInvariant {
            frame: "PARTIAL_RESULT".to_string(),
            ..invariant
        }];
        let error = build_wire_conformance_execution_plan(&target, &[scenario], artifacts)
            .expect_err("an invariant outside allowed_frames must be rejected");
        assert!(error.to_string().contains("outside allowed_frames"));
    }

    #[test]
    fn wire_plan_rejects_frame_payload_invariants_on_host_route_scenario() {
        let target = sample_host_route_target();
        let mut scenario = sample_host_route_scenario();
        scenario.expect.frame_payload_invariants = vec![WireConformanceFramePayloadInvariant {
            frame: "TRACE_CONTEXT".to_string(),
            direction: Some(WireConformanceFrameDirection::Received),
            fields: BTreeMap::from([("frame_id".to_string(), serde_json::json!(1))]),
        }];
        let error = build_wire_conformance_execution_plan(
            &target,
            &[scenario],
            AdapterArtifactContext {
                results_path: "artifacts/wire-results.json".to_string(),
                evidence_dir: "artifacts/wire-evidence".to_string(),
            },
        )
        .expect_err("host-route evidence must not declare frame payload invariants");
        assert!(
            error
                .to_string()
                .contains("must not declare frame_payload_invariants")
        );
    }

    #[test]
    fn wire_plan_rejects_transport_security_mode_mismatch() {
        let mut target = sample_host_route_target();
        target.wire_conformance.host_route_providers[0]
            .security_modes
            .push(WireHostRouteSecurityMode::Wss);
        let mut scenario = sample_host_route_scenario();
        let tcp_route = &mut scenario
            .host_route
            .as_mut()
            .expect("sample host route should exist")
            .routes[0];
        tcp_route.security = WireHostRouteSecurity {
            mode: WireHostRouteSecurityMode::Wss,
            credential_owner: WireHostCredentialOwner::Target,
        };
        let error = build_wire_conformance_execution_plan(
            &target,
            &[scenario],
            AdapterArtifactContext {
                results_path: "artifacts/wire-results.json".to_string(),
                evidence_dir: "artifacts/wire-evidence".to_string(),
            },
        )
        .expect_err("TCP must not accept WebSocket security modes");
        assert!(
            error
                .to_string()
                .contains("incompatible Wss security on Tcp")
        );
    }

    #[test]
    fn wire_results_reject_multiple_active_client_carriers() {
        let plan = build_wire_conformance_execution_plan(
            &sample_host_route_target(),
            &[sample_host_route_scenario()],
            AdapterArtifactContext {
                results_path: "artifacts/wire-results.json".to_string(),
                evidence_dir: "artifacts/wire-evidence".to_string(),
            },
        )
        .expect("host-route plan should build");
        let report = WireConformanceCaseResultReport {
            schema: None,
            protocol_version: "nnrp-1-preview4".to_string(),
            suite_version: "0.1.0".to_string(),
            target_name: "sample-target".to_string(),
            results: vec![WireConformanceCaseResult {
                id: "wire.host-route.client.multi-route".to_string(),
                outcome: ApiProfileCaseOutcome::Passed,
                terminal: WireConformanceTerminal::Success,
                observed_frames: vec![],
                route_evidence: Some(sample_host_route_evidence(2)),
                message: None,
                evidence_paths: vec![],
            }],
        };
        let error = validate_wire_conformance_results(&plan, &report)
            .expect_err("two selected carriers must fail one-carrier semantics");
        assert!(
            error
                .to_string()
                .contains("selected 2 client carriers; at most one is permitted")
        );
    }

    #[test]
    fn wire_results_reject_candidate_identity_drift() {
        let plan = build_wire_conformance_execution_plan(
            &sample_host_route_target(),
            &[sample_host_route_scenario()],
            AdapterArtifactContext {
                results_path: "artifacts/wire-results.json".to_string(),
                evidence_dir: "artifacts/wire-evidence".to_string(),
            },
        )
        .expect("host-route plan should build");
        let mut evidence = sample_host_route_evidence(1);
        evidence.candidates[0].requested_locator =
            "suite://allocate/tcp/target-substituted".to_string();
        let report = WireConformanceCaseResultReport {
            schema: None,
            protocol_version: "nnrp-1-preview4".to_string(),
            suite_version: "0.1.0".to_string(),
            target_name: "sample-target".to_string(),
            results: vec![WireConformanceCaseResult {
                id: "wire.host-route.client.multi-route".to_string(),
                outcome: ApiProfileCaseOutcome::Passed,
                terminal: WireConformanceTerminal::Success,
                observed_frames: vec![],
                route_evidence: Some(evidence),
                message: None,
                evidence_paths: vec![],
            }],
        };
        let error = validate_wire_conformance_results(&plan, &report)
            .expect_err("target-substituted route evidence must fail");
        assert!(error.to_string().contains("suite-owned route"));
    }

    #[test]
    fn wire_results_reject_selected_ineligible_route() {
        let plan = build_wire_conformance_execution_plan(
            &sample_host_route_target(),
            &[sample_host_route_scenario()],
            AdapterArtifactContext {
                results_path: "artifacts/wire-results.json".to_string(),
                evidence_dir: "artifacts/wire-evidence".to_string(),
            },
        )
        .expect("host-route plan should build");
        let mut evidence = sample_host_route_evidence(1);
        evidence.candidates[0].security_satisfied = false;
        let report = WireConformanceCaseResultReport {
            schema: None,
            protocol_version: "nnrp-1-preview4".to_string(),
            suite_version: "0.1.0".to_string(),
            target_name: "sample-target".to_string(),
            results: vec![WireConformanceCaseResult {
                id: "wire.host-route.client.multi-route".to_string(),
                outcome: ApiProfileCaseOutcome::Passed,
                terminal: WireConformanceTerminal::Success,
                observed_frames: vec![],
                route_evidence: Some(evidence),
                message: None,
                evidence_paths: vec![],
            }],
        };
        let error = validate_wire_conformance_results(&plan, &report)
            .expect_err("selected security-incompatible route must fail");
        assert!(error.to_string().contains("security-incompatible"));
    }

    #[test]
    fn wire_results_reject_selected_route_with_rejection_reason() {
        let plan = build_wire_conformance_execution_plan(
            &sample_host_route_target(),
            &[sample_host_route_scenario()],
            AdapterArtifactContext {
                results_path: "artifacts/wire-results.json".to_string(),
                evidence_dir: "artifacts/wire-evidence".to_string(),
            },
        )
        .expect("host-route plan should build");
        let mut evidence = sample_host_route_evidence(1);
        evidence.candidates[0].rejection_reason =
            Some(WireHostRouteRejectionReason::RouteUnresolved);
        let report = WireConformanceCaseResultReport {
            schema: None,
            protocol_version: "nnrp-1-preview4".to_string(),
            suite_version: "0.1.0".to_string(),
            target_name: "sample-target".to_string(),
            results: vec![WireConformanceCaseResult {
                id: "wire.host-route.client.multi-route".to_string(),
                outcome: ApiProfileCaseOutcome::Passed,
                terminal: WireConformanceTerminal::Success,
                observed_frames: vec![],
                route_evidence: Some(evidence),
                message: None,
                evidence_paths: vec![],
            }],
        };
        let error = validate_wire_conformance_results(&plan, &report)
            .expect_err("selected candidate cannot simultaneously be rejected");
        assert!(error.to_string().contains("marks selected"));
    }

    #[test]
    fn wire_results_reject_client_session_on_unselected_carrier() {
        let plan = build_wire_conformance_execution_plan(
            &sample_host_route_target(),
            &[sample_host_route_scenario()],
            AdapterArtifactContext {
                results_path: "artifacts/wire-results.json".to_string(),
                evidence_dir: "artifacts/wire-evidence".to_string(),
            },
        )
        .expect("host-route plan should build");
        let mut evidence = sample_host_route_evidence(1);
        evidence.accepted_sessions[0] = WireHostAcceptedSessionEvidence {
            transport: WireConformanceTransport::Ipc,
            provider_id: "nnrp.transport.ipc.native".to_string(),
            active_transport: WireConformanceTransport::Ipc,
        };
        let report = WireConformanceCaseResultReport {
            schema: None,
            protocol_version: "nnrp-1-preview4".to_string(),
            suite_version: "0.1.0".to_string(),
            target_name: "sample-target".to_string(),
            results: vec![WireConformanceCaseResult {
                id: "wire.host-route.client.multi-route".to_string(),
                outcome: ApiProfileCaseOutcome::Passed,
                terminal: WireConformanceTerminal::Success,
                observed_frames: vec![],
                route_evidence: Some(evidence),
                message: None,
                evidence_paths: vec![],
            }],
        };
        let error = validate_wire_conformance_results(&plan, &report)
            .expect_err("client session must use the selected carrier");
        assert!(error.to_string().contains("does not match"));
    }

    #[test]
    fn wire_results_reject_selected_client_carrier_without_session() {
        let plan = build_wire_conformance_execution_plan(
            &sample_host_route_target(),
            &[sample_host_route_scenario()],
            AdapterArtifactContext {
                results_path: "artifacts/wire-results.json".to_string(),
                evidence_dir: "artifacts/wire-evidence".to_string(),
            },
        )
        .expect("host-route plan should build");
        let mut evidence = sample_host_route_evidence(1);
        evidence.accepted_sessions.clear();
        let report = WireConformanceCaseResultReport {
            schema: None,
            protocol_version: "nnrp-1-preview4".to_string(),
            suite_version: "0.1.0".to_string(),
            target_name: "sample-target".to_string(),
            results: vec![WireConformanceCaseResult {
                id: "wire.host-route.client.multi-route".to_string(),
                outcome: ApiProfileCaseOutcome::Passed,
                terminal: WireConformanceTerminal::Success,
                observed_frames: vec![],
                route_evidence: Some(evidence),
                message: None,
                evidence_paths: vec![],
            }],
        };
        let error = validate_wire_conformance_results(&plan, &report)
            .expect_err("selected client carrier requires session evidence");
        assert!(error.to_string().contains("does not match"));
    }

    #[test]
    fn wire_plan_selects_scenarios_claimed_by_target_manifest() {
        let scenarios = vec![
            sample_wire_scenario(
                "selected",
                WireConformanceMode::SuiteAsClient,
                WireConformanceTransport::Tcp,
                vec!["control.cancel_abort"],
            ),
            sample_wire_scenario(
                "missing-capability",
                WireConformanceMode::SuiteAsClient,
                WireConformanceTransport::Tcp,
                vec!["control.priority_update"],
            ),
            sample_wire_scenario(
                "missing-transport",
                WireConformanceMode::SuiteAsClient,
                WireConformanceTransport::Quic,
                vec!["control.cancel_abort"],
            ),
            sample_wire_scenario(
                "missing-mode",
                WireConformanceMode::SuiteAsServer,
                WireConformanceTransport::Tcp,
                vec!["control.cancel_abort"],
            ),
        ];

        let plan = build_wire_conformance_execution_plan(
            &sample_wire_target(),
            &scenarios,
            AdapterArtifactContext {
                results_path: "artifacts/wire-results.json".to_string(),
                evidence_dir: "artifacts/wire-evidence".to_string(),
            },
        )
        .expect("wire execution plan should build");

        assert_eq!(plan.target_name, "sample-target");
        assert_eq!(plan.scenarios.len(), 1);
        assert_eq!(plan.scenarios[0].id, "selected");
    }

    #[test]
    fn wire_plan_rejects_transport_security_mismatches() {
        let scenarios = [sample_wire_scenario(
            "wire.control.priority-deadline.proxy",
            WireConformanceMode::SuiteAsClient,
            WireConformanceTransport::Quic,
            vec!["control.cancel_abort"],
        )];
        let artifacts = AdapterArtifactContext {
            results_path: "artifacts/wire-results.json".to_string(),
            evidence_dir: "artifacts/wire-evidence".to_string(),
        };

        let mut target = sample_wire_target();
        target.wire_conformance.transports[0] = WireConformanceTransportEndpoint {
            name: WireConformanceTransport::Quic,
            endpoint: "127.0.0.1:44002".to_string(),
            tls: false,
            security: None,
        };
        let error = build_wire_conformance_execution_plan(&target, &scenarios, artifacts.clone())
            .expect_err("QUIC without TLS material must be rejected");
        assert!(error.to_string().contains("TLS flag and security material"));

        target.wire_conformance.transports[0] = WireConformanceTransportEndpoint {
            name: WireConformanceTransport::Websocket,
            endpoint: "ws://127.0.0.1:44003/nnrp".to_string(),
            tls: true,
            security: Some(WireConformanceTransportSecurity {
                server_name: "localhost".to_string(),
                trusted_certificate_der_path: "certs/server.der".to_string(),
                certificate_der_path: "certs/server.der".to_string(),
                private_key_pkcs8_der_path: "certs/server-key.der".to_string(),
            }),
        };
        let error = build_wire_conformance_execution_plan(&target, &scenarios, artifacts)
            .expect_err("plain ws endpoint with TLS material must be rejected");
        assert!(error.to_string().contains("TLS flag and security material"));
    }

    #[test]
    fn wire_plan_accepts_plain_and_tls_tcp_endpoints() {
        let scenarios = [sample_wire_scenario(
            "wire.control.cancel-abort.client",
            WireConformanceMode::SuiteAsClient,
            WireConformanceTransport::Tcp,
            vec!["control.cancel_abort"],
        )];
        let artifacts = AdapterArtifactContext {
            results_path: "artifacts/wire-results.json".to_string(),
            evidence_dir: "artifacts/wire-evidence".to_string(),
        };

        build_wire_conformance_execution_plan(&sample_wire_target(), &scenarios, artifacts.clone())
            .expect("plain TCP endpoint should remain valid");

        let mut secure_target = sample_wire_target();
        secure_target.wire_conformance.transports[0].tls = true;
        secure_target.wire_conformance.transports[0].security =
            Some(WireConformanceTransportSecurity {
                server_name: "localhost".to_string(),
                trusted_certificate_der_path: "certs/server.der".to_string(),
                certificate_der_path: "certs/server.der".to_string(),
                private_key_pkcs8_der_path: "certs/server-key.der".to_string(),
            });
        build_wire_conformance_execution_plan(&secure_target, &scenarios, artifacts)
            .expect("TCP TLS endpoint should be valid with route-local security material");
    }

    #[test]
    fn wire_results_validate_when_report_matches_selected_scenarios() {
        let plan = build_wire_conformance_execution_plan(
            &sample_wire_target(),
            &[sample_wire_scenario(
                "selected",
                WireConformanceMode::SuiteAsClient,
                WireConformanceTransport::Tcp,
                vec!["control.cancel_abort"],
            )],
            AdapterArtifactContext {
                results_path: "artifacts/wire-results.json".to_string(),
                evidence_dir: "artifacts/wire-evidence".to_string(),
            },
        )
        .expect("wire execution plan should build");

        let summary = validate_wire_conformance_results(
            &plan,
            &WireConformanceCaseResultReport {
                schema: None,
                protocol_version: "nnrp-1-preview4".to_string(),
                suite_version: "0.1.0".to_string(),
                target_name: "sample-target".to_string(),
                results: vec![WireConformanceCaseResult {
                    id: "selected".to_string(),
                    outcome: ApiProfileCaseOutcome::Passed,
                    terminal: WireConformanceTerminal::Cancelled,
                    observed_frames: vec![WireConformanceObservedFrame {
                        direction: WireConformanceFrameDirection::Received,
                        frame: "CANCEL_ACK".to_string(),
                        payload: None,
                        timestamp_us: Some(100),
                    }],
                    route_evidence: None,
                    message: None,
                    evidence_paths: vec![],
                }],
            },
        )
        .expect("wire results should validate");

        assert_eq!(summary.selected_scenarios, 1);
        assert_eq!(summary.passed_scenarios, 1);
    }

    #[test]
    fn wire_results_reject_missing_expected_frame() {
        let plan = build_wire_conformance_execution_plan(
            &sample_wire_target(),
            &[sample_wire_scenario(
                "selected",
                WireConformanceMode::SuiteAsClient,
                WireConformanceTransport::Tcp,
                vec!["control.cancel_abort"],
            )],
            AdapterArtifactContext {
                results_path: "artifacts/wire-results.json".to_string(),
                evidence_dir: "artifacts/wire-evidence".to_string(),
            },
        )
        .expect("wire execution plan should build");

        let error = validate_wire_conformance_results(
            &plan,
            &WireConformanceCaseResultReport {
                schema: None,
                protocol_version: "nnrp-1-preview4".to_string(),
                suite_version: "0.1.0".to_string(),
                target_name: "sample-target".to_string(),
                results: vec![WireConformanceCaseResult {
                    id: "selected".to_string(),
                    outcome: ApiProfileCaseOutcome::Passed,
                    terminal: WireConformanceTerminal::Cancelled,
                    observed_frames: vec![],
                    route_evidence: None,
                    message: None,
                    evidence_paths: vec![],
                }],
            },
        )
        .expect_err("wire results should reject missing expected frame");

        assert!(error.to_string().contains("CANCEL_ACK"));
    }

    #[test]
    fn wire_results_require_the_declared_result_drop_reason_code() {
        let mut scenario = sample_wire_scenario(
            "selected",
            WireConformanceMode::SuiteAsClient,
            WireConformanceTransport::Tcp,
            vec!["control.cancel_abort"],
        );
        scenario.expect.frames = vec!["RESULT_DROP_REASON".to_string()];
        scenario.expect.allowed_frames = scenario.expect.frames.clone();
        scenario.expect.result_drop_reason_code = Some(3);
        let plan = build_wire_conformance_execution_plan(
            &sample_wire_target(),
            &[scenario],
            AdapterArtifactContext {
                results_path: "artifacts/wire-results.json".to_string(),
                evidence_dir: "artifacts/wire-evidence".to_string(),
            },
        )
        .expect("wire execution plan should build");
        let report = |drop_reason_code| WireConformanceCaseResultReport {
            schema: None,
            protocol_version: "nnrp-1-preview4".to_string(),
            suite_version: "0.1.0".to_string(),
            target_name: "sample-target".to_string(),
            results: vec![WireConformanceCaseResult {
                id: "selected".to_string(),
                outcome: ApiProfileCaseOutcome::Passed,
                terminal: WireConformanceTerminal::Cancelled,
                observed_frames: vec![WireConformanceObservedFrame {
                    direction: WireConformanceFrameDirection::Received,
                    frame: "RESULT_DROP_REASON".to_string(),
                    payload: Some(serde_json::json!({
                        "drop_reason_code": drop_reason_code,
                    })),
                    timestamp_us: Some(100),
                }],
                route_evidence: None,
                message: None,
                evidence_paths: vec![],
            }],
        };

        validate_wire_conformance_results(&plan, &report(3))
            .expect("matching result drop reason should validate");
        let error = validate_wire_conformance_results(&plan, &report(1))
            .expect_err("mismatched result drop reason should fail");
        assert!(error.to_string().contains("expected 3, got [Some(1)]"));

        let mut repeated = report(3);
        repeated.results[0]
            .observed_frames
            .push(WireConformanceObservedFrame {
                direction: WireConformanceFrameDirection::Received,
                frame: "RESULT_DROP_REASON".to_string(),
                payload: Some(serde_json::json!({ "drop_reason_code": 2 })),
                timestamp_us: Some(101),
            });
        let error = validate_wire_conformance_results(&plan, &repeated)
            .expect_err("every repeated drop reason must retain the declared code");
        assert!(error.to_string().contains("[Some(3), Some(2)]"));
    }

    #[test]
    fn wire_results_accept_allowed_repetition_with_required_order_intact() {
        let mut scenario = sample_wire_scenario(
            "selected",
            WireConformanceMode::SuiteAsClient,
            WireConformanceTransport::Tcp,
            vec!["control.cancel_abort"],
        );
        scenario.expect.frames = vec!["TRACE_CONTEXT".to_string(), "RESULT_PUSH".to_string()];
        scenario.expect.allowed_frames = vec![
            "REQUEST".to_string(),
            "TRACE_CONTEXT".to_string(),
            "RESULT_PUSH".to_string(),
        ];
        let plan = build_wire_conformance_execution_plan(
            &sample_wire_target(),
            &[scenario],
            AdapterArtifactContext {
                results_path: "artifacts/wire-results.json".to_string(),
                evidence_dir: "artifacts/wire-evidence".to_string(),
            },
        )
        .expect("wire execution plan should build");

        validate_wire_conformance_results(
            &plan,
            &WireConformanceCaseResultReport {
                schema: None,
                protocol_version: "nnrp-1-preview4".to_string(),
                suite_version: "0.1.0".to_string(),
                target_name: "sample-target".to_string(),
                results: vec![WireConformanceCaseResult {
                    id: "selected".to_string(),
                    outcome: ApiProfileCaseOutcome::Passed,
                    terminal: WireConformanceTerminal::Cancelled,
                    observed_frames: vec![
                        observed_wire_frame("REQUEST", 10),
                        observed_wire_frame("TRACE_CONTEXT", 20),
                        observed_wire_frame("TRACE_CONTEXT", 30),
                        observed_wire_frame("RESULT_PUSH", 40),
                    ],
                    route_evidence: None,
                    message: None,
                    evidence_paths: vec![],
                }],
            },
        )
        .expect("allowed extra and repeated frames should preserve ordered matching");
    }

    #[test]
    fn wire_results_enforce_payload_invariants_on_every_matching_frame() {
        let mut scenario = sample_wire_scenario(
            "selected",
            WireConformanceMode::SuiteAsClient,
            WireConformanceTransport::Tcp,
            vec!["control.cancel_abort"],
        );
        scenario.expect.frames = vec!["TRACE_CONTEXT".to_string()];
        scenario.expect.allowed_frames = scenario.expect.frames.clone();
        scenario.expect.frame_payload_invariants = vec![WireConformanceFramePayloadInvariant {
            frame: "TRACE_CONTEXT".to_string(),
            direction: Some(WireConformanceFrameDirection::Received),
            fields: BTreeMap::from([("frame_id".to_string(), serde_json::json!(1))]),
        }];
        let plan = build_wire_conformance_execution_plan(
            &sample_wire_target(),
            &[scenario],
            AdapterArtifactContext {
                results_path: "artifacts/wire-results.json".to_string(),
                evidence_dir: "artifacts/wire-evidence".to_string(),
            },
        )
        .expect("wire execution plan should build");
        let report = |payloads: &[Option<u64>]| WireConformanceCaseResultReport {
            schema: None,
            protocol_version: "nnrp-1-preview4".to_string(),
            suite_version: "0.1.0".to_string(),
            target_name: "sample-target".to_string(),
            results: vec![WireConformanceCaseResult {
                id: "selected".to_string(),
                outcome: ApiProfileCaseOutcome::Passed,
                terminal: WireConformanceTerminal::Cancelled,
                observed_frames: payloads
                    .iter()
                    .enumerate()
                    .map(|(index, frame_id)| match frame_id {
                        Some(frame_id) => observed_wire_frame_with_payload(
                            "TRACE_CONTEXT",
                            index as u64,
                            serde_json::json!({ "frame_id": frame_id }),
                        ),
                        None => observed_wire_frame("TRACE_CONTEXT", index as u64),
                    })
                    .collect(),
                route_evidence: None,
                message: None,
                evidence_paths: vec![],
            }],
        };

        validate_wire_conformance_results(&plan, &report(&[Some(1), Some(1)]))
            .expect("every matching repeated frame satisfies the invariant");
        let error = validate_wire_conformance_results(&plan, &report(&[Some(1), Some(2)]))
            .expect_err("one invalid repeated frame must fail the scenario");
        assert!(
            error
                .to_string()
                .contains("payload field frame_id mismatch")
        );
        let error = validate_wire_conformance_results(&plan, &report(&[None]))
            .expect_err("a missing payload must fail the scenario");
        assert!(error.to_string().contains("has no object payload"));
    }

    #[test]
    fn wire_results_reject_unexpected_frame() {
        let plan = build_wire_conformance_execution_plan(
            &sample_wire_target(),
            &[sample_wire_scenario(
                "selected",
                WireConformanceMode::SuiteAsClient,
                WireConformanceTransport::Tcp,
                vec!["control.cancel_abort"],
            )],
            AdapterArtifactContext {
                results_path: "artifacts/wire-results.json".to_string(),
                evidence_dir: "artifacts/wire-evidence".to_string(),
            },
        )
        .expect("wire execution plan should build");

        let error = validate_wire_conformance_results(
            &plan,
            &WireConformanceCaseResultReport {
                schema: None,
                protocol_version: "nnrp-1-preview4".to_string(),
                suite_version: "0.1.0".to_string(),
                target_name: "sample-target".to_string(),
                results: vec![WireConformanceCaseResult {
                    id: "selected".to_string(),
                    outcome: ApiProfileCaseOutcome::Passed,
                    terminal: WireConformanceTerminal::Cancelled,
                    observed_frames: vec![
                        observed_wire_frame("REQUEST", 10),
                        observed_wire_frame("CANCEL_ACK", 20),
                    ],
                    route_evidence: None,
                    message: None,
                    evidence_paths: vec![],
                }],
            },
        )
        .expect_err("wire results should reject an undeclared frame");

        assert!(error.to_string().contains("unexpected frame REQUEST"));
    }

    #[test]
    fn wire_results_reject_reordered_required_frames() {
        let mut scenario = sample_wire_scenario(
            "selected",
            WireConformanceMode::SuiteAsClient,
            WireConformanceTransport::Tcp,
            vec!["control.cancel_abort"],
        );
        scenario.expect.frames = vec!["TRACE_CONTEXT".to_string(), "RESULT_PUSH".to_string()];
        scenario.expect.allowed_frames = scenario.expect.frames.clone();
        let plan = build_wire_conformance_execution_plan(
            &sample_wire_target(),
            &[scenario],
            AdapterArtifactContext {
                results_path: "artifacts/wire-results.json".to_string(),
                evidence_dir: "artifacts/wire-evidence".to_string(),
            },
        )
        .expect("wire execution plan should build");

        let error = validate_wire_conformance_results(
            &plan,
            &WireConformanceCaseResultReport {
                schema: None,
                protocol_version: "nnrp-1-preview4".to_string(),
                suite_version: "0.1.0".to_string(),
                target_name: "sample-target".to_string(),
                results: vec![WireConformanceCaseResult {
                    id: "selected".to_string(),
                    outcome: ApiProfileCaseOutcome::Passed,
                    terminal: WireConformanceTerminal::Cancelled,
                    observed_frames: vec![
                        observed_wire_frame("RESULT_PUSH", 10),
                        observed_wire_frame("TRACE_CONTEXT", 20),
                    ],
                    route_evidence: None,
                    message: None,
                    evidence_paths: vec![],
                }],
            },
        )
        .expect_err("wire results should reject reordered required frames");

        assert!(
            error
                .to_string()
                .contains("missing or reordered expected frame RESULT_PUSH")
        );
    }

    #[test]
    fn wire_external_runner_requires_a_typed_scenario_executor() {
        let known = sample_wire_scenario(
            "wire.control.cancel-abort.client",
            WireConformanceMode::SuiteAsClient,
            WireConformanceTransport::Tcp,
            vec!["control.cancel_abort"],
        );
        assert_eq!(
            wire_external_case_for_scenario(&known)
                .expect("frozen scenario should have a typed executor")
                .scenario_id(),
            known.id
        );

        let deadline = sample_wire_scenario(
            "wire.control.deadline-before-submit.client",
            WireConformanceMode::SuiteAsClient,
            WireConformanceTransport::Tcp,
            vec!["control.deadline_expire"],
        );
        assert_eq!(
            wire_external_case_for_scenario(&deadline)
                .expect("deadline scenario should have a typed executor")
                .scenario_id(),
            deadline.id
        );

        let unknown = sample_wire_scenario(
            "wire.control.unknown.client",
            WireConformanceMode::SuiteAsClient,
            WireConformanceTransport::Tcp,
            vec!["control.cancel_abort"],
        );
        let error = wire_external_case_for_scenario(&unknown)
            .expect_err("unknown scenario should not receive a synthetic executor");
        assert!(error.to_string().contains("no typed external executor"));
    }

    #[tokio::test]
    async fn wire_external_runner_rejects_target_mismatch() {
        let mut target = sample_wire_target();
        let plan = build_wire_conformance_execution_plan(
            &target,
            &[sample_wire_scenario(
                "selected",
                WireConformanceMode::SuiteAsClient,
                WireConformanceTransport::Tcp,
                vec!["control.cancel_abort"],
            )],
            AdapterArtifactContext {
                results_path: "artifacts/wire-results.json".to_string(),
                evidence_dir: "artifacts/wire-evidence".to_string(),
            },
        )
        .expect("wire execution plan should build");
        target.target_name = "other-target".to_string();

        let error = run_wire_conformance_external(&plan, &target, Path::new("target.json"))
            .await
            .expect_err("target mismatch should be rejected");

        assert!(error.to_string().contains("wire target name mismatch"));
    }
}
mod host_route;
