# Extended quality evidence

These observations are advisory and do not change calibrated architecture scores.

## Mutation effectiveness and per-test flakiness

Enable the existing verification options and run `rqlens verify`:

```toml
[verification]
mutation = true
flaky_test_runs = 5
```

`rust_practices.json` retains command gate status and adds `checks[].metrics` for
`rust.tests.mutation` and `rust.tests.flaky-repeat`. Incomplete structured results
also make the practices artifact partial, even if a command returned success.

Mutation runs use a new private output directory on every invocation. The
cargo-mutants `outcomes.json` adapter reports source locations, enclosing function
when available, and killed, survived, timeout and unviable counts. Its score is
`100 * killed / (killed + survived)`; a zero denominator is null. Timeouts and
unviable mutants are not counted as killed. Excluded/equivalent mutant counts are
unknown, not zero. Tool-configured selection still applies: this is not a measure
of every possible fault. Missing/unsupported reports, unsuccessful baselines and
interrupted commands remain partial. Logs in the private directory are temporary;
normalized records and command tails are retained in the practices artifact.

Repeated testing builds once with Cargo's configured workspace/target/features
scope, discovers executables from Cargo JSON, lists tests, and repeats each
libtest executable. Identity includes package ID, target kind/name, and qualified
test name. Each test records passed, failed, ignored and missing observations;
`flaky` means both a pass and failure were observed. Failure rate uses only actual
passes/failures, not ignored or missing runs. It is an observed sample rate, not a
prediction or proof that consistently passing tests are non-flaky.

Executables run in their package directory with `CARGO_MANIFEST_DIR` set. This is
native libtest execution, not a replacement for Cargo's normal test gate: custom
Cargo runners, custom harnesses and doctests are not covered. Unsupported listings,
crashes, missing results and truncated output are explicit incomplete evidence.
Stable libtest has no per-test durations; these remain null, not suite-time estimates.

## Temporal coupling and ownership concentration

`rqlens measure module-cohesion` now includes `data.maintenance`:

```toml
[history]
window_days = 90
max_commit_files = 50
exclude_paths = ["src/generated", "tests/fixtures"]
```

The recent window is `(HEAD_time - window_days, HEAD_time]`; the previous window
is the immediately preceding interval of equal length. HEAD's committer timestamp
anchors both windows, making reruns deterministic. These are observations as of
that revision, not a wall-clock claim about whether a maintainer is active today.

Only non-merge commits touching current measured module paths contribute. Large
commits are excluded before path filtering, with excluded counts retained. Path
exclusions are project-relative file/directory prefixes, not globs. Renames are
not followed, deleted modules are not reconstructed, and formatting-only commits
below the size limit are not automatically recognized. Ownership uses raw author
emails (no mailmap) and commit touches, not changed lines, review work or expertise.

Each window reports:

- Per-module contributor shares, largest share, HHI concentration (0–1) and Shannon
  entropy in bits, including the underlying author/commit counts.
- Pairwise co-change counts, support (`pair commits / eligible commits`), directional
  conditional probabilities, and lift (`pair commits * eligible commits / (A commits
  * B commits)`). The population is eligible commits touching measured modules.
- Whether a static dependency was observed in either direction. Absence is not
  proof that the modules are independent; graph identity confidence still applies.
- Mean, median and maximum measured modules touched per eligible commit.
- Whether each previous primary author appears anywhere in recent measured history.

No observations means empty rows and null amplification statistics, not zero risk.
Shallow, missing or malformed Git history is partial. Source/graph limitations are
also retained in the parent artifact. These metrics are not developer productivity
scores, a definitive bus factor or calibrated defect predictions.

## Structural cohesion

`rqlens measure module-cohesion` also writes `data.structural`. The old
`responsibility_focus` and `responsibility_dimension_count` fields remain for
compatibility and still mean declaration-category diversity, not actual cohesion.
`declaration_category_count` makes that meaning explicit.

Structural model v1 groups receiver methods across inherent impl blocks of a
locally declared struct **within one file and lexical module**. Each record has
Cargo module/package/target identity, qualified type name, method source lines,
directly accessed fields (including tuple fields), and `self.method()` calls.
Trait methods and static functions/constructors are excluded and counted.

For `n` receiver methods there are `n * (n - 1) / 2` possible unordered pairs:

- `field_sharing_percent`: pairs directly accessing at least one common field,
  divided by possible pairs. This is a TCC-like observation, not a canonical
  language-independent TCC implementation.
- `connected_pair_percent`: pairs in the same connected component of the graph
  whose edges are shared fields **or** direct internal receiver calls. This makes
  delegation and indirect connectivity visible without pretending they share fields.
- `component_count` and `observed_components`: separate method clusters, analogous
  to LCOM4-style relationship evidence.

Fewer than two methods or no directly observed receiver state is not applicable:
ratios/component count are null, not a penalty for stateless utilities. This is
not whole-type or whole-module semantic cohesion: other files, trait impls and
generated methods are outside the scope. Reads and writes both count as access;
no claim of alias analysis or mutation-flow resolution is made.

