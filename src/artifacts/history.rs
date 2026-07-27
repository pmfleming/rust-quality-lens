use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;

use crate::config::LensConfig;
use crate::facts::ModuleGraph;
use crate::measurement::module_for_path;
use crate::risk_model::{ChangeFacts, CorrectnessFacts};
use crate::util::normalize_slashes;

#[derive(Default)]
struct ModuleGitFacts {
    churn: u64,
    commits: BTreeSet<String>,
    contributors: BTreeSet<String>,
    defect_commits: BTreeSet<String>,
    cochange_count: usize,
}

pub(crate) struct GitHistoryIndex {
    by_module: BTreeMap<String, ModuleGitFacts>,
    status: &'static str,
    reason: Option<String>,
}

impl GitHistoryIndex {
    pub(crate) fn for_module(
        &self,
        module: &str,
        correctness: Option<&CorrectnessFacts>,
    ) -> Option<ChangeFacts> {
        if !self.is_available() {
            return None;
        }
        let facts = self.by_module.get(module);
        Some(ChangeFacts {
            churn: facts.map(|facts| facts.churn).unwrap_or_default(),
            commit_count: facts.map(|facts| facts.commits.len()).unwrap_or_default(),
            contributor_count: facts
                .map(|facts| facts.contributors.len())
                .unwrap_or_default(),
            defect_commit_count: facts
                .map(|facts| facts.defect_commits.len())
                .unwrap_or_default(),
            has_test_evidence: correctness.is_some_and(|facts| {
                facts.test_count > 0
                    || facts
                        .line_coverage_percent
                        .is_some_and(|coverage| coverage > 0.0)
            }),
        })
    }

    pub(crate) fn raw_for_module(&self, module: &str) -> Value {
        let Some(facts) = self.by_module.get(module) else {
            return json!({
                "churn": 0,
                "commit_count": 0,
                "contributor_count": 0,
                "defect_commit_count": 0,
                "cochange_count": 0,
            });
        };
        json!({
            "churn": facts.churn,
            "commit_count": facts.commits.len(),
            "contributor_count": facts.contributors.len(),
            "defect_commit_count": facts.defect_commits.len(),
            "cochange_count": facts.cochange_count,
        })
    }

    pub(crate) fn status_json(&self) -> Value {
        json!({
            "status": self.status,
            "required": true,
            "path": ".git",
            "reason": self.reason,
        })
    }

    pub(crate) fn is_available(&self) -> bool {
        self.status == "available"
    }
}

pub(crate) fn git_history_facts(config: &LensConfig, graph: &ModuleGraph) -> GitHistoryIndex {
    if !is_git_work_tree(config) {
        return unavailable("project root is not inside a git work tree");
    }
    let output = match git_log(config, graph) {
        Ok(output) if output.status.success() => output,
        _ => return unavailable("git log could not be read"),
    };
    GitHistoryIndex {
        by_module: parse_git_history(config, &String::from_utf8_lossy(&output.stdout), graph),
        status: "available",
        reason: None,
    }
}

fn unavailable(reason: &str) -> GitHistoryIndex {
    GitHistoryIndex {
        by_module: BTreeMap::new(),
        status: "unavailable",
        reason: Some(reason.to_string()),
    }
}

fn is_git_work_tree(config: &LensConfig) -> bool {
    Command::new("git")
        .arg("rev-parse")
        .arg("--is-inside-work-tree")
        .current_dir(&config.project_root)
        .output()
        .as_ref()
        .is_ok_and(|output| output.status.success())
}

fn git_log(config: &LensConfig, graph: &ModuleGraph) -> std::io::Result<std::process::Output> {
    let mut command = Command::new("git");
    command
        .arg("log")
        .arg("--numstat")
        .arg("--no-renames")
        .arg("--format=commit:%H%x09%an%x09%s")
        .arg("--");
    for root in module_source_roots(config, graph) {
        command.arg(root);
    }
    command.current_dir(&config.project_root).output()
}

fn parse_git_history(
    config: &LensConfig,
    text: &str,
    graph: &ModuleGraph,
) -> BTreeMap<String, ModuleGitFacts> {
    let mut by_module = BTreeMap::new();
    let mut current = CommitAccumulator::default();
    for line in text.lines() {
        if line.starts_with("commit:") {
            std::mem::take(&mut current).flush_into(&mut by_module);
            current = CommitAccumulator::from_header(line);
        } else if !line.is_empty()
            && let Some(module) = changed_module(config, line, graph)
        {
            current.modules.insert(module.name);
            by_module.entry(module.name_for_churn).or_default().churn += module.churn;
        }
    }
    current.flush_into(&mut by_module);
    by_module
}

struct ChangedModule {
    name: String,
    name_for_churn: String,
    churn: u64,
}

fn changed_module(config: &LensConfig, line: &str, graph: &ModuleGraph) -> Option<ChangedModule> {
    let parts = line.split('\t').collect::<Vec<_>>();
    if parts.len() < 3 {
        return None;
    }
    let module = git_path_to_module(config, parts[2], graph)?;
    let added = parts[0].parse::<u64>().unwrap_or(0);
    let deleted = parts[1].parse::<u64>().unwrap_or(0);
    Some(ChangedModule {
        name: module.clone(),
        name_for_churn: module,
        churn: added + deleted,
    })
}

