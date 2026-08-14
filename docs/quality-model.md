# Rust Quality Model

rust-quality-lens separates evidence into three classes:

1. **Verified gates** execute an authoritative tool and report `passed`, `failed`, `unavailable`, `timed-out`, or `skipped`.
2. **Static findings** identify concrete source or repository evidence requiring review.
3. **Heuristic signals** rank complexity, coupling, duplication, and architecture pressure for triage.

These classes are not combined into a universal quality score. A failed command is not a heuristic, an unsafe block is not automatically a defect, and a missing tool never counts as a pass.

RQLens also preserves **contextual evidence** that should not be collapsed into
those three classes: Criterion performance estimates, inferred or reviewed
repository outcomes, and normalized operational telemetry. Outcome labels are
association data rather than proof that a risk score predicted a defect.
Operational observations retain explicit health status, source, window, and
freshness. Benchmark changes remain workload-specific and are not architecture
scores.

## Authoritative sources

The Rust Foundation supports the ecosystem but does not mandate one coding standard for all Rust projects. Baseline rules primarily follow Rust Project documentation:

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

- Verification, correctness runs, and coverage use the same configured workspace, targets, features, exclusions, and lockfile policy. `--all-features` is opt-in because features can be mutually exclusive.
- Optional tools report `skipped` unless enabled. Once enabled, unavailable tools make measurement confidence partial. Optional gates include RustSec, cargo-deny, cargo-semver-checks, cargo-hack feature matrices, cargo-mutants mutation testing, repeated flaky-test detection, and Miri.
- Unsafe findings check for reviewable contracts such as `// SAFETY:` rationale and `# Safety` documentation. Presence of text is evidence, not proof of soundness. Optional fuzz targets, Miri, and compiler-sanitizer test runs add behavioral reachability evidence; their reports state exactly which configured target or sanitizer executed.
- Panic-path findings are warnings in production code and advisory in test code. `unwrap`, `expect`, and `panic!` can be valid when an invariant is clear.
- API-health counts are syntax-level `pub` documentation inventory. They can include items inside private modules and are not a replacement for rustdoc's authoritative reachability and warning checks.
- Coverage is aggregate execution evidence from `cargo-llvm-cov`; it does not prove assertion quality or attribute a covered line to a particular test. Branch percentages are `null` with `available: false` when the LLVM export has no branch instrumentation (the stable-toolchain default), rather than being presented as 100%.
- Architecture percentile bands are empirical triage aids from a limited corpus, not defect probabilities or ecosystem-wide standards.
- Criterion output records point estimates and percentage deltas. RQLens does not replace benchmark design, statistical review, or controlled execution; `--no-run` output is partial because existing estimates may be stale.
- Repository outcome inference uses commit-message terms and is always marked `review_required`. Validation excludes inferred outcomes by default and measures retrospective ranking association, not causation.
- Operational telemetry is accepted only in a normalized, vendor-neutral shape. `breached` can fail an enabled operational policy; `unknown` and stale windows reduce confidence instead of passing.

## Confidence and freshness

Project artifacts record the generator version, generation time, input
fingerprint, and structured `measurement_confidence`. Confidence reports
required, observed, missing, stale, and unsupported inputs. `complete: false`
does not mean a defect was found; it means the available evidence cannot support
a complete claim.

`rqlens check --fail-on partial` treats missing, partial, and fingerprint-stale
standard measurement artifacts as policy failures. Baseline comparisons require
a compatible risk-model version. Risk, line-coverage, and Criterion regression
deltas use the configured `--max-regression` tolerance in their respective
units.

## Waivers

Policy waivers are reserved for accepted, temporary exceptions. Every waiver requires a stable rule ID, reason, owner, and `YYYY-MM-DD` expiry, and can optionally target an exact path or a `/**` path prefix. Active waivers remain visible in the policy report and are omitted from SARIF; expired waivers stop suppressing findings. A waiver does not turn a failed measurement into a pass or alter the underlying artifact.

## Rule stability

Machine-readable rules use stable IDs such as `rust.official.clippy` and `rust.safety.undocumented-unsafe`. Rule meaning, source, severity, command, scope, and evidence are emitted in artifacts. Material rule changes require tests and release notes.

Configured `[policy.rules."<rule-id>"]` entries evaluate absolute finding limits after active waivers and explicit path/package exclusions. An exceeded `error` limit fails policy; `warning` and `advisory` limits provide non-blocking regression visibility. Heuristic architecture thresholds remain separate and are enabled with `check --fail-on threshold`; operational, regression, and confidence policies are likewise opt-in through their corresponding `--fail-on` values.
