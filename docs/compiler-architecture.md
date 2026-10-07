# Compiler-backed architecture rules (opt-in)

`rust.architecture_backend = "compiler"` uses compiler-resolved, post-expansion
static module dependencies for **architecture-rules only**. It does not promote
source complexity, safety, locality/leverage scores, map, or type-health evidence.
Zero observed violations still cannot pass an incomplete architecture gate.

## Enable on a trusted project

Build and retain the pinned nightly adapter as described in
[compiler inventory](compiler-inventory.md). Rebuild the adapter after upgrading;
an older type-only adapter cannot certify architecture. In `rqlens.toml`:

```toml
[rust]
architecture_backend = "compiler" # default: "syntax"
compiler_inventory_driver = "/absolute/path/to/rqlens-compiler-inventory"
# compiler_inventory_target = "x86_64-unknown-linux-gnu"
# rust-analyzer is not required by this backend:
identity_resolution = "disabled"

[verification]
workspace = true
all_targets = true
locked = true
```

This explicitly executes trusted build scripts and procedural macros. It is not
a sandbox and is independent of `identity_build_macros`. Other identity-consuming
source measurements still obey their rust-analyzer configuration. Declare non-Rust
build/generator inputs with `fingerprint_inputs`; arbitrary build scripts are not
made hermetic by compiler capture.

```sh
rqlens measure architecture-rules --config rqlens.toml
rqlens check --config rqlens.toml
```

Use a fresh output directory when switching backends and remeasure both sides of
comparisons. Backend selection participates in configuration fingerprints; adapter
bytes and compiler flags participate in input provenance. Compiled graphs have
architecture model version 1; no source risk model is silently recalibrated.

A configured type-inventory driver alone does not select compiler architecture.
Focused type-health captures only declarations. Focused architecture-rules with
this backend skips syntax extraction and rust-analyzer navigation. If multiple
requested tools need compiler capture, one capture is shared within that run.
Unrelated producers do not launch the adapter. The explicit `compiler-inventory`
command includes diagnostic architecture captures/graphs, but its outer confidence
still refers to **type inventory**, not architecture-policy certification.

## Scope and policy selectors

The graph includes explicit static HIR references from imports/re-exports, types,
trait bounds/impls, bodies, compiler-resolved method/operator targets and inferred
ADT field accesses. This includes active declarative/procedural/attribute/derive
expansions, expression/statement macros, and generated/nested includes.

Module ownership comes from compiler definitions, not filenames, suffix matching,
macro-generator locations or OUT_DIR directory names. Impl-body references belong
to the module containing the impl, not the module containing the implemented type.
Block-local types remain lexical declarations, not invented architecture modules.

Each Cargo target/test/feature/cfg context keeps a separate graph. Rules apply to
outgoing references in these selected contexts; they do not certify unselected
packages, inactive feature combinations or runtime reachability. Cross-context
links use exact compiler crate and module IDs. Captured targets acquire their
validated Cargo package identity; dependencies outside the capture set remain
explicit external endpoints (not necessarily third-party packages).

Selectors are an explicit compatibility view, not the graph's actual identity:

- Captured modules: `<package>::shared::<logical-module-path>`.
- Captured crate roots: `<package>::shared::crate`.
- Uncaptured dependencies: `external::<compiler-crate-name>::<module-path>`;
  use `crate` for their roots. Dependency aliases do not establish package identity.

Existing `package::shared::module` rules can match without rewriting. Root or
specific-target syntax selectors need explicit review when switching backends.
Violation rows contain context-qualified IDs, matching selectors and reference
origins. Generated origins identify authored invocation anchors, not generated
body coordinates. Repeated edges retain their observations; violations repeated
in normal/test contexts are contextual findings, not new source defects.

## Completeness and remaining limits

Every selected context must compile and be captured. Missing reference categories,
unknown owners/endpoints, missing origins, incompatible adapters, incomplete
fingerprints and unsuccessful builds remain partial. Compiler failure discards
fragments rather than substituting a syntax graph. Configured error-level rules
require complete, fresh evidence; a partial empty violation list remains blocked.

The adapter still fails closed on selected examples/benches, custom target JSON,
ambiguous repeated compilation contexts and inherited compiler wrappers/bootstrap.
No unsupported target is silently excluded. Assembly dependencies remain partial.
Unresolved type-dependent paths remain unknown. This is not a monomorphized call
graph; implicit drop/coercion machinery and macro-generator implementation
internals are not modeled as project runtime references.

Only compiler-tagged standard-library injection and test/proc-macro harness
infrastructure is excluded, with exclusions recorded. User test bodies and user
macro output remain included. No exemption is inferred from a name or missing span.

Native validation checks captures, reprojects graphs, and recomputes violations
against recorded rules. Policy consumption also checks the backend and rules
against the active configuration. Source/body partial flags are unchanged.

See [the ownership contract](../contracts/compiler-architecture.md).

## Regression validation

```sh
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
RQLENS_TEST_COMPILER_DRIVER=/absolute/path/to/rqlens-compiler-inventory \
  cargo test --locked --test compiler_architecture --test compiler_inventory -- --include-ignored
```

Fixtures cover lexical shadowing/forward declarations, same-file modules,
non-local impl locations, macros/includes, method and field resolution, renamed
workspace dependencies, test/feature contexts, forbidden-edge mutation detection,
forged violations/rules, missing inputs/backends, failed compilation, and selected
unsupported targets. Frozen daemon acceptance/configuration rollout is separate.
