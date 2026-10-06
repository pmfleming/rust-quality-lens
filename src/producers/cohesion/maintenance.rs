//! Recent ownership and temporal coupling; independent of calibrated risk.
use crate::{
    command_runner::checked_output,
    config::{HistoryConfig, LensConfig},
    facts::ModuleGraph,
    util::{project_relative_path, round2},
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Duration,
};

pub(super) fn produce(config: &LensConfig, graph: &ModuleGraph) -> Value {
    match collect(config, graph) {
        Ok(value) => value,
        Err(error) => json!({"model_version": 1, "recent": null, "previous": null,
            "measurement_confidence": confidence(&[error.to_string()])}),
    }
}

fn confidence(reasons: &[String]) -> Value {
    json!({"complete": reasons.is_empty(), "partial": !reasons.is_empty(),
        "confidence_scope": "bounded_git_maintenance_history", "required_inputs": ["git_history", "current_module_paths"],
        "observed_inputs": {}, "missing_input": reasons, "stale_input": [], "unsupported_pattern": []})
}

fn git(config: &LensConfig, args: &[String]) -> Result<String> {
    let mut command = std::process::Command::new("git");
    command.args(args).current_dir(&config.project_root);
    Ok(checked_output(
        &mut command,
        Duration::from_secs(config.verification.timeout_seconds),
    )?
    .stdout)
}

fn collect(config: &LensConfig, graph: &ModuleGraph) -> Result<Value> {
    let head = git(
        config,
        &[
            "show".into(),
            "-s".into(),
            "--format=%H%n%ct".into(),
            "HEAD".into(),
        ],
    )?;
    let (revision, timestamp) = head
        .trim()
        .split_once('\n')
        .context("invalid Git HEAD timestamp")?;
    let end: i64 = timestamp.parse()?;
    let seconds = i64::from(config.history.window_days) * 86400;
    let start = end - 2 * seconds;
    let text = git(
        config,
        &[
            "log".into(),
            "--no-merges".into(),
            "--no-renames".into(),
            "--root".into(),
            "--format=%x00%H%x00%ae%x00%ct%x00".into(),
            "--relative".into(),
            "--name-only".into(),
            "-z".into(),
            format!("--since-as-filter=@{start}"),
            revision.into(),
            "--".into(),
        ],
    )?;
    let shallow = git(
        config,
        &["rev-parse".into(), "--is-shallow-repository".into()],
    )?;
    let commits = parse_log(&text)?;
    let mut value = analyze(&commits, graph, config, end);
    let reasons = if shallow.trim() == "true" {
        vec!["shallow Git history may omit observations".into()]
    } else {
        Vec::new()
    };
    value["measurement_confidence"] = confidence(&reasons);
    value["head"] = json!(revision);
    Ok(value)
}

#[derive(Debug)]
struct Commit {
    author: String,
    timestamp: i64,
    paths: Vec<String>,
}

fn parse_log(text: &str) -> Result<Vec<Commit>> {
    let mut tokens = text.split('\0').peekable();
    let mut commits = Vec::new();
    while let Some(hash) = tokens.next() {
        if hash.is_empty() {
            continue;
        }
        ensure!(
            hash.len() >= 40 && hash.bytes().all(|b| b.is_ascii_hexdigit()),
            "invalid NUL-delimited Git commit header"
        );
        let author = tokens
            .next()
            .filter(|s| !s.is_empty())
            .context("missing Git author")?
            .to_string();
        let timestamp = tokens.next().context("missing Git timestamp")?.parse()?;
        ensure!(tokens.next() == Some(""), "missing Git header separator");
        let mut paths = Vec::new();
        while let Some(path) = tokens.peek() {
            if path.is_empty() {
                break;
            }
            let path = tokens.next().context("missing Git path")?;
            let path = if paths.is_empty() {
                path.strip_prefix('\n').unwrap_or(path)
            } else {
                path
            };
            ensure!(!path.is_empty(), "empty Git path");
            paths.push(path.to_string());
        }
        commits.push(Commit {
            author,
            timestamp,
            paths,
        });
    }
    Ok(commits)
}

#[derive(Default)]
struct Window {
    commits: usize,
    excluded_large_commits: usize,
    module_authors: BTreeMap<String, BTreeMap<String, usize>>,
    author_last_seen: BTreeMap<String, i64>,
    pairs: BTreeMap<(String, String), usize>,
    changed_modules: Vec<usize>,
}

fn excluded(path: &str, config: &HistoryConfig) -> bool {
    config.exclude_paths.iter().any(|prefix| {
        let prefix = prefix.trim_end_matches('/');
        path == prefix
            || path
                .strip_prefix(prefix)
                .is_some_and(|rest| rest.starts_with('/'))
    })
}

