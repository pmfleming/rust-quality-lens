use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

use crate::policy::FailPolicy;
use crate::tool::MeasureTool;

#[derive(Parser)]
#[command(name = "rqlens", version)]
#[command(about = "Reusable Rust measurement JSON producers")]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Commands,
}

#[derive(Subcommand)]
pub(crate) enum Commands {
    Measure(MeasureArgs),
    Catalog(ConfigArgs),
    Init(InitArgs),
    #[command(name = "config-schema")]
    ConfigSchema,
    #[command(name = "artifact-schema")]
    ArtifactSchema(ArtifactSchemaArgs),
    Verify(ConfigArgs),
    Sarif(SarifArgs),
    Review(ReviewArgs),
    Check(CheckArgs),
    Calibrate(ProjectReportArgs),
    Performance(PerformanceArgs),
    Outcomes(OutcomesArgs),
    Telemetry(TelemetryArgs),
    Ingest(IngestArgs),
    Validate(ValidationArgs),
}

#[derive(Args)]
pub(crate) struct ConfigArgs {
    #[arg(long)]
    pub(crate) config: Option<PathBuf>,
}

#[derive(Args)]
pub(crate) struct MeasureArgs {
    #[arg(default_value = "all")]
    pub(crate) tool: MeasureTool,
    #[arg(long)]
    pub(crate) config: Option<PathBuf>,
}

#[derive(Args)]
pub(crate) struct InitArgs {
    #[arg(long)]
    pub(crate) path: Option<PathBuf>,
    #[arg(long)]
    pub(crate) force: bool,
}

#[derive(Args)]
pub(crate) struct ArtifactSchemaArgs {
    #[arg(default_value = "all")]
    pub(crate) tool: MeasureTool,
}

#[derive(Args)]
pub(crate) struct SarifArgs {
    #[arg(long)]
    pub(crate) output: Option<PathBuf>,
    #[arg(long)]
    pub(crate) config: Option<PathBuf>,
}

#[derive(Args)]
pub(crate) struct ReviewArgs {
    #[arg(long)]
    pub(crate) changed_since: Option<String>,
    #[arg(long)]
    pub(crate) diff_file: Option<PathBuf>,
    #[arg(long)]
    pub(crate) config: Option<PathBuf>,
}

#[derive(Args)]
pub(crate) struct CheckArgs {
    #[arg(long)]
    pub(crate) baseline: Option<PathBuf>,
    #[arg(long, value_enum)]
    pub(crate) fail_on: Vec<FailPolicy>,
    #[arg(long, default_value_t = 600.0)]
    pub(crate) max_total_score: f64,
    #[arg(long, default_value_t = 0.0)]
    pub(crate) max_regression: f64,
    #[arg(long)]
    pub(crate) config: Option<PathBuf>,
}

#[derive(Args)]
pub(crate) struct ProjectReportArgs {
    #[arg(long = "project", required = true)]
    pub(crate) projects: Vec<String>,
    #[arg(long, default_value = "target/calibration")]
    pub(crate) output_dir: PathBuf,
}

#[derive(Args)]
pub(crate) struct PerformanceArgs {
    #[arg(long)]
    pub(crate) baseline: Option<PathBuf>,
    #[arg(long)]
    pub(crate) no_run: bool,
    #[arg(long)]
    pub(crate) config: Option<PathBuf>,
}

#[derive(Args)]
pub(crate) struct OutcomesArgs {
    #[arg(long)]
    pub(crate) labels: Option<PathBuf>,
    #[arg(long)]
    pub(crate) config: Option<PathBuf>,
}

#[derive(Args)]
pub(crate) struct TelemetryArgs {
    #[arg(long)]
    pub(crate) input: PathBuf,
    #[arg(long, default_value_t = 24)]
    pub(crate) max_age_hours: u64,
    #[arg(long)]
    pub(crate) config: Option<PathBuf>,
}

#[derive(Args)]
pub(crate) struct IngestArgs {
    #[arg(long)]
    pub(crate) adapter: String,
    #[arg(long)]
    pub(crate) input: PathBuf,
    #[arg(long)]
    pub(crate) config: Option<PathBuf>,
}

#[derive(Args)]
pub(crate) struct ValidationArgs {
    #[arg(long = "project", required = true)]
    pub(crate) projects: Vec<String>,
    #[arg(long, default_value = "target/validation")]
    pub(crate) output_dir: PathBuf,
    #[arg(long)]
    pub(crate) include_inferred: bool,
}
