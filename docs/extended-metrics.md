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
