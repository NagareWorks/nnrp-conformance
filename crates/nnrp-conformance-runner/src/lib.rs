use nnrp_conformance_fixtures::{
    CapabilityManifest, CaseManifest, CaseStatus, FixtureError, ProtocolManifest,
    validate_protocol_alignment,
};
use serde::Serialize;
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CaseSelection {
    Selected,
    NotClaimed,
    Informational,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlannedCase {
    pub id: String,
    pub feature: String,
    pub status: CaseStatus,
    pub selection: CaseSelection,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExecutionPlanSummary {
    pub protocol_version: String,
    pub implementation_name: String,
    pub declared_capabilities: Vec<String>,
    pub selected_cases: usize,
    pub not_claimed_cases: usize,
    pub informational_cases: usize,
    pub cases: Vec<PlannedCase>,
}

pub fn build_execution_plan(
    protocol_manifest: &ProtocolManifest,
    case_manifest: &CaseManifest,
    capability_manifest: Option<&CapabilityManifest>,
    case_manifest_path: &std::path::Path,
    capability_manifest_path: Option<&std::path::Path>,
) -> Result<ExecutionPlanSummary, FixtureError> {
    validate_protocol_alignment(
        protocol_manifest,
        case_manifest,
        capability_manifest,
        case_manifest_path,
        capability_manifest_path,
    )?;

    let declared_capabilities = capability_manifest
        .map(|manifest| manifest.supports.iter().cloned().collect::<BTreeSet<_>>())
        .unwrap_or_default();
    let implementation_name = capability_manifest
        .map(|manifest| manifest.implementation_name.clone())
        .unwrap_or_else(|| "unclaimed".to_string());

    let mut selected_cases = 0;
    let mut not_claimed_cases = 0;
    let mut informational_cases = 0;

    let cases = case_manifest
        .cases
        .iter()
        .map(|case| {
            let capabilities_satisfied = case
                .required_capabilities
                .iter()
                .all(|capability| declared_capabilities.contains(capability));

            let selection = match case.status {
                CaseStatus::Mandatory => {
                    if capabilities_satisfied {
                        selected_cases += 1;
                        CaseSelection::Selected
                    } else {
                        not_claimed_cases += 1;
                        CaseSelection::NotClaimed
                    }
                }
                CaseStatus::Optional => {
                    if capabilities_satisfied {
                        selected_cases += 1;
                        CaseSelection::Selected
                    } else {
                        not_claimed_cases += 1;
                        CaseSelection::NotClaimed
                    }
                }
                CaseStatus::Experimental | CaseStatus::Deprecated => {
                    informational_cases += 1;
                    CaseSelection::Informational
                }
            };

            PlannedCase {
                id: case.id.clone(),
                feature: case.feature.clone(),
                status: case.status,
                selection,
            }
        })
        .collect();

    Ok(ExecutionPlanSummary {
        protocol_version: protocol_manifest.protocol_version.clone(),
        implementation_name,
        declared_capabilities: declared_capabilities.into_iter().collect(),
        selected_cases,
        not_claimed_cases,
        informational_cases,
        cases,
    })
}

#[cfg(test)]
mod tests {
    use super::{CaseSelection, build_execution_plan};
    use nnrp_conformance_fixtures::{
        CapabilityManifest, CaseDefinition, CaseLayer, CaseManifest, CaseStatus, ProtocolManifest,
    };
    use std::path::Path;

    #[test]
    fn marks_unclaimed_capabilities_as_not_claimed() {
        let protocol_manifest = ProtocolManifest {
            schema: None,
            protocol_version: "nnrp-1-preview3".to_string(),
            suite_version: "0.1.0".to_string(),
            status: "draft".to_string(),
            case_manifests: vec![],
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
            supports: vec!["handshake.basic".to_string()],
        };

        let summary = build_execution_plan(
            &protocol_manifest,
            &case_manifest,
            Some(&capability_manifest),
            Path::new("cases/mandatory-core.json"),
            Some(Path::new("example-capabilities.json")),
        )
        .expect("execution plan should build");

        assert_eq!(summary.selected_cases, 0);
        assert_eq!(summary.not_claimed_cases, 1);
        assert_eq!(summary.cases[0].selection, CaseSelection::NotClaimed);
    }

    #[test]
    fn keeps_experimental_cases_informational() {
        let protocol_manifest = ProtocolManifest {
            schema: None,
            protocol_version: "nnrp-1-preview3".to_string(),
            suite_version: "0.1.0".to_string(),
            status: "draft".to_string(),
            case_manifests: vec![],
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

        assert_eq!(summary.informational_cases, 1);
        assert_eq!(summary.cases[0].selection, CaseSelection::Informational);
    }
}
