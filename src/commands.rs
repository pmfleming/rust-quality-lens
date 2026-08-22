use anyhow::Result;
use std::path::PathBuf;

use crate::cli::Commands;

mod evidence;
mod inspect;
mod project;

pub(crate) fn run(command: Commands) -> Result<()> {
    match command {
        Commands::Calibrate(_)
        | Commands::Performance(_)
        | Commands::Outcomes(_)
        | Commands::Telemetry(_)
        | Commands::Ingest(_)
        | Commands::Validate(_) => evidence::run(command),
        Commands::Catalog(_)
        | Commands::Init(_)
        | Commands::ConfigSchema
        | Commands::ArtifactSchema(_) => inspect::run(command),
        _ => project::run(command),
    }
}

fn print_json(value: &serde_json::Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

fn write_report(path: PathBuf, label: &str) -> Result<()> {
    println!("Wrote {label} to {}", path.display());
    Ok(())
}
