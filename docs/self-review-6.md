# Self-review 6: measurement boundaries and value-based refactoring

Work starts at `5be2262`. Original self evidence is retained locally under
`target/self-review-6/before/`; final comparisons and validation follow below.
No score weights, gates or budgets are relaxed.

## Measurement corrections

- Unresolved lexical impl owners and unsupported impl target shapes now remain
  **type-identity** limitations. They no longer invalidate successfully parsed
  source bodies and test inventories. Unresolved impl observations remain counted;
  type-health still abstains, including aliases, generic shadowing and non-path
  targets. The native lexical-ownership journey checks both partial type evidence
  and complete syntax evidence for the same compiling input.
- Semantic candidate selection now includes a binary's own package library by
  its Cargo library target name. Previously these calls were never submitted to
  rust-analyzer and observed reuse could incorrectly be zero. Declared children
  with shared physical files are also resolver candidates. Actual edges still
  require resolved definitions; names alone do not prove a dependency. Semantic
  cache version 16 invalidates results made with the old candidate selection.
- The native resolver journey exercises library→module and binary→same-package
  library resolution, then the cache, rather than merely testing a classifier.

These are evaluator corrections, not architectural improvements. Topology and
confidence changes must be distinguished from refactoring gains. Type ownership
and macro/compiler limitations are not waived to obtain complete syntax evidence.

## Initial cleanup

Simplified lexical item binding and impl-owner extraction into explicit domain
operations, reducing deeply nested control flow without changing resolution rules.
Removed the test campaign's wildcard import using explicit dependencies rather
than suppressing the escape-hatch finding. Bundled helper copies remain identical.

Validation at this checkpoint: native lexical and trust CLI suites, the live
resolver/cache test, and all helper crate tests pass. Full checks and numerical
comparisons are recorded after the remaining refactors.
