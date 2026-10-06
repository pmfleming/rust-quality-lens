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
