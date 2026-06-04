use chrono::Utc;
use std::env;

use crate::measurement::{MODEL_ID, MODEL_VERSION};

#[derive(Clone)]
pub(crate) struct Provenance {
    pub(crate) measured_at: String,
    pub(crate) command: String,
    pub(crate) host: String,
}

pub(crate) fn provenance() -> Provenance {
    Provenance {
        measured_at: Utc::now().to_rfc3339(),
        command: env::args().collect::<Vec<_>>().join(" "),
        host: hostname::get()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
    }
}

#[derive(Clone)]
pub(crate) struct ToolScoreMetadata {
    pub(crate) risk_model_id: &'static str,
    pub(crate) risk_model_version: u64,
    pub(crate) risk_calibration: &'static str,
}

pub(crate) fn tool_score_metadata(tool: &'static str) -> ToolScoreMetadata {
    ToolScoreMetadata {
        risk_model_id: MODEL_ID,
        risk_model_version: MODEL_VERSION,
        risk_calibration: tool,
    }
}
