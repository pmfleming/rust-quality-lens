use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant, UNIX_EPOCH};

use crate::config::{LensConfig, SemanticIdentityMode};
use crate::facts::{FileFacts, ResolvedDependencyFact};
use crate::util::{normalize_slashes, write_json};

const CACHE_VERSION: u64 = 8;

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
    let candidates = facts
        .iter()
        .map(|fact| {
            fact.graph
                .dependency_references
                .iter()
                .filter(|reference| is_semantic_candidate(&reference.raw_path, fact, facts))
                .cloned()
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let reference_count = candidates.iter().map(Vec::len).sum();
    if config.identity_resolution == SemanticIdentityMode::Disabled || reference_count == 0 {
        return Ok(IdentityResolutionSummary::disabled(
            config.identity_resolution,
            reference_count,
        ));
    }

    let analyzer_version = analyzer_version(config);
    let fingerprint = fingerprint(config, facts, analyzer_version.as_deref());
    if let Some(mut cached) = read_cache(config, &fingerprint) {
        for fact in facts.iter_mut() {
            fact.graph.resolved_dependencies = cached
                .resolved_by_path
                .remove(&normalize_slashes(&fact.path))
                .unwrap_or_default();
        }
        cached.summary.mode = config.identity_resolution;
        cached.summary.cache_hit = true;
        cached.summary.duration_ms = 0;
        return enforce_required(config, cached.summary);
    }

    let started = Instant::now();
    let deadline = started + Duration::from_secs(config.identity_timeout_seconds);
    let linked_project = detached_rust_project(config, facts)?;
    let client = LspClient::start(config, linked_project.as_deref(), deadline);
    let mut client = match client {
        Ok(client) => client,
        Err(error) => {
            let mut summary =
                IdentityResolutionSummary::disabled(config.identity_resolution, reference_count);
            summary.reason = Some(error.to_string());
            summary.analyzer_version = analyzer_version;
            return enforce_required(config, summary);
        }
    };

    let path_index = fact_path_index(config, facts);
    let mut resolved_count = 0;
    let mut local_definition_count = 0;
    let mut external_definition_count = 0;
    let mut unresolved_count = 0;
    for (fact, references) in facts.iter_mut().zip(candidates) {
        let source_path = absolute_path(config, &fact.path);
        let source_uri = file_uri(&source_path);
        let mut resolved = Vec::new();
        for reference in &references {
            if Instant::now() >= deadline {
                unresolved_count += 1;
                resolved.push(unresolved(reference, "timeout"));
                continue;
            }
            let definition = client.definition(
                &source_uri,
                reference.line.saturating_sub(1),
                reference.column,
                deadline,
            );
            match definition {
                Ok(Some(location)) => {
                    resolved_count += 1;
                    let target_path = uri_path(&location.uri);
                    let target_key = target_path.as_ref().map(canonical_key);
                    let target_fact = target_key.as_ref().and_then(|key| path_index.get(key));
                    let symbol = target_path
                        .as_ref()
                        .and_then(|path| identifier_at(path, location.line, location.character))
                        .unwrap_or_else(|| reference_symbol(&reference.raw_path));
                    let (status, target_module_id, target_module_key, symbol_identity) =
                        if let Some(target) = target_fact {
                            local_definition_count += 1;
                            (
                                "resolved_local",
                                Some(target.0.clone()),
                                Some(target.1.clone()),
                                Some(format!("{}::{symbol}", target.0)),
                            )
                        } else {
                            external_definition_count += 1;
                            ("resolved_external", None, None, None)
                        };
                    resolved.push(ResolvedDependencyFact {
                        raw_path: reference.raw_path.clone(),
                        line: reference.line,
                        column: reference.column,
                        status: status.to_string(),
                        backend: "rust_analyzer".to_string(),
                        target_path: target_path.map(normalize_slashes),
                        target_module_id,
                        target_module_key,
                        symbol_identity,
                    });
                }
                Ok(None) => {
                    unresolved_count += 1;
                    resolved.push(unresolved(reference, "unresolved"));
                }
                Err(error) => {
                    unresolved_count += 1;
                    resolved.push(unresolved(reference, &format!("error: {error}")));
                }
            }
        }
        fact.graph.resolved_dependencies = resolved;
    }
    client.shutdown();

    let summary = IdentityResolutionSummary {
        mode: config.identity_resolution,
        backend: "rust_analyzer".to_string(),
        available: true,
        complete: unresolved_count == 0,
        cache_hit: false,
        reference_count,
        resolved_count,
        local_definition_count,
        external_definition_count,
        unresolved_count,
        duration_ms: started.elapsed().as_millis(),
        analyzer_version,
        reason: (unresolved_count > 0)
            .then(|| format!("{unresolved_count} dependency references were not resolved")),
    };
    write_cache(config, facts, &fingerprint, &summary)?;
    enforce_required(config, summary)
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
    Command::new(&config.rust_analyzer)
        .arg("--version")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|version| version.trim().to_string())
}

