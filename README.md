# rust-quality-lens

Reusable measurement JSON producers for Rust projects.

The goal is to make quality, correctness, and architecture-map
measurements portable across Rust projects without requiring each project to
vendor dashboard code or project-specific scripts.

## What It Measures

- verified Rust gates for rustfmt, compilation, Clippy, tests, doctests, and rustdoc
- safety-contract findings and explicit panic paths such as undocumented unsafe, `unwrap`, `expect`, and `panic!`
- project maturity evidence for MSRV, licensing, contribution, security, and release documentation
- function- and module-level complexity hotspots with score explanations
- token, AST, module-responsibility, and test-body duplication groups
- Rust escape hatches such as unsafe, FFI, raw memory, globals, and lint suppressions
- type health for broad structs, large enums, and wide impl surfaces
- code locality and hidden coupling
- architecture leverage and style pressure
- Cargo-qualified package, target, and module identities
- correctness catalog and aggregate coverage attribution
- architecture risk map data

The architecture map uses a versioned risk model so raw facts and derived risk
scores remain separate. See `docs/risk-model-v4.md` for the current weights and
empirical calibration.
Recent extractor and scoring upgrades are summarized in
`docs/measurement-upgrades.md`.

## Quick Start

Create a config in the Rust project you want to inspect:

```toml
project_name = "my-rust-project"
project_root = "."
output_dir = "target/analysis"

# source_roots is optional. RQLens discovers local Cargo package roots.
[rust]
identity_resolution = "auto"
rust_analyzer = "rust-analyzer"
identity_timeout_seconds = 60
identity_offline = true

[verification]
timeout_seconds = 600
workspace = true
all_targets = true
all_features = false
# Recommended in CI when Cargo.lock is committed:
locked = true
# Optional project-specific gates:
audit = false
deny = false
semver = false
# semver_baseline_rev = "v1.2.3"
feature_matrix = false
mutation = false       # cargo-mutants
flaky_test_runs = 1    # set to 2+ in a scheduled CI job
fuzz_targets = []      # cargo-fuzz target names
fuzz_seconds = 30
sanitizers = []        # address, leak, memory, thread; usually nightly
miri = false
```

Or scaffold one:

```powershell
cargo run --bin rqlens -- init
```

Run all static, dynamic, and architecture producers:

```powershell
cargo run --bin rqlens -- measure all --config rqlens.toml
```

Run the official Rust baseline and repository-practice checks:

```powershell
cargo run --bin rqlens -- verify --config rqlens.toml
cargo run --bin rqlens -- check --fail-on practice-failure --fail-on reliability-finding --config rqlens.toml
```

Verification writes `rust_practices.json`. It keeps authoritative command gates separate from static findings and heuristic architecture scores.

Export failed gates and source findings for GitHub code scanning:

```powershell
cargo run --bin rqlens -- sarif --config rqlens.toml
```

This writes `target/analysis/rqlens.sarif` by default.

Stable rule limits can preserve an accepted baseline while preventing regressions:

```toml
[policy.rules."rust.reliability.expect"]
level = "error"
max = 3
exclude_paths = ["src/generated/**"]
exclude_packages = ["generated-bindings"]
```

Every configured rule is reported in `policy_rule_evaluations`; exceeding an `error` limit fails `rqlens check`. `warning` and `advisory` limits remain visible without failing, and `off` disables the limit.

Temporary policy exceptions must be explicit and expiring:

```toml
[[policy.waivers]]
rule_id = "rust.project.license"
reason = "Repository owner license decision is pending"
owner = "project-maintainers"
expires = "2026-12-31"
# Optional path scope; `/**` performs a prefix match.
# path = "src/generated/**"
```

Active waivers are recorded in `policy_report.json` and omitted from SARIF. Expired waivers remain visible and no longer suppress findings.

Run one producer:

```powershell
cargo run --bin rqlens -- measure escape-hatches --config rqlens.toml
cargo run --bin rqlens -- measure reliability --config rqlens.toml
cargo run --bin rqlens -- measure api-health --config rqlens.toml
cargo run --bin rqlens -- measure clones --config rqlens.toml
cargo run --bin rqlens -- measure type-health --config rqlens.toml
cargo run --bin rqlens -- measure correctness --config rqlens.toml
cargo run --bin rqlens -- measure correctness-run --config rqlens.toml
cargo run --bin rqlens -- measure locality --config rqlens.toml
cargo run --bin rqlens -- measure leverage --config rqlens.toml
cargo run --bin rqlens -- measure map --config rqlens.toml
cargo run --bin rqlens -- measure coverage --config rqlens.toml
```

Coverage uses `cargo-llvm-cov`. If it is not installed, `coverage.json` is
still written with partial confidence and an explicit missing-input reason.
The Nix development shell and CI both include the coverage tool.

Apply CI policies to generated artifacts:

```powershell
cargo run --bin rqlens -- check --fail-on partial --fail-on test-failure --config rqlens.toml
cargo run --bin rqlens -- check --baseline baseline/analysis --fail-on regression --max-regression 5 --config rqlens.toml
```

Baseline regression checks compare both architecture risk and aggregate line coverage; `max-regression` is interpreted as risk-score points and coverage percentage points respectively.

Ingest normalized operational telemetry without coupling RQLens to a monitoring vendor:

```powershell
cargo run --bin rqlens -- telemetry --input production-signals.json --max-age-hours 24 --config rqlens.toml
cargo run --bin rqlens -- check --fail-on operational-failure --config rqlens.toml
```

The input contains a `window.end` RFC 3339 timestamp and a `signals` array. Each signal requires `id`, `kind`, explicit `healthy`/`breached`/`unknown` status, numeric `value`, and `source`; optional module identities connect observations to architecture evidence. Stale windows and unknown statuses reduce confidence rather than silently passing.

Collect inferred defect, revert, and security outcomes from Git history:

```powershell
cargo run --bin rqlens -- outcomes --config rqlens.toml
cargo run --bin rqlens -- outcomes --labels reviewed-outcomes.json --config rqlens.toml
```

The optional labels file is a JSON array with `commit`, `kind`, and optional `modules`, `paths`, and `source`. Inferred commit-message records remain explicitly marked `review_required`; reviewed labels are kept separate in `repository_outcomes.json`.

Capture Criterion benchmark estimates and compare them with a prior artifact:

```powershell
cargo run --bin rqlens -- performance --config rqlens.toml
cargo run --bin rqlens -- performance --baseline baseline/analysis --config rqlens.toml
```

This writes `performance.json`. `rqlens check --fail-on regression` also evaluates benchmark percentage increases when both current and baseline performance artifacts exist. Use `--no-run` to parse existing Criterion output without executing benches.

Validate whether risk rankings are associated with reviewed outcome labels across held-out projects:

```powershell
cargo run --bin rqlens -- validate \
  --project app=/path/to/app/target/analysis \
  --project library=/path/to/library/target/analysis \
  --output-dir target/validation
```

The report calculates precision-at-10, recall-at-10, mean percentile rank, and top-quartile lift. Reviewed labels are the default; `--include-inferred` is explicitly lower-confidence. Results are associations, not defect probabilities. For prospective validation, capture `map.json` before the outcome-label window.

Calibrate score distributions against multiple local checkouts:

```powershell
cargo run --bin rqlens -- calibrate \
  --project app=/path/to/app \
  --project library=/path/to/library \
  --output-dir target/calibration
```

The report contains per-project and pooled percentiles, identity coverage, and
top function/module hotspots. Percentile bands are triage aids, not defect
probabilities.

Print the task catalog:

```powershell
cargo run --bin rqlens -- catalog --config rqlens.toml
```

Print the config schema:

```powershell
cargo run --bin rqlens -- config-schema
```

Print artifact schemas:

```powershell
cargo run --bin rqlens -- artifact-schema
cargo run --bin rqlens -- artifact-schema map
```

Review changed Rust files:

```powershell
cargo run --bin rqlens -- review --changed-since main --config rqlens.toml
cargo run --bin rqlens -- review --diff-file pr.diff --config rqlens.toml
```

The review command writes `target/analysis/review.json`. It records changed hunk ranges and, when a fresh coverage artifact is available, reports executable changed-line coverage. It runs the standard
project extraction and filters hotspot, clone, escape-hatch, and type-health
rows to changed Rust files so module keys remain comparable to full-project
measurements. Review scope also reports changed tool entrypoints in
`scope.entrypoints`.

## Reading Outputs

RQLens deliberately separates three evidence classes:

- **verified gates**: authoritative commands passed, failed, timed out, or were unavailable
- **static findings**: concrete source or repository evidence requiring review
- **heuristic signals**: calibrated complexity, duplication, coupling, and architecture triage

These classes are not combined into a universal quality score. Missing tools never count as passing. See `docs/quality-model.md` for sources and interpretation.

Generated artifacts use a versioned envelope. Each document records `generated_at`, the producing `generator_version`, and a path-and-content `input_fingerprint` for traceability and stale-artifact detection. Array-oriented measurements are
stored under `records`; structured measurements such as correctness, coverage,
and the architecture map are stored under `data`. Every envelope carries
artifact-level `measurement_confidence`, a summary, tool identity, and risk
model identity, so an empty result remains distinguishable from incomplete
extraction. Artifact schema version `2` is the current contract.

`clones.json` reports multiple clone and duplication engines:

- `token`: normalized token-window repeats
- `ast`: repeated function or method structure
- `module-responsibility`: modules with similar API, type, impl, and dependency footprints
- `test-ast`: repeated non-trivial test body structures discovered from Cargo test targets

Syntax-backed outputs include target metadata when available:

- `target_kind`: `lib`, `bin`, `test`, `bench`, `example`, or `module`
- `entrypoint_kind`: runnable target kind for tool/test/example/bench entrypoints
- `is_entrypoint`: whether the module is a runnable tool-style entrypoint
- `module_id`: collision-safe `package::target::module` identity used by map nodes and edges

Architecture measurements resolve internal dependency references through one
bounded rust-analyzer LSP session. `identity_resolution` accepts `auto`
(semantic results with explicit fallback), `required` (fail unless every
candidate resolves), or `disabled` (Cargo/syntax identities only). Results are
cached in `semantic_identity_cache.json` using analyzer version and source
fingerprints. Offline projects that cannot load a complete Cargo graph receive
a generated `rust-project.json` with local targets, platform cfgs, and declared
features; inactive platform or macro-generated references remain labeled
`syntax_fallback`.

Map, locality, leverage, clone responsibility rows, and review output preserve
this metadata. Entrypoints remain visible in the architecture map, but outbound
orchestration and a small amount of layer-boundary crossing receive a scoring
allowance so `main.rs`, `src/bin/*.rs`, and explicit Cargo `[[bin]]` tools are
not judged like ordinary domain modules.

`rust-quality-lens` only writes JSON artifacts. It does not own a dashboard,
local web server, or project UI. The current dashboard and local runner live in
the sibling `project-management-board` repository, which consumes this lens'
catalog and JSON outputs.

For Scratchpad specifically, Scratchpad is now only the Rust editor project
being measured. The Scratchpad checkout provides the source tree, Cargo targets,
Rust probe binaries, benches, and `target/analysis` output directory. Dashboard
orchestration belongs to `project-management-board`.

## External Tools

The CLI is Rust. Syntax-aware extraction uses bundled helper sources. In a source checkout the workspace helper crate is reused; packaged installations materialize the same versioned sources in the user cache before building them. This keeps `cargo install` packages independent of the original checkout.

Baseline verification requires Cargo with rustfmt, Clippy, and rustdoc. Coverage requires `cargo-llvm-cov` and `llvm-tools-preview`. `cargo-audit` and `cargo-deny` are optional unless enabled in `[verification]`. Verification, correctness runs, and coverage share the configured workspace, target, feature, exclusion, and lockfile scope; doctests remain a separate library-only Cargo gate because Cargo does not combine `--doc` with `--all-targets`.

For production CI, commit `Cargo.lock` where appropriate and set `locked = true`; enable RustSec and cargo-deny (or run equivalent dedicated CI jobs), and keep a separate job that compiles with the declared MSRV. RQLens' dependency MSRV row is metadata evidence, not a substitute for executing the MSRV toolchain.

## Validation

For changes inside this repository, run the same checks the lens uses on
itself:

```powershell
cargo fmt
cargo check --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run --bin rqlens -- verify
cargo run --bin rqlens -- measure all
bash scripts/check-package.sh
```

The final command refreshes `target/analysis/*.json`. Check
`correctness_review.json.summary` for failed or unknown tests and
`map.json.meta.summary.artifact_status` for missing or stale inputs.
