use clap::ValueEnum;

/// Standard measurement producers exposed by the CLI and artifact contracts.
#[derive(Clone, Debug, ValueEnum)]
pub(crate) enum MeasureTool {
    All,
    Hotspots,
    Clones,
    #[value(name = "escape-hatches")]
    EscapeHatches,
    #[value(name = "type-health")]
    TypeHealth,
    Reliability,
    #[value(name = "test-quality")]
    TestQuality,
    #[value(name = "architecture-rules")]
    ArchitectureRules,
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
            Self::ArchitectureRules,
            Self::TestQuality,
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

    pub(crate) fn requires_source_facts(&self) -> bool {
        !matches!(self, Self::All | Self::Practices)
    }

    pub(crate) fn requires_correctness_facts(&self) -> bool {
        matches!(
            self,
            Self::Clones | Self::TestQuality | Self::Correctness | Self::CorrectnessRun
        )
    }

    pub(crate) fn requires_semantic_identity(&self) -> bool {
        matches!(
            self,
            Self::ArchitectureRules | Self::Locality | Self::Leverage | Self::Map
        )
    }

    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Hotspots => "hotspots",
            Self::Clones => "clones",
            Self::EscapeHatches => "escape-hatches",
            Self::TypeHealth => "type-health",
            Self::Reliability => "reliability",
            Self::ArchitectureRules => "architecture-rules",
            Self::TestQuality => "test-quality",
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
            Self::ArchitectureRules => "architecture_rules.json",
            Self::TestQuality => "test_quality.json",
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

#[cfg(test)]
mod tests {
    use super::MeasureTool;

    #[test]
    fn measure_all_includes_escape_hatches() {
        assert!(
            MeasureTool::all_tools()
                .iter()
                .any(|tool| matches!(tool, MeasureTool::EscapeHatches))
        );
    }

    #[test]
    fn tool_requirements_select_only_needed_fact_backends() {
        assert!(MeasureTool::Hotspots.requires_source_facts());
        assert!(MeasureTool::Clones.requires_correctness_facts());
        assert!(MeasureTool::Map.requires_semantic_identity());
        assert!(!MeasureTool::Practices.requires_source_facts());
        assert!(!MeasureTool::Hotspots.requires_correctness_facts());
        assert!(!MeasureTool::Coverage.requires_semantic_identity());
    }
}
