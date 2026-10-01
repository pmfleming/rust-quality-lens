# Type identity and semantic abstention

T3 is a fail-closed evidence guarantee, not a claim of compiler-complete analysis.

## Type-health scope

Struct/enum declarations and explicit impl blocks are syntax observations; type-health is not an inventory of every Rust type kind. Joins are qualified by Cargo package, target and lexical module. `crate::`, `self::` and `super::` owners are supported, including the extractor's `lib::`/`main::` root representation. An arbitrary matching suffix is **not** ownership evidence. Imports and aliases are not resolved by this joiner.

Unresolved owners, duplicate identities, conditional compilation, block-local scope, unsupported impl targets, unexpanded item/impl/expression macros and active attributes (including derives), incomplete extraction, or incomplete input provenance make the inventory partial. Item macros can generate types even when their names or tokens do not mention modules/tests. Empty type records therefore do not establish that no types exist.

Abstention reaches:

- the artifact's `measurement_confidence`;
- every type row's confidence;
- the project-input capability entry;
- policy artifact status and review comparability.

When partial, `method_count`, `impl_block_count`, `impl_file_count`, `structural_risk` and `structural_score` are **null**, not zero. `observed_method_count`, `observed_impl_block_count`, `observed_impl_file_count` and `impl_files` retain resolved syntactic observations. These are not compiled totals or, in conditional code, guaranteed lower bounds on compiled totals. Declarations/fields remain source observations. No calibrated risk is synthesized from unknown totals.

When complete in this syntax scope, a genuinely absent explicit implementation remains numeric zero. Completeness does not certify compilation, layout, configuration selection or derived/generated implementations. It does not mean that rust-analyzer's architecture resolver also resolved type-health owners. Reasons and `type_identity.count_scope` retain that distinction. `unresolved_impl_blocks` counts failed joins of extracted impl facts, not unseen macro output.

The helper's `type_identity_limitations` field is required: an older helper omitting it is not interpreted as reporting no limitations. Rebuild custom helpers when upgrading.

The native validator and published type-health schema encode nullable metrics and observed counts. A complete envelope cannot contain partial rows; partial rows cannot carry authoritative numeric totals/scores. Re-measure older type-health artifacts rather than treating legacy zero counts as evidence of absence.

## Requested architecture semantics

For identity-consuming tools, unresolved references, unavailable rust-analyzer, or explicitly disabled resolution with references to analyze produce partial confidence. The nested semantic status, outer artifact, custom payload confidence and capability cannot disagree by claiming full evaluation. Diagnostics are preserved in `missing_input` and `semantic_identity`.

An empty reference scope is different: there is nothing to resolve. No semantic backend is needed to establish an empty syntax reference set. Execution telemetry remains outside semantic measurement JSON, as specified in `determinism.md`.

## Validation and limitations

`tests/trust.rs` covers compiling aliases, generated type omission, empty partial inventories, generated impl methods (including compiling non-local impls from statement macros), compiling attribute-generated types, reference/block-local owners, suffix collisions, conditional/duplicate definitions, known zero, qualified owners, and disabled/unavailable identity resolution. `src/producers/type_health_tests.rs` retains package/target isolation; native validation rejects contradictory zero/null confidence.

This conservative implementation can abstain on otherwise valid Rust, including RQLens's own visitor macros and cfg-gated code. The strict partial-evidence policy gate must report that limitation; it must not be weakened to manufacture a passing native CI result. Full compiler/configuration expansion, exhaustive rule applicability and cross-reporter equivalence remain separate acceptance work. See the ledger and `../acceptance/trust-t3-results.json` for measured scope.
