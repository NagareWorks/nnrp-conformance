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
    WireConformanceScenario, WireConformanceTargetManifest, WireConformanceTerminal,
    validate_protocol_alignment,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::Duration;

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
                && target_transports.contains(&scenario.transport)
                && scenario
                    .required_capabilities
                    .iter()
                    .all(|capability| target_capabilities.contains(capability))
        })
        .cloned()
        .collect();

    Ok(WireConformanceExecutionPlan {
        schema: Some(
            "https://github.com/NagareWorks/nnrp-conformance/schemas/wire-conformance-execution-plan.schema.json"
                .to_string(),
        ),
        protocol_version: target_manifest.protocol_version.clone(),
        suite_version: target_manifest.suite_version.clone(),
        target_name: target_manifest.target_name.clone(),
        artifacts,
        scenarios: selected_scenarios,
    })
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
    let requires_security = match endpoint.name {
        WireConformanceTransport::Tcp | WireConformanceTransport::Ipc => false,
        WireConformanceTransport::Quic => true,
        WireConformanceTransport::Websocket => {
            if endpoint.endpoint.starts_with("wss://") {
                true
            } else if endpoint.endpoint.starts_with("ws://") {
                false
            } else {
                return Err(FixtureError::Validation {
                    message: "WebSocket wire endpoint must use ws:// or wss://".to_string(),
                });
            }
        }
    };
    if endpoint.tls != requires_security || endpoint.security.is_some() != requires_security {
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
                for expected_frame in &expected_scenario.expect.frames {
                    if !result
                        .observed_frames
                        .iter()
                        .any(|frame| &frame.frame == expected_frame)
                    {
                        return Err(FixtureError::Validation {
                            message: format!(
                                "wire scenario {} missing expected frame {}",
                                result.id, expected_frame
                            ),
                        });
                    }
                }
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

pub async fn run_wire_conformance_external(
    plan: &WireConformanceExecutionPlan,
    target_manifest: &WireConformanceTargetManifest,
    target_manifest_path: &Path,
) -> Result<WireConformanceCaseResultReport, FixtureError> {
    validate_wire_plan_target_alignment(plan, target_manifest)?;

    let mut results = Vec::with_capacity(plan.scenarios.len());
    for scenario in &plan.scenarios {
        results.push(
            run_wire_external_scenario(plan, target_manifest, target_manifest_path, scenario)
                .await?,
        );
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
    let endpoint_manifest = target_manifest
        .wire_conformance
        .transports
        .iter()
        .find(|endpoint| endpoint.name == scenario.transport)
        .ok_or_else(|| FixtureError::Validation {
            message: format!(
                "target manifest does not declare {:?} transport endpoint",
                scenario.transport
            ),
        })?;
    let case = wire_external_case_for_scenario(scenario)?;
    let endpoint = wire_reference_endpoint(endpoint_manifest, target_manifest_path)?;
    let timeout = wire_scenario_timeout(scenario);

    let execution = tokio::time::timeout(timeout, run_wire_external_case(case, &endpoint)).await;
    let evidence_paths = vec![wire_evidence_path(plan, &scenario.id)];
    match execution {
        Ok(Ok(report)) => Ok(wire_external_case_result(scenario, report, evidence_paths)),
        Ok(Err(error)) => Ok(WireConformanceCaseResult {
            id: scenario.id.clone(),
            outcome: ApiProfileCaseOutcome::Failed,
            terminal: WireConformanceTerminal::Error,
            observed_frames: Vec::new(),
            message: Some(format!("external wire target failed: {error}")),
            evidence_paths,
        }),
        Err(_) => Ok(WireConformanceCaseResult {
            id: scenario.id.clone(),
            outcome: ApiProfileCaseOutcome::Failed,
            terminal: WireConformanceTerminal::Error,
            observed_frames: Vec::new(),
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
        || wire_transport(case.transport()) != scenario.transport
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
        validate_wire_conformance_results, wire_external_case_for_scenario,
    };
    use nnrp_conformance_fixtures::{
        AdapterArtifactContext, ApiProfileCapabilityManifest, ApiProfileCaseOutcome,
        ApiProfileCaseResult, ApiProfileCaseResultReport, ApiProfileExpectation,
        ApiProfileExpectedEvent, ApiProfileExtensionCapability, ApiProfileObservedEvent,
        ApiProfileOperationCapability, ApiProfileRecipe, ApiProfileRecipeRequest,
        ApiProfileTerminal, BenchmarkArtifactContext, CapabilityManifest, CaseDefinition,
        CaseLayer, CaseManifest, CaseStatus, ProtocolManifest, WireConformanceCaseResult,
        WireConformanceCaseResultReport, WireConformanceExpectation, WireConformanceFrameDirection,
        WireConformanceLimits, WireConformanceMode, WireConformanceObservedFrame,
        WireConformanceScenario, WireConformanceStep, WireConformanceTarget,
        WireConformanceTargetManifest, WireConformanceTerminal, WireConformanceTransport,
        WireConformanceTransportEndpoint, WireConformanceTransportSecurity, load_json_file,
    };
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};

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
                },
                CaseDefinition {
                    id: "l1.transport.tcp.minimum".to_string(),
                    layer: CaseLayer::L3,
                    status: CaseStatus::Optional,
                    feature: "transport.tcp".to_string(),
                    required_capabilities: vec!["transport.tcp".to_string()],
                    description: "test".to_string(),
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
                },
                CaseDefinition {
                    id: "l3.transport.quic.minimum".to_string(),
                    layer: CaseLayer::L3,
                    status: CaseStatus::Optional,
                    feature: "transport.quic".to_string(),
                    required_capabilities: vec!["transport.quic".to_string()],
                    description: "not claimed".to_string(),
                },
                CaseDefinition {
                    id: "l1.flow_update.connection.scope.validation".to_string(),
                    layer: CaseLayer::L1,
                    status: CaseStatus::Experimental,
                    feature: "flow_update".to_string(),
                    required_capabilities: vec!["flow_update".to_string()],
                    description: "informational".to_string(),
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
            transport,
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
            },
        }
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
                    message: None,
                    evidence_paths: vec![],
                }],
            },
        )
        .expect_err("wire results should reject missing expected frame");

        assert!(error.to_string().contains("CANCEL_ACK"));
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
