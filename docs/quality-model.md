# Rust Quality Model

rust-quality-lens separates evidence into three classes:

1. **Verified gates** execute an authoritative tool and report `passed`, `failed`, `unavailable`, `timed-out`, or `skipped`.
2. **Static findings** identify concrete source or repository evidence requiring review.
3. **Heuristic signals** rank complexity, coupling, duplication, and architecture pressure for triage.

These classes are not combined into a universal quality score. A failed command is not a heuristic, an unsafe block is not automatically a defect, and a missing tool never counts as a pass.

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

- Verification commands use the project's configured workspace, targets, and features. `--all-features` is opt-in because features can be mutually exclusive.
- Optional tools report `skipped` unless enabled. Once enabled, unavailable tools make measurement confidence partial.
- Unsafe findings check for reviewable contracts such as `// SAFETY:` rationale and `# Safety` documentation. Presence of text is evidence, not proof of soundness.
- Panic-path findings are warnings in production code and advisory in test code. `unwrap`, `expect`, and `panic!` can be valid when an invariant is clear.
- Architecture percentile bands are empirical triage aids from a limited corpus, not defect probabilities or ecosystem-wide standards.

## Rule stability

Machine-readable rules use stable IDs such as `rust.official.clippy` and `rust.safety.undocumented-unsafe`. Rule meaning, source, severity, command, scope, and evidence are emitted in artifacts. Material rule changes require tests and release notes.
