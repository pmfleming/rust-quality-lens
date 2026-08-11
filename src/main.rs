//! Command-line interface for Rust quality measurement and verification.

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use std::fs;
use std::path::PathBuf;

mod artifacts;
mod calibration;
mod catalog;
mod command_runner;
mod config;
mod contracts;
mod facts;
mod measurement;
mod outcomes;
mod performance;
mod policy;
mod producers;
mod review;
mod risk_model;
mod sarif;
mod semantic;
mod util;

use catalog::print_catalog;
use config::{LensConfig, config_schema, write_default_config};
use contracts::{artifact_document, artifact_schemas};
use facts::RunContext;
use util::write_json;

#[derive(Parser)]
#[command(name = "rqlens", version)]
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
    Verify {
        #[arg(long)]
        config: Option<PathBuf>,
    },
    Sarif {
        #[arg(long)]
        output: Option<PathBuf>,
        #[arg(long)]
        config: Option<PathBuf>,
    },
    Review {
        #[arg(long)]
        changed_since: Option<String>,
        #[arg(long)]
        diff_file: Option<PathBuf>,
        #[arg(long)]
        config: Option<PathBuf>,
    },
    Check {
        #[arg(long)]
        baseline: Option<PathBuf>,
        #[arg(long, value_enum)]
        fail_on: Vec<policy::FailPolicy>,
        #[arg(long, default_value_t = 600.0)]
        max_total_score: f64,
        #[arg(long, default_value_t = 0.0)]
        max_regression: f64,
        #[arg(long)]
        config: Option<PathBuf>,
    },
    Calibrate {
        #[arg(long = "project", required = true)]
        projects: Vec<String>,
        #[arg(long, default_value = "target/calibration")]
        output_dir: PathBuf,
    },
    Performance {
        #[arg(long)]
        baseline: Option<PathBuf>,
        #[arg(long)]
        no_run: bool,
        #[arg(long)]
        config: Option<PathBuf>,
    },
    Outcomes {
        #[arg(long)]
        labels: Option<PathBuf>,
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
    Reliability,
    #[value(name = "api-health")]
    ApiHealth,
    Correctness,
    #[value(name = "correctness-run")]
    CorrectnessRun,
    Locality,
    Leverage,
    Map,
    Coverage,
    Practices,
}

impl MeasureTool {
    pub(crate) fn all_tools() -> Vec<Self> {
        vec![
            Self::Hotspots,
            Self::Clones,
            Self::EscapeHatches,
            Self::Reliability,
            Self::ApiHealth,
            Self::TypeHealth,
            Self::Coverage,
            Self::CorrectnessRun,
            Self::Locality,
            Self::Leverage,
            Self::Map,
        ]
    }

    pub(crate) fn schema_tools() -> Vec<Self> {
        let mut tools = Self::all_tools();
        tools.push(Self::Practices);
        tools
    }

    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Hotspots => "hotspots",
            Self::Clones => "clones",
            Self::EscapeHatches => "escape-hatches",
            Self::TypeHealth => "type-health",
            Self::Reliability => "reliability",
            Self::ApiHealth => "api-health",
            Self::Correctness => "correctness",
            Self::CorrectnessRun => "correctness-run",
            Self::Locality => "locality",
            Self::Leverage => "leverage",
            Self::Map => "map",
            Self::Coverage => "coverage",
            Self::Practices => "practices",
        }
    }

    pub(crate) fn output_file(&self) -> &'static str {
        match self {
            Self::Hotspots => "hotspots.json",
            Self::Clones => "clones.json",
            Self::EscapeHatches => "rust_escape_hatches.json",
            Self::TypeHealth => "type_health.json",
            Self::Reliability => "reliability_findings.json",
            Self::ApiHealth => "api_health.json",
            Self::Correctness | Self::CorrectnessRun => "correctness_review.json",
            Self::Locality => "locality_metrics.json",
            Self::Leverage => "leverage_metrics.json",
            Self::Map => "map.json",
            Self::Coverage => "coverage.json",
            Self::Practices => "rust_practices.json",
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
        Commands::Verify { config } => measure(MeasureTool::Practices, LensConfig::load(config)?),
        Commands::Sarif { output, config } => {
            let output = sarif::write(&LensConfig::load(config)?, output)?;
            println!("Wrote SARIF report to {}", output.display());
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
        Commands::Check {
            baseline,
            fail_on,
            max_total_score,
            max_regression,
            config,
        } => policy::run_check(
            &LensConfig::load(config)?,
            baseline,
            &fail_on,
            max_total_score,
            max_regression,
        ),
        Commands::Calibrate {
            projects,
            output_dir,
        } => {
            let output = calibration::run(&projects, output_dir)?;
            println!("Wrote calibration report to {}", output.display());
            Ok(())
        }
        Commands::Performance {
            baseline,
            no_run,
            config,
        } => {
            let output = performance::run_benchmarks(&LensConfig::load(config)?, baseline, no_run)?;
            println!("Wrote performance evidence to {}", output.display());
            Ok(())
        }
        Commands::Outcomes { labels, config } => {
            let output = outcomes::collect(&LensConfig::load(config)?, labels)?;
            println!("Wrote repository outcomes to {}", output.display());
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
        if matches!(tool, MeasureTool::Correctness | MeasureTool::CorrectnessRun) {
            write_json(
                &config.output_dir.join("test_catalog.json"),
                &payload["tests"],
            )?;
        }
        write_json(
            &output,
            &artifact_document(&tool, &config, &context, payload),
        )?;
        println!(
            "Wrote {} visibility data to {}",
            tool.name(),
            output.display()
        );
    }
    Ok(())
}
