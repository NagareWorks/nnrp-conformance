use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use thiserror::Error;

mod preview2_vectors;

pub use preview2_vectors::{
    GoldenVector, Preview2SemanticVectorManifest, VectorManifest, build_preview2_vector_manifest,
    verify_preview2_vector_manifest,
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
pub struct ConformanceReport {
    pub protocol_version: String,
    pub implementation_name: String,
    pub summary: ReportSummary,
    pub cases: Vec<ReportCase>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportSummary {
    pub selected_cases: usize,
    pub not_claimed_cases: usize,
    pub informational_cases: usize,
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
        CapabilityManifest, CaseManifest, CaseStatus, Preview2SemanticVectorManifest,
        ProtocolManifest, build_preview2_vector_manifest, load_json_file,
        validate_protocol_alignment, verify_preview2_vector_manifest,
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
    fn generates_preview2_vectors_from_semantic_fixture() {
        let semantic_manifest: Preview2SemanticVectorManifest = load_json_file(
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
            build_preview2_vector_manifest(&semantic_manifest, "vectors/semantic-vectors.json")
                .expect("preview2 vectors should generate");

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
    fn generates_preview2_vectors_deterministically() {
        let semantic_manifest: Preview2SemanticVectorManifest = load_json_file(
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
            build_preview2_vector_manifest(&semantic_manifest, "vectors/semantic-vectors.json")
                .expect("preview2 vectors should generate");

        verify_preview2_vector_manifest(
            &semantic_manifest,
            &vector_manifest,
            "vectors/semantic-vectors.json",
        )
        .expect("generated preview2 vectors should match semantic recipe deterministically");
    }
}
