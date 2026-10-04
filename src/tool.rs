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
    #[value(name = "semantic-api")]
    SemanticApi,
    /// Opt-in expansion observations; deliberately not part of measure all.
    #[value(name = "source-expansion")]
    SourceExpansion,
    Correctness,
    #[value(name = "correctness-run")]
    CorrectnessRun,
    Locality,
    Leverage,
    #[value(name = "module-cohesion")]
    ModuleCohesion,
    Map,
    Coverage,
    #[value(name = "function-risk")]
    FunctionRisk,
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
            Self::SemanticApi,
            Self::TypeHealth,
            Self::Coverage,
            Self::FunctionRisk,
            Self::CorrectnessRun,
            Self::Locality,
            Self::Leverage,
            Self::ModuleCohesion,
            Self::Map,
        ]
    }

    pub(crate) fn schema_tools() -> Vec<Self> {
        let mut tools = Self::all_tools();
        tools.push(Self::Practices);
        tools.push(Self::SourceExpansion);
        tools
    }

    pub(crate) fn requires_source_facts(&self) -> bool {
        !matches!(self, Self::All | Self::Practices | Self::SemanticApi)
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
            Self::ArchitectureRules
                | Self::Locality
                | Self::Leverage
                | Self::ModuleCohesion
                | Self::Map
        )
    }

    pub(crate) fn name(&self) -> String {
        // Use the CLI vocabulary rather than maintaining a second name table.
        self.to_possible_value()
            .map_or_else(|| format!("{self:?}"), |value| value.get_name().to_string())
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
            Self::SemanticApi => "semantic_api.json",
            Self::SourceExpansion => "source_expansion.json",
            Self::Correctness | Self::CorrectnessRun => "correctness_review.json",
            Self::Locality => "locality_metrics.json",
            Self::Leverage => "leverage_metrics.json",
            Self::ModuleCohesion => "module_cohesion.json",
            Self::Map => "map.json",
            Self::Coverage => "coverage.json",
            Self::FunctionRisk => "function_risk.json",
            Self::Practices => "rust_practices.json",
            Self::All => unreachable!("all has no direct output file"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::MeasureTool;

    #[test]
    fn tool_scope_and_backend_requirements_are_explicit() {
        assert!(
            MeasureTool::all_tools()
                .iter()
                .any(|tool| matches!(tool, MeasureTool::EscapeHatches))
        );
        assert!(MeasureTool::Hotspots.requires_source_facts());
        assert!(MeasureTool::Clones.requires_correctness_facts());
        assert!(MeasureTool::Map.requires_semantic_identity());
        assert!(!MeasureTool::Practices.requires_source_facts());
        assert!(!MeasureTool::Hotspots.requires_correctness_facts());
        assert!(!MeasureTool::Coverage.requires_semantic_identity());
    }
}