fn analyze(commits: &[Commit], graph: &ModuleGraph, config: &LensConfig, end: i64) -> Value {
    let seconds = i64::from(config.history.window_days) * 86400;
    let boundary = end - seconds;
    let mut by_path: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for module in graph.modules.values() {
        by_path
            .entry(project_relative_path(&config.project_root, &module.path))
            .or_default()
            .insert(module.id.clone());
    }
    let mut recent = Window::default();
    let mut previous = Window::default();
    for commit in commits {
        if commit.timestamp > end || commit.timestamp <= end - 2 * seconds {
            continue;
        }
        let window = if commit.timestamp > boundary {
            &mut recent
        } else {
            &mut previous
        };
        // Count every file before exclusions so bulk changes cannot appear narrow.
        if commit.paths.len() > config.history.max_commit_files {
            window.excluded_large_commits += 1;
            continue;
        }
        let modules: BTreeSet<_> = commit
            .paths
            .iter()
            .filter(|path| !excluded(path, &config.history))
            .filter_map(|path| by_path.get(path))
            .flatten()
            .cloned()
            .collect();
        if modules.is_empty() {
            continue;
        }
        window.commits += 1;
        window.changed_modules.push(modules.len());
        window
            .author_last_seen
            .entry(commit.author.clone())
            .and_modify(|time| *time = (*time).max(commit.timestamp))
            .or_insert(commit.timestamp);
        let modules: Vec<_> = modules.into_iter().collect();
        for (index, module) in modules.iter().enumerate() {
            *window
                .module_authors
                .entry(module.clone())
                .or_default()
                .entry(commit.author.clone())
                .or_default() += 1;
            for other in &modules[index + 1..] {
                *window
                    .pairs
                    .entry((module.clone(), other.clone()))
                    .or_default() += 1;
            }
        }
    }
    let recent_report = window_report(&recent, graph);
    let previous_report = window_report(&previous, graph);
    let owner_activity: Vec<_> = previous
        .module_authors
        .iter()
        .filter_map(|(module, authors)| {
            let (author, _) = primary_author(authors)?;
            Some(
                json!({"module_id": module, "previous_primary_author": author,
            "observed_in_recent_measured_history": recent.author_last_seen.contains_key(author),
            "last_recent_commit_timestamp": recent.author_last_seen.get(author)}),
            )
        })
        .collect();
    json!({"model_version": 1, "calibration": "uncalibrated-advisory",
        "scope": "non-merge commits touching current measured module paths; renames not followed; commit-touch ownership, not lines or reviews",
        "window": {"anchor": "HEAD committer timestamp", "end_timestamp": end,
            "boundary_timestamp": boundary, "start_timestamp": end - 2 * seconds, "interval": "(start, end]",
            "days_per_window": config.history.window_days},
        "filters": {"max_commit_files": config.history.max_commit_files, "exclude_paths": config.history.exclude_paths},
        "recent": recent_report, "previous": previous_report, "previous_owner_activity": owner_activity})
}

fn primary_author(authors: &BTreeMap<String, usize>) -> Option<(&String, &usize)> {
    // BTree iteration makes ties deterministic (first lexicographic email wins).
    authors
        .iter()
        .max_by(|(a, ac), (b, bc)| ac.cmp(bc).then_with(|| b.cmp(a)))
}

fn window_report(window: &Window, graph: &ModuleGraph) -> Value {
    let totals: BTreeMap<_, usize> = window
        .module_authors
        .iter()
        .map(|(module, authors)| (module, authors.values().sum()))
        .collect();
    let ownership: Vec<_> = window.module_authors.iter().map(|(module, authors)| {
        let total = totals[module];
        let shares = authors.values().map(|count| *count as f64 / total as f64).collect::<Vec<_>>();
        let primary = primary_author(authors);
        json!({"module_id": module, "commit_count": total, "contributor_count": authors.len(),
            "primary_author": primary.map(|(author, _)| author),
            "largest_contributor_share_percent": primary.map(|(_, count)| round2(100.0 * *count as f64 / total as f64)),
            "concentration_hhi": round2(shares.iter().map(|p| p * p).sum()),
            "contribution_entropy_bits": round2(shares.iter().map(|p| -p * p.log2()).sum()),
            "authors": authors})
    }).collect();
    let pairs: Vec<_> = window
        .pairs
        .iter()
        .map(|((a, b), count)| {
            let a_count = totals[a];
            let b_count = totals[b];
            let dependency = graph
                .dependencies
                .get(a)
                .is_some_and(|targets| targets.contains(b))
                || graph
                    .dependencies
                    .get(b)
                    .is_some_and(|targets| targets.contains(a));
            json!({"module_a": a, "module_b": b, "cochange_commits": count,
            "support_percent": round2(100.0 * *count as f64 / window.commits as f64),
            "b_given_a_percent": round2(100.0 * *count as f64 / a_count as f64),
            "a_given_b_percent": round2(100.0 * *count as f64 / b_count as f64),
            "lift": round2(*count as f64 * window.commits as f64 / (a_count * b_count) as f64),
            "observed_static_dependency": dependency})
        })
        .collect();
    let mut sizes = window.changed_modules.clone();
    sizes.sort();
    json!({"eligible_commits": window.commits, "excluded_large_commits": window.excluded_large_commits,
        "ownership": ownership, "pairs": pairs,
        "change_amplification": {"mean_modules_per_commit": (!sizes.is_empty()).then(|| round2(sizes.iter().sum::<usize>() as f64 / sizes.len() as f64)),
            "median_modules_per_commit": median(&sizes), "max_modules_per_commit": sizes.last()}})
}

