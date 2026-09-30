# Cross-lens capability evidence v1

`capability-cases.json` is the same conformance corpus in all three lens repositories. Native implementations must accept and reject the same cases.

Fields: `schema_version: 1`, nonempty `capability` and `scope`, `status`, nonnegative safe-integer `evaluated`/`skipped` counts, and nonempty-string `reasons`.

- `completed`: no skipped targets or incomplete reasons. It does not mean defect-free.
- `partial`: useful observations with explicit limitations.
- `unavailable`, `disabled`, `not_applicable`, `failed_execution`: no evaluated targets and an explicit reason.

Quality verdict, severity and policy disposition remain distinct from capability status. A crashing runner cannot become a successful empty finding set. Tool identity belongs in run provenance, not the capability name.

The capability contract is additive to existing artifact envelopes. Existing native contracts still carry run identities, findings and measurements; they are not asserted to be wire-compatible across languages. Consumers must check native schema/version and input freshness before interpreting findings.

Current granularity: TS/Rust expose project-input capabilities; QML additionally exposes per-rule evaluation. Fine-grained applicability for every TS/Rust rule family is still an explicit follow-up, not implied by a project-input result.
