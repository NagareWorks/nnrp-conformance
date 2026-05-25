use anyhow::Result;
use clap::{Parser, Subcommand};
use nnrp_conformance_fixtures::{
    AdapterArtifactContext, AdapterCaseOutcome, AdapterCaseResultReport, AdapterExecutionPlan,
    BenchmarkArtifactContext, BenchmarkExecutionPlan, BenchmarkOutcome, BenchmarkResultReport,
    CapabilityManifest, CaseManifest, ProtocolManifest, SemanticVectorManifest, VectorManifest,
    build_vector_manifest, load_json_file, verify_vector_manifest,
};
use nnrp_conformance_runner::{
    build_adapter_execution_plan, build_adapter_execution_plan_for_manifests,
    build_benchmark_execution_plan, build_execution_plan, build_execution_plan_for_manifests,
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "nnrp-conformance-runner")]
#[command(
    about = "Load a versioned NNRP conformance baseline and print public conformance reports"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Summary {
        #[arg(long)]
        protocol: PathBuf,
        #[arg(long)]
        cases: Option<PathBuf>,
        #[arg(long)]
        capabilities: Option<PathBuf>,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    AdapterPlan {
        #[arg(long)]
        protocol: PathBuf,
        #[arg(long)]
        cases: Option<PathBuf>,
        #[arg(long)]
        capabilities: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value = "artifacts/adapter-results.json")]
        results_path: PathBuf,
        #[arg(long, default_value = "artifacts/evidence")]
        evidence_dir: PathBuf,
    },
    BenchmarkPlan {
        #[arg(long)]
        protocol: PathBuf,
        #[arg(long)]
        capabilities: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value = "artifacts/benchmark-results.json")]
        results_path: PathBuf,
        #[arg(long, default_value = "artifacts/benchmark-evidence")]
        evidence_dir: PathBuf,
    },
    GenerateVectors {
        #[arg(long)]
        recipe: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    VerifyVectors {
        #[arg(long)]
        recipe: PathBuf,
        #[arg(long)]
        manifest: PathBuf,
    },
    CompareVectorManifests {
        #[arg(long)]
        expected: PathBuf,
        #[arg(long)]
        actual: PathBuf,
    },
    ValidateAdapterResults {
        #[arg(long)]
        plan: PathBuf,
        #[arg(long)]
        results: PathBuf,
    },
    ValidateBenchmarkResults {
        #[arg(long)]
        plan: PathBuf,
        #[arg(long)]
        results: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Summary {
            protocol,
            cases,
            capabilities,
            output,
        } => {
            let protocol_manifest: ProtocolManifest = load_json_file(&protocol)?;
            let capability_manifest = match &capabilities {
                Some(path) => Some(load_json_file::<CapabilityManifest>(path)?),
                None => None,
            };

            let case_paths = match cases {
                Some(case_path) => vec![case_path],
                None => {
                    let protocol_dir = protocol.parent().unwrap_or(std::path::Path::new("."));
                    protocol_manifest
                        .case_manifests
                        .iter()
                        .map(|relative_path| protocol_dir.join(relative_path))
                        .collect()
                }
            };
            let case_manifests = case_paths
                .iter()
                .map(load_json_file::<CaseManifest>)
                .collect::<Result<Vec<_>, _>>()?;

            let summary = if case_manifests.len() == 1 {
                build_execution_plan(
                    &protocol_manifest,
                    &case_manifests[0],
                    capability_manifest.as_ref(),
                    &case_paths[0],
                    capabilities.as_deref(),
                )?
            } else {
                build_execution_plan_for_manifests(
                    &protocol_manifest,
                    case_manifests
                        .iter()
                        .zip(case_paths.iter())
                        .map(|(manifest, path)| (manifest, path.as_path())),
                    capability_manifest.as_ref(),
                    capabilities.as_deref(),
                )?
            };
            let rendered = format!("{}\n", serde_json::to_string_pretty(&summary)?);
            if let Some(output_path) = output {
                std::fs::write(output_path, rendered)?;
            } else {
                print!("{rendered}");
            }
        }
        Command::AdapterPlan {
            protocol,
            cases,
            capabilities,
            output,
            results_path,
            evidence_dir,
        } => {
            let protocol_manifest: ProtocolManifest = load_json_file(&protocol)?;
            let capability_manifest: CapabilityManifest = load_json_file(&capabilities)?;

            let case_paths = match cases {
                Some(case_path) => vec![case_path],
                None => {
                    let protocol_dir = protocol.parent().unwrap_or(std::path::Path::new("."));
                    protocol_manifest
                        .case_manifests
                        .iter()
                        .map(|relative_path| protocol_dir.join(relative_path))
                        .collect()
                }
            };
            let case_manifests = case_paths
                .iter()
                .map(load_json_file::<CaseManifest>)
                .collect::<Result<Vec<_>, _>>()?;
            let artifacts = AdapterArtifactContext {
                results_path: results_path.display().to_string(),
                evidence_dir: evidence_dir.display().to_string(),
            };

            let plan = if case_manifests.len() == 1 {
                build_adapter_execution_plan(
                    &protocol_manifest,
                    &case_manifests[0],
                    &capability_manifest,
                    &case_paths[0],
                    &capabilities,
                    artifacts.clone(),
                )?
            } else {
                build_adapter_execution_plan_for_manifests(
                    &protocol_manifest,
                    case_manifests
                        .iter()
                        .zip(case_paths.iter())
                        .map(|(manifest, path)| (manifest, path.as_path())),
                    &capability_manifest,
                    &capabilities,
                    artifacts,
                )?
            };
            std::fs::write(
                output,
                format!("{}\n", serde_json::to_string_pretty(&plan)?),
            )?;
        }
        Command::BenchmarkPlan {
            protocol,
            capabilities,
            output,
            results_path,
            evidence_dir,
        } => {
            let protocol_manifest: ProtocolManifest = load_json_file(&protocol)?;
            let capability_manifest: CapabilityManifest = load_json_file(&capabilities)?;
            anyhow::ensure!(
                protocol_manifest.protocol_version == capability_manifest.protocol_version,
                "benchmark capability protocol version mismatch: expected {}, got {}",
                protocol_manifest.protocol_version,
                capability_manifest.protocol_version
            );
            let artifacts = BenchmarkArtifactContext {
                results_path: results_path.display().to_string(),
                evidence_dir: evidence_dir.display().to_string(),
            };
            let plan =
                build_benchmark_execution_plan(&protocol_manifest, &capability_manifest, artifacts);
            std::fs::write(
                output,
                format!("{}\n", serde_json::to_string_pretty(&plan)?),
            )?;
        }
        Command::GenerateVectors { recipe, output } => {
            let semantic_manifest: SemanticVectorManifest = load_json_file(&recipe)?;
            let generated_from = recipe
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| format!("vectors/{name}"))
                .unwrap_or_else(|| recipe.display().to_string());
            let vector_manifest = build_vector_manifest(&semantic_manifest, &generated_from)?;
            std::fs::write(
                &output,
                format!("{}\n", serde_json::to_string_pretty(&vector_manifest)?),
            )?;
        }
        Command::VerifyVectors { recipe, manifest } => {
            let semantic_manifest: SemanticVectorManifest = load_json_file(&recipe)?;
            let vector_manifest: VectorManifest = load_json_file(&manifest)?;
            let generated_from = recipe
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| format!("vectors/{name}"))
                .unwrap_or_else(|| recipe.display().to_string());
            verify_vector_manifest(&semantic_manifest, &vector_manifest, &generated_from)?;
        }
        Command::CompareVectorManifests { expected, actual } => {
            let expected_manifest: VectorManifest = load_json_file(&expected)?;
            let actual_manifest: VectorManifest = load_json_file(&actual)?;
            compare_vector_manifests(&expected_manifest, &actual_manifest)?;
        }
        Command::ValidateAdapterResults { plan, results } => {
            let adapter_plan: AdapterExecutionPlan = load_json_file(&plan)?;
            let adapter_results: AdapterCaseResultReport = load_json_file(&results)?;
            let summary = validate_adapter_results(&adapter_plan, &adapter_results)?;
            println!("{}", serde_json::to_string_pretty(&summary)?);
        }
        Command::ValidateBenchmarkResults { plan, results } => {
            let benchmark_plan: BenchmarkExecutionPlan = load_json_file(&plan)?;
            let benchmark_results: BenchmarkResultReport = load_json_file(&results)?;
            let summary = validate_benchmark_results(&benchmark_plan, &benchmark_results)?;
            println!("{}", serde_json::to_string_pretty(&summary)?);
        }
    }

    Ok(())
}

