use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use crate::config::{LensConfig, SemanticIdentityMode};
use crate::facts::{DependencyReferenceFact, FileFacts, ResolvedDependencyFact};
use crate::util::{lock_file, normalize_slashes, write_json};

const CACHE_VERSION: u64 = 13;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct IdentityResolutionSummary {
    pub(crate) mode: SemanticIdentityMode,
    pub(crate) backend: String,
    pub(crate) available: bool,
    pub(crate) complete: bool,
    pub(crate) cache_hit: bool,
    pub(crate) reference_count: usize,
    pub(crate) resolved_count: usize,
    pub(crate) local_definition_count: usize,
    pub(crate) external_definition_count: usize,
    pub(crate) unresolved_count: usize,
    pub(crate) duration_ms: u128,
    pub(crate) analyzer_version: Option<String>,
    pub(crate) reason: Option<String>,
}

impl IdentityResolutionSummary {
    pub(crate) fn disabled(mode: SemanticIdentityMode, reference_count: usize) -> Self {
        Self {
            mode,
            backend: "syntax_fallback".to_string(),
            available: false,
            complete: mode == SemanticIdentityMode::Disabled,
            cache_hit: false,
            reference_count,
            resolved_count: 0,
            local_definition_count: 0,
            external_definition_count: 0,
            unresolved_count: reference_count,
            duration_ms: 0,
            analyzer_version: None,
            reason: (mode != SemanticIdentityMode::Disabled)
                .then(|| "rust-analyzer resolution was unavailable".to_string()),
        }
    }

    pub(crate) fn to_json(&self) -> Value {
        let mut value = serde_json::to_value(self).unwrap_or_else(|_| json!({}));
        if let Some(object) = value.as_object_mut() {
            let percent = if self.reference_count == 0 {
                100.0
            } else {
                crate::util::round2(
                    self.resolved_count as f64 * 100.0 / self.reference_count as f64,
                )
            };
            object.insert("resolution_percent".to_string(), Value::from(percent));
        }
        value
    }
}

#[derive(Deserialize, Serialize)]
struct IdentityCache {
    cache_version: u64,
    fingerprint: String,
    summary: IdentityResolutionSummary,
    resolved_by_path: BTreeMap<String, Vec<ResolvedDependencyFact>>,
}

pub(crate) fn resolve(
    config: &LensConfig,
    facts: &mut [FileFacts],
) -> Result<IdentityResolutionSummary> {
    let candidates = semantic_candidates(facts);
    let reference_count = candidates.iter().map(Vec::len).sum();
    if config.identity_resolution == SemanticIdentityMode::Disabled || reference_count == 0 {
        return Ok(IdentityResolutionSummary::disabled(
            config.identity_resolution,
            reference_count,
        ));
    }
    let _cache_lock = lock_file(&cache_path(config))?;
    let analyzer_version = analyzer_version(config);
    let fingerprint = fingerprint(config, facts, analyzer_version.as_deref());
    if let Some(summary) = fingerprint
        .as_deref()
        .and_then(|key| restore_cache(config, facts, key))
    {
        return enforce_required(config, summary);
    }

    let started = Instant::now();
    let deadline = started + Duration::from_secs(config.identity_timeout_seconds);
    let linked_project = detached_rust_project(config, facts)?;
    let mut client = match LspClient::start(config, linked_project.as_deref(), deadline) {
        Ok(client) => client,
        Err(error) => {
            return unavailable_summary(config, reference_count, analyzer_version, error);
        }
    };
    let path_index = fact_path_index(config, facts);
    let counts = resolve_candidates(
        config,
        facts,
        candidates,
        &path_index,
        &mut client,
        deadline,
    );
    client.shutdown();

    let summary = counts.summary(
        config.identity_resolution,
        reference_count,
        started.elapsed(),
        analyzer_version,
    );
    if let Some(fingerprint) = fingerprint {
        write_cache(config, facts, &fingerprint, &summary)?;
    }
    enforce_required(config, summary)
}

#[derive(Default)]
struct ResolutionCounts {
    resolved: usize,
    local: usize,
    external: usize,
    unresolved: usize,
}

impl ResolutionCounts {
    fn record(&mut self, fact: &ResolvedDependencyFact) {
        match fact.status.as_str() {
            "resolved_local" => {
                self.resolved += 1;
                self.local += 1;
            }
            "resolved_external" => {
                self.resolved += 1;
                self.external += 1;
            }
            _ => self.unresolved += 1,
        }
    }

    fn summary(
        self,
        mode: SemanticIdentityMode,
        reference_count: usize,
        duration: Duration,
        analyzer_version: Option<String>,
    ) -> IdentityResolutionSummary {
        IdentityResolutionSummary {
            mode,
            backend: "rust_analyzer".to_string(),
            available: true,
            complete: self.unresolved == 0,
            cache_hit: false,
            reference_count,
            resolved_count: self.resolved,
            local_definition_count: self.local,
            external_definition_count: self.external,
            unresolved_count: self.unresolved,
            duration_ms: duration.as_millis(),
            analyzer_version,
            reason: (self.unresolved > 0).then(|| {
                format!(
                    "{} dependency references were not resolved",
                    self.unresolved
                )
            }),
        }
    }
}

