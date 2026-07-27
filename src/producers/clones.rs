use anyhow::Result;
use regex::Regex;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;

use crate::config::LensConfig;
use crate::facts::{AstCloneFact, FileFacts, RunContext, ast_clone_facts_for_paths};
use crate::measurement::{
    MODEL_ID, MODEL_VERSION, project_relative_path, source_confidence,
    source_scan_confidence_with_errors,
};
use crate::util::{iter_rust_files, normalize_slashes, round2, stable_hash};

struct CloneInstance {
    file_path: String,
    start_line: usize,
    end_line: usize,
    snippet: String,
}

const CLONE_WINDOW_TOKENS: usize = 50;
const MIN_CLONE_LINE_SPAN: usize = 5;
const MIN_RESPONSIBILITY_LINES: usize = 8;

pub(super) fn produce(config: &LensConfig, context: &RunContext) -> Result<Value> {
    let mut rows = token_clone_rows(config)?;
    let source_fact_confidence = source_confidence(&config.source_roots, &context.source_facts);
    rows.extend(ast_clone_rows(
        config,
        &ast_clone_facts_for_paths(config, &config.source_roots)?,
        source_fact_confidence.clone(),
    ));
    rows.extend(module_responsibility_rows(
        &context.source_facts,
        source_fact_confidence,
    ));
    rows.extend(test_ast_clone_rows(
        config,
        &context.correctness_facts,
        &ast_clone_facts_for_paths(config, &context.correctness_paths)?,
        source_confidence(&context.correctness_paths, &context.correctness_facts),
    ));
    rows.sort_by(|a, b| {
        crate::artifacts::json_f64(b, "score").total_cmp(&crate::artifacts::json_f64(a, "score"))
    });
    Ok(Value::Array(rows))
}

fn token_clone_rows(config: &LensConfig) -> Result<Vec<Value>> {
    let token_re = Regex::new(
        r"[A-Za-z_][A-Za-z0-9_]*|\d+|::|->|=>|==|!=|<=|>=|&&|\|\||[{}\(\)\[\];,.:+\-*/%&|^!<>=?]",
    )?;
    let (windows, read_errors) = collect_clone_windows(config, &token_re);
    let confidence = source_scan_confidence_with_errors(&config.source_roots, read_errors);
    Ok(coalesce_clone_groups(windows)
        .into_iter()
        .filter_map(|(key, instances)| token_clone_row(&key, instances, &confidence))
        .collect())
}

fn collect_clone_windows(
    config: &LensConfig,
    token_re: &Regex,
) -> (HashMap<String, Vec<CloneInstance>>, Vec<String>) {
    let mut windows = HashMap::new();
    let mut read_errors = Vec::new();
    for path in iter_rust_files(&config.source_roots) {
        match fs::read_to_string(&path) {
            Ok(text) => add_file_windows(&mut windows, &path, &text, token_re),
            Err(error) => read_errors.push(format!("{}: {error}", normalize_slashes(path))),
        }
    }
    (windows, read_errors)
}

fn add_file_windows(
    windows: &mut HashMap<String, Vec<CloneInstance>>,
    path: &std::path::Path,
    text: &str,
    token_re: &Regex,
) {
    let lines = text.lines().collect::<Vec<_>>();
    let tokens = normalized_tokens(&lines, token_re);
    for window in tokens.windows(CLONE_WINDOW_TOKENS) {
        let Some((key, start_line, end_line)) = clone_window(window) else {
            continue;
        };
        windows.entry(key).or_default().push(CloneInstance {
            file_path: normalize_slashes(path),
            start_line,
            end_line,
            snippet: lines[start_line - 1..end_line.min(lines.len())].join("\n"),
        });
    }
}

fn normalized_tokens(lines: &[&str], token_re: &Regex) -> Vec<(String, usize)> {
    lines
        .iter()
        .enumerate()
        .flat_map(|(line, text)| {
            token_re
                .find_iter(text)
                .map(move |token| (normalize_token(token.as_str()), line + 1))
        })
        .collect()
}