Macro bodies, whole-receiver aliases/escapes/destructuring, calls outside the group,
unknown fields, ambiguous ownership and cfg-dependent inventories are explicit
limitations. Observed relationships remain visible, but affected ratios are null
and confidence is partial. These source-only observations are not compiler-backed,
and do not change existing calibrated risk scores.

## Executed compatibility and resource profiles

Profiles are deliberately opt-in and are **not** run by `measure all`:

```bash
rqlens profiles --config rqlens.toml
rqlens profiles --require-complete
rqlens profiles --schema
```

They write `quality_profiles.json`, a separate versioned artifact with the input
fingerprint, runner/toolchain environment, commands, per-profile confidence and
explicit declared-profile denominators. Failures, unavailable tools and resource
budget breaches produce a nonzero exit **after writing evidence**. Profiles for
other host OSes are skipped and remain in the denominator; `--require-complete`
also rejects these skips and all other partial evidence. Each report describes one
runner; cross-runner matrix aggregation and baseline regression comparison are not
implemented. This command's gates are separate from `rqlens check` policies.

### Compatibility

```toml
[[profiles.compatibility]]
id = "minimum-default"
toolchain = "msrv"
operation = "check"

[[profiles.compatibility]]
id = "active-minimal-tests"
toolchain = "active"
operation = "test"
no_default_features = true
features = []

[[profiles.compatibility]]
id = "wasm-minimal"
toolchain = "stable"
host_os = "linux"
target = "wasm32-unknown-unknown"
operation = "check"
no_default_features = true
```

`active` uses the current Cargo/compiler. `msrv` resolves the effective root or
workspace `rust-version`, then executes that compiler, not merely a metadata
comparison. Other names select installed toolchains through `rustup run`. An exact
version matching the active compiler can run without rustup. Missing toolchains or
target support remain unavailable; RQLens never installs them automatically.

Each row overrides feature selection explicitly; workspace, all-targets, exclusions
and lockfile policy are inherited from `[verification]`. Cargo target defaults apply
when `target` is omitted. Builds use isolated target directories. `operation="test"`
executes configured Cargo tests, while `check` claims compilation only. It does not
claim doctest or runtime compatibility from cross-compilation. `no_std`/`alloc`
coverage requires declaring feature/target combinations that exercise those modes.

### Resources

```toml
[[profiles.resources]]
id = "release-size-and-build"
kind = "build"
release = true
peak_rss = true
budgets = { artifact_bytes = 20000000 }

[[profiles.resources]]
id = "parser-workload"
kind = "command"
command = ["target/release/parser-bench", "fixtures/large.json"]
runs = 3
peak_rss = true
instrumented_metrics = ["allocations", "allocated_bytes"]
budgets = { elapsed_ms = 2000, peak_rss_bytes = 100000000, allocations = 5000 }
```

Build profiles execute one cold build in a fresh target directory, then one warm
**no-source-change** build in the same directory with Cargo incremental mode enabled.
The latter is a warm/no-op cost, not a fabricated incremental-edit benchmark. External
compiler caches and OS caches are not cleared. Existing project target directories
are never cleaned. Output artifact records retain per-file sizes, package, target
and kind; `artifact_bytes` sums current workspace outputs excluding external
packages, build-script executables and `.rmeta`/`.d`/`.pdb` metadata. It is not a
stripped binary-size estimate or an attribution of dependency code inside binaries.
Temporary build outputs are removed after measurement.

Command profiles execute an argv directly (no implicit shell) in the project root.
Use trusted projects and commands: build scripts and workloads execute code. Declare
non-Rust workload inputs and prebuilt executables in `fingerprint_inputs` when their
contents affect a result. Build profile feature scope comes from `[verification]`;
command profiles run exactly the argv provided and can include build overhead.

Every observation has elapsed milliseconds. Opt-in peak RSS uses the GNU `time`
executable on Linux; `%M` is converted from KiB to bytes. It is the reported maximum
for the command/children, **not** the sum of simultaneous process-tree RSS, retained
heap or a leak measurement. Unsupported platforms or missing GNU time remain
partial, never zero. Command tails, status, profiler and runner provenance are retained.

Allocations, allocated bytes, instructions and cache misses require instrumentation.
The workload/adapter must write a fresh report to the unique path supplied in
`RQLENS_RESOURCE_REPORT` on **each** invocation:

```json
{"schema_version":1,"metrics":{"allocations":250,"allocated_bytes":16384}}
```

Use allocator instrumentation or an appropriate profiler adapter to supply real
counters. RQLens validates and normalizes them; it does not derive them from source
or claim a profiler ran merely because JSON exists. All requested counters must be
nonnegative integers; missing reports, missing counters or unsupported versions are
partial. Each run uses a new file path, preventing reuse of old results. Budgets
apply independently to each observation and unknown measurements cannot pass a
budget. Measurements are workload/profile/environment-specific, not universal scores.