fn semantic_candidates(facts: &[FileFacts]) -> Vec<Vec<DependencyReferenceFact>> {
    facts
        .iter()
        .map(|fact| {
            fact.graph
                .dependency_references
                .iter()
                .filter(|reference| is_semantic_candidate(&reference.raw_path, fact, facts))
                .cloned()
                .collect()
        })
        .collect()
}

fn restore_cache(
    config: &LensConfig,
    facts: &mut [FileFacts],
    fingerprint: &str,
) -> Option<IdentityResolutionSummary> {
    let mut cached = read_cache(config, fingerprint)?;
    for fact in facts {
        fact.graph.resolved_dependencies = cached
            .resolved_by_path
            .remove(&normalize_slashes(&fact.path))
            .unwrap_or_default();
    }
    cached.summary.mode = config.identity_resolution;
    cached.summary.cache_hit = true;
    cached.summary.duration_ms = 0;
    Some(cached.summary)
}

fn unavailable_summary(
    config: &LensConfig,
    reference_count: usize,
    analyzer_version: Option<String>,
    error: anyhow::Error,
) -> Result<IdentityResolutionSummary> {
    let mut summary =
        IdentityResolutionSummary::disabled(config.identity_resolution, reference_count);
    summary.reason = Some(error.to_string());
    summary.analyzer_version = analyzer_version;
    enforce_required(config, summary)
}

fn resolve_candidates(
    config: &LensConfig,
    facts: &mut [FileFacts],
    candidates: Vec<Vec<DependencyReferenceFact>>,
    path_index: &BTreeMap<String, (String, String)>,
    client: &mut LspClient,
    deadline: Instant,
) -> ResolutionCounts {
    let mut counts = ResolutionCounts::default();
    for (fact, references) in facts.iter_mut().zip(candidates) {
        let source_uri = file_uri(&absolute_path(config, &fact.path));
        fact.graph.resolved_dependencies = references
            .iter()
            .map(|reference| {
                resolve_reference(client, &source_uri, reference, path_index, deadline)
            })
            .inspect(|resolved| counts.record(resolved))
            .collect();
    }
    counts
}

fn resolve_reference(
    client: &mut LspClient,
    source_uri: &str,
    reference: &DependencyReferenceFact,
    path_index: &BTreeMap<String, (String, String)>,
    deadline: Instant,
) -> ResolvedDependencyFact {
    if Instant::now() >= deadline {
        return unresolved(reference, "timeout");
    }
    match client.definition(
        source_uri,
        reference.line.saturating_sub(1),
        reference.column,
        deadline,
    ) {
        Ok(locations) => resolved_definitions(reference, locations, path_index),
        Err(error) => unresolved(reference, &format!("error: {error}")),
    }
}

fn resolved_definitions(
    reference: &DependencyReferenceFact,
    locations: Vec<DefinitionLocation>,
    path_index: &BTreeMap<String, (String, String)>,
) -> ResolvedDependencyFact {
    // Proc-macro navigation can return the external generator first and the
    // local expansion origin second. The generated item belongs to the latter.
    let multiple = locations.len() > 1;
    let location = locations
        .into_iter()
        .min_by_key(|location| match uri_path(&location.uri) {
            Some(path) if path_index.contains_key(&canonical_key(&path)) => 0,
            None => 1, // Without a local origin, do not mistake a generator for the item.
            Some(_) => 2,
        });
    let Some(location) = location else {
        return unresolved(reference, "unresolved");
    };
    let Some(target_path) = uri_path(&location.uri) else {
        // A virtual expansion URI is not evidence of an external dependency.
        // Keep it unresolved unless the analyzer supplies a source location.
        return unresolved(reference, "unmapped_generated_definition");
    };
    let target_path = Some(target_path);
    let target = target_path
        .as_ref()
        .map(canonical_key)
        .and_then(|key| path_index.get(&key));
    let symbol = if multiple && target.is_some() {
        // The origin's token names the generator, not the generated item.
        reference_symbol(&reference.raw_path)
    } else {
        target_path
            .as_ref()
            .and_then(|path| identifier_at(path, location.line, location.character))
            .unwrap_or_else(|| reference_symbol(&reference.raw_path))
    };
    let (status, target_module_id, target_module_key, symbol_identity) = match target {
        Some((module_id, module_key)) => (
            "resolved_local",
            Some(module_id.clone()),
            Some(module_key.clone()),
            Some(format!("{module_id}::{symbol}")),
        ),
        None => ("resolved_external", None, None, None),
    };
    ResolvedDependencyFact {
        raw_path: reference.raw_path.clone(),
        line: reference.line,
        column: reference.column,
        status: status.to_string(),
        backend: "rust_analyzer".to_string(),
        target_path: target_path.map(normalize_slashes),
        target_module_id,
        target_module_key,
        symbol_identity,
    }
}

