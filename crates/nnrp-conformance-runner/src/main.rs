use anyhow::Result;
use clap::{Parser, Subcommand};
use nnrp_conformance_fixtures::{
    CapabilityManifest, CaseManifest, ProtocolManifest, load_json_file,
};
use nnrp_conformance_runner::build_execution_plan;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "nnrp-conformance-runner")]
#[command(about = "Load a versioned NNRP conformance baseline and print execution-plan summaries")]
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
        cases: PathBuf,
        #[arg(long)]
        capabilities: Option<PathBuf>,
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
            let case_manifest: CaseManifest = load_json_file(&cases)?;
            let capability_manifest = match &capabilities {
                Some(path) => Some(load_json_file::<CapabilityManifest>(path)?),
                None => None,
            };

            let summary = build_execution_plan(
                &protocol_manifest,
                &case_manifest,
                capability_manifest.as_ref(),
                &cases,
                capabilities.as_deref(),
            )?;
            println!("{}", serde_json::to_string_pretty(&summary)?);
        }
    }

    Ok(())
}