fn compare_vector_manifests(expected: &VectorManifest, actual: &VectorManifest) -> Result<()> {
    anyhow::ensure!(
        expected.protocol_version == actual.protocol_version,
        "protocol version mismatch: expected {}, got {}",
        expected.protocol_version,
        actual.protocol_version
    );

    let expected_vectors = expected
        .vectors
        .iter()
        .map(|vector| {
            (
                vector.name.as_str(),
                (&vector.kind, &vector.hex, vector.bytes),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let actual_vectors = actual
        .vectors
        .iter()
        .map(|vector| {
            (
                vector.name.as_str(),
                (&vector.kind, &vector.hex, vector.bytes),
            )
        })
        .collect::<BTreeMap<_, _>>();

    anyhow::ensure!(
        expected_vectors.len() == actual_vectors.len(),
        "vector count mismatch: expected {}, got {}",
        expected_vectors.len(),
        actual_vectors.len()
    );

    for (name, expected_entry) in expected_vectors {
        let actual_entry = actual_vectors
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("missing vector in actual manifest: {name}"))?;
        anyhow::ensure!(
            expected_entry == *actual_entry,
            "vector mismatch for {name}"
        );
    }

    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct AdapterValidationSummary {
    selected_cases: usize,
    pass_cases: usize,
    fail_cases: usize,
    skipped_cases: usize,
    error_cases: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct BenchmarkValidationSummary {
    selected_scenarios: usize,
    measured_scenarios: usize,
    skipped_scenarios: usize,
    error_scenarios: usize,
}

fn validate_benchmark_results(
    expected_plan: &BenchmarkExecutionPlan,
    actual_report: &BenchmarkResultReport,
) -> Result<BenchmarkValidationSummary> {
    anyhow::ensure!(
        expected_plan.protocol_version == actual_report.protocol_version,
        "benchmark protocol version mismatch: expected {}, got {}",
        expected_plan.protocol_version,
        actual_report.protocol_version
    );
    anyhow::ensure!(
        expected_plan.implementation_name == actual_report.implementation_name,
        "benchmark implementation name mismatch: expected {}, got {}",
        expected_plan.implementation_name,
        actual_report.implementation_name
    );

    let expected_ids = expected_plan
        .scenarios
        .iter()
        .map(|scenario| scenario.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut actual_ids = BTreeSet::new();

    let mut summary = BenchmarkValidationSummary {
        selected_scenarios: expected_ids.len(),
        measured_scenarios: 0,
        skipped_scenarios: 0,
        error_scenarios: 0,
    };

    for result in &actual_report.results {
        anyhow::ensure!(
            expected_ids.contains(result.id.as_str()),
            "benchmark results contain an unexpected scenario id: {}",
            result.id
        );
        anyhow::ensure!(
            actual_ids.insert(result.id.as_str()),
            "benchmark results contain a duplicate scenario id: {}",
            result.id
        );

        match result.outcome {
            BenchmarkOutcome::Measured => {
                anyhow::ensure!(
                    result.metrics.is_some() || !result.samples.is_empty(),
                    "benchmark measured scenario {} must include metrics or samples",
                    result.id
                );
                summary.measured_scenarios += 1;
            }
            BenchmarkOutcome::Skip => summary.skipped_scenarios += 1,
            BenchmarkOutcome::Error => summary.error_scenarios += 1,
        }
    }

    anyhow::ensure!(
        actual_ids.len() == expected_ids.len(),
        "benchmark results are missing {} selected scenario(s)",
        expected_ids.len().saturating_sub(actual_ids.len())
    );

    Ok(summary)
}

fn validate_adapter_results(
    expected_plan: &AdapterExecutionPlan,
    actual_report: &AdapterCaseResultReport,
) -> Result<AdapterValidationSummary> {
    anyhow::ensure!(
        expected_plan.protocol_version == actual_report.protocol_version,
        "adapter protocol version mismatch: expected {}, got {}",
        expected_plan.protocol_version,
        actual_report.protocol_version
    );
    anyhow::ensure!(
        expected_plan.implementation_name == actual_report.implementation_name,
        "adapter implementation name mismatch: expected {}, got {}",
        expected_plan.implementation_name,
        actual_report.implementation_name
    );

    let expected_ids = expected_plan
        .cases
        .iter()
        .map(|case| case.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut actual_ids = BTreeSet::new();

    let mut summary = AdapterValidationSummary {
        selected_cases: expected_ids.len(),
        pass_cases: 0,
        fail_cases: 0,
        skipped_cases: 0,
        error_cases: 0,
    };

    for result in &actual_report.results {
        anyhow::ensure!(
            expected_ids.contains(result.id.as_str()),
            "adapter results contain an unexpected case id: {}",
            result.id
        );
        anyhow::ensure!(
            actual_ids.insert(result.id.as_str()),
            "adapter results contain a duplicate case id: {}",
            result.id
        );

        match result.outcome {
            AdapterCaseOutcome::Pass => summary.pass_cases += 1,
            AdapterCaseOutcome::Fail => summary.fail_cases += 1,
            AdapterCaseOutcome::Skip => summary.skipped_cases += 1,
            AdapterCaseOutcome::Error => summary.error_cases += 1,
        }
    }

    anyhow::ensure!(
        actual_ids.len() == expected_ids.len(),
        "adapter results are missing {} selected case(s)",
        expected_ids.len().saturating_sub(actual_ids.len())
    );

    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::{validate_adapter_results, validate_benchmark_results};
    use nnrp_conformance_fixtures::{
        AdapterArtifactContext, AdapterCaseOutcome, AdapterCaseResult, AdapterCaseResultReport,
        AdapterExecutionCase, AdapterExecutionPlan, BenchmarkArtifactContext, BenchmarkCategory,
        BenchmarkEnvironment, BenchmarkExecutionPlan, BenchmarkMetrics, BenchmarkOutcome,
        BenchmarkScenario, BenchmarkScenarioResult, BenchmarkWorkload, CaseLayer, CaseStatus,
    };

    fn sample_plan() -> AdapterExecutionPlan {
        AdapterExecutionPlan {
            schema: None,
            protocol_version: "nnrp-1-preview3".to_string(),
            suite_version: "preview3-bootstrap".to_string(),
            implementation_name: "nnrp-rs".to_string(),
            artifacts: AdapterArtifactContext {
                results_path: "artifacts/adapter-results.json".to_string(),
                evidence_dir: "artifacts/evidence".to_string(),
            },
            cases: vec![
                AdapterExecutionCase {
                    id: "l1.handshake.basic".to_string(),
                    layer: CaseLayer::L1,
                    status: CaseStatus::Mandatory,
                    feature: "handshake".to_string(),
                    required_capabilities: vec!["control.client_hello".to_string()],
                    description: "Basic handshake path".to_string(),
                },
                AdapterExecutionCase {
                    id: "l1.session.open_close".to_string(),
                    layer: CaseLayer::L1,
                    status: CaseStatus::Mandatory,
                    feature: "session_lifecycle".to_string(),
                    required_capabilities: vec!["control.session_open".to_string()],
                    description: "Session open close path".to_string(),
                },
            ],
        }
    }

    #[test]
    fn adapter_results_validate_when_report_matches_selected_cases() {
        let summary = validate_adapter_results(
            &sample_plan(),
            &AdapterCaseResultReport {
                schema: None,
                protocol_version: "nnrp-1-preview3".to_string(),
                implementation_name: "nnrp-rs".to_string(),
                results: vec![
                    AdapterCaseResult {
                        id: "l1.handshake.basic".to_string(),
                        outcome: AdapterCaseOutcome::Error,
                        failure_kind: Some("not_implemented".to_string()),
                        message: None,
                        evidence_paths: vec![],
                    },
                    AdapterCaseResult {
                        id: "l1.session.open_close".to_string(),
                        outcome: AdapterCaseOutcome::Skip,
                        failure_kind: None,
                        message: Some("not wired yet".to_string()),
                        evidence_paths: vec![],
                    },
                ],
            },
        )
        .expect("adapter results should validate");

        assert_eq!(summary.selected_cases, 2);
        assert_eq!(summary.error_cases, 1);
        assert_eq!(summary.skipped_cases, 1);
    }

    #[test]
    fn adapter_results_reject_missing_selected_case() {
        let error = validate_adapter_results(
            &sample_plan(),
            &AdapterCaseResultReport {
                schema: None,
                protocol_version: "nnrp-1-preview3".to_string(),
                implementation_name: "nnrp-rs".to_string(),
                results: vec![AdapterCaseResult {
                    id: "l1.handshake.basic".to_string(),
                    outcome: AdapterCaseOutcome::Pass,
                    failure_kind: None,
                    message: None,
                    evidence_paths: vec![],
                }],
            },
        )
        .expect_err("adapter results should reject missing selected case");

        assert!(error.to_string().contains("missing 1 selected case"));
    }

    #[test]
    fn adapter_results_reject_implementation_name_mismatch() {
        let error = validate_adapter_results(
            &sample_plan(),
            &AdapterCaseResultReport {
                schema: None,
                protocol_version: "nnrp-1-preview3".to_string(),
                implementation_name: "nnrp-py".to_string(),
                results: vec![],
            },
        )
        .expect_err("adapter results should reject mismatched implementation name");

        assert!(error.to_string().contains("implementation name mismatch"));
    }

    fn sample_benchmark_plan() -> BenchmarkExecutionPlan {
        BenchmarkExecutionPlan {
            schema: None,
            protocol_version: "nnrp-1-preview3".to_string(),
            suite_version: "preview3-bootstrap".to_string(),
            implementation_name: "nnrp-rs".to_string(),
            artifacts: BenchmarkArtifactContext {
                results_path: "artifacts/benchmark-results.json".to_string(),
                evidence_dir: "artifacts/benchmark-evidence".to_string(),
            },
            scenarios: vec![
                BenchmarkScenario {
                    id: "l4.header.encode_decode.latency".to_string(),
                    category: BenchmarkCategory::Latency,
                    feature: "benchmark.header".to_string(),
                    required_capabilities: vec![],
                    description: "Header latency".to_string(),
                    workload: BenchmarkWorkload {
                        operation: "header_encode_decode".to_string(),
                        payload: "l0_header".to_string(),
                        transport: None,
                        iterations: Some(100),
                        warmup_iterations: Some(10),
                        duration_seconds: None,
                    },
                },
                BenchmarkScenario {
                    id: "l4.submit_result.inline_tensor.throughput".to_string(),
                    category: BenchmarkCategory::Throughput,
                    feature: "benchmark.submit_result".to_string(),
                    required_capabilities: vec!["result_push.basic".to_string()],
                    description: "Submit/result throughput".to_string(),
                    workload: BenchmarkWorkload {
                        operation: "submit_result_loop".to_string(),
                        payload: "inline_tensor_4k".to_string(),
                        transport: None,
                        iterations: None,
                        warmup_iterations: Some(10),
                        duration_seconds: Some(1),
                    },
                },
            ],
        }
    }

    fn sample_environment() -> BenchmarkEnvironment {
        BenchmarkEnvironment {
            sdk_commit: Some("abc123".to_string()),
            nnrp_rs_artifact: None,
            host_runtime: Some("cargo 1.90.0".to_string()),
            os: "linux".to_string(),
            arch: "x86_64".to_string(),
            cpu: Some("sample cpu".to_string()),
            notes: None,
        }
    }

    #[test]
    fn benchmark_results_validate_when_report_matches_selected_scenarios() {
        let summary = validate_benchmark_results(
            &sample_benchmark_plan(),
            &nnrp_conformance_fixtures::BenchmarkResultReport {
                schema: None,
                protocol_version: "nnrp-1-preview3".to_string(),
                implementation_name: "nnrp-rs".to_string(),
                environment: sample_environment(),
                results: vec![
                    BenchmarkScenarioResult {
                        id: "l4.header.encode_decode.latency".to_string(),
                        outcome: BenchmarkOutcome::Measured,
                        samples: vec![],
                        metrics: Some(BenchmarkMetrics {
                            p50_us: Some(1.0),
                            p95_us: Some(2.0),
                            p99_us: Some(3.0),
                            throughput_ops_per_sec: None,
                            cpu_percent: None,
                            peak_memory_bytes: None,
                            gc_alloc_bytes: None,
                        }),
                        message: None,
                        evidence_paths: vec![],
                    },
                    BenchmarkScenarioResult {
                        id: "l4.submit_result.inline_tensor.throughput".to_string(),
                        outcome: BenchmarkOutcome::Skip,
                        samples: vec![],
                        metrics: None,
                        message: Some("runtime not wired yet".to_string()),
                        evidence_paths: vec![],
                    },
                ],
            },
        )
        .expect("benchmark results should validate");

        assert_eq!(summary.selected_scenarios, 2);
        assert_eq!(summary.measured_scenarios, 1);
        assert_eq!(summary.skipped_scenarios, 1);
    }

    #[test]
    fn benchmark_results_reject_measured_scenario_without_metrics() {
        let error = validate_benchmark_results(
            &sample_benchmark_plan(),
            &nnrp_conformance_fixtures::BenchmarkResultReport {
                schema: None,
                protocol_version: "nnrp-1-preview3".to_string(),
                implementation_name: "nnrp-rs".to_string(),
                environment: sample_environment(),
                results: vec![
                    BenchmarkScenarioResult {
                        id: "l4.header.encode_decode.latency".to_string(),
                        outcome: BenchmarkOutcome::Measured,
                        samples: vec![],
                        metrics: None,
                        message: None,
                        evidence_paths: vec![],
                    },
                    BenchmarkScenarioResult {
                        id: "l4.submit_result.inline_tensor.throughput".to_string(),
                        outcome: BenchmarkOutcome::Skip,
                        samples: vec![],
                        metrics: None,
                        message: None,
                        evidence_paths: vec![],
                    },
                ],
            },
        )
        .expect_err("measured benchmark without metrics should be rejected");

        assert!(
            error
                .to_string()
                .contains("must include metrics or samples")
        );
    }
}