fn fingerprint(config: &LensConfig, facts: &[FileFacts], analyzer_version: Option<&str>) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    CACHE_VERSION.hash(&mut hasher);
    analyzer_version.hash(&mut hasher);
    config.identity_offline.hash(&mut hasher);
    config.identity_timeout_seconds.hash(&mut hasher);
    normalize_slashes(&config.rust_analyzer).hash(&mut hasher);
    for fact in facts {
        let path = absolute_path(config, &fact.path);
        normalize_slashes(&path).hash(&mut hasher);
        if let Ok(metadata) = fs::metadata(path) {
            metadata.len().hash(&mut hasher);
            metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                .map(|duration| duration.as_nanos())
                .hash(&mut hasher);
        }
    }
    for name in ["Cargo.toml", "Cargo.lock"] {
        hash_file_metadata(&config.project_root.join(name), &mut hasher);
    }
    format!("{:016x}", hasher.finish())
}

fn hash_file_metadata(path: &Path, hasher: &mut impl Hasher) {
    normalize_slashes(path).hash(hasher);
    if let Ok(metadata) = fs::metadata(path) {
        metadata.len().hash(hasher);
        metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_nanos())
            .hash(hasher);
    }
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
        .rsplit("::")
        .next()
        .unwrap_or(raw)
        .to_string()
}

