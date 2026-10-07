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

## Graph projection and policy selectors

Compiler graph nodes use context-qualified compiler module IDs. Edges retain
all reference observations, even when multiple references collapse to one module
edge. Module-internal references do not create inter-module self edges. Normal,
test and feature contexts retain separate graphs.

Policy matching uses an explicit compatibility view:
`<package>::shared::<logical-module-path>`. The root is named `crate` in this
view, not inferred from a source filename. These selectors are labels, not IDs;
inline modules in the same file are separate nodes. Cross-crate links use exact
compiler crate and module IDs, with Cargo package identity supplied by the
validated target profile. Dependency renames are not joined by name. A reference
to a crate outside the selected profiles is represented as
`external::<compiler-crate-name>::<logical-module-path>` (root `crate`), with its
opaque compiler identity preserved. Ambiguous or missing endpoints in a selected
compiler-linked context stay partial, never external-by-default.

Compiler graph projection replaces the graph for a selected context; it does not
append generated fragments to a pre-expansion syntax graph. Test and normal builds
remain separate, never summed into a fictitious program total. Architecture policy
can be complete independently of type health, safety and complexity. Syntax scoring
is not silently recalibrated against compiler-expanded graphs.

## Producer selection and policy

`rust.architecture_backend = "compiler"` explicitly opts architecture-rules into
this graph; the default remains `syntax`. Configuring type-health's adapter alone
does not opt architecture policy in. Focused type-health does not request reference
traversal. The explicit compiler-inventory diagnostic tool may expose graphs, but
its outer confidence still describes type counts.

Architecture evidence is complete only with complete selected context coverage,
complete graph ownership/reference/provenance, successful compilation and a
complete input fingerprint. Missing capture or unsupported contexts remain partial
without syntax fallback. Confidence is independent of type and body producers.

Compiler architecture payloads include model version, captured profiles, graphs,
rules and violations. Native validation recomputes projection and policy results;
policy consumers also bind recorded backend/rules to active configuration. The
compiler and adapter are trusted producers, not authenticated or sandboxed inputs.
Selected examples/benches are currently unsupported and fail closed; automatic
exclusion is not an acceptable path to complete evidence.

Backend/adapter/configuration changes invalidate fingerprint compatibility. Capture
is fresh per run; compiler graphs are not reused from the semantic navigation cache.
See [configuration and limitations](../docs/compiler-architecture.md).

The backend remains opt-in trusted execution. Selected unsupported targets or
failed builds fail closed; no automatic feature/target exclusions are permitted.
