use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use thiserror::Error;

mod semantic_vectors;

pub use semantic_vectors::{
    GoldenVector, SemanticVectorManifest, SemanticVectorRecipe, VectorManifest,
    build_vector_manifest, verify_vector_manifest,
};

#[derive(Debug, Error)]
pub enum FixtureError {
    #[error("failed to read json fixture {path}: {source}")]
    Read {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse json fixture {path}: {source}")]
    Parse {
        path: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("protocol version mismatch: expected {expected}, got {actual} in {path}")]
    VersionMismatch {
        expected: String,
        actual: String,
        path: String,
    },
    #[error("fixture validation failed: {message}")]
    Validation { message: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolManifest {
    #[serde(rename = "$schema", default)]
    pub schema: Option<String>,
    pub protocol_version: String,
    pub suite_version: String,
    pub status: String,
    pub case_manifests: Vec<String>,
    #[serde(default)]
    pub vector_recipe_manifests: Vec<String>,
    pub vector_manifests: Vec<String>,
    pub report_schema: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaseManifest {
    #[serde(rename = "$schema", default)]
    pub schema: Option<String>,
    pub protocol_version: String,
    pub manifest_name: String,
    pub cases: Vec<CaseDefinition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityManifest {
    #[serde(rename = "$schema", default)]
    pub schema: Option<String>,
    pub implementation_name: String,
    pub protocol_version: String,
    pub supports: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterExecutionPlan {
    #[serde(rename = "$schema", default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    pub protocol_version: String,
    pub suite_version: String,
    pub implementation_name: String,
    pub artifacts: AdapterArtifactContext,
    pub cases: Vec<AdapterExecutionCase>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterArtifactContext {
    pub results_path: String,
    pub evidence_dir: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterExecutionCase {
    pub id: String,
    pub layer: CaseLayer,
    pub status: CaseStatus,
    pub feature: String,
    pub required_capabilities: Vec<String>,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterCaseResultReport {
    #[serde(rename = "$schema", default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    pub protocol_version: String,
    pub implementation_name: String,
    pub results: Vec<AdapterCaseResult>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BenchmarkExecutionPlan {
    #[serde(rename = "$schema", default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    pub protocol_version: String,
    pub suite_version: String,
    pub implementation_name: String,
    pub artifacts: BenchmarkArtifactContext,
    pub scenarios: Vec<BenchmarkScenario>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BenchmarkArtifactContext {
    pub results_path: String,
    pub evidence_dir: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BenchmarkScenario {
    pub id: String,
    pub category: BenchmarkCategory,
    pub feature: String,
    pub required_capabilities: Vec<String>,
    pub description: String,
    pub workload: BenchmarkWorkload,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BenchmarkCategory {
    Latency,
    Throughput,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BenchmarkWorkload {
    pub operation: String,
    pub payload: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transport: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iterations: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warmup_iterations: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_seconds: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchmarkResultReport {
    #[serde(rename = "$schema", default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    pub protocol_version: String,
    pub implementation_name: String,
    pub environment: BenchmarkEnvironment,
    pub results: Vec<BenchmarkScenarioResult>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BenchmarkEnvironment {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sdk_commit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nnrp_rs_artifact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host_runtime: Option<String>,
    pub os: String,
    pub arch: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchmarkScenarioResult {
    pub id: String,
    pub outcome: BenchmarkOutcome,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub samples: Vec<BenchmarkSample>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics: Option<BenchmarkMetrics>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence_paths: Vec<String>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BenchmarkOutcome {
    Measured,
    Skip,
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchmarkSample {
    pub value: f64,
    pub unit: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchmarkMetrics {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p50_us: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p95_us: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p99_us: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub throughput_ops_per_sec: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_percent: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peak_memory_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gc_alloc_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterCaseResult {
    pub id: String,
    pub outcome: AdapterCaseOutcome,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence_paths: Vec<String>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AdapterCaseOutcome {
    Pass,
    Fail,
    Skip,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiProfileCapabilityManifest {
    #[serde(rename = "$schema", default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    pub adapter: String,
    pub profile: String,
    pub schema_version: String,
    pub compatibility_levels: Vec<u32>,
    pub operations: Vec<ApiProfileOperationCapability>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<ApiProfileExtensionCapability>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiProfileSuiteManifest {
    #[serde(rename = "$schema", default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    pub profile: String,
    pub schema_version: String,
    pub level: u32,
    pub protocol_baselines: Vec<String>,
    pub recipe_manifests: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiProfileOperationCapability {
    pub name: String,
    pub streaming: bool,
    pub non_streaming: bool,
    pub tool_calls: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub cancellation: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiProfileExtensionCapability {
    pub name: String,
    pub critical: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiProfileRecipe {
    #[serde(rename = "$schema", default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    pub id: String,
    pub profile: String,
    pub schema_version: String,
    pub operation: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_capabilities: Vec<String>,
    #[serde(default = "mandatory_case_status")]
    pub status: CaseStatus,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub parameters: BTreeMap<String, String>,
    pub request: ApiProfileRecipeRequest,
    pub expect: ApiProfileExpectation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiProfileRecipeRequest {
    pub body: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nnrp: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiProfileExpectation {
    pub events: Vec<ApiProfileExpectedEvent>,
    pub terminal: ApiProfileTerminal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiProfileExpectedEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub optional: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiProfileExecutionPlan {
    #[serde(rename = "$schema", default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    pub profile: String,
    pub schema_version: String,
    pub adapter: String,
    pub artifacts: AdapterArtifactContext,
    pub coverage_matrix: Vec<CompatibilityMatrixEntry>,
    pub cases: Vec<ApiProfileExecutionCase>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiProfileExecutionCase {
    pub id: String,
    pub operation: String,
    pub status: CaseStatus,
    pub required_capabilities: Vec<String>,
    pub request: ApiProfileRecipeRequest,
    pub expect: ApiProfileExpectation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiProfileCaseResultReport {
    #[serde(rename = "$schema", default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    pub profile: String,
    pub schema_version: String,
    pub adapter: String,
    pub results: Vec<ApiProfileCaseResult>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiProfileCaseResult {
    pub id: String,
    pub outcome: ApiProfileCaseOutcome,
    pub terminal: ApiProfileTerminal,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<ApiProfileObservedEvent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostics: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiProfileObservedEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(flatten)]
    pub fields: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApiProfileTerminal {
    Success,
    Error,
    Cancelled,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApiProfileCaseOutcome {
    Passed,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WireConformanceSuiteManifest {
    #[serde(rename = "$schema", default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    pub protocol_version: String,
    pub suite_version: String,
    pub status: String,
    pub modes: Vec<WireConformanceMode>,
    pub transports: Vec<WireConformanceTransport>,
    pub scenario_manifests: Vec<String>,
    pub target_schema: String,
    pub execution_plan_schema: String,
    pub case_results_schema: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WireConformanceScenarioManifest {
    #[serde(rename = "$schema", default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    pub protocol_version: String,
    pub manifest_name: String,
    pub scenarios: Vec<WireConformanceScenario>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WireConformanceTargetManifest {
    #[serde(rename = "$schema", default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    pub target_name: String,
    pub protocol_version: String,
    pub suite_version: String,
    pub wire_conformance: WireConformanceTarget,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WireConformanceTarget {
    pub modes: Vec<WireConformanceMode>,
    pub transports: Vec<WireConformanceTransportEndpoint>,
    pub capabilities: Vec<String>,
    pub limits: WireConformanceLimits,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WireConformanceTransportEndpoint {
    pub name: WireConformanceTransport,
    pub endpoint: String,
    pub tls: bool,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WireConformanceMode {
    SuiteAsClient,
    SuiteAsServer,
    SuiteAsProxy,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WireConformanceTransport {
    Tcp,
    Quic,
    Websocket,
    Ipc,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireConformanceLimits {
    pub max_frame_bytes: u64,
    pub max_in_flight: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WireConformanceExecutionPlan {
    #[serde(rename = "$schema", default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    pub protocol_version: String,
    pub suite_version: String,
    pub target_name: String,
    pub artifacts: AdapterArtifactContext,
    pub scenarios: Vec<WireConformanceScenario>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WireConformanceScenario {
    pub id: String,
    pub mode: WireConformanceMode,
    pub transport: WireConformanceTransport,
    pub status: CaseStatus,
    pub feature: String,
    pub required_capabilities: Vec<String>,
    pub description: String,
    pub steps: Vec<WireConformanceStep>,
    pub expect: WireConformanceExpectation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WireConformanceStep {
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireConformanceExpectation {
    pub terminal: WireConformanceTerminal,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub frames: Vec<String>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WireConformanceTerminal {
    Success,
    Cancelled,
    Dropped,
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WireConformanceCaseResultReport {
    #[serde(rename = "$schema", default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    pub protocol_version: String,
    pub suite_version: String,
    pub target_name: String,
    pub results: Vec<WireConformanceCaseResult>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WireConformanceCaseResult {
    pub id: String,
    pub outcome: ApiProfileCaseOutcome,
    pub terminal: WireConformanceTerminal,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub observed_frames: Vec<WireConformanceObservedFrame>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence_paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WireConformanceObservedFrame {
    pub direction: WireConformanceFrameDirection,
    pub frame: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp_us: Option<u64>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WireConformanceFrameDirection {
    Sent,
    Received,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConformanceReport {
    pub protocol_version: String,
    pub implementation_name: String,
    pub summary: ReportSummary,
    pub compatibility_matrix: Vec<CompatibilityMatrixEntry>,
    pub cases: Vec<ReportCase>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ReportSummary {
    pub selected_cases: usize,
    pub not_claimed_cases: usize,
    pub informational_cases: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompatibilityMatrixEntry {
    pub feature: String,
    pub required_capabilities: Vec<String>,
    pub summary: ReportSummary,
    pub statuses: ReportStatusSummary,
    pub case_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ReportStatusSummary {
    pub mandatory_cases: usize,
    pub optional_cases: usize,
    pub experimental_cases: usize,
    pub deprecated_cases: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportCase {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<CaseStatus>,
    pub selection: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaseDefinition {
    pub id: String,
    pub layer: CaseLayer,
    pub status: CaseStatus,
    pub feature: String,
    pub required_capabilities: Vec<String>,
    pub description: String,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CaseLayer {
    #[serde(rename = "L0")]
    L0,
    #[serde(rename = "L1")]
    L1,
    #[serde(rename = "L2")]
    L2,
    #[serde(rename = "L3")]
    L3,
    #[serde(rename = "L4")]
    L4,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CaseStatus {
    Mandatory,
    Optional,
    Experimental,
    Deprecated,
}

fn mandatory_case_status() -> CaseStatus {
    CaseStatus::Mandatory
}

fn is_false(value: &bool) -> bool {
    !*value
}

pub fn load_json_file<T>(path: impl AsRef<Path>) -> Result<T, FixtureError>
where
    T: DeserializeOwned,
{
    let path_ref = path.as_ref();
    let display = path_ref.display().to_string();
    let content = fs::read_to_string(path_ref).map_err(|source| FixtureError::Read {
        path: display.clone(),
        source,
    })?;
    serde_json::from_str(&content).map_err(|source| FixtureError::Parse {
        path: display,
        source,
    })
}

pub fn validate_protocol_alignment(
    protocol_manifest: &ProtocolManifest,
    case_manifest: &CaseManifest,
    capability_manifest: Option<&CapabilityManifest>,
    case_manifest_path: impl AsRef<Path>,
    capability_manifest_path: Option<impl AsRef<Path>>,
) -> Result<(), FixtureError> {
    if protocol_manifest.protocol_version != case_manifest.protocol_version {
        return Err(FixtureError::VersionMismatch {
            expected: protocol_manifest.protocol_version.clone(),
            actual: case_manifest.protocol_version.clone(),
            path: case_manifest_path.as_ref().display().to_string(),
        });
    }

    if let (Some(capability_manifest), Some(capability_manifest_path)) =
        (capability_manifest, capability_manifest_path)
    {
        if protocol_manifest.protocol_version != capability_manifest.protocol_version {
            return Err(FixtureError::VersionMismatch {
                expected: protocol_manifest.protocol_version.clone(),
                actual: capability_manifest.protocol_version.clone(),
                path: capability_manifest_path.as_ref().display().to_string(),
            });
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        AdapterCaseResultReport, AdapterExecutionPlan, ApiProfileCapabilityManifest,
        ApiProfileCaseResultReport, ApiProfileExecutionPlan, ApiProfileRecipe,
        ApiProfileSuiteManifest, BenchmarkExecutionPlan, BenchmarkResultReport, CapabilityManifest,
        CaseManifest, CaseStatus, ProtocolManifest, SemanticVectorManifest,
        WireConformanceCaseResultReport, WireConformanceExecutionPlan,
        WireConformanceScenarioManifest, WireConformanceSuiteManifest,
        WireConformanceTargetManifest, build_vector_manifest, load_json_file,
        validate_protocol_alignment, verify_vector_manifest,
    };
    use std::path::PathBuf;

    #[test]
    fn parses_case_manifest_from_json() {
        let manifest: CaseManifest = serde_json::from_str(
            r#"{
                "protocol_version":"nnrp-1-preview3",
                "manifest_name":"mandatory-core",
                "cases":[{
                    "id":"l1.handshake.basic",
                    "layer":"L1",
                    "status":"mandatory",
                    "feature":"handshake.basic",
                    "required_capabilities":["handshake.basic"],
                    "description":"test"
                }]
            }"#,
        )
        .expect("case manifest should parse");

        assert_eq!(manifest.cases.len(), 1);
        assert_eq!(manifest.cases[0].status, CaseStatus::Mandatory);
    }

    #[test]
    fn validates_protocol_version_alignment() {
        let protocol_manifest = ProtocolManifest {
            schema: None,
            protocol_version: "nnrp-1-preview3".to_string(),
            suite_version: "0.1.0".to_string(),
            status: "draft".to_string(),
            case_manifests: vec!["cases/mandatory-core.json".to_string()],
            vector_recipe_manifests: vec![],
            vector_manifests: vec![],
            report_schema: "../../schemas/report.schema.json".to_string(),
        };
        let case_manifest = CaseManifest {
            schema: None,
            protocol_version: "nnrp-1-preview3".to_string(),
            manifest_name: "mandatory-core".to_string(),
            cases: vec![],
        };
        let capability_manifest = CapabilityManifest {
            schema: None,
            implementation_name: "sample".to_string(),
            protocol_version: "nnrp-1-preview3".to_string(),
            supports: vec![],
        };

        validate_protocol_alignment(
            &protocol_manifest,
            &case_manifest,
            Some(&capability_manifest),
            PathBuf::from("cases/mandatory-core.json"),
            Some(PathBuf::from("example-capabilities.json")),
        )
        .expect("versions should align");
    }

    #[test]
    fn loads_protocol_manifest_from_repo_fixture() {
        let manifest: ProtocolManifest = load_json_file(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("..")
                .join("protocol")
                .join("nnrp-1-preview3")
                .join("manifest.json"),
        )
        .expect("protocol manifest should load");

        assert_eq!(manifest.protocol_version, "nnrp-1-preview3");
    }

    #[test]
    fn loads_preview3_case_manifests_from_repo_fixture() {
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

        assert_eq!(protocol_manifest.case_manifests.len(), 8);

        for relative_path in &protocol_manifest.case_manifests {
            let case_manifest: CaseManifest = load_json_file(protocol_root.join(relative_path))
                .unwrap_or_else(|error| {
                    panic!("case manifest {relative_path} should load: {error}")
                });

            validate_protocol_alignment(
                &protocol_manifest,
                &case_manifest,
                Some(&capability_manifest),
                PathBuf::from(relative_path),
                Some(PathBuf::from("example-capabilities.json")),
            )
            .unwrap_or_else(|error| panic!("case manifest {relative_path} should align: {error}"));
        }

        assert!(
            protocol_manifest
                .case_manifests
                .iter()
                .any(|path| path == "cases/l2-binding-driver.json")
        );
    }

    #[test]
    fn loads_preview4_case_manifests_from_repo_fixture() {
        let protocol_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("protocol")
            .join("nnrp-1-preview4");
        let protocol_manifest: ProtocolManifest =
            load_json_file(protocol_root.join("manifest.json"))
                .expect("preview4 protocol manifest should load");
        let capability_manifest: CapabilityManifest =
            load_json_file(protocol_root.join("example-capabilities.json"))
                .expect("preview4 example capability manifest should load");

        assert_eq!(protocol_manifest.protocol_version, "nnrp-1-preview4");
        assert_eq!(protocol_manifest.case_manifests.len(), 4);

        for relative_path in &protocol_manifest.case_manifests {
            let case_manifest: CaseManifest = load_json_file(protocol_root.join(relative_path))
                .unwrap_or_else(|error| {
                    panic!("preview4 case manifest {relative_path} should load: {error}")
                });

            validate_protocol_alignment(
                &protocol_manifest,
                &case_manifest,
                Some(&capability_manifest),
                PathBuf::from(relative_path),
                Some(PathBuf::from("example-capabilities.json")),
            )
            .unwrap_or_else(|error| {
                panic!("preview4 case manifest {relative_path} should align: {error}")
            });
        }
    }

    #[test]
    fn loads_adapter_execution_plan_example() {
        let plan: AdapterExecutionPlan = load_json_file(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("..")
                .join("docs")
                .join("examples")
                .join("adapter-execution-plan.sample.json"),
        )
        .expect("adapter execution plan example should load");

        assert_eq!(plan.protocol_version, "nnrp-1-preview3");
        assert_eq!(plan.cases.len(), 2);
        assert_eq!(
            plan.artifacts.results_path,
            "artifacts/adapter-results.json"
        );
    }

    #[test]
    fn loads_adapter_case_results_example() {
        let report: AdapterCaseResultReport = load_json_file(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("..")
                .join("docs")
                .join("examples")
                .join("adapter-case-results.sample.json"),
        )
        .expect("adapter case results example should load");

        assert_eq!(report.protocol_version, "nnrp-1-preview3");
        assert_eq!(report.results.len(), 2);
        assert_eq!(report.results[1].outcome, super::AdapterCaseOutcome::Fail);
    }

    #[test]
    fn loads_benchmark_execution_plan_example() {
        let plan: BenchmarkExecutionPlan = load_json_file(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("..")
                .join("docs")
                .join("examples")
                .join("benchmark-execution-plan.sample.json"),
        )
        .expect("benchmark execution plan example should load");

        assert_eq!(plan.protocol_version, "nnrp-1-preview3");
        assert_eq!(plan.scenarios.len(), 9);
        assert_eq!(
            plan.artifacts.results_path,
            "artifacts/benchmark-results.json"
        );
    }

    #[test]
    fn loads_benchmark_results_example() {
        let report: BenchmarkResultReport = load_json_file(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("..")
                .join("docs")
                .join("examples")
                .join("benchmark-results.sample.json"),
        )
        .expect("benchmark results example should load");

        assert_eq!(report.protocol_version, "nnrp-1-preview3");
        assert_eq!(report.results.len(), 9);
        assert_eq!(report.environment.os, "linux");
    }

    #[test]
    fn loads_api_profile_capabilities_example() {
        let manifest: ApiProfileCapabilityManifest = load_json_file(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("..")
                .join("docs")
                .join("examples")
                .join("api-profile-capabilities.sample.json"),
        )
        .expect("api profile capabilities example should load");

        assert_eq!(manifest.profile, "openai-compatible");
        assert_eq!(manifest.schema_version, "openai-compatible/1");
        assert_eq!(manifest.operations[0].name, "chat.completions.create");
    }

    #[test]
    fn loads_api_profile_recipe_example() {
        let recipe: ApiProfileRecipe = load_json_file(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("..")
                .join("docs")
                .join("examples")
                .join("api-profile-recipe.sample.json"),
        )
        .expect("api profile recipe example should load");

        assert_eq!(recipe.status, CaseStatus::Mandatory);
        assert_eq!(
            recipe.expect.events[0].event_type,
            "response.output_text.delta"
        );
    }

    #[test]
    fn loads_openai_api_profile_suite_manifest_and_recipes() {
        let profile_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("profiles")
            .join("openai-compatible")
            .join("1");
        let manifest: ApiProfileSuiteManifest = load_json_file(profile_root.join("manifest.json"))
            .expect("api profile suite manifest should load");

        assert_eq!(manifest.profile, "openai-compatible");
        assert_eq!(manifest.schema_version, "openai-compatible/1");
        assert_eq!(manifest.recipe_manifests.len(), 8);

        for recipe_path in &manifest.recipe_manifests {
            let recipe: ApiProfileRecipe = load_json_file(profile_root.join(recipe_path))
                .unwrap_or_else(|error| {
                    panic!("api profile recipe {recipe_path} should load: {error}")
                });
            assert_eq!(recipe.profile, manifest.profile);
            assert_eq!(recipe.schema_version, manifest.schema_version);
            assert!(
                !recipe.required_capabilities.is_empty(),
                "api profile recipe {recipe_path} must declare selection capabilities"
            );
        }
    }

    #[test]
    fn loads_api_profile_execution_plan_example() {
        let plan: ApiProfileExecutionPlan = load_json_file(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("..")
                .join("docs")
                .join("examples")
                .join("api-profile-execution-plan.sample.json"),
        )
        .expect("api profile execution plan example should load");

        assert_eq!(plan.profile, "openai-compatible");
        assert_eq!(plan.cases.len(), 1);
    }

    #[test]
    fn loads_api_profile_case_results_example() {
        let report: ApiProfileCaseResultReport = load_json_file(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("..")
                .join("docs")
                .join("examples")
                .join("api-profile-case-results.sample.json"),
        )
        .expect("api profile case results example should load");

        assert_eq!(report.adapter, "vllm-nnrp-adapter");
        assert_eq!(report.results.len(), 1);
    }

    #[test]
    fn loads_wire_conformance_target_example() {
        let manifest: WireConformanceTargetManifest = load_json_file(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("..")
                .join("docs")
                .join("examples")
                .join("wire-conformance-target.sample.json"),
        )
        .expect("wire conformance target example should load");

        assert_eq!(manifest.protocol_version, "nnrp-1-preview4");
        assert_eq!(manifest.wire_conformance.transports.len(), 2);
        assert!(
            manifest
                .wire_conformance
                .capabilities
                .iter()
                .any(|capability| capability == "control.cancel_abort")
        );
    }

    #[test]
    fn loads_wire_conformance_execution_plan_example() {
        let plan: WireConformanceExecutionPlan = load_json_file(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("..")
                .join("docs")
                .join("examples")
                .join("wire-conformance-execution-plan.sample.json"),
        )
        .expect("wire conformance execution plan example should load");

        assert_eq!(plan.protocol_version, "nnrp-1-preview4");
        assert_eq!(plan.scenarios.len(), 1);
        assert_eq!(plan.scenarios[0].required_capabilities.len(), 3);
    }

    #[test]
    fn loads_wire_conformance_case_results_example() {
        let report: WireConformanceCaseResultReport = load_json_file(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("..")
                .join("docs")
                .join("examples")
                .join("wire-conformance-case-results.sample.json"),
        )
        .expect("wire conformance case results example should load");

        assert_eq!(report.protocol_version, "nnrp-1-preview4");
        assert_eq!(report.results.len(), 1);
        assert_eq!(report.results[0].observed_frames.len(), 2);
    }

    #[test]
    fn loads_preview4_wire_suite_manifest_and_scenarios() {
        let wire_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("wire-conformance")
            .join("nnrp-1-preview4");
        let manifest: WireConformanceSuiteManifest =
            load_json_file(wire_root.join("manifest.json"))
                .expect("wire conformance suite manifest should load");

        assert_eq!(manifest.protocol_version, "nnrp-1-preview4");
        assert_eq!(manifest.scenario_manifests.len(), 1);

        for scenario_path in &manifest.scenario_manifests {
            let scenarios: WireConformanceScenarioManifest =
                load_json_file(wire_root.join(scenario_path)).unwrap_or_else(|error| {
                    panic!("wire scenario manifest {scenario_path} should load: {error}")
                });

            assert_eq!(scenarios.protocol_version, manifest.protocol_version);
            assert!(
                scenarios
                    .scenarios
                    .iter()
                    .any(|scenario| scenario.feature == "control.cancel_abort")
            );
        }
    }

    #[test]
    fn generates_preview3_semantic_vectors_from_repo_fixture() {
        let semantic_manifest: SemanticVectorManifest = load_json_file(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("..")
                .join("protocol")
                .join("nnrp-1-preview3")
                .join("vectors")
                .join("semantic-vectors.json"),
        )
        .expect("preview3 semantic vector manifest should load");

        let vector_manifest =
            build_vector_manifest(&semantic_manifest, "vectors/semantic-vectors.json")
                .expect("preview3 semantic vectors should generate");

        assert_eq!(vector_manifest.protocol_version, "nnrp-1-preview3");
        assert_eq!(vector_manifest.vectors.len(), 23);
        assert_eq!(
            vector_manifest.generated_from.as_deref(),
            Some("vectors/semantic-vectors.json")
        );

        let resumed = vector_manifest
            .vectors
            .iter()
            .find(|vector| vector.name == "preview3.metadata.session_open_ack.resumed")
            .expect("resumed metadata vector should exist");
        assert_eq!(resumed.bytes, 56);

        let operation_flow = vector_manifest
            .vectors
            .iter()
            .find(|vector| vector.name == "preview3.packet.flow_update.operation_pause")
            .expect("operation flow-update vector should exist");
        assert_eq!(operation_flow.kind, "flow_update_packet");
        assert_eq!(operation_flow.bytes, 72);

        let cache_error = vector_manifest
            .vectors
            .iter()
            .find(|vector| vector.name == "preview3.value.cache_error_code.schema_mismatch")
            .expect("schema_mismatch cache-error vector should exist");
        assert_eq!(cache_error.bytes, 4);

        let schema_error = vector_manifest
            .vectors
            .iter()
            .find(|vector| vector.name == "preview3.value.schema_error_code.schema_update_rejected")
            .expect("schema_update_rejected schema-error vector should exist");
        assert_eq!(schema_error.bytes, 4);

        let typed_descriptor = vector_manifest
            .vectors
            .iter()
            .find(|vector| {
                vector.name == "preview3.metadata.typed_payload_descriptor.token_partial"
            })
            .expect("typed payload descriptor vector should exist");
        assert_eq!(typed_descriptor.bytes, 24);
    }

    #[test]
    fn generates_preview4_semantic_vectors_from_repo_fixture() {
        let semantic_manifest: SemanticVectorManifest = load_json_file(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("..")
                .join("protocol")
                .join("nnrp-1-preview4")
                .join("vectors")
                .join("semantic-vectors.json"),
        )
        .expect("preview4 semantic vector manifest should load");

        let vector_manifest =
            build_vector_manifest(&semantic_manifest, "vectors/semantic-vectors.json")
                .expect("preview4 semantic vectors should generate");

        assert_eq!(vector_manifest.protocol_version, "nnrp-1-preview4");
        assert_eq!(vector_manifest.vectors.len(), 16);

        let cancel = vector_manifest
            .vectors
            .iter()
            .find(|vector| vector.name == "preview4.value.control_frame.cancel")
            .expect("cancel control frame vector should exist");
        assert_eq!(cancel.kind, "control_frame");
        assert_eq!(cancel.hex, "01000400");

        let object_delta = vector_manifest
            .vectors
            .iter()
            .find(|vector| vector.name == "preview4.value.object_frame.delta")
            .expect("object delta vector should exist");
        assert_eq!(object_delta.kind, "object_frame");
        assert_eq!(object_delta.bytes, 4);
    }

    #[test]
    fn generates_semantic_vectors_from_preview2_fixture() {
        let semantic_manifest: SemanticVectorManifest = load_json_file(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("..")
                .join("protocol")
                .join("nnrp-1-preview2")
                .join("vectors")
                .join("semantic-vectors.json"),
        )
        .expect("semantic vector manifest should load");

        let vector_manifest =
            build_vector_manifest(&semantic_manifest, "vectors/semantic-vectors.json")
                .expect("semantic vectors should generate");

        assert_eq!(vector_manifest.protocol_version, "nnrp-1-preview2");
        assert_eq!(
            vector_manifest.generated_from.as_deref(),
            Some("vectors/semantic-vectors.json")
        );
        assert_eq!(vector_manifest.vectors.len(), 12);
        assert_eq!(vector_manifest.vectors[1].bytes, 64);
        assert_eq!(
            vector_manifest.vectors[0].hex,
            "4e4e525001001028210000003000000000100000070000000b0000000200000015cd5b0700000000"
        );
    }

    #[test]
    fn generates_semantic_vectors_deterministically() {
        let semantic_manifest: SemanticVectorManifest = load_json_file(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("..")
                .join("protocol")
                .join("nnrp-1-preview2")
                .join("vectors")
                .join("semantic-vectors.json"),
        )
        .expect("semantic vector manifest should load");

        let vector_manifest =
            build_vector_manifest(&semantic_manifest, "vectors/semantic-vectors.json")
                .expect("semantic vectors should generate");

        verify_vector_manifest(
            &semantic_manifest,
            &vector_manifest,
            "vectors/semantic-vectors.json",
        )
        .expect("generated semantic vectors should match semantic recipe deterministically");
    }
}
