# rust-quality-lens

`rust-quality-lens` (RQLens) produces versioned quality, correctness, and
architecture evidence for Rust projects. It writes JSON for CI systems and
other consumers; it does not provide a dashboard or collapse the evidence into
one universal quality score.

See the [documentation index](docs/README.md) for the quality model, measurement
design, validation protocol, and risk-model history.

## Requirements

- Rust 1.95 or newer
- Cargo with rustfmt, Clippy, and rustdoc for `verify`
- Git for history-based architecture and outcome evidence
- rust-analyzer for compiler-assisted dependency identity (optional in `auto`
  mode)

Coverage, auditing, mutation, fuzzing, sanitizers, and Miri require their
corresponding optional tools. Opt-in resource peak-RSS profiles require GNU time
on Linux; compatibility profiles may require installed rustup toolchains. Missing tools are reported explicitly and never
count as passing.

For opt-in compiler-expanded type inventory, build the separately pinned
`rustc-dev` adapter and configure `rust.compiler_inventory_driver`;
see [compiler inventory](docs/compiler-inventory.md). This handles macro/derive,
attribute, cfg, and resolved-ownership evidence for type-health, without claiming
complete body-level analysis. For separately opted-in post-expansion static
architecture rules, see [compiler architecture](docs/compiler-architecture.md).

## What RQLens measures

RQLens keeps three kinds of evidence separate:

- **Verified gates:** rustfmt, compilation, Clippy, tests, doctests, rustdoc,
  and configured optional tools.
- **Static findings:** panic paths, safety contracts, escape hatches, API and
  type health, test quality, architecture rules, and project practices.
- **Heuristic signals:** complexity hotspots, duplication, locality, leverage,
  cohesion, change pressure, and architecture risk.

It also records contextual performance, repository-outcome, and operational
telemetry evidence. See the [quality model](docs/quality-model.md) for the
interpretation and limits of each class.

## Install

From a source checkout:

```bash
cargo install --path . --locked
rqlens --version
```

The examples below use `rqlens`. In a source checkout, replace it with
`cargo run --locked --bin rqlens --`.

## Quick start

Scaffold `rqlens.toml`:

```bash
rqlens init
```

A minimal configuration is:

```toml
project_name = "my-rust-project"
project_root = "."
output_dir = "target/analysis"

[rust]
identity_resolution = "auto"

[verification]
workspace = true
all_targets = true
locked = true
```

`project_root` is relative to the configuration file. RQLens discovers local
Cargo package roots unless `source_roots` is set. If `--config` is omitted, it
searches the current directory and its parents for `rqlens.toml`.

Print the complete option set and defaults with:

```bash
rqlens config-schema
```

Unknown configuration fields are rejected. Declare non-Rust build/test inputs
(such as `include_str!` assets, schemas, or fixtures) explicitly when they affect
your results:

```toml
fingerprint_inputs = ["assets/schema.json", "tests/data"]
```

These are project-relative files or directories, not glob patterns. Missing
paths make evidence partial. Standard Rust/Cargo inputs, formatting/lint
configuration, and the repository-policy files checked by `verify` are included
automatically; Cargo's target directory and the analysis output are excluded
unless explicitly listed.

### Generated-symbol identity in trusted projects

Rust-analyzer build scripts and procedural macros are disabled by default during
identity resolution. To resolve generated protocol bindings, explicitly opt in:

```toml
fingerprint_inputs = ["protocols"] # XML and other non-Rust generator inputs

[rust]
identity_build_macros = true
identity_timeout_seconds = 180
```

**Only enable this for a trusted checkout and dependencies:** build scripts and
procedural macros execute their code. The setting participates in the semantic
cache fingerprint; declare generator inputs so their content changes invalidate
cached identities too. Macro definitions with a local invocation origin are
mapped to that measured module, even when the analyzer reports the external
generator first. Unmapped virtual definitions remain unresolved/partial rather
than being counted as external dependencies. Glob navigation targets the module,
while escape-hatch evidence still targets `*`.

### Measure a project

```bash
rqlens measure all --config rqlens.toml
```

`measure all` includes coverage and an executed correctness run, so it can be
expensive. Run a focused producer when full evidence is unnecessary:

```bash
rqlens measure hotspots
rqlens measure clones
rqlens measure reliability
rqlens measure map
```

For trusted projects with environment-dependent ignored fixtures, opt in explicitly:

```toml
[verification]
include_ignored = true
```

This passes `--include-ignored` to executed libtest tests, correctness runs, coverage,
and repeated test runs (also sanitizer/Miri tests when enabled). It does not change
compile-only test discovery or doctests. The default remains `false`; supply fixture
prerequisites before enabling it. Failures remain failures, not skipped evidence.

Use `rqlens measure --help` for the full producer list. Coverage uses
`cargo-llvm-cov`; when unavailable, RQLens writes partial evidence with an
explicit missing-input reason.

### Verify and enforce policy

```bash
rqlens verify
rqlens check \
  --fail-on practice-failure \
  --fail-on reliability-finding
```

`verify` writes authoritative command results to `rust_practices.json`.
`check` evaluates generated evidence without treating heuristic scores as
verified failures unless threshold policy is enabled.