fn is_semantic_candidate(raw: &str, source: &FileFacts, facts: &[FileFacts]) -> bool {
    let raw = raw.trim_start_matches("::").trim_end_matches("::*");
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

struct LspClient {
    child: Child,
    stdin: ChildStdin,
    receiver: Receiver<Result<Value, String>>,
    next_id: u64,
    configuration: Value,
}

impl LspClient {
    fn start(
        config: &LensConfig,
        linked_project: Option<&Path>,
        deadline: Instant,
    ) -> Result<Self> {
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
                match read_lsp_message(&mut reader) {
                    Ok(Some(message)) => {
                        if sender.send(Ok(message)).is_err() {
                            break;
                        }
                    }
                    Ok(None) => break,
                    Err(error) => {
                        let _ = sender.send(Err(error.to_string()));
                        break;
                    }
                }
            }
        });
        let mut configuration = json!({
            "cargo": {
                "buildScripts": {"enable": false},
                "extraArgs": if config.identity_offline { json!(["--offline", "--all-features"]) } else { json!(["--all-features"]) },
                "features": "all",
                "allTargets": true
            },
            "procMacro": {"enable": false},
            "checkOnSave": false
        });
        if let Some(project) = linked_project {
            configuration["linkedProjects"] = json!([canonical_key(project)]);
        }
        let mut client = Self {
            child,
            stdin,
            receiver,
            next_id: 1,
            configuration: configuration.clone(),
        };
        let root_uri = file_uri(&config.project_root);
        let initialization_options = configuration;
        client.request(
            "initialize",
            json!({
                "processId": Value::Null,
                "rootUri": root_uri,
                "workspaceFolders": [{"uri": root_uri, "name": config.project_name}],
                "capabilities": {
                    "experimental": {"serverStatusNotification": true}
                },
                "initializationOptions": initialization_options,
            }),
            deadline,
        )?;
        client.notify("initialized", json!({}))?;
        client.notify(
            "workspace/didChangeConfiguration",
            json!({"settings": {"rust-analyzer": client.configuration.clone()}}),
        )?;
        client.wait_until_ready(deadline)?;
        Ok(client)
    }

    fn definition(
        &mut self,
        uri: &str,
        line: usize,
        character: usize,
        deadline: Instant,
    ) -> Result<Option<DefinitionLocation>> {
        let result = self.request(
            "textDocument/definition",
            json!({
                "textDocument": {"uri": uri},
                "position": {"line": line, "character": character},
            }),
            deadline,
        )?;
        Ok(definition_location(&result))
    }

    fn request(&mut self, method: &str, params: Value, deadline: Instant) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))?;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                bail!("rust-analyzer request timed out");
            }
            let message = self
                .receiver
                .recv_timeout(remaining)
                .map_err(|_| anyhow!("rust-analyzer request timed out"))?
                .map_err(|error| anyhow!(error))?;
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
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                bail!("rust-analyzer project loading timed out");
            }
            let message = self
                .receiver
                .recv_timeout(remaining)
                .map_err(|_| anyhow!("rust-analyzer project loading timed out"))?
                .map_err(|error| anyhow!(error))?;
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
    format!("file://{}", percent_encode(&normalized))
}

fn uri_path(uri: &str) -> Option<PathBuf> {
    let encoded = uri.strip_prefix("file://")?;
    Some(PathBuf::from(percent_decode(encoded)?))
}

fn percent_encode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'-' | b'_' | b'.' | b'~') {
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
    let metadata_ok = metadata
        .current_dir(&config.project_root)
        .output()
        .is_ok_and(|output| output.status.success());
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
    let target_cfgs = rustc_cfgs();
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

fn rustc_cfgs() -> Vec<String> {
    Command::new("rustc")
        .args(["--print", "cfg"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
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
mod tests {
    use super::{
        definition_location, percent_decode, percent_encode, resolve, utf16_column_to_byte,
    };
    use crate::config::{LensConfig, SemanticIdentityMode};
    use crate::facts::{DependencyReferenceFact, FileFacts};
    use serde_json::json;
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;

    #[test]
    fn uri_encoding_round_trips_paths() {
        let path = "/tmp/a project/src/über.rs";
        assert_eq!(percent_decode(&percent_encode(path)).as_deref(), Some(path));
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
    fn required_mode_rejects_an_unavailable_analyzer() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let config = LensConfig {
            project_name: "semantic-test".to_string(),
            project_root: root.path().to_path_buf(),
            source_roots: vec![root.path().join("src").to_string_lossy().to_string()],
            output_dir: root.path().join("target/analysis"),
            helper_manifest: PathBuf::from("unused"),
            identity_resolution: SemanticIdentityMode::Required,
            rust_analyzer: root.path().join("missing-rust-analyzer"),
            identity_timeout_seconds: 1,
            identity_offline: true,
        };
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
        if !Command::new("rust-analyzer")
            .arg("--version")
            .output()
            .is_ok_and(|output| output.status.success())
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
        let config = LensConfig {
            project_name: "semantic-fixture".to_string(),
            project_root: root.path().to_path_buf(),
            source_roots: vec![root.path().join("src").to_string_lossy().to_string()],
            output_dir: root.path().join("target/analysis"),
            helper_manifest: PathBuf::from("unused"),
            identity_resolution: SemanticIdentityMode::Required,
            rust_analyzer: PathBuf::from("rust-analyzer"),
            identity_timeout_seconds: 20,
            identity_offline: true,
        };
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