fn enforce_required(
    config: &LensConfig,
    summary: IdentityResolutionSummary,
) -> Result<IdentityResolutionSummary> {
    if config.identity_resolution == SemanticIdentityMode::Required && !summary.complete {
        bail!(
            "semantic identity resolution is required but incomplete: {}",
            summary.reason.as_deref().unwrap_or("unknown reason")
        );
    }
    Ok(summary)
}

fn unresolved(
    reference: &crate::facts::DependencyReferenceFact,
    status: &str,
) -> ResolvedDependencyFact {
    ResolvedDependencyFact {
        raw_path: reference.raw_path.clone(),
        line: reference.line,
        column: reference.column,
        status: status.to_string(),
        backend: "syntax_fallback".to_string(),
        target_path: None,
        target_module_id: None,
        target_module_key: None,
        symbol_identity: None,
    }
}

fn analyzer_version(config: &LensConfig) -> Option<String> {
    crate::command_runner::checked_output(
        Command::new(&config.rust_analyzer)
            .arg("--version")
            .current_dir(&config.project_root),
        Duration::from_secs(3),
    )
    .ok()
    .map(|output| output.stdout)
    .map(|version| version.trim().to_string())
}

fn fingerprint(
    config: &LensConfig,
    facts: &[FileFacts],
    analyzer_version: Option<&str>,
) -> Option<String> {
    // Includes member manifests and configuration, and detects edits even when
    // file lengths and timestamps are preserved. Never cache unreadable inputs.
    let source = config.source_fingerprint();
    if source["complete"] != true {
        return None;
    }
    let identities = facts.iter().map(|fact| {
        json!({"path": fact.path, "module_id": fact.module_id, "module_key": fact.module_key})
    }).collect::<Vec<_>>();
    Some(crate::util::stable_hash(
        &json!({
            "cache_version": CACHE_VERSION,
            "analyzer_version": analyzer_version,
            "config": config,
            "source": source["digest"],
            "identities": identities,
        })
        .to_string(),
    ))
}

fn cache_path(config: &LensConfig) -> PathBuf {
    config.output_dir.join("semantic_identity_cache.json")
}

fn read_cache(config: &LensConfig, fingerprint: &str) -> Option<IdentityCache> {
    let cache: IdentityCache =
        serde_json::from_str(&fs::read_to_string(cache_path(config)).ok()?).ok()?;
    (cache.cache_version == CACHE_VERSION && cache.fingerprint == fingerprint).then_some(cache)
}

fn write_cache(
    config: &LensConfig,
    facts: &[FileFacts],
    fingerprint: &str,
    summary: &IdentityResolutionSummary,
) -> Result<()> {
    let resolved_by_path = facts
        .iter()
        .map(|fact| {
            (
                normalize_slashes(&fact.path),
                fact.graph.resolved_dependencies.clone(),
            )
        })
        .collect();
    write_json(
        &cache_path(config),
        &serde_json::to_value(IdentityCache {
            cache_version: CACHE_VERSION,
            fingerprint: fingerprint.to_string(),
            summary: summary.clone(),
            resolved_by_path,
        })?,
    )
}

fn fact_path_index(config: &LensConfig, facts: &[FileFacts]) -> BTreeMap<String, (String, String)> {
    facts
        .iter()
        .map(|fact| {
            (
                canonical_key(absolute_path(config, &fact.path)),
                (fact.module_id.clone(), fact.module_key.clone()),
            )
        })
        .collect()
}

fn absolute_path(config: &LensConfig, path: impl AsRef<Path>) -> PathBuf {
    let path = path.as_ref();
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        config.project_root.join(path)
    }
}

fn canonical_key(path: impl AsRef<Path>) -> String {
    normalize_slashes(
        fs::canonicalize(path.as_ref()).unwrap_or_else(|_| path.as_ref().to_path_buf()),
    )
}

fn reference_symbol(raw: &str) -> String {
    raw.trim_end_matches("::*")
        .trim_end_matches("::self")
        .rsplit("::")
        .next()
        .unwrap_or(raw)
        .to_string()
}

fn is_semantic_candidate(raw: &str, source: &FileFacts, facts: &[FileFacts]) -> bool {
    let raw = raw.trim_start_matches("::").trim_end_matches("::*");
    if matches!(raw, "crate" | "self" | "super") {
        return false;
    }
    let first = raw.split("::").next().unwrap_or_default();
    if matches!(first, "crate" | "self" | "super") {
        return true;
    }
    let local_module = facts
        .iter()
        .filter(|fact| fact.package_name == source.package_name)
        .filter_map(|fact| fact.module_key.split("::").next())
        .any(|module| module == first);
    let workspace_package = facts.iter().any(|fact| {
        fact.package_name.replace('-', "_") == first && fact.package_name != source.package_name
    });
    local_module || workspace_package
}