fn median(values: &[usize]) -> Option<f64> {
    if values.is_empty() {
        None
    } else {
        Some((values[(values.len() - 1) / 2] + values[values.len() / 2]) as f64 / 2.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facts::{FileFacts, module_graph};
    #[test]
    fn known_history_has_exact_ownership_and_pairwise_probabilities() {
        let mut config = LensConfig::test_config("history", std::path::Path::new("/project"));
        config.history.window_days = 1;
        config.history.max_commit_files = 2;
        config.history.exclude_paths = vec!["src/generated".into()];
        let graph = module_graph(&[
            FileFacts::test_fact("/project/src/a.rs", "a"),
            FileFacts::test_fact("/project/src/b.rs", "b"),
        ]);
        let commit = |author: &str, timestamp, paths: &[&str]| Commit {
            author: author.into(),
            timestamp,
            paths: paths.iter().map(|s| s.to_string()).collect(),
        };
        let commits = vec![
            commit("ada", 200000, &["src/a.rs", "src/b.rs"]),
            commit("ada", 199999, &["src/a.rs", "src/b.rs"]),
            commit("bob", 199998, &["src/a.rs"]),
            commit("bob", 199997, &["src/b.rs"]),
            commit("old", 100000, &["src/a.rs"]),
            commit("bulk", 199999, &["src/a.rs", "src/b.rs", "README.md"]),
        ];
        let report = analyze(&commits, &graph, &config, 200000);
        let recent = &report["recent"];
        assert_eq!(recent["eligible_commits"], 4);
        assert_eq!(recent["excluded_large_commits"], 1);
        assert_eq!(
            recent["ownership"][0]["largest_contributor_share_percent"],
            66.67
        );
        assert_eq!(recent["pairs"][0]["support_percent"], 50.0);
        assert_eq!(recent["pairs"][0]["b_given_a_percent"], 66.67);
        assert_eq!(recent["pairs"][0]["lift"], 0.89);
        assert_eq!(
            recent["change_amplification"]["median_modules_per_commit"],
            1.5
        );
        assert_eq!(
            report["previous_owner_activity"][0]["observed_in_recent_measured_history"],
            false
        );
        assert!(excluded("src/generated/x.rs", &config.history));
        assert!(!excluded("src/generated_extra/x.rs", &config.history));
    }
    #[test]
    fn nul_paths_and_empty_history_do_not_invent_ownership() -> Result<()> {
        let header = format!(
            "\0{}\0ada\0{}\0\0\nsrc/with\nnewline.rs\0\0{}\0bob\0{}\0\0",
            "a".repeat(40),
            200000,
            "b".repeat(40),
            199999
        );
        let commits = parse_log(&header)?;
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].paths, ["src/with\nnewline.rs"]);
        assert!(commits[1].paths.is_empty());
        assert!(parse_log("malformed").is_err());
        let config = LensConfig::test_config("empty", std::path::Path::new("/project"));
        let report = analyze(&[], &module_graph(&[]), &config, 200000);
        assert!(report["recent"]["change_amplification"]["mean_modules_per_commit"].is_null());
        Ok(())
    }
    #[test]
    fn live_git_history_and_shallow_confidence() -> Result<()> {
        let root = tempfile::tempdir()?;
        let config = LensConfig::test_config("git", root.path());
        git(&config, &["init".into(), "-q".into()])?;
        std::fs::create_dir(root.path().join("src"))?;
        std::fs::write(root.path().join("src/a.rs"), "pub fn a() {}")?;
        git(&config, &["add".into(), ".".into()])?;
        git(
            &config,
            &[
                "-c".into(),
                "user.name=Ada".into(),
                "-c".into(),
                "user.email=ada@example.test".into(),
                "commit".into(),
                "-qm".into(),
                "initial".into(),
            ],
        )?;
        let graph = module_graph(&[FileFacts::test_fact(
            root.path()
                .join("src/a.rs")
                .to_str()
                .context("UTF-8 path")?,
            "a",
        )]);
        let report = produce(&config, &graph);
        assert_eq!(
            report["measurement_confidence"]["complete"], true,
            "{report:#}"
        );
        assert_eq!(report["recent"]["eligible_commits"], 1);
        let head = git(&config, &["rev-parse".into(), "HEAD".into()])?;
        std::fs::write(root.path().join(".git/shallow"), head)?;
        assert_eq!(
            produce(&config, &graph)["measurement_confidence"]["complete"],
            false
        );
        Ok(())
    }
}