fn normalize_token(token: &str) -> String {
    const KEYWORDS: &[&str] = &[
        "fn", "let", "pub", "struct", "enum", "impl", "if", "match", "for", "while", "loop", "use",
        "mod", "return", "Self", "self",
    ];
    let is_identifier = token
        .chars()
        .next()
        .is_some_and(|character| character.is_ascii_alphabetic() || character == '_');
    if is_identifier && !KEYWORDS.contains(&token) {
        "ID".to_string()
    } else if token.chars().all(|character| character.is_ascii_digit()) {
        "LIT".to_string()
    } else {
        token.to_string()
    }
}

fn clone_window(window: &[(String, usize)]) -> Option<(String, usize, usize)> {
    let (first, last) = (window.first()?, window.last()?);
    let (start_line, end_line) = (first.1, last.1);
    (end_line.saturating_sub(start_line) + 1 >= 3).then(|| {
        let key = window
            .iter()
            .map(|(token, _)| token.as_str())
            .collect::<Vec<_>>()
            .join("|");
        (key, start_line, end_line)
    })
}

fn coalesce_clone_groups(
    windows: HashMap<String, Vec<CloneInstance>>,
) -> Vec<(String, Vec<CloneInstance>)> {
    let mut groups: Vec<(String, Vec<CloneInstance>)> = Vec::new();
    let mut windows = windows.into_iter().collect::<Vec<_>>();
    windows.sort_by(|left, right| left.0.cmp(&right.0));
    for (key, instances) in windows {
        let instances = non_overlapping_clone_instances(instances);
        if instances.len() < 2 {
            continue;
        }
        if let Some((_, existing)) = groups
            .iter_mut()
            .find(|(_, existing)| clone_groups_overlap(existing, &instances))
        {
            merge_clone_instances(existing, instances);
        } else {
            groups.push((key, instances));
        }
    }
    groups
}

fn clone_groups_overlap(left: &[CloneInstance], right: &[CloneInstance]) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            left.file_path == right.file_path
                && left.start_line <= right.end_line.saturating_add(1)
                && right.start_line <= left.end_line.saturating_add(1)
        })
}

fn merge_clone_instances(existing: &mut [CloneInstance], incoming: Vec<CloneInstance>) {
    for (existing, incoming) in existing.iter_mut().zip(incoming) {
        existing.start_line = existing.start_line.min(incoming.start_line);
        existing.end_line = existing.end_line.max(incoming.end_line);
        existing.snippet =
            snippet_for_range(&existing.file_path, existing.start_line, existing.end_line);
    }
}

fn token_clone_row(key: &str, instances: Vec<CloneInstance>, confidence: &Value) -> Option<Value> {
    if instances.len() < 2 {
        return None;
    }
    let file_count = instances
        .iter()
        .map(|instance| &instance.file_path)
        .collect::<HashSet<_>>()
        .len();
    let max_line_span = instances
        .iter()
        .map(|instance| instance.end_line - instance.start_line + 1)
        .max()
        .unwrap_or_default();
    if max_line_span < MIN_CLONE_LINE_SPAN {
        return None;
    }
    let score = round2(instances.len() as f64 * 5.0);
    Some(json!({
        "engine": "token",
        "hash": stable_hash(key),
        "token_count": CLONE_WINDOW_TOKENS,
        "instance_count": instances.len(),
        "file_count": file_count,
        "max_line_span": max_line_span,
        "score": score,
        "signals": if file_count >= 2 { format!("cross-file x{file_count}") } else { "same-file repeat".to_string() },
        "score_components": [{"signal": "instances", "raw": instances.len(), "weight": 5.0, "contribution": score}],
        "risk_model_id": MODEL_ID,
        "risk_model_version": MODEL_VERSION,
        "risk_calibration": "clones_token",
        "instances": instances.into_iter().map(|instance| json!({
            "file_path": instance.file_path,
            "start_line": instance.start_line,
            "end_line": instance.end_line,
            "snippet": instance.snippet,
        })).collect::<Vec<_>>(),
        "measurement_confidence": confidence,
    }))
}