fn identifier_at(path: &Path, line: usize, character: usize) -> Option<String> {
    let contents = fs::read_to_string(path).ok()?;
    let line = contents.lines().nth(line)?;
    let byte = utf16_column_to_byte(line, character);
    let bytes = line.as_bytes();
    let mut start = byte.min(bytes.len());
    while start > 0 && is_ident_byte(bytes[start - 1]) {
        start -= 1;
    }
    let mut end = byte.min(bytes.len());
    while end < bytes.len() && is_ident_byte(bytes[end]) {
        end += 1;
    }
    (start < end).then(|| line[start..end].to_string())
}

fn utf16_column_to_byte(line: &str, column: usize) -> usize {
    let mut units = 0;
    for (byte, character) in line.char_indices() {
        if units >= column {
            return byte;
        }
        units += character.len_utf16();
    }
    line.len()
}

fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

struct DefinitionLocation {
    uri: String,
    line: usize,
    character: usize,
}

type LspReceiver = Receiver<Result<Value, String>>;
type LspTransport = (Child, ChildStdin, LspReceiver);

struct LspClient {
    child: Child,
    stdin: ChildStdin,
    receiver: LspReceiver,
    next_id: u64,
    configuration: Value,
}

fn lsp_transport(config: &LensConfig) -> Result<LspTransport> {
    let mut child = Command::new(&config.rust_analyzer)
        .current_dir(&config.project_root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("starting {}", config.rust_analyzer.display()))?;
    let stdin = child
        .stdin
        .take()
        .context("rust-analyzer stdin unavailable")?;
    let stdout = child
        .stdout
        .take()
        .context("rust-analyzer stdout unavailable")?;
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        loop {
            let message = match read_lsp_message(&mut reader) {
                Ok(Some(message)) => Ok(message),
                Ok(None) => return,
                Err(error) => Err(error.to_string()),
            };
            let failed = message.is_err();
            if sender.send(message).is_err() || failed {
                return;
            }
        }
    });
    Ok((child, stdin, receiver))
}

fn analyzer_configuration(config: &LensConfig, linked_project: Option<&Path>) -> Value {
    let extra_args = if config.identity_offline {
        json!(["--offline"])
    } else {
        json!([])
    };
    let mut configuration = json!({
        "cargo": {
            "buildScripts": {"enable": config.identity_build_macros},
            "extraArgs": extra_args,
            "features": "all",
            "allTargets": true
        },
        "procMacro": {"enable": config.identity_build_macros},
        "checkOnSave": false
    });
    if let Some(project) = linked_project {
        configuration["linkedProjects"] = json!([canonical_key(project)]);
    }
    configuration
}

impl LspClient {
    fn start(
        config: &LensConfig,
        linked_project: Option<&Path>,
        deadline: Instant,
    ) -> Result<Self> {
        let (child, stdin, receiver) = lsp_transport(config)?;
        let configuration = analyzer_configuration(config, linked_project);
        let mut client = Self {
            child,
            stdin,
            receiver,
            next_id: 1,
            configuration,
        };
        client.initialize(config, deadline)?;
        client.wait_until_ready(deadline)?;
        Ok(client)
    }

    fn initialize(&mut self, config: &LensConfig, deadline: Instant) -> Result<()> {
        let root_uri = file_uri(&config.project_root);
        self.request(
            "initialize",
            json!({
                "processId": Value::Null,
                "rootUri": root_uri,
                "workspaceFolders": [{"uri": root_uri, "name": config.project_name}],
                "capabilities": {"experimental": {"serverStatusNotification": true}},
                "initializationOptions": self.configuration,
            }),
            deadline,
        )?;
        self.notify("initialized", json!({}))?;
        self.notify(
            "workspace/didChangeConfiguration",
            json!({"settings": {"rust-analyzer": self.configuration}}),
        )
    }

    fn definition(
        &mut self,
        uri: &str,
        line: usize,
        character: usize,
        deadline: Instant,
    ) -> Result<Vec<DefinitionLocation>> {
        let result = self.request(
            "textDocument/definition",
            json!({
                "textDocument": {"uri": uri},
                "position": {"line": line, "character": character},
            }),
            deadline,
        )?;
        Ok(result
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or(std::slice::from_ref(&result))
            .iter()
            .filter_map(definition_location)
            .collect())
    }

