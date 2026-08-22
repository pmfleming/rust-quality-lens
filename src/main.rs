#![recursion_limit = "256"]

//! Command-line interface for Rust quality measurement and verification.

use anyhow::Result;
use clap::Parser;

mod architecture;
mod artifacts;
mod calibration;
mod catalog;
mod cli;
mod command_runner;
mod commands;
mod config;
mod contracts;
mod external;
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
mod toolchain;
mod util;
mod validation;

use cli::Cli;

fn main() -> Result<()> {
    commands::run(Cli::parse().command)
}
