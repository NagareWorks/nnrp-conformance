use anyhow::Result;
use clap::{Parser, Subcommand};
use nnrp_conformance_fixtures::{
    CapabilityManifest, CaseManifest, Preview2SemanticVectorManifest, ProtocolManifest,
    VectorManifest, build_preview2_vector_manifest, load_json_file,
    verify_preview2_vector_manifest,
};
use nnrp_conformance_runner::{build_execution_plan, build_execution_plan_for_manifests};
use std::collections::BTreeMap;
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
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Summary {
            protocol,
            cases,
            capabilities,
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
            println!("{}", serde_json::to_string_pretty(&summary)?);
        }
        Command::GenerateVectors { recipe, output } => {
            let semantic_manifest: Preview2SemanticVectorManifest = load_json_file(&recipe)?;
            let generated_from = recipe
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| format!("vectors/{name}"))
                .unwrap_or_else(|| recipe.display().to_string());
            let vector_manifest =
                build_preview2_vector_manifest(&semantic_manifest, &generated_from)?;
            std::fs::write(
                &output,
                format!("{}\n", serde_json::to_string_pretty(&vector_manifest)?),
            )?;
        }
        Command::VerifyVectors { recipe, manifest } => {
            let semantic_manifest: Preview2SemanticVectorManifest = load_json_file(&recipe)?;
            let vector_manifest: VectorManifest = load_json_file(&manifest)?;
            let generated_from = recipe
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| format!("vectors/{name}"))
                .unwrap_or_else(|| recipe.display().to_string());
            verify_preview2_vector_manifest(&semantic_manifest, &vector_manifest, &generated_from)?;
        }
        Command::CompareVectorManifests { expected, actual } => {
            let expected_manifest: VectorManifest = load_json_file(&expected)?;
            let actual_manifest: VectorManifest = load_json_file(&actual)?;
            compare_vector_manifests(&expected_manifest, &actual_manifest)?;
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
