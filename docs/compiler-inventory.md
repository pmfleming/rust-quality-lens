# Compiler-backed type inventory (opt-in)

`compiler-inventory` captures **compiler-analyzed declarations**, after macro
expansion, attribute/derive processing, and effective `cfg` selection. When enabled,
`type-health` uses these definitions and compiler-resolved impl owners instead of
appending expansion text to its syntax inventory.

This is not a complete compiler-backed replacement for every RQLens measurement.
It can establish complete **per-context type counts**. The separately selected
[compiler architecture backend](compiler-architecture.md) can establish scoped
static architecture-rule evidence, not panic/safety, complexity, or other body-level
rule coverage. Type inventory alone does not certify architecture or a T4 pass,
and does not remove other producers' macro-related partial flags.

## Install and enable

The adapter uses unstable `rustc_private` APIs. It is a separate source-checkout
package, not part of the stable workspace or automatically downloaded/built by the
CLI. The required compiler is exactly:

- Toolchain: **nightly-2026-02-14**, with `rustc-dev`
- Compiler: **rustc 1.95.0-nightly (a423f68a0 2026-02-13)**

From the RQLens checkout:

```sh
rustup toolchain install nightly-2026-02-14 --component rustc-dev
cargo +nightly-2026-02-14 build --locked \
  --manifest-path compiler_helpers/Cargo.toml \
  --target-dir target/compiler-helper-build
```

Keep the matching toolchain installed: the adapter links against its compiler
libraries and records its absolute compiler path. The live adapter has been tested
on Linux. Other hosts are not certified by this regression run.

In the measured project's `rqlens.toml`:

```toml
[rust]
compiler_inventory_driver = "/absolute/path/to/rqlens/target/compiler-helper-build/debug/rqlens-compiler-inventory"
# Optional named target triple; otherwise the adapter's host is selected explicitly.
# compiler_inventory_target = "x86_64-unknown-linux-gnu"

[verification]
workspace = true
all_targets = true
locked = true
# features = ["some-feature"]
# all_features = true
# no_default_features = true
```

The driver path resolves relative to the configuration file. This is an explicit
**trusted-code execution opt-in**: Cargo runs build scripts and procedural macros
and compiles the selected crates. It is not a sandbox and is independent of the
rust-analyzer `identity_build_macros` setting. Compiler identity does not require
rust-analyzer; `identity_resolution = "disabled"` is compatible with this backend.

```sh
rqlens measure compiler-inventory --config rqlens.toml
rqlens measure type-health --config rqlens.toml
rqlens artifact-schema compiler-inventory
```

The focused command writes `compiler_inventory.json`. It is excluded from
`measure all`; however, a configured driver is used for the ordinary `type-health`
task, including that task within `measure all`. Architecture-rules launches the
adapter only when `rust.architecture_backend = "compiler"` is separately selected.
Other focused measurements do not launch it. Without a configured driver, ordinary
type-health retains its existing syntax-only behavior and abstention rules.

Focused type-health requests declarations only. The explicit compiler-inventory
command additionally exposes optional architecture captures and per-context graph
projections. Its outer confidence remains scoped to type counts; graph completeness
and architecture-policy certification are separate.

## Build context and completeness

- Cargo metadata and compilation use the pinned compiler and explicit target.
  `[verification]` supplies workspace/exclusions, feature selection, all-targets,
  locking, and command timeout. Compiler scope is determined by Cargo targets;
  `source_roots` does not filter this compiled inventory. `rust.identity_offline`
  also applies to these Cargo
  commands. Each bounded command has its own timeout.
- Supported contexts are selected libraries/binaries, their enabled test builds,
  and integration tests selected by `all_targets`. Feature-gated targets are
  considered only when their required features are enabled. Build scripts run but
  their own declarations are excluded from the project type inventory.
- Selected unsupported kinds (currently examples/benches), custom target JSON,
  ambiguous repeated compilation contexts, inherited compiler wrappers/bootstrap,
  incompatible adapters, missing captures, and unsuccessful compilation fail closed.
  Exclusions are matched by exact workspace package name. Contexts are currently
  distinguished by manifest directory, source root, and test mode; distinct targets
  sharing that tuple are not silently merged.