    fn request(&mut self, method: &str, params: Value, deadline: Instant) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))?;
        loop {
            let message = self.receive(deadline, "request")?;
            if message["id"].as_u64() == Some(id) {
                if !message["error"].is_null() {
                    bail!("rust-analyzer {method} error: {}", message["error"]);
                }
                return Ok(message.get("result").cloned().unwrap_or(Value::Null));
            }
            if message.get("method").is_some() && message.get("id").is_some() {
                self.respond_to_server(&message)?;
            }
        }
    }

    fn respond_to_server(&mut self, message: &Value) -> Result<()> {
        let result = match message["method"].as_str().unwrap_or_default() {
            "workspace/configuration" => {
                let values = message["params"]["items"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|item| {
                        item["section"]
                            .as_str()
                            .map(|section| configuration_value(&self.configuration, section))
                            .unwrap_or(Value::Null)
                    })
                    .collect();
                Value::Array(values)
            }
            "workspace/workspaceFolders" => json!([]),
            _ => Value::Null,
        };
        self.send(&json!({"jsonrpc": "2.0", "id": message["id"], "result": result}))
    }

    fn wait_until_ready(&mut self, deadline: Instant) -> Result<()> {
        loop {
            let message = self.receive(deadline, "project loading")?;
            if message["method"] == "experimental/serverStatus"
                && message["params"]["quiescent"] == true
            {
                if message["params"]["health"] == "error" {
                    bail!(
                        "rust-analyzer project loading failed: {}",
                        message["params"]["message"]
                    );
                }
                return Ok(());
            }
            if message.get("method").is_some() && message.get("id").is_some() {
                self.respond_to_server(&message)?;
            }
        }
    }

    fn receive(&self, deadline: Instant, operation: &str) -> Result<Value> {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            bail!("rust-analyzer {operation} timed out");
        }
        self.receiver
            .recv_timeout(remaining)
            .map_err(|_| anyhow!("rust-analyzer {operation} timed out"))?
            .map_err(|error| anyhow!(error))
    }

    fn notify(&mut self, method: &str, params: Value) -> Result<()> {
        self.send(&json!({"jsonrpc": "2.0", "method": method, "params": params}))
    }

    fn send(&mut self, message: &Value) -> Result<()> {
        let body = serde_json::to_vec(message)?;
        write!(self.stdin, "Content-Length: {}\r\n\r\n", body.len())?;
        self.stdin.write_all(&body)?;
        self.stdin.flush()?;
        Ok(())
    }

    fn shutdown(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(2);
        let _ = self.request("shutdown", Value::Null, deadline);
        let _ = self.notify("exit", Value::Null);
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn configuration_value(configuration: &Value, section: &str) -> Value {
    let section = section.strip_prefix("rust-analyzer.").unwrap_or(section);
    let mut value = configuration;
    for part in section.split('.') {
        let Some(next) = value.get(part) else {
            return Value::Null;
        };
        value = next;
    }
    value.clone()
}

impl Drop for LspClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn read_lsp_message(reader: &mut impl BufRead) -> Result<Option<Value>> {
    let mut content_length = None;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header)? == 0 {
            return Ok(None);
        }
        if header == "\r\n" || header == "\n" {
            break;
        }
        if let Some(length) = header
            .strip_prefix("Content-Length:")
            .and_then(|value| value.trim().parse::<usize>().ok())
        {
            content_length = Some(length);
        }
    }
    let length = content_length.context("LSP message omitted Content-Length")?;
    let mut body = vec![0; length];
    reader.read_exact(&mut body)?;
    Ok(Some(serde_json::from_slice(&body)?))
}

fn definition_location(value: &Value) -> Option<DefinitionLocation> {
    let location = value
        .as_array()
        .and_then(|locations| locations.first())
        .unwrap_or(value);
    let uri = location["targetUri"]
        .as_str()
        .or_else(|| location["uri"].as_str())?;
    let range = location
        .get("targetSelectionRange")
        .or_else(|| location.get("range"))?;
    Some(DefinitionLocation {
        uri: uri.to_string(),
        line: range["start"]["line"].as_u64()? as usize,
        character: range["start"]["character"].as_u64()? as usize,
    })
}

fn file_uri(path: &Path) -> String {
    let path = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let normalized = normalize_slashes(path);
    #[cfg(windows)]
    let normalized = if let Some(path) = normalized.strip_prefix("//?/UNC/") {
        format!("//{path}")
    } else {
        normalized
            .strip_prefix("//?/")
            .unwrap_or(&normalized)
            .to_string()
    };
    let encoded = percent_encode(&normalized);
    if normalized.starts_with("//") {
        format!("file:{encoded}")
    } else if normalized.starts_with('/') {
        format!("file://{encoded}")
    } else {
        format!("file:///{encoded}")
    }
}

fn uri_path(uri: &str) -> Option<PathBuf> {
    let encoded = uri.strip_prefix("file://")?;
    let decoded = percent_decode(encoded)?;
    #[cfg(windows)]
    let decoded = if decoded
        .as_bytes()
        .get(1)
        .is_some_and(u8::is_ascii_alphabetic)
        && decoded.as_bytes().get(2) == Some(&b':')
    {
        decoded[1..].to_string()
    } else if !decoded.starts_with('/') {
        format!("//{decoded}")
    } else {
        decoded
    };
    Some(PathBuf::from(decoded))
}

fn percent_encode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric()
                || matches!(byte, b'/' | b':' | b'-' | b'_' | b'.' | b'~')
            {
                (byte as char).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}

