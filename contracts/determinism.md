# Deterministic analysis evidence

For unchanged project/helper inputs, effective configuration, tool versions and prerequisite evidence, repeated static analysis preserves artifact values and array order. Comparisons may remove **only** `generated_at` and `measured_at` wall-clock provenance. They must not sort arrays or discard confidence, findings, identities, counts, reasons, scores or input fingerprints to obtain equality.

Map modules are emitted in Cargo-qualified identity order. For each module, dependency edges precede containment edges, and targets within each kind are sorted. Backend and symbol corroboration sets are already ordered.

Semantic `cache_hit` and `duration_ms` describe execution, not a different semantic result. They remain in the native cache/diagnostic state and are written to stderr as `semantic identity execution: ...`; they are no longer included in the artifact's semantic summary. Consumers previously reading these optional JSON properties should use the execution log. Resolution counts, availability, backend, completeness, reasons and analyzer version remain on the wire and must compare normally. This does not hide cold/warm disagreements in resolution.

`tests/trust.rs` repeats independent CLI processes against a fan-out fixture, preserves edge array order, and proves real source mutations still change evidence. `src/semantic.rs` checks that cold/warm execution telemetry does not alter semantic evidence and that changed unresolved counts do. The live macro/cache integration test remains required.

Run the real-project campaign after building:

```sh
cargo build --locked --bin rqlens
python3 acceptance/trust_campaign.py --output target/trust-determinism-new
```

The final T2 campaign passed 182 commands and 91 artifact comparisons; revisions, hashes and investigated failed attempts are recorded in `../acceptance/trust-t2-results.json`. Expansion to daemon-framework also exposed a panic from indexing a nonexistent root Cargo `[package]`; optional lookup and a virtual-workspace CLI regression now cover it.

The destination must not exist. The campaign freezes the executable, retains existing project policy, performs two serial runs of all 13 focused static producers (map last), records source state before Nix setup and after analysis, and exits nonzero for command failures, missing artifacts, source changes or differing normalized output. Nix setup uses `--no-write-lock-file`; update-daemon has no devShell and uses the invoking Rust environment. Reports are isolated from the daemon's existing output directory. It intentionally preserves partial evidence rather than calling exit 0 a verified-clean result.

New test, benchmark, coverage, profiler or external-tool execution observations are new inputs, not deterministic static re-analysis of the same observations. This campaign does not certify reproducibility of external programs or certify complete evidence from an unsupported/partial semantic backend.
