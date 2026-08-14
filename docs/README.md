# Documentation

This directory contains the design and interpretation notes for rust-quality-lens.

## Start here

- [Project README](../README.md): installation, common commands, generated outputs, and external tools
- [Quality model](quality-model.md): evidence classes, authoritative sources, confidence, and policy interpretation
- [Measurement upgrades](measurement-upgrades.md): extractor, artifact, confidence, and scoring changes
- [Validation corpus](validation-corpus.md): calibration limits and the outcome-validation protocol

The CLI is the source of truth for machine-readable interfaces:

```bash
rqlens --help
rqlens config-schema
rqlens artifact-schema
rqlens catalog
```

When running from a source checkout, replace `rqlens` with `cargo run --bin rqlens --`.

## Risk model history

- [Version 4](risk-model-v4.md) — current model; compiler-assisted dependency identity
- [Version 3](risk-model-v3.md) — historical; function hotspots and Cargo-qualified identities
- [Version 2](risk-model-v2.md) — historical; corrected aggregate formula and coverage evidence
- [Version 1](risk-model-v1.md) — historical; original weights and score categories

Risk scores from different model versions are not directly comparable. Artifacts include `risk_model_id` and `risk_model_version` so consumers can reject incompatible baselines.

## Project notes

- [Fallow comparison opportunities](fallow-comparison-opportunities.md): historical architecture review and implementation status
- [Contributing](../CONTRIBUTING.md): development gates and pull-request expectations
- [Code of Conduct](../CODE_OF_CONDUCT.md): community behavior
- [Security Policy](../SECURITY.md): private vulnerability reporting and support scope
- [Changelog](../CHANGELOG.md): unreleased user-visible changes

These notes are not additional CLI contracts. Generated config and artifact schemas take precedence if prose and a machine-readable interface ever disagree.
