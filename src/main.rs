use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use std::fs;
use std::path::PathBuf;

mod artifacts;
mod catalog;
mod config;
mod contracts;
mod facts;
mod measurement;
mod producers;
mod review;
mod risk_model;
mod util;

use catalog::print_catalog;
use config::{LensConfig, config_schema, write_default_config};
use contracts::artifact_schemas;
use facts::RunContext;
use util::write_json;

#[derive(Parser)]
#[command(name = "rqlens")]
#[command(about = "Reusable Rust measurement JSON producers")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Measure {
        #[arg(default_value = "all")]
        tool: MeasureTool,
        #[arg(long)]
        config: Option<PathBuf>,
    },
    Catalog {
        #[arg(long)]
        config: Option<PathBuf>,
    },
    Init {
        #[arg(long)]
        path: Option<PathBuf>,
        #[arg(long)]
        force: bool,
    },
    #[command(name = "config-schema")]
    ConfigSchema,
    #[command(name = "artifact-schema")]
    ArtifactSchema {
        #[arg(default_value = "all")]
        tool: MeasureTool,
    },
    Review {
        #[arg(long)]
        changed_since: Option<String>,
        #[arg(long)]
        diff_file: Option<PathBuf>,
        #[arg(long)]
        config: Option<PathBuf>,
    },
}

#[derive(Clone, Debug, ValueEnum)]
enum MeasureTool {
    All,
    Hotspots,
    Clones,
    #[value(name = "escape-hatches")]
    EscapeHatches,
    #[value(name = "type-health")]
    TypeHealth,
    Correctness,
    #[value(name = "correctness-run")]
    CorrectnessRun,
    Locality,
    Leverage,
    Map,
}

impl MeasureTool {
    pub(crate) fn all_tools() -> Vec<Self> {
        vec![
            Self::Hotspots,
            Self::Clones,
            Self::EscapeHatches,
            Self::TypeHealth,
            Self::Correctness,
            Self::CorrectnessRun,
            Self::Locality,
            Self::Leverage,
            Self::Map,
        ]
    }

    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Hotspots => "hotspots",
            Self::Clones => "clones",
            Self::EscapeHatches => "escape-hatches",
            Self::TypeHealth => "type-health",
            Self::Correctness => "correctness",
            Self::CorrectnessRun => "correctness-run",
            Self::Locality => "locality",
            Self::Leverage => "leverage",
            Self::Map => "map",
        }
    }

    pub(crate) fn output_file(&self) -> &'static str {
        match self {
            Self::Hotspots => "hotspots.json",
            Self::Clones => "clones.json",
            Self::EscapeHatches => "rust_escape_hatches.json",
            Self::TypeHealth => "type_health.json",
            Self::Correctness | Self::CorrectnessRun => "correctness_review.json",
            Self::Locality => "locality_metrics.json",
            Self::Leverage => "leverage_metrics.json",
            Self::Map => "map.json",
            Self::All => unreachable!("all has no direct output file"),
        }
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Catalog { config } => print_catalog(&LensConfig::load(config)?),
        Commands::Measure { tool, config } => measure(tool, LensConfig::load(config)?),
        Commands::Init { path, force } => {
            let path = write_default_config(path, force)?;
            println!("Wrote config to {}", path.display());
            Ok(())
        }
        Commands::ConfigSchema => {
            println!("{}", serde_json::to_string_pretty(&config_schema())?);
            Ok(())
        }
        Commands::ArtifactSchema { tool } => {
            println!(
                "{}",
                serde_json::to_string_pretty(&artifact_schemas(&tool))?
            );
            Ok(())
        }
        Commands::Review {
            changed_since,
            diff_file,
            config,
        } => {
            let output = review::run_review(LensConfig::load(config)?, changed_since, diff_file)?;
            println!("Wrote review data to {}", output.display());
            Ok(())
        }
    }
}

fn measure(tool: MeasureTool, config: LensConfig) -> Result<()> {
    fs::create_dir_all(&config.output_dir)?;
    let tools = if matches!(tool, MeasureTool::All) {
        MeasureTool::all_tools()
    } else {
        vec![tool]
    };
    let context = RunContext::new(&config, &tools)?;
    for tool in tools {
        let output = config.output_dir.join(tool.output_file());
        let payload = producers::produce_measurement(&tool, &config, &context)?;
        write_json(&output, &payload)?;
        if matches!(tool, MeasureTool::Correctness | MeasureTool::CorrectnessRun) {
            write_json(
                &config.output_dir.join("test_catalog.json"),
                &payload["tests"],
            )?;
        }
        println!(
            "Wrote {} visibility data to {}",
            tool.name(),
            output.display()
        );
    }
    Ok(())
}
