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
