#![recursion_limit = "256"]

//! Command-line interface for Rust quality measurement and verification.

use anyhow::Result;
use clap::Parser;

mod architecture;
mod artifacts;
mod calibration;
mod capability_evidence;
mod catalog;
mod cli;
mod command_runner;
mod commands;
mod config;
mod contracts;
mod external;
mod facts;
mod findings;
mod fingerprint;
#[cfg(test)]
mod labeled_evaluation;
mod measurement;
mod outcomes;
mod performance;
mod policy;
mod producers;
#[cfg(test)]
mod refactor_behavior_tests;
mod review;
mod risk_model;
mod sarif;
mod semantic;
mod telemetry;
#[cfg(test)]
mod test_support;
mod tool;
mod toolchain;
mod util;
mod validation;
mod wire_evidence;

use cli::Cli;

fn main() -> Result<()> {
    commands::run(Cli::parse().command)
}
