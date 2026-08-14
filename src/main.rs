//! Command-line interface for Rust quality measurement and verification.

use anyhow::Result;
use clap::Parser;
use std::fs;
use std::path::PathBuf;

mod artifacts;
mod calibration;
mod catalog;
mod cli;
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
mod tool;
mod util;
mod validation;

use catalog::print_catalog;
use cli::{Cli, Commands};
use config::{LensConfig, config_schema, write_default_config};
use contracts::{artifact_document, artifact_schemas};
use facts::RunContext;
use tool::MeasureTool;
use util::write_json;

fn main() -> Result<()> {
    run_command(Cli::parse().command)
}

fn run_command(command: Commands) -> Result<()> {
    match command {
        Commands::Calibrate(args) => write_report(
            calibration::run(&args.projects, args.output_dir)?,
            "calibration report",
        ),
        Commands::Performance(args) => write_report(
            performance::run_benchmarks(
                &LensConfig::load(args.config)?,
                args.baseline,
                args.no_run,
            )?,
            "performance evidence",
        ),
        Commands::Outcomes(args) => write_report(
            outcomes::collect(&LensConfig::load(args.config)?, args.labels)?,
            "repository outcomes",
        ),
        Commands::Telemetry(args) => write_report(
            telemetry::ingest(
                &LensConfig::load(args.config)?,
                &args.input,
                args.max_age_hours.max(1),
            )?,
            "operational evidence",
        ),
        Commands::Validate(args) => write_report(
            validation::run(&args.projects, args.output_dir, args.include_inferred)?,
            "outcome validation report",
        ),
        command => run_quality_command(command),
    }
}

fn run_quality_command(command: Commands) -> Result<()> {
    match command {
        Commands::Catalog(args) => print_catalog(&LensConfig::load(args.config)?),
        Commands::Measure(args) => measure(args.tool, LensConfig::load(args.config)?),
        Commands::Init(args) => {
            write_report(write_default_config(args.path, args.force)?, "config")
        }
        Commands::ConfigSchema => print_json(&config_schema()),
        Commands::ArtifactSchema(args) => print_json(&artifact_schemas(&args.tool)),
        Commands::Verify(args) => measure(MeasureTool::Practices, LensConfig::load(args.config)?),
        Commands::Sarif(args) => write_report(
            sarif::write(&LensConfig::load(args.config)?, args.output)?,
            "SARIF report",
        ),
        Commands::Review(args) => write_report(
            review::run_review(
                LensConfig::load(args.config)?,
                args.changed_since,
                args.diff_file,
            )?,
            "review data",
        ),
        Commands::Check(args) => policy::run_check(
            &LensConfig::load(args.config)?,
            args.baseline,
            &args.fail_on,
            args.max_total_score,
            args.max_regression,
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