Useful policy modes include:

```bash
rqlens check --fail-on partial --fail-on test-failure
rqlens check \
  --baseline baseline/analysis \
  --fail-on regression \
  --max-regression 5
```

Regression tolerance is measured in risk-score points, line-coverage percentage
points, or benchmark percentage increase, depending on the evidence. Baseline
artifacts must have compatible schemas and complete evidence. When current
coverage or benchmark evidence exists, its baseline counterpart is required;
unmatched benchmark IDs are reported as incomparable and fail regression policy.
Historical fingerprints need not match the current source.

Stable per-rule limits prevent finding-count regressions:

```toml
[policy.rules."rust.reliability.expect"]
level = "error"
max = 3
exclude_paths = ["src/generated/**"]
exclude_packages = ["generated-bindings"]
```

Configured rules require supported, current, complete evaluation evidence. Missing,
skipped, stale or unsupported checks report `incomplete`, not a verified zero;
error-level incomplete rules block policy. `rqlens catalog` exposes the draft
blocking-rule contracts and their remaining coverage gaps. See
[`contracts/blocking-rule-coverage.md`](contracts/blocking-rule-coverage.md),
including current compiler-diagnostic and external-rule limitations.

Temporary exceptions must include ownership and an expiry:

```toml
[[policy.waivers]]
rule_id = "rust.project.license"
reason = "Repository owner license decision is pending"
owner = "project-maintainers"
expires = "2026-12-31"
```

Waivable findings accepted by active waivers remain visible in `policy_report.json`
and in SARIF as accepted suppressions. Expired waivers no longer suppress findings.

### Export SARIF

```bash
rqlens sarif
```

This writes `target/analysis/rqlens.sarif` by default for code-scanning tools.
Practice diagnostics share policy's projection, with contributing observations
and invocation evidence preserved. Projected diagnostic fingerprints use the new
`rqlens/exact-diagnostic-v1` namespace; original IDs remain in observations.
See [the projection contract](contracts/practice-projection.md) for identity,
waiver/exclusion, scope and remaining reporter limitations.
Source and imported findings likewise retain their original evidence and explicit
policy selection; see [source/imported projection](contracts/source-projection.md).
Import adapter names must use only ASCII letters, digits, `-` or `_`; invalid names
are rejected rather than silently sanitized into another report's filename.

## Other workflows

| Goal | Command |
| --- | --- |
| Review changed Rust files | `rqlens review --changed-since main` |
| Read an existing diff | `rqlens review --diff-file pr.diff` |
| Capture Criterion results | `rqlens performance` |
| Compare benchmark results | `rqlens performance --baseline baseline/analysis` |
| Execute declared compatibility/resource profiles | `rqlens profiles --require-complete` |
| Collect repository outcomes | `rqlens outcomes` |
| Import reviewed outcome labels | `rqlens outcomes --labels reviewed-outcomes.json` |
| Ingest operational signals | `rqlens telemetry --input production-signals.json --max-age-hours 24` |
| Calibrate pinned projects | `rqlens calibrate --project app=/path/to/app` |
| Validate risk rankings | `rqlens validate --project app=/path/to/app/target/analysis` |
| Ingest external evidence | `rqlens ingest --adapter cargo-crap --input evidence.json` |
| Print task metadata | `rqlens catalog` |
| Print artifact schemas | `rqlens artifact-schema` |