fn ast_clone_rows(config: &LensConfig, facts: &[AstCloneFact], confidence: Value) -> Vec<Value> {
    let mut groups: BTreeMap<String, Vec<&AstCloneFact>> = BTreeMap::new();
    for fact in facts {
        groups.entry(fact.ast_hash.clone()).or_default().push(fact);
    }
    groups
        .into_iter()
        .filter_map(|(hash, facts)| ast_clone_row(config, hash, facts, &confidence))
        .collect()
}

fn ast_clone_row(
    config: &LensConfig,
    hash: String,
    facts: Vec<&AstCloneFact>,
    confidence: &Value,
) -> Option<Value> {
    if facts.len() < 2 {
        return None;
    }
    let file_count = facts
        .iter()
        .map(|fact| normalize_slashes(&fact.file))
        .collect::<HashSet<_>>()
        .len();
    let max_line_span = facts
        .iter()
        .map(|fact| fact.end_line.saturating_sub(fact.start_line) + 1)
        .max()
        .unwrap_or(0);
    if max_line_span < MIN_CLONE_LINE_SPAN {
        return None;
    }
    let max_node_count = facts.iter().map(|fact| fact.node_count).max().unwrap_or(0);
    let score =
        round2(facts.len() as f64 * max_line_span as f64 * ast_cross_file_factor(file_count));
    Some(json!({
        "engine": "ast",
        "hash": hash,
        "node_count": max_node_count,
        "instance_count": facts.len(),
        "file_count": file_count,
        "max_line_span": max_line_span,
        "score": score,
        "signals": if file_count >= 2 { format!("ast cross-file x{file_count}") } else { "ast same-file repeat".to_string() },
        "score_components": [
            {"signal": "instances_x_span", "raw": {"instances": facts.len(), "max_line_span": max_line_span}, "contribution": round2(facts.len() as f64 * max_line_span as f64)},
            {"signal": "cross_file_factor", "raw": file_count, "contribution": round2(score - facts.len() as f64 * max_line_span as f64)},
        ],
        "risk_model_id": MODEL_ID,
        "risk_model_version": MODEL_VERSION,
        "risk_calibration": "clones_ast",
        "instances": facts.into_iter().map(|fact| json!({
            "file_path": normalize_slashes(&fact.file),
            "module_path": project_relative_path(&config.project_root, &fact.file),
            "name": fact.name,
            "qualified_name": fact.qualified_name,
            "start_line": fact.start_line,
            "end_line": fact.end_line,
            "node_count": fact.node_count,
            "snippet": snippet_for_range(&fact.file, fact.start_line, fact.end_line),
        })).collect::<Vec<_>>(),
        "measurement_confidence": confidence,
    }))
}

fn ast_cross_file_factor(file_count: usize) -> f64 {
    if file_count >= 2 { 1.5 } else { 1.0 }
}

fn module_responsibility_rows(facts: &[FileFacts], confidence: Value) -> Vec<Value> {
    let mut groups: BTreeMap<String, Vec<&FileFacts>> = BTreeMap::new();
    for fact in facts.iter().filter(|fact| responsibility_candidate(fact)) {
        groups
            .entry(module_responsibility_signature(fact))
            .or_default()
            .push(fact);
    }
    groups
        .into_iter()
        .filter_map(|(signature, facts)| module_responsibility_row(signature, facts, &confidence))
        .collect()
}

fn responsibility_candidate(fact: &FileFacts) -> bool {
    fact.parse_status == "ok"
        && fact.source.source_nonblank_line_count >= MIN_RESPONSIBILITY_LINES
        && (fact.graph.public_api_count >= 2
            || fact.source.function_count >= 3
            || fact.items.types.len() + fact.items.impls.len() >= 2)
}

fn module_responsibility_signature(fact: &FileFacts) -> String {
    format!(
        "target:{}|entrypoint:{}|api:{}|functions:{}|types:{}|impls:{}|deps:{}",
        fact.target_kind,
        fact.entrypoint_kind.as_deref().unwrap_or("none"),
        fact.graph.public_api_count,
        fact.source.function_count,
        type_shape_signature(fact),
        impl_shape_signature(fact),
        dependency_signature(fact),
    )
}

