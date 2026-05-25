use nnrp_conformance_fixtures::{
    AdapterArtifactContext, AdapterExecutionCase, AdapterExecutionPlan, BenchmarkArtifactContext,
    BenchmarkCategory, BenchmarkExecutionPlan, BenchmarkScenario, BenchmarkWorkload,
    CapabilityManifest, CaseDefinition, CaseManifest, CaseStatus, CompatibilityMatrixEntry,
    ConformanceReport, FixtureError, ProtocolManifest, ReportCase, ReportStatusSummary,
    ReportSummary, validate_protocol_alignment,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum CaseSelection {
    Selected,
    NotClaimed,
    Informational,
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
        build_benchmark_execution_plan, build_execution_plan, build_execution_plan_for_manifests,
    };
    use nnrp_conformance_fixtures::{
        AdapterArtifactContext, BenchmarkArtifactContext, CapabilityManifest, CaseDefinition,
        CaseLayer, CaseManifest, CaseStatus, ProtocolManifest, load_json_file,
    };
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
}
