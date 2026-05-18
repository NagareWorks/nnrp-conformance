use nnrp_conformance_fixtures::{
    CapabilityManifest, CaseDefinition, CaseManifest, CaseStatus, ConformanceReport, FixtureError,
    ProtocolManifest, ReportCase, ReportSummary, validate_protocol_alignment,
};
use std::collections::BTreeSet;
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

    let cases = cases
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

            ReportCase {
                id: case.id.clone(),
                feature: Some(case.feature.clone()),
                status: Some(case.status),
                selection: selection.as_str().to_string(),
            }
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
        cases,
    }
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

    Ok(build_execution_plan_from_cases(
        protocol_manifest,
        case_manifests
            .into_iter()
            .flat_map(|(case_manifest, _)| case_manifest.cases.iter()),
        capability_manifest,
    ))
}

#[cfg(test)]
mod tests {
    use super::{build_execution_plan, build_execution_plan_for_manifests};
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

        assert_eq!(summary.summary.selected_cases, 0);
        assert_eq!(summary.summary.not_claimed_cases, 1);
        assert_eq!(summary.cases[0].selection, "not_claimed");
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
    }
}