fn type_shape_signature(fact: &FileFacts) -> String {
    let mut shapes = fact
        .items
        .types
        .iter()
        .map(|ty| {
            format!(
                "{}:{}:fields{}:variants{}:payload{}",
                ty.kind,
                ty.shape,
                bucket_count(ty.field_count),
                bucket_count(ty.variant_count),
                bucket_count(ty.variant_field_count)
            )
        })
        .collect::<Vec<_>>();
    shapes.sort();
    shapes.join(",")
}

fn impl_shape_signature(fact: &FileFacts) -> String {
    let mut shapes = fact
        .items
        .impls
        .iter()
        .map(|imp| format!("methods{}", bucket_count(imp.method_count)))
        .collect::<Vec<_>>();
    shapes.sort();
    shapes.join(",")
}

fn dependency_signature(fact: &FileFacts) -> String {
    let mut deps = fact
        .graph
        .dependencies
        .iter()
        .map(|dep| {
            dep.split("::")
                .last()
                .unwrap_or(dep.as_str())
                .to_ascii_lowercase()
        })
        .collect::<Vec<_>>();
    deps.sort();
    deps.dedup();
    deps.join(",")
}

fn bucket_count(value: usize) -> &'static str {
    match value {
        0 => "0",
        1 => "1",
        2..=3 => "2-3",
        4..=7 => "4-7",
        _ => "8+",
    }
}

fn module_responsibility_row(
    signature: String,
    facts: Vec<&FileFacts>,
    confidence: &Value,
) -> Option<Value> {
    if facts.len() < 2 {
        return None;
    }
    let file_count = facts
        .iter()
        .map(|fact| normalize_slashes(&fact.path))
        .collect::<HashSet<_>>()
        .len();
    let max_public_api_count = facts
        .iter()
        .map(|fact| fact.graph.public_api_count)
        .max()
        .unwrap_or(0);
    let max_function_count = facts
        .iter()
        .map(|fact| fact.source.function_count)
        .max()
        .unwrap_or(0);
    let score = round2(
        facts.len() as f64 * (8.0 + max_public_api_count as f64 + max_function_count as f64 * 1.5),
    );
    Some(json!({
        "engine": "module-responsibility",
        "hash": stable_hash(&signature),
        "signature": signature,
        "instance_count": facts.len(),
        "file_count": file_count,
        "target_kind": facts[0].target_kind,
        "entrypoint_kind": facts[0].entrypoint_kind,
        "is_entrypoint": facts.iter().any(|fact| fact.is_entrypoint),
        "max_public_api_count": max_public_api_count,
        "max_function_count": max_function_count,
        "score": score,
        "signals": if file_count >= 2 { format!("similar module responsibility x{file_count}") } else { "same-file module responsibility repeat".to_string() },
        "score_components": [{"signal": "responsibility_instances", "raw": {"instances": facts.len(), "max_public_api_count": max_public_api_count, "max_function_count": max_function_count}, "contribution": score}],
        "risk_model_id": MODEL_ID,
        "risk_model_version": MODEL_VERSION,
        "risk_calibration": "clones_module_responsibility",
        "instances": facts.into_iter().map(|fact| json!({
            "file_path": normalize_slashes(&fact.path),
            "module_key": fact.module_key,
            "module_id": fact.module_id,
            "package_name": fact.package_name,
            "target_name": fact.target_name,
            "identity_backend": fact.identity_backend,
            "target_kind": fact.target_kind,
            "entrypoint_kind": fact.entrypoint_kind,
            "is_entrypoint": fact.is_entrypoint,
            "public_api_count": fact.graph.public_api_count,
            "function_count": fact.source.function_count,
            "type_shapes": type_shape_signature(fact),
            "impl_shapes": impl_shape_signature(fact),
            "dependencies": dependency_signature(fact),
            "source_nonblank_line_count": fact.source.source_nonblank_line_count,
        })).collect::<Vec<_>>(),
        "measurement_confidence": confidence,
    }))
}

