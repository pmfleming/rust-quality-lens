use anyhow::Result;

use super::write_report;
use crate::cli::Commands;
use crate::config::LensConfig;
use crate::{calibration, external, outcomes, performance, telemetry, validation};

pub(super) fn run(command: Commands) -> Result<()> {
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
        Commands::Ingest(args) => write_report(
            external::ingest(&LensConfig::load(args.config)?, &args.adapter, &args.input)?,
            "external evidence",
        ),
        Commands::Validate(args) => write_report(
            validation::run(&args.projects, args.output_dir, args.include_inferred)?,
            "outcome validation report",
        ),
        _ => unreachable!("non-evidence command routed to evidence handler"),
    }
}