fn percent_decode(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = std::str::from_utf8(bytes.get(index + 1..index + 3)?).ok()?;
            decoded.push(u8::from_str_radix(hex, 16).ok()?);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

fn detached_rust_project(config: &LensConfig, facts: &[FileFacts]) -> Result<Option<PathBuf>> {
    let mut metadata = Command::new("cargo");
    metadata.args(["metadata", "--format-version", "1", "--all-features"]);
    if config.identity_offline {
        metadata.arg("--offline");
    }
    let metadata_ok = crate::command_runner::checked_output(
        metadata.current_dir(&config.project_root),
        Duration::from_secs(config.identity_timeout_seconds),
    )
    .is_ok();
    if metadata_ok {
        return Ok(None);
    }
    fs::create_dir_all(&config.output_dir)?;
    let path = config.output_dir.join("rust-project.json");
    let roots = facts
        .iter()
        .filter(|fact| fact.target_kind != "module")
        .collect::<Vec<_>>();
    let roots = if roots.is_empty() {
        facts.first().into_iter().collect::<Vec<_>>()
    } else {
        roots
    };
    let dependency_roots = roots
        .iter()
        .enumerate()
        .map(|(index, fact)| (index, fact.package_name.clone(), fact.target_kind.clone()))
        .collect::<Vec<_>>();
    let target_cfgs = rustc_cfgs(&config.project_root);
    let crates = roots
        .iter()
        .enumerate()
        .map(|(index, fact)| {
            let mut cfgs = target_cfgs.clone();
            cfgs.extend(feature_cfgs(config, &fact.path));
            let mut seen_packages = std::collections::BTreeSet::new();
            let deps = dependency_roots
                .iter()
                .filter(|(candidate, package, kind)| {
                    if *candidate == index {
                        return false;
                    }
                    if package == &fact.package_name {
                        return fact.target_kind != "lib"
                            && kind == "lib"
                            && seen_packages.insert(package.clone());
                    }
                    seen_packages.insert(package.clone())
                })
                .map(|(candidate, package, _)| {
                    json!({"crate": candidate, "name": package.replace('-', "_")})
                })
                .collect::<Vec<_>>();
            json!({
                "root_module": canonical_key(absolute_path(config, &fact.path)),
                "edition": "2024",
                "display_name": fact.target_name,
                "deps": deps,
                "cfg": cfgs,
                "env": {},
                "is_workspace_member": true,
                "source": {
                    "include_dirs": [canonical_key(&config.project_root)],
                    "exclude_dirs": [canonical_key(config.project_root.join("target"))]
                }
            })
        })
        .collect::<Vec<_>>();
    write_json(&path, &json!({"crates": crates}))?;
    Ok(Some(path))
}

fn rustc_cfgs(project_root: &Path) -> Vec<String> {
    crate::command_runner::checked_output(
        Command::new("rustc")
            .args(["--print", "cfg"])
            .current_dir(project_root),
        Duration::from_secs(3),
    )
    .ok()
    .map(|output| output.stdout)
    .map(|output| output.lines().map(str::to_string).collect())
    .unwrap_or_default()
}

fn feature_cfgs(config: &LensConfig, source_path: &str) -> Vec<String> {
    let mut directory = absolute_path(config, source_path)
        .parent()
        .map(Path::to_path_buf);
    while let Some(current) = directory {
        let manifest = current.join("Cargo.toml");
        if manifest.is_file() {
            let features = fs::read_to_string(manifest)
                .ok()
                .and_then(|contents| toml::from_str::<toml::Value>(&contents).ok())
                .and_then(|manifest| manifest.get("features").cloned())
                .and_then(|features| features.as_table().cloned())
                .map(|features| {
                    features
                        .keys()
                        .map(|feature| format!("feature=\"{feature}\""))
                        .collect()
                })
                .unwrap_or_default();
            return features;
        }
        if current == config.project_root {
            break;
        }
        directory = current.parent().map(Path::to_path_buf);
    }
    Vec::new()
}

#[cfg(test)]
#[path = "semantic_macro_tests.rs"]
mod macro_tests;

#[cfg(test)]
mod tests {
    use super::{
        definition_location, file_uri, is_semantic_candidate, percent_decode, percent_encode,
        resolve, uri_path, utf16_column_to_byte,
    };
    use crate::config::LensConfig;
    use crate::facts::{DependencyReferenceFact, FileFacts};
    use serde_json::json;
    use std::fs;
    use std::process::Command;

    #[test]
    fn cache_keys_cover_contents_member_manifests_and_configuration() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        fs::create_dir_all(root.path().join("src"))?;
        fs::create_dir_all(root.path().join("member"))?;
        let source = root.path().join("src/lib.rs");
        fs::write(&source, "pub fn before() {}")?;
        let manifest = root.path().join("member/Cargo.toml");
        fs::write(&manifest, "before")?;
        let config_path = root.path().join("rqlens.toml");
        fs::write(&config_path, "source_roots = [\"src\"]\n")?;
        let mut config = LensConfig::load(Some(config_path))?;
        for path in [&source, &manifest] {
            let before = super::fingerprint(&config, &[], Some("analyzer-v1"));
            let text = fs::read_to_string(path)?.replace("before", "after_");
            crate::util::write_preserving_mtime(path, text)?;
            assert_ne!(
                before,
                super::fingerprint(&config, &[], Some("analyzer-v1"))
            );
        }
        let before = super::fingerprint(&config, &[], Some("analyzer-v1"));
        config.identity_build_macros = true;
        assert_ne!(
            before,
            super::fingerprint(&config, &[], Some("analyzer-v1"))
        );
        let protocol = root.path().join("protocol.xml");
        fs::write(&protocol, "<protocol name='before' />")?;
        config
            .fingerprint_inputs
            .push(protocol.to_string_lossy().into_owned());
        let before = super::fingerprint(&config, &[], Some("analyzer-v1"));
        crate::util::write_preserving_mtime(&protocol, "<protocol name='after_' />")?;
        assert_ne!(
            before,
            super::fingerprint(&config, &[], Some("analyzer-v1"))
        );
        let before = super::fingerprint(&config, &[], Some("analyzer-v1"));
        config.verification.all_features = true;
        assert_ne!(
            before,
            super::fingerprint(&config, &[], Some("analyzer-v1"))
        );
        config
            .source_roots
            .push(root.path().join("missing").to_string_lossy().into_owned());
        assert!(super::fingerprint(&config, &[], Some("analyzer-v1")).is_none());
        Ok(())
    }

    #[test]
    fn macro_execution_requires_explicit_trust_and_does_not_repeat_feature_flags()
    -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let mut config = required_config(root.path(), "trust-fixture")?;
        for enabled in [false, true] {
            config.identity_build_macros = enabled;
            for offline in [false, true] {
                config.identity_offline = offline;
                let settings = super::analyzer_configuration(&config, None);
                assert_eq!(settings["procMacro"]["enable"], enabled);
                assert_eq!(settings["cargo"]["buildScripts"]["enable"], enabled);
                assert_eq!(settings["cargo"]["features"], "all");
                assert_eq!(
                    settings["cargo"]["extraArgs"],
                    if offline {
                        json!(["--offline"])
                    } else {
                        json!([])
                    }
                );
            }
        }
        Ok(())
    }

    #[test]
    fn unmapped_virtual_origin_is_not_disguised_by_an_external_generator() {
        let reference = DependencyReferenceFact {
            raw_path: "crate::generated::Message".into(),
            line: 1,
            column: 0,
        };
        for virtual_first in [false, true] {
            let mut uris = [
                "file:///registry/generator.rs",
                "rust-analyzer://macro/generated.rs",
            ];
            if virtual_first {
                uris.reverse();
            }
            let locations = uris
                .into_iter()
                .map(|uri| super::DefinitionLocation {
                    uri: uri.into(),
                    line: 0,
                    character: 0,
                })
                .collect();
            let resolved = super::resolved_definitions(&reference, locations, &Default::default());
            assert_eq!(resolved.status, "unmapped_generated_definition");
        }
    }

    #[test]
    fn generated_navigation_prefers_local_origin_over_external_generator() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let path = root.path().join("lib.rs");
        fs::write(&path, "scanner::generate_code!();\n")?;
        let index = [(
            super::canonical_key(&path),
            ("fixture::lib::protocol".into(), "protocol".into()),
        )]
        .into();
        let reference = DependencyReferenceFact {
            raw_path: "crate::protocol::generated::Message".into(),
            line: 2,
            column: 0,
        };
        for local_first in [false, true] {
            let mut locations = vec![
                super::DefinitionLocation {
                    uri: "file:///registry/scanner/src/lib.rs".into(),
                    line: 10,
                    character: 7,
                },
                super::DefinitionLocation {
                    uri: file_uri(&path),
                    line: 0,
                    character: 0,
                },
            ];
            if local_first {
                locations.reverse();
            }
            let resolved = super::resolved_definitions(&reference, locations, &index);
            assert_eq!(resolved.status, "resolved_local");
            assert_eq!(
                resolved.target_module_id.as_deref(),
                Some("fixture::lib::protocol")
            );
            assert_eq!(
                resolved.symbol_identity.as_deref(),
                Some("fixture::lib::protocol::Message")
            );
        }
        let virtual_location = super::DefinitionLocation {
            uri: "rust-analyzer://macro-expansion/generated.rs".into(),
            line: 0,
            character: 0,
        };
        let unresolved = super::resolved_definitions(&reference, vec![virtual_location], &index);
        assert_eq!(unresolved.status, "unmapped_generated_definition");
        assert!(unresolved.target_module_id.is_none());
        let external = super::resolved_definitions(
            &reference,
            vec![super::DefinitionLocation {
                uri: "file:///registry/external/lib.rs".into(),
                line: 0,
                character: 0,
            }],
            &index,
        );
        assert_eq!(external.status, "resolved_external");
        assert_eq!(
            super::reference_symbol("crate::protocol::generated::self"),
            "generated"
        );
        assert_eq!(
            super::reference_symbol("crate::protocol::generated::*"),
            "generated"
        );
        Ok(())
    }

    #[test]
    fn uri_encoding_round_trips_paths() {
        let path = "/tmp/a project/src/über.rs";
        assert_eq!(percent_decode(&percent_encode(path)).as_deref(), Some(path));
    }

    #[test]
    fn file_uris_round_trip_native_paths() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let path = root.path().join("a project.rs");
        fs::write(&path, "")?;
        let uri = file_uri(&path);
        assert!(uri.starts_with("file:///"));
        let decoded = uri_path(&uri).ok_or_else(|| anyhow::anyhow!("file URI should decode"))?;
        assert_eq!(fs::canonicalize(decoded)?, fs::canonicalize(path)?);
        Ok(())
    }

    #[test]
    fn parses_location_links() {
        let Some(location) = definition_location(&json!([{
            "targetUri": "file:///tmp/lib.rs",
            "targetSelectionRange": {"start": {"line": 4, "character": 7}, "end": {"line": 4, "character": 10}}
        }])) else {
            panic!("location should parse");
        };
        assert_eq!(location.line, 4);
        assert_eq!(location.character, 7);
    }

    #[test]
    fn converts_utf16_columns() {
        assert_eq!(utf16_column_to_byte("a🦀b", 3), 5);
    }

    #[test]
    fn bare_relative_roots_are_not_semantic_dependencies() {
        let source = FileFacts::test_fact("/project/src/lib.rs", "lib");
        let facts = [source.clone()];
        for raw_path in ["crate", "self", "super", "crate::*"] {
            assert!(!is_semantic_candidate(raw_path, &source, &facts));
        }
        assert!(is_semantic_candidate("crate::domain", &source, &facts));
    }

    fn required_config(root: &std::path::Path, name: &str) -> anyhow::Result<LensConfig> {
        let path = root.join("rqlens.toml");
        fs::write(
            &path,
            format!(
                "project_name = '{name}'\nsource_roots = ['src']\n[rust]\nidentity_resolution = 'required'\n"
            ),
        )?;
        LensConfig::load(Some(path))
    }

    #[test]
    fn required_mode_rejects_an_unavailable_analyzer() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let mut config = required_config(root.path(), "semantic-test")?;
        config.rust_analyzer = root.path().join("missing-rust-analyzer");
        config.identity_timeout_seconds = 1;
        let source_path = root.path().join("src/lib.rs").to_string_lossy().to_string();
        let mut fact = FileFacts::test_fact(&source_path, "lib");
        fact.graph
            .dependency_references
            .push(DependencyReferenceFact {
                raw_path: "crate::domain".to_string(),
                line: 1,
                column: 0,
            });
        let error = match resolve(&config, &mut [fact]) {
            Ok(_) => panic!("required mode must fail"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("required but incomplete"));
        Ok(())
    }

    #[test]
    fn detached_project_resolves_and_caches_local_definitions() -> anyhow::Result<()> {
        if crate::command_runner::checked_output(
            Command::new("rust-analyzer").arg("--version"),
            std::time::Duration::from_secs(3),
        )
        .is_err()
        {
            return Ok(());
        }
        let root = tempfile::tempdir()?;
        fs::create_dir_all(root.path().join("src"))?;
        fs::write(
            root.path().join("src/lib.rs"),
            "mod domain;\nfn run() { domain::go(); }\n",
        )?;
        fs::write(root.path().join("src/domain.rs"), "pub fn go() {}\n")?;
        let mut config = required_config(root.path(), "semantic-fixture")?;
        config.identity_timeout_seconds = 20;
        let library_path = root.path().join("src/lib.rs").to_string_lossy().to_string();
        let mut library = FileFacts::test_fact(&library_path, "lib");
        library.package_name = "semantic-fixture".to_string();
        library.target_name = "semantic_fixture".to_string();
        library.module_id = "semantic-fixture::semantic_fixture::lib".to_string();
        library.target_kind = "lib".to_string();
        library
            .graph
            .dependency_references
            .push(DependencyReferenceFact {
                raw_path: "domain::go".to_string(),
                line: 2,
                column: 19,
            });
        let domain_path = root
            .path()
            .join("src/domain.rs")
            .to_string_lossy()
            .to_string();
        let mut domain = FileFacts::test_fact(&domain_path, "domain");
        domain.package_name = "semantic-fixture".to_string();
        domain.target_name = "semantic_fixture".to_string();
        domain.module_id = "semantic-fixture::semantic_fixture::domain".to_string();
        let mut facts = vec![library, domain];

        let first = resolve(&config, &mut facts)?;
        assert_eq!(first.local_definition_count, 1);
        assert!(first.complete);
        assert_eq!(
            facts[0].graph.resolved_dependencies[0]
                .symbol_identity
                .as_deref(),
            Some("semantic-fixture::semantic_fixture::domain::go")
        );
        let second = resolve(&config, &mut facts)?;
        assert!(second.cache_hit);
        assert_eq!(second.duration_ms, 0);
        Ok(())
    }
}