fn test_ast_clone_rows(
    config: &LensConfig,
    facts: &[FileFacts],
    ast_facts: &[AstCloneFact],
    confidence: Value,
) -> Vec<Value> {
    let mut tests = HashSet::new();
    for fact in facts {
        for test in &fact.items.tests {
            tests.insert((normalize_slashes(&test.path), test.name.clone()));
        }
    }
    let mut groups: BTreeMap<String, Vec<&AstCloneFact>> = BTreeMap::new();
    for fact in ast_facts
        .iter()
        .filter(|fact| tests.contains(&(normalize_slashes(&fact.file), fact.name.clone())))
    {
        groups.entry(fact.ast_hash.clone()).or_default().push(fact);
    }
    groups
        .into_iter()
        .filter_map(|(hash, facts)| test_ast_clone_row(config, hash, facts, &confidence))
        .collect()
}

fn test_ast_clone_row(
    config: &LensConfig,
    hash: String,
    facts: Vec<&AstCloneFact>,
    confidence: &Value,
) -> Option<Value> {
    if facts.len() < 2 {
        return None;
    }
    let file_count = facts
        .iter()
        .map(|fact| normalize_slashes(&fact.file))
        .collect::<HashSet<_>>()
        .len();
    let max_line_span = facts
        .iter()
        .map(|fact| fact.end_line.saturating_sub(fact.start_line) + 1)
        .max()
        .unwrap_or(0);
    if max_line_span < MIN_CLONE_LINE_SPAN {
        return None;
    }
    let max_node_count = facts.iter().map(|fact| fact.node_count).max().unwrap_or(0);
    Some(json!({
        "engine": "test-ast",
        "hash": hash,
        "node_count": max_node_count,
        "instance_count": facts.len(),
        "file_count": file_count,
        "max_line_span": max_line_span,
        "score": round2(facts.len() as f64 * max_line_span as f64 * 2.0),
        "signals": if file_count >= 2 { format!("duplicated test body shape x{file_count}") } else { "same-file duplicated test body shape".to_string() },
        "score_components": [{"signal": "test_instances_x_span", "raw": {"instances": facts.len(), "max_line_span": max_line_span}, "weight": 2.0, "contribution": round2(facts.len() as f64 * max_line_span as f64 * 2.0)}],
        "risk_model_id": MODEL_ID,
        "risk_model_version": MODEL_VERSION,
        "risk_calibration": "clones_test_ast",
        "instances": facts.into_iter().map(|fact| json!({
            "file_path": normalize_slashes(&fact.file),
            "module_path": project_relative_path(&config.project_root, &fact.file),
            "name": fact.name,
            "qualified_name": fact.qualified_name,
            "start_line": fact.start_line,
            "end_line": fact.end_line,
            "node_count": fact.node_count,
            "snippet": snippet_for_range(&fact.file, fact.start_line, fact.end_line),
        })).collect::<Vec<_>>(),
        "measurement_confidence": confidence,
    }))
}

fn snippet_for_range(path: &str, start_line: usize, end_line: usize) -> String {
    let Ok(text) = fs::read_to_string(path) else {
        return String::new();
    };
    text.lines()
        .skip(start_line.saturating_sub(1))
        .take(end_line.saturating_sub(start_line) + 1)
        .collect::<Vec<_>>()
        .join("\n")
}

fn non_overlapping_clone_instances(mut instances: Vec<CloneInstance>) -> Vec<CloneInstance> {
    instances.sort_by(|a, b| {
        a.file_path
            .cmp(&b.file_path)
            .then_with(|| a.start_line.cmp(&b.start_line))
            .then_with(|| a.end_line.cmp(&b.end_line))
    });
    let mut retained: Vec<CloneInstance> = Vec::new();
    let mut last_by_file: HashMap<String, usize> = HashMap::new();
    for instance in instances {
        let last_end = last_by_file
            .get(&instance.file_path)
            .copied()
            .unwrap_or_default();
        if instance.start_line <= last_end {
            continue;
        }
        last_by_file.insert(instance.file_path.clone(), instance.end_line);
        retained.push(instance);
    }
    retained
}
