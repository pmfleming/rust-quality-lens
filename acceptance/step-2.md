# Closure step 2 — Evidence boundaries

Implemented an additive typed wire vocabulary for run identity, findings, measurements and comparisons, complementing capability v1. Shared malformed/missing-field fixtures execute through native validators. Native artifact readers reject malformed `evidence_contract` extensions; legacy envelopes may omit the extension.

Reporter conformance now tests disposition separately from impact and retains waived findings. TS adds `audit --format sarif`; QML SARIF records accepted suppressions. Rust policy and SARIF share disposition/operational-waivability interpretation.

Fixed a reproduced QML required-verification bug: disabling/suppressing test findings could yield a passing audit after a required test failed. Audit and quality contract now use unsuppressible required-check failures, independently of findings.

Validation: TS full CI 62 passed including package/performance smoke; QML 112 passed; Rust 168 passed, one normally ignored, with strict Clippy and formatting. Shared cases: `contracts/wire-cases.json` and `contracts/reporting-cases.json`. Native tests: TS/QML `test/{wire-evidence,reporting-contract}.test.ts`; Rust `src/{wire_evidence,sarif_reporting_tests}.rs`.

Still open: exhaustive blocking-rule prerequisite/applicability/negative inventory; every producer projecting the vocabulary; common reference integrity; end-to-end CLI/JSON/SARIF/policy equivalence across all native families. These are not discharged by the structural fixtures or suite totals. No acceptance criterion is waived by this step.
