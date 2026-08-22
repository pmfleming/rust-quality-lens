use anyhow::Result;

use super::{print_json, write_report};
use crate::catalog::print_catalog;
use crate::cli::Commands;
use crate::config::{LensConfig, config_schema, write_default_config};
use crate::contracts::artifact_schemas;

pub(super) fn run(command: Commands) -> Result<()> {
    match command {
        Commands::Catalog(args) => print_catalog(&LensConfig::load(args.config)?),
        Commands::Init(args) => {
            write_report(write_default_config(args.path, args.force)?, "config")
        }
        Commands::ConfigSchema => print_json(&config_schema()),
        Commands::ArtifactSchema(args) => print_json(&artifact_schemas(&args.tool)),
        _ => unreachable!("non-inspection command routed to inspection handler"),
    }
}