#[derive(Default)]
struct CommitAccumulator {
    hash: String,
    author: String,
    subject: String,
    modules: BTreeSet<String>,
}

impl CommitAccumulator {
    fn from_header(line: &str) -> Self {
        let Some(rest) = line.strip_prefix("commit:") else {
            return Self::default();
        };
        let parts = rest.splitn(3, '\t').collect::<Vec<_>>();
        Self {
            hash: parts.first().copied().unwrap_or_default().to_string(),
            author: parts.get(1).copied().unwrap_or_default().to_string(),
            subject: parts.get(2).copied().unwrap_or_default().to_string(),
            modules: BTreeSet::new(),
        }
    }

    fn flush_into(self, by_module: &mut BTreeMap<String, ModuleGitFacts>) {
        if self.hash.is_empty() {
            return;
        }
        let defect = is_defect_subject(&self.subject);
        for module in &self.modules {
            let facts = by_module.entry(module.clone()).or_default();
            facts.commits.insert(self.hash.clone());
            facts.contributors.insert(self.author.clone());
            if defect {
                facts.defect_commits.insert(self.hash.clone());
            }
            facts.cochange_count += self.modules.len().saturating_sub(1);
        }
    }
}

fn is_defect_subject(subject: &str) -> bool {
    subject
        .to_lowercase()
        .split(|character: char| !character.is_alphanumeric())
        .any(|word| {
            matches!(
                word,
                "bug"
                    | "bugs"
                    | "bugfix"
                    | "fix"
                    | "fixes"
                    | "fixed"
                    | "defect"
                    | "defects"
                    | "regression"
                    | "regressions"
                    | "panic"
                    | "crash"
            )
        })
}

fn module_source_roots(config: &LensConfig, graph: &ModuleGraph) -> Vec<String> {
    let mut roots = BTreeSet::new();
    for module in graph.modules.values() {
        if let Some(first) = relative_to_project_root(&module.path, config)
            .split('/')
            .find(|part| !part.is_empty())
        {
            roots.insert(first.to_string());
        }
    }
    if roots.is_empty() {
        roots.extend(config.source_roots.iter().filter_map(|root| {
            normalize_slashes(Path::new(root))
                .split('/')
                .find(|part| !part.is_empty())
                .map(str::to_string)
        }));
    }
    if roots.is_empty() {
        roots.insert("src".to_string());
    }
    roots.into_iter().collect()
}

fn git_path_to_module(config: &LensConfig, path: &str, graph: &ModuleGraph) -> Option<String> {
    let normalized = normalize_slashes(Path::new(path));
    graph
        .modules
        .values()
        .find(|module| relative_to_project_root(&module.path, config) == normalized)
        .map(|module| module.key.clone())
        .or_else(|| {
            let module = module_for_path(&normalized);
            graph.modules.contains_key(&module).then_some(module)
        })
}

fn relative_to_project_root(path: &Path, config: &LensConfig) -> String {
    let relative = path.strip_prefix(&config.project_root).unwrap_or(path);
    normalize_slashes(relative)
}

#[cfg(test)]
mod tests {
    use super::{is_defect_subject, parse_git_history};
    use crate::config::{LensConfig, SemanticIdentityMode};
    use crate::facts::module_graph;
    use std::path::PathBuf;

    #[test]
    fn blank_line_between_header_and_numstat_keeps_commit_attribution() {
        let config = LensConfig {
            project_name: "history".to_string(),
            project_root: PathBuf::from("/project"),
            source_roots: vec!["/project/src".to_string()],
            output_dir: PathBuf::from("/project/target/analysis"),
            helper_manifest: PathBuf::from("unused"),
            identity_resolution: SemanticIdentityMode::Disabled,
            rust_analyzer: PathBuf::from("rust-analyzer"),
            identity_timeout_seconds: 1,
            identity_offline: true,
            verification: Default::default(),
        };
        let facts = vec![crate::facts::FileFacts::test_fact(
            "/project/src/lib.rs",
            "lib",
        )];
        let graph = module_graph(&facts);
        let history = parse_git_history(
            &config,
            "commit:abc\tAda\tfix parser\n\n10\t2\tsrc/lib.rs\n",
            &graph,
        );
        let Some(facts) = history.get("test::shared::lib") else {
            panic!("lib history should exist");
        };
        assert_eq!(facts.churn, 12);
        assert_eq!(facts.commits.len(), 1);
        assert_eq!(facts.contributors.len(), 1);
        assert_eq!(facts.defect_commits.len(), 1);
    }

    #[test]
    fn defect_subject_matching_uses_words_not_substrings() {
        assert!(is_defect_subject("fix parser crash"));
        assert!(!is_defect_subject("refresh fixture snapshots"));
        assert!(!is_defect_subject("prefix command output"));
    }
}
