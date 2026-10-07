# Compiler architecture and lexical ownership

Architecture model 1 describes static dependencies in selected compiler contexts,
not runtime reachability, dynamic dispatch targets, or all feature combinations.
Type inventory and architecture traversal are independent capabilities. An older
adapter without architecture records is not a complete empty graph.

## Identities

- The enclosing compiler profile supplies package, source target, target triple,
  effective cfg/test selection and compiler version. IDs are context-scoped.
- Modules carry compiler definition IDs, logical Rust paths, and explicit parents.
  Inline modules are distinct even when they share a source file.
- Declarations carry their lexical module owner. Impl body dependencies belong to
  the module containing the impl, not necessarily the implemented type's module.
- Lexical blocks/functions are ownership scopes, not architecture modules. Syntax
  scope IDs are snapshot-local; display names and arbitrary suffix matches are
  not identity evidence. Rust type and value bindings must not be conflated.

## References and provenance

References carry source declaration, kind (import, type, trait, body, method),
resolved local declaration or external compiler crate/definition/module identity,
and origin. Unknown targets require a reason. External crate IDs are not package
names; cross-context joins require exact compiler identity, not crate-name guesses.

Authored spans and generated invocation anchors remain distinct. A macro generator
is not the owner of emitted declarations. OUT_DIR paths are not ownership evidence.
Missing origins remain explicit limitations; source comments never become safety
rationales for generated bodies. References include inferred ADT field accesses
and compiler-resolved method/operator targets. Implicit drop/coercion machinery,
monomorphized runtime targets and macro-generator implementation dependencies are
not the static HIR reference graph. Assembly produces an explicit limitation.

Only compiler expansion tags for standard-library injection, test-harness and
proc-macro-harness infrastructure are excluded, with named exclusions recorded in
the capture. User test bodies and proc-macro-emitted project code are included;
file names, identifier names and missing spans never authorize an exclusion.

## Completeness

A complete graph requires successful capture of every selected context, traversal
of imports/types/traits/bodies, known owners/endpoints, attributable origins and
complete input fingerprints. Cyclic/dangling module ownership, duplicate IDs,
unknown schemas and invalid local endpoints are rejected. Unknown references or
missing traversal categories retain partial confidence even with zero violations.

Compiler graph projection replaces the graph for a selected context; it does not
append generated fragments to a pre-expansion syntax graph. Test and normal builds
remain separate, never summed into a fictitious program total. Architecture policy
can be complete independently of type health, safety and complexity. Syntax scoring
is not silently recalibrated against compiler-expanded graphs.

The backend remains opt-in trusted execution. Selected unsupported targets or
failed builds fail closed; no automatic feature/target exclusions are permitted.
