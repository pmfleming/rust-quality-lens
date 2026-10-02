//! Opt-in expansion observations. Never promote these to complete source facts:
//! cfg/target selection, hygiene, generated origins and owner joins are pending.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

use super::{LspClient, absolute_path, analyzer_configuration, analyzer_version, file_uri};
use crate::config::{LensConfig, SemanticIdentityMode};
use crate::contracts::MeasurementConfidence;
use crate::facts::{FileFacts, MacroInvocation, RunContext};
mod generated;

#[derive(Deserialize, Serialize)]
struct SourceSite {
    path: String,
    package_name: String,
    syntax_target: String,
    #[serde(flatten)]
    invocation: MacroInvocation,
}

#[derive(Clone, Copy, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
enum Status {
    Expanded,
    Unavailable,
    Disabled,
}

#[derive(Deserialize, Serialize)]
struct Expansion {
    source: SourceSite,
    status: Status,
    backend_macro: Option<String>,
    expanded_source: Option<String>,
    generated_syntax: Option<generated::GeneratedSyntax>,
    reason: Option<String>,
}

impl Expansion {
    fn unavailable(source: SourceSite, status: Status, reason: &str) -> Self {
        Self {
            source,
            status,
            backend_macro: None,
            expanded_source: None,
            generated_syntax: None,
            reason: Some(reason.to_string()),
        }
    }