- The adapter captures local struct/enum/union declarations, impls, and named
  function/method declarations after analysis. Aliases are resolved for ownership,
  not counted as additional nominal types. Closures, monomorphizations, inherited
  trait defaults, dependency impls, and blanket applicability are not extra methods
  on a local type. External/non-nominal impl owners are explicitly distinguished.
- Each profile records package/source identity, compiler version, target triple,
  test mode, and the compiler's actual sorted `cfg` set, including build-script
  values. Package feature cfg must match Cargo's resolved features. This is evidence
  of one selected configuration, not an assertion about inactive configurations.
- Coverage must match the metadata-derived expected contexts exactly. All captures
  are discarded if any selected compilation fails; successful fragments never
  certify the whole build. Compilation uses a fresh temporary directory under
  Cargo's target root, avoiding Cargo cache hits that would omit callbacks. This
  intentionally costs a fresh check build on every measurement; the temporary
  directory is removed afterward.
- Source fingerprints include the configured adapter binary and relevant compiler
  flag environment. This does not make arbitrary build scripts hermetic: declare
  non-Rust generator inputs with `fingerprint_inputs`, as for other measurements.
  Missing fingerprint inputs prevent complete counts.

## Attribution, identity, and deduplication

`inventory_model_version: 1` records opaque compiler definition-path hashes. IDs
are scoped to their `compiler_context_id`, not assumed stable across arbitrary
compiler versions or compared by identifier suffix.

Each declaration/impl carries an `origin`:

- `authored`: original compiler source span;
- `invocation_anchor`: ultimate authored invocation of generated code;
- `unavailable`: no trustworthy local coordinates.

Coordinates are one-based, with an exclusive end. Compiler columns count Unicode
characters, not UTF-16 LSP units. An invocation anchor is **not** a span inside the
emitted code; generated nodes are explicitly labeled. Attribution follows the
compiler resolver's expansion-parent chain, not span hygiene alone: `include!`
and pass-through attributes can retain root-context token spans. Nested macros
inside generated includes are anchored back to the authored include invocation,
not to temporary build-directory files. Missing type/local-impl
anchors prevent complete type-health counts. Safety rationales and other source
comments are not transferred onto generated bodies.

Impls refer directly to local ADT IDs, and their method IDs must refer to captured
functions. Duplicate definitions, multiply assigned methods, unknown owners, and
contradictory provenance are rejected. Type-health **replaces**, rather than
concatenates, syntax rows with the validated compiler projection. Each type occurs
once per compiled context. A normal and test build can intentionally have two rows
for the same nominal type: never sum those rows into a fictitious program total.
The report's `type_count_across_contexts` is explicitly only a contextual row count.
`expected_contexts` contains `[manifest_directory, source_root, test_mode]` tuples
used to validate coverage.

## Type-health interpretation

Compiler rows expose the context, definition ID, origins, and implementation
records. `method_count`, `impl_block_count`, and `impl_file_count` are authoritative
only when their scoped `measurement_confidence.complete` is true. Otherwise those
fields are null and observed counts remain separately labeled. If capture fails
entirely, syntax observations remain available with compiler failure reasons and
null authoritative counts; stale or partial compiled fragments are not substituted.

Even with complete counts, `structural_risk` and `structural_score` stay **null**,
with `structural_score_status = "not_calibrated_for_compiler_inventory"`. The source
risk model has not been calibrated for generated declarations or separate build
contexts. Complete inventory is not a zero-risk score, and it must not silently
change hotspot/architecture scoring.

The existing [rust-analyzer expansion observations](source-expansion.md) remain a
separate experimental artifact. They are neither compiler inventory inputs nor a
shortcut to clearing source/body-level completeness warnings.

## Regression checks

Ordinary tests cover unavailable-backend abstention, nonexecution for unrelated
tasks, invalid ownership/provenance, context identity, duplicate coverage, and
serialized-count validation. The explicit live test also exercises item/impl and
statement macros, derives, attribute replacement/passthrough, generated/nested
includes, imported/type aliases, identical names in separate modules, build-script
cfg, feature/test variants and feature-gated integration targets, deterministic
fresh captures, and compiler-error fallback:

```sh
RQLENS_TEST_COMPILER_DRIVER="$PWD/target/compiler-helper-build/debug/rqlens-compiler-inventory" \
  cargo test --locked --test compiler_inventory -- --include-ignored
```
