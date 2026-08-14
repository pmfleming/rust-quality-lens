# Fallow Comparison Opportunities

This historical architecture review records nine improvement opportunities found by comparing `rust-quality-lens` with `fallow-rs/fallow` at commit `56b0a0da804d1b6824d54c0a6ffbe1b7e592ce27`.

The intent was not to copy Fallow wholesale. The useful patterns were clearer contracts, stronger conformance testing, CI-ready automation, and smaller internal boundaries. Most near-term items are now implemented; this document is retained to explain why the current structure exists.

## Original signals

The initial local review successfully ran:

```bash
cargo run --bin rqlens -- catalog
cargo run --bin rqlens -- measure all
cargo test --workspace
```

The largest implementation hotspots were originally concentrated in
`src/facts.rs`, `src/producers.rs`, `src/artifacts.rs`, and `src/main.rs`.
Fallow's relevant contrast was a more productized shape: a multi-crate
workspace, strict lint policy, typed output contracts, schemas, CI/action
surfaces, conformance fixtures, fuzzing, benchmarks, packaging, and editor or
agent integrations.

## Opportunities and current status

1. **Split the architecture — partially complete.**

   The repository is now a Cargo workspace and `rust_helpers` is a workspace
   member. A heavier split into packages such as `rqlens-core`, `rqlens-types`,
   `rqlens-extract`, and `rqlens-cli` remains deferred. The current module
   boundaries should stabilize before adding cross-crate API costs.

2. **Shrink producer hotspots — complete for the practical module split.**

   `src/producers.rs` is now a small dispatcher. Producer implementations live
   under `src/producers/`, including hotspots, clones, correctness, coverage,
   escape hatches, reliability, API health, type health, topology, practices,
   and map generation.

3. **Move scoring out of the CLI — complete.**

   `src/main.rs` parses commands and dispatches work. Shared confidence,
   provenance, path, and scoring logic lives under `src/measurement/`, while
   architecture formulas live in `src/risk_model.rs`.

4. **Add typed output contracts — partially complete.**

   Review output and shared envelope surfaces have typed contract scaffolding,
   and `rqlens artifact-schema` exposes machine-readable schemas for standard
   producer artifacts. Some producer payloads are still assembled with
   `serde_json::json!`; fully typed payloads and schemas for auxiliary reports
   remain useful follow-up work.

5. **Add config schema and initialization — complete.**

   Configuration discovery, value validation, `rqlens init`, and
   `rqlens config-schema` are implemented. The generated schema records the
   supported keys, accepted values, and defaults for external validation.

6. **Introduce changed-code review mode — complete.**

   `rqlens review --changed-since <rev>` and `rqlens review --diff-file <path>`
   scope findings to changed Rust files. Review output includes hunk ranges,
   changed entrypoints, and changed-line coverage when fresh coverage evidence
   exists.

7. **Avoid repeated Cargo helper overhead — complete for caching.**

   Bundled syntax helpers are built and cached, then executed directly instead
   of being invoked through repeated `cargo run` calls. Parallel extraction can
   still be considered where deterministic output and shared caches permit it.

8. **Strengthen lints and CI discipline — complete.**

   Workspace lint settings, formatter, check, Clippy, tests, rustdoc, helper
   synchronization, package smoke tests, MSRV checks, cross-platform tests,
   self-measurement, policy enforcement, and SARIF export are represented in
   `.github/workflows/ci.yml` and repository scripts.

9. **Broaden golden and conformance testing — partially complete.**

   The test suite includes mini and golden Rust fixtures, normalized snapshots,
   artifact-envelope conformance checks, and CLI integration coverage. The
   external validation corpus described in
   [Validation Corpus](validation-corpus.md) still needs periodic, reproducible
   execution and retained reports across more project types.

## Remaining priorities

The practical follow-up sequence is now:

1. Type the remaining public payload contracts and add schemas for auxiliary
   performance, outcome, telemetry, validation, calibration, policy, SARIF, and
   review outputs.
2. Automate pinned external-corpus runs while retaining project revisions,
   toolchains, configurations, fingerprints, and reviewed false positives.
3. Re-evaluate a multi-crate core/types/extract/CLI split only when module APIs
   are stable enough to justify the migration.
4. Add safe extraction parallelism only with deterministic conformance tests.

The original comparison was a planning aid, not a compatibility claim with
Fallow and not a commitment to adopt its packaging or integration surfaces.