    fn apply(&mut self, response: Value) -> Result<()> {
        let name = response["name"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing expansion name"))?;
        let text = response["expansion"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing expansion text"))?;
        ensure!(
            macro_name(name) == macro_name(&self.source.invocation.macro_path),
            "expansion name does not match the requested macro"
        );
        self.generated_syntax = Some(generated::parse(text, &self.source.invocation.kind));
        self.status = Status::Expanded;
        self.backend_macro = Some(name.to_string());
        self.expanded_source = Some(text.to_string()); // Empty expansion is valid, unlike null.
        self.reason = None;
        Ok(())
    }
}

fn macro_name(name: &str) -> &str {
    name.trim_end_matches('!')
        .rsplit("::")
        .next()
        .unwrap_or(name)
}

#[derive(Deserialize, Serialize)]
struct Summary {
    inventory_complete: bool,
    observed_sites: usize,
    expanded_sites: usize,
    response_complete: bool,
    semantic_inventory_complete: bool,
}

#[derive(Deserialize, Serialize)]
struct Report {
    summary: Summary,
    backend: String,
    analyzer_version: Option<String>,
    backend_configuration: Value,
    scope: String,
    expansions: Vec<Expansion>,
    measurement_confidence: MeasurementConfidence,
}

pub(crate) fn produce(config: &LensConfig, context: &RunContext) -> Result<Value> {
    let inventory_complete = inventory_complete(&context.source_facts, context.source_files.len());
    let mut expansions = sites(context);
    let mut profile = analyzer_configuration(config, None);
    profile["cargo"]["features"] = if config.verification.all_features {
        json!("all")
    } else {
        json!(config.verification.features)
    };
    profile["cargo"]["noDefaultFeatures"] = json!(config.verification.no_default_features);
    profile["cargo"]["allTargets"] = json!(config.verification.all_targets);
    let version = expand(config, &profile, &mut expansions);
    let expanded = expansions
        .iter()
        .filter(|e| e.status == Status::Expanded)
        .count();
    let response_complete = inventory_complete && expanded == expansions.len();
    ensure!(
        config.identity_resolution != SemanticIdentityMode::Required
            || (response_complete
                && expansions
                    .iter()
                    .all(|e| e.generated_syntax.as_ref().is_some_and(|s| s.parsed))),
        "required source-expansion responses or syntax are incomplete"
    );
    let mut confidence: MeasurementConfidence =
        serde_json::from_value(context.source_confidence())?;
    confidence.insert("confidence_scope", json!("macro_expansion_observations"));
    confidence.mark_partial("Expansion observations do not yet establish effective cfg, compiler target ownership or compiler-verified generated facts");
    if !inventory_complete {
        confidence
            .mark_partial("Macro call-site inventory is incomplete or missing helper metadata");
    }
    Ok(serde_json::to_value(Report {
        summary: Summary { inventory_complete, observed_sites: expansions.len(), expanded_sites: expanded, response_complete, semantic_inventory_complete: false },
        backend: "rust-analyzer/expandMacro".into(), analyzer_version: version,
        backend_configuration: profile,
        scope: "Explicit item/impl macro call sites only; excludes expression/attribute/derive requests. Source coordinates are invocation anchors, not generated-code spans. Syntax target labels are not compiler ownership evidence. Generated syntax contains observed struct/enum/function declarations, with UTF-8 byte ranges in expanded_source only; cfg, hygiene, nested macros and declaration owners remain unresolved.".into(),
        expansions, measurement_confidence: confidence,
    })?)
}

fn inventory_complete(facts: &[FileFacts], expected: usize) -> bool {
    facts.len() == expected
        && facts.iter().all(|f| {
            f.parse_status == "ok"
                && f.syntax_backend == "syn"
                && f.syntax_error_count == 0
                && f.items.macro_invocations.is_some()
        })
}

fn sites(context: &RunContext) -> Vec<Expansion> {
    let mut sites = context
        .source_facts
        .iter()
        .flat_map(|fact| {
            fact.items
                .macro_invocations
                .iter()
                .flatten()
                .map(|invocation| {
                    Expansion::unavailable(
                        SourceSite {
                            path: fact.path.clone(),
                            package_name: fact.package_name.clone(),
                            syntax_target: fact.target_name.clone(),
                            invocation: invocation.clone(),
                        },
                        Status::Unavailable,
                        "expansion was not evaluated",
                    )
                })
        })
        .collect::<Vec<_>>();
    sites.sort_by(|a, b| {
        (
            &a.source.path,
            a.source.invocation.line,
            a.source.invocation.column_utf16,
        )
            .cmp(&(
                &b.source.path,
                b.source.invocation.line,
                b.source.invocation.column_utf16,
            ))
    });
    sites
}

fn expand(config: &LensConfig, profile: &Value, sites: &mut [Expansion]) -> Option<String> {
    if sites.is_empty() {
        return None;
    }
    if config.identity_resolution == SemanticIdentityMode::Disabled {
        for site in sites {
            site.status = Status::Disabled;
            site.reason = Some("semantic backend explicitly disabled".into());
        }
        return None;
    }
    let version = analyzer_version(config);
    let deadline = Instant::now() + Duration::from_secs(config.identity_timeout_seconds);
    match LspClient::start_configured(config, profile.clone(), deadline) {
        Ok(mut client) => {
            query_sites(config, &mut client, sites, deadline);
            client.shutdown();
        }
        Err(error) => sites
            .iter_mut()
            .for_each(|site| site.reason = Some(error.to_string())),
    }
    version
}

fn query_sites(
    config: &LensConfig,
    client: &mut LspClient,
    sites: &mut [Expansion],
    deadline: Instant,
) {
    for site in sites {
        let uri = file_uri(&absolute_path(config, &site.source.path));
        let invocation = &site.source.invocation;
        let response = client.request("rust-analyzer/expandMacro", json!({
            "textDocument": {"uri": uri},
            "position": {"line": invocation.line.saturating_sub(1), "character": invocation.column_utf16},
        }), deadline);
        match response {
            Ok(value) => {
                if let Err(error) = site.apply(value) {
                    site.reason = Some(error.to_string());
                }
            }
            Err(error) => {
                site.reason = Some(error.to_string());
                break; // No more requests after transport failure/timeout.
            }
        }
    }
}

pub(crate) fn validate(value: &Value) -> Result<()> {
    let report: Report = serde_json::from_value(value.clone())?;
    ensure!(
        report.backend == "rust-analyzer/expandMacro",
        "unrecognized expansion backend"
    );
    ensure!(
        !report.measurement_confidence.is_complete() && !report.summary.semantic_inventory_complete,
        "expansion observations cannot certify semantic inventory"
    );
    let expanded = report
        .expansions
        .iter()
        .filter(|e| e.status == Status::Expanded)
        .count();
    ensure!(
        report.summary.observed_sites == report.expansions.len()
            && report.summary.expanded_sites == expanded,
        "inconsistent expansion counters"
    );
    ensure!(
        report.summary.response_complete
            == (report.summary.inventory_complete && expanded == report.expansions.len()),
        "inconsistent expansion response completeness"
    );
    for expansion in report.expansions {
        validate_expansion(expansion)?;
    }
    Ok(())
}

fn validate_expansion(expansion: Expansion) -> Result<()> {
    ensure!(
        expansion.source.invocation.line > 0
            && !expansion.source.path.is_empty()
            && !expansion.source.invocation.macro_path.is_empty(),
        "missing macro source location"
    );
    match expansion.status {
        Status::Expanded => {
            let name = expansion
                .backend_macro
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("missing backend macro name"))?;
            ensure!(
                macro_name(name) == macro_name(&expansion.source.invocation.macro_path),
                "macro identity mismatch"
            );
            let text = expansion
                .expanded_source
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("missing expanded source"))?;
            ensure!(
                expansion.reason.is_none()
                    && expansion.generated_syntax
                        == Some(generated::parse(text, &expansion.source.invocation.kind)),
                "generated observations do not match expansion text/context"
            );
        }
        _ => ensure!(
            expansion.expanded_source.is_none()
                && expansion.generated_syntax.is_none()
                && expansion.backend_macro.is_none()
                && expansion.reason.as_deref().is_some_and(|r| !r.is_empty()),
            "unavailable expansion masquerades as source"
        ),
    }
    Ok(())
}

#[cfg(test)]
mod tests;
