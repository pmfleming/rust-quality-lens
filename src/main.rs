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
mod telemetry;
mod util;
mod validation;

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
    Telemetry {
        #[arg(long)]
        input: PathBuf,
        #[arg(long, default_value_t = 24)]
        max_age_hours: u64,
        #[arg(long)]
        config: Option<PathBuf>,
    },
    Validate {
        #[arg(long = "project", required = true)]
        projects: Vec<String>,
        #[arg(long, default_value = "target/validation")]
        output_dir: PathBuf,
        #[arg(long)]
        include_inferred: bool,
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
    run_command(Cli::parse().command)
}

fn run_command(command: Commands) -> Result<()> {
    match command {
        Commands::Calibrate {
            projects,
            output_dir,
        } => write_report(
            calibration::run(&projects, output_dir)?,
            "calibration report",
        ),
        Commands::Performance {
            baseline,
            no_run,
            config,
        } => write_report(
            performance::run_benchmarks(&LensConfig::load(config)?, baseline, no_run)?,
            "performance evidence",
        ),
        Commands::Outcomes { labels, config } => write_report(
            outcomes::collect(&LensConfig::load(config)?, labels)?,
            "repository outcomes",
        ),
        Commands::Telemetry {
            input,
            max_age_hours,
            config,
        } => write_report(
            telemetry::ingest(&LensConfig::load(config)?, &input, max_age_hours.max(1))?,
            "operational evidence",
        ),
        Commands::Validate {
            projects,
            output_dir,
            include_inferred,
        } => write_report(
            validation::run(&projects, output_dir, include_inferred)?,
            "outcome validation report",
        ),
        command => run_quality_command(command),
    }
}

fn run_quality_command(command: Commands) -> Result<()> {
    match command {
        Commands::Catalog { config } => print_catalog(&LensConfig::load(config)?),
        Commands::Measure { tool, config } => measure(tool, LensConfig::load(config)?),
        Commands::Init { path, force } => {
            write_report(write_default_config(path, force)?, "config")
        }
        Commands::ConfigSchema => print_json(&config_schema()),
        Commands::ArtifactSchema { tool } => print_json(&artifact_schemas(&tool)),
        Commands::Verify { config } => measure(MeasureTool::Practices, LensConfig::load(config)?),
        Commands::Sarif { output, config } => write_report(
            sarif::write(&LensConfig::load(config)?, output)?,
            "SARIF report",
        ),
        Commands::Review {
            changed_since,
            diff_file,
            config,
        } => write_report(
            review::run_review(LensConfig::load(config)?, changed_since, diff_file)?,
            "review data",
        ),
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
        _ => unreachable!("evidence commands are handled by run_command"),
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