`review` reports added/replaced lines and executable changed-line coverage when
fresh coverage exists. Unified diff context lines are excluded; Git-quoted UTF-8
filenames and renames are supported. Truncated or combined diffs are rejected
rather than silently producing incomplete review scopes. `performance` isolates each execution using Criterion's
`CRITERION_HOME` under Cargo's configured target directory (`rqlens-bench-runs/`).
Benchmarks that override that directory are reported as missing rather than
reusing old estimates. `performance --no-run` parses existing `target/criterion`
output (respecting Cargo's target-directory configuration) but marks it partial
because execution time cannot be established.

Outcome inference from commit messages is always marked `review_required`.
Validation uses reviewed labels by default and reports ranking associations,
not defect probabilities. Telemetry retains explicit status, source, window,
and freshness rather than turning missing or stale observations into a pass.
Observation age is checked with exact durations both at ingestion and whenever
operational policy consumes the evidence.

See [extended metrics](docs/extended-metrics.md) for structured mutation/flakiness,
maintenance history, structural cohesion, and opt-in compatibility/resource profiles.
The [latest self-review](docs/self-refactoring-4.md) records comparable before/after
measurements, measurement fixes, and remaining quality-gate failures.
The subsequent [strict-gate checkpoints](docs/strict-gates.md) document which
ceilings now pass and which evidence/size requirements remain open.

Use `<command> --help` for input contracts and command-specific options.

## Reading outputs

Standard measurement files use artifact envelope version 3. Each envelope
contains:

- generator and toolchain provenance;
- generation time and an input fingerprint;
- risk-model identity;
- structured measurement confidence and summary data;
- either `records` for row-oriented output or `data` for structured output.

Toolchain provenance includes active Rust, Cargo, host, LLVM, declared MSRV,
and pinned channel evidence. Auxiliary performance, outcome, telemetry,
validation, calibration, policy, SARIF, and review outputs use separate
contracts.

Print machine-readable producer schemas with:

```bash
rqlens artifact-schema
rqlens artifact-schema map
```

### Confidence and recovery

Confidence records required, observed, missing, stale, and unsupported inputs.
An empty complete result is therefore distinct from incomplete extraction.
`check --fail-on partial` rejects incomplete or fingerprint-stale standard
artifacts.

Each measurement run shares one pre-extraction fingerprint, toolchain snapshot,
source inventory, Cargo metadata result, and lazily constructed module graph.
Outputs are staged and input fingerprints are checked again before publication;
if inputs changed, the run fails without replacing previous artifacts. See
[Evidence Pipeline](docs/evidence-pipeline.md) for the contracts and limits.

RQLens normally extracts Rust facts with `syn`. If a readable file does not
parse, `tree-sitter-rust` recovers text metrics, function spans, and raw
complexity where possible. Recovered rows remain partial; calibrated hotspot
scores stay `null`, and missing architecture inputs do not become zero risk.

### Type-health abstention

Type/impl ownership is joined conservatively within Cargo package, target and
lexical module. Aliases, conditional compilation, unexpanded macros/attributes,
ambiguous owners and incomplete provenance make the inventory partial—even if
no type rows were extracted. Opaque expression macros can also generate
non-local implementations.

Unknown method/impl totals and structural scores are `null`, not zero.
`observed_method_count`, `observed_impl_block_count`, `observed_impl_file_count`
and `impl_files` retain syntactic observations, not compiled totals. Known zero
remains numeric when the syntax inventory is complete. Confidence propagates to
rows, envelopes, capabilities and policy; conservative abstention can therefore
fail the strict partial-evidence gate. See [Semantic abstention](contracts/semantic-abstention.md).

`rqlens measure source-expansion` provides opt-in item/impl macro expansion
observations. It is excluded from `measure all` and does **not** clear partial
flags or populate compiler-verified type counts. See
[Source expansion](docs/source-expansion.md) for implemented scope and remaining integration.

### Complexity and clones

`hotspots.json` reports versioned cyclomatic and cognitive complexity.
Cyclomatic complexity counts Rust decisions, match arms and guards, `?`, and
short-circuit operators. Cognitive complexity also reflects nesting, `else`
branches, logical sequences, labeled jumps, and closures. These raw metrics do
not alter the calibrated hotspot score.

Hotspots also report versioned **Halstead** operator/operand counts, volume,
difficulty, and effort for functions and whole files. `halstead_effort` is
derived from actual syntax-token counts, not LOC or cyclomatic complexity.
Missing syntax yields `null`, not zero. This metric does not change risk scores
or predict developer hours; see [Halstead measurement](docs/halstead.md) for
counting rules, aggregation, research references, and historical compatibility.

`clones.json` distinguishes four engines:

- `token`: normalized token-window repeats within extracted function spans;
- `ast`: repeated function or method structure;
- `module-responsibility`: similar module responsibilities;
- `test-ast`: repeated non-trivial test bodies.

### Architecture identity

Architecture measurements use one bounded rust-analyzer session to resolve
internal dependency references. `[rust].identity_resolution` accepts:

- `auto`: semantic results with explicit syntax fallback;
- `required`: fail unless every candidate resolves;
- `disabled`: use Cargo and syntax identities only; references needing semantic
  resolution keep the evidence partial. An empty reference scope needs no backend.

Results are cached by analyzer version and source fingerprint. Map, locality,
leverage, clone, and review rows preserve Cargo-qualified package, target, and
module identities. Entrypoints remain visible but receive a small orchestration
allowance so tool wiring is not scored like domain code.

The current architecture model is documented in
[Risk Model v4](docs/risk-model-v4.md). Extractor and scoring changes are
summarized in [Measurement Design and Upgrades](docs/measurement-upgrades.md).

## Optional tools

Coverage requires `cargo-llvm-cov` and `llvm-tools-preview`. Optional gates use
`cargo-audit`, `cargo-deny`, `cargo-semver-checks`, `cargo-hack`,
`cargo-mutants`, cargo-fuzz, and nightly components for sanitizers or Miri.
rust-analyzer is optional in semantic `auto` mode and required in `required`
mode.

For CI, commit `Cargo.lock` where appropriate, set `locked = true`, and compile
with the declared MSRV in a dedicated job. Dependency MSRV evidence is not a
substitute for running that toolchain.

## Development

Run the repository checks documented in [Contributing](CONTRIBUTING.md). The
short form is:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo run --locked --bin rqlens -- verify
cargo run --locked --bin rqlens -- measure all
bash scripts/check-self-metrics.sh
```

RQLens only writes artifacts. Dashboard and local-runner integration belong to
consumer projects.

## Community, security, and license

Contributions are welcome; see [Contributing](CONTRIBUTING.md) and the
[Code of Conduct](CODE_OF_CONDUCT.md). Report vulnerabilities through the
private process in [Security](SECURITY.md).

Licensed under the [MIT License](LICENSE).
