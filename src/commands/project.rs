use anyhow::Result;

use super::write_report;
use crate::cli::Commands;
use crate::config::LensConfig;
use crate::{policy, review, sarif};

mod measure;

pub(super) fn run(command: Commands) -> Result<()> {
    match command {
        Commands::Measure(args) => measure::run(args.tool, LensConfig::load(args.config)?),
        Commands::Verify(args) => measure::run(
            crate::tool::MeasureTool::Practices,
            LensConfig::load(args.config)?,
        ),
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
        _ => unreachable!("non-project command routed to project handler"),
    }
}
