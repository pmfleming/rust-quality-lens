# Rust Quality Model

RQLens separates evidence that has different authority and interpretation. It
does not combine the evidence into a universal quality score.

## Evidence classes

| Class | Purpose | Examples |
| --- | --- | --- |
| Verified gates | Execute an authoritative tool | rustfmt, Clippy, tests, rustdoc |
| Static findings | Identify source or repository evidence for review | panic paths, unsafe contracts, missing project files |
| Heuristic signals | Rank maintenance and architecture pressure | complexity, duplication, coupling, change pressure |
| Contextual evidence | Preserve observations outside the quality score | benchmarks, repository outcomes, operational telemetry |

A failed command is not a heuristic. An unsafe block is not automatically a
defect. A missing tool never counts as a pass.

Contextual evidence also remains distinct. Outcome labels show association, not
proof that a score predicted a defect. Operational observations retain their
status, source, window, and freshness. Benchmark changes remain specific to the
measured workload.

## Authoritative sources

The Rust Foundation supports the ecosystem but does not mandate one coding
standard for every project. RQLens baseline rules primarily follow:

- [Rust Style Guide](https://doc.rust-lang.org/style-guide/)
- [rustfmt](https://github.com/rust-lang/rustfmt)
- [Clippy](https://doc.rust-lang.org/clippy/)
- [Cargo Book](https://doc.rust-lang.org/cargo/)
- [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- [Rust Reference](https://doc.rust-lang.org/reference/)
- [Rustonomicon](https://doc.rust-lang.org/nomicon/)
- [Unsafe Code Guidelines](https://rust-lang.github.io/unsafe-code-guidelines/)
- [RustSec](https://rustsec.org/)

## Interpretation

### Verification

Verification, correctness runs, and coverage share the configured workspace,
targets, features, exclusions, and lockfile policy. `--all-features` is opt-in
because Cargo features can be mutually exclusive.

Optional tools report `skipped` unless enabled. Once enabled, an unavailable
tool makes confidence partial. Optional gates include RustSec, cargo-deny,
cargo-semver-checks, cargo-hack, cargo-mutants, repeated tests, fuzz targets,
sanitizers, and Miri.

### Static findings

- Unsafe findings look for reviewable `// SAFETY:` rationale and `# Safety`
  documentation. Text is evidence, not proof of soundness.
- Panic paths are warnings in production and advisory in tests. `unwrap`,
  `expect`, and `panic!` can be valid when an invariant is clear.
- API-health findings count syntax-level `pub` documentation. They can include
  items in private modules and do not replace rustdoc reachability checks.

Fuzzing, Miri, and sanitizers add behavioral reachability evidence. Their
reports identify the target or sanitizer that ran; they do not prove that all
unsafe code is sound.

### Heuristic and contextual evidence

- Coverage is aggregate execution evidence from `cargo-llvm-cov`. It does not
  prove assertion quality or identify which test covered a line. Unsupported
  branch instrumentation is reported as unavailable, not as 100% coverage.
- Architecture percentiles are triage bands from a limited corpus, not defect
  probabilities or ecosystem standards.
- Criterion records estimates and percentage deltas. Existing results parsed
  with `--no-run` are partial because they may be stale.
- Commit-message outcomes are always marked `review_required`. Validation uses
  reviewed labels by default and measures ranking association, not causation.
- Telemetry uses a vendor-neutral contract. `breached` can fail policy;
  `unknown` and stale windows reduce confidence instead of passing.

## Confidence and freshness

Project artifacts record the generator version, generation time, input
fingerprint, and structured `measurement_confidence`. Confidence lists required,
observed, missing, stale, and unsupported inputs.

`complete: false` does not indicate a defect. It means the evidence cannot
support a complete claim. `rqlens check --fail-on partial` rejects missing,
partial, and fingerprint-stale standard artifacts.

Baseline comparisons require compatible risk-model versions. Risk, line
coverage, and Criterion regressions use `--max-regression` in their respective
units.

## Policy and waivers

Configured `[policy.rules."<rule-id>"]` entries set finding limits after active
waivers and path or package exclusions. Exceeding an `error` limit fails policy;
`warning` and `advisory` remain non-blocking. Architecture thresholds,
operational failures, regressions, and incomplete confidence are enabled through
separate `--fail-on` modes.

Waivers are temporary accepted exceptions. Each waiver requires a stable rule
ID, reason, owner, and `YYYY-MM-DD` expiry. It may target an exact path or a
`/**` prefix. Active waivers remain visible in the policy report and are omitted
from SARIF; expired waivers stop suppressing findings. Waivers do not alter the
underlying artifact or turn a failed measurement into a pass.

## Rule stability

Rules use stable IDs such as `rust.official.clippy` and
`rust.safety.undocumented-unsafe`. Artifacts include each rule's meaning,
source, severity, command, scope, and evidence. Material rule changes require
tests and release notes.
