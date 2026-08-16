use crate::config::{ArchitectureConfig, ArchitectureRule, PolicyRuleLevel};
use crate::facts::ModuleGraph;
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ArchitectureViolation {
    pub(crate) rule_id: String,
    pub(crate) level: PolicyRuleLevel,
    pub(crate) source_module_id: String,
    pub(crate) target_module_id: String,
    pub(crate) message: String,
}

#[derive(Clone, Debug, Default, Serialize)]
pub(crate) struct ArchitectureEvaluation {
    pub(crate) violations: Vec<ArchitectureViolation>,
    pub(crate) unresolved_references: usize,
}

pub(crate) fn evaluate(config: &ArchitectureConfig, graph: &ModuleGraph) -> ArchitectureEvaluation {
    let mut violations = Vec::new();
    for rule in &config.rules {
        evaluate_rule(rule, graph, &mut violations);
    }
    violations.sort_by(|left, right| {
        (
            &left.rule_id,
            &left.source_module_id,
            &left.target_module_id,
        )
            .cmp(&(
                &right.rule_id,
                &right.source_module_id,
                &right.target_module_id,
            ))
    });
    ArchitectureEvaluation {
        violations,
        unresolved_references: graph
            .facts
            .iter()
            .flat_map(|fact| &fact.graph.resolved_dependencies)
            .filter(|dependency| !dependency.status.starts_with("resolved_"))
            .count(),
    }
}

fn evaluate_rule(
    rule: &ArchitectureRule,
    graph: &ModuleGraph,
    violations: &mut Vec<ArchitectureViolation>,
) {
    for (source, targets) in &graph.dependencies {
        if !rule
            .from
            .iter()
            .any(|pattern| module_matches(pattern, source))
        {
            continue;
        }
        violations.extend(targets.iter().filter_map(|target| {
            let reason = violation_reason(rule, target)?;
            Some(ArchitectureViolation {
                rule_id: rule.id.clone(),
                level: rule.level,
                source_module_id: source.clone(),
                target_module_id: target.clone(),
                message: format!("{source} depends on {target}, which {reason}"),
            })
        }));
    }
}

fn violation_reason(rule: &ArchitectureRule, target: &str) -> Option<&'static str> {
    if rule
        .deny
        .iter()
        .any(|pattern| module_matches(pattern, target))
    {
        return Some("matches a denied dependency pattern");
    }
    let allowed = rule
        .allow
        .iter()
        .any(|pattern| module_matches(pattern, target));
    (!rule.allow.is_empty() && !allowed).then_some("is outside the allowed dependency patterns")
}

pub(crate) fn module_matches(pattern: &str, module_id: &str) -> bool {
    let pattern = pattern.split("::").collect::<Vec<_>>();
    let module_id = module_id.split("::").collect::<Vec<_>>();
    match_segments(&pattern, &module_id)
}

fn match_segments(pattern: &[&str], value: &[&str]) -> bool {
    match pattern.split_first() {
        None => value.is_empty(),
        Some((segment, rest)) if *segment == "**" => {
            match_segments(rest, value)
                || (!value.is_empty() && match_segments(pattern, &value[1..]))
        }
        Some((segment, rest)) => {
            !value.is_empty()
                && (*segment == "*" || *segment == value[0])
                && match_segments(rest, &value[1..])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{evaluate, module_matches};
    use crate::config::{ArchitectureConfig, ArchitectureRule, PolicyRuleLevel};
    use crate::facts::{FileFacts, module_graph};

    #[test]
    fn module_patterns_support_single_and_recursive_wildcards() {
        assert!(module_matches(
            "app::*::domain::**",
            "app::lib::domain::model"
        ));
        assert!(module_matches(
            "**::adapters::**",
            "app::lib::adapters::http"
        ));
        assert!(!module_matches("app::*::domain", "app::lib::domain::model"));
        assert!(!module_matches("app::bin::**", "app::lib::domain"));
    }

    #[test]
    fn denied_edges_produce_architecture_violations() {
        let mut domain = FileFacts::test_fact("/app/src/domain.rs", "domain");
        domain.package_name = "app".to_string();
        domain.target_name = "app".to_string();
        domain.module_id = "app::app::domain".to_string();
        domain
            .graph
            .dependencies
            .push("crate::infrastructure".to_string());
        let mut infrastructure =
            FileFacts::test_fact("/app/src/infrastructure.rs", "infrastructure");
        infrastructure.package_name = "app".to_string();
        infrastructure.target_name = "app".to_string();
        infrastructure.module_id = "app::app::infrastructure".to_string();
        let graph = module_graph(&[domain, infrastructure]);
        let evaluation = evaluate(
            &ArchitectureConfig {
                rules: vec![ArchitectureRule {
                    id: "domain-boundary".to_string(),
                    from: vec!["app::app::domain".to_string()],
                    allow: Vec::new(),
                    deny: vec!["app::app::infrastructure".to_string()],
                    level: PolicyRuleLevel::Error,
                }],
            },
            &graph,
        );
        assert_eq!(evaluation.violations.len(), 1);
        assert_eq!(evaluation.violations[0].rule_id, "domain-boundary");
    }
}
