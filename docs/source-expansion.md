# Source expansion observations (experimental)

This is the first implementation stage toward the required self-analysis scope,
**not a completed compiler-backed fact pipeline or a T4 pass**.

```sh
rqlens measure source-expansion --config rqlens.toml
rqlens artifact-schema source-expansion
```

The focused command writes `source_expansion.json`. It is deliberately excluded
from `measure all`. Ordinary measurements do not request these expansions.

## What is implemented

- The syntax helper records explicit item/impl macro call sites, their lexical
  module and UTF-16 LSP coordinates. Macro definitions themselves are not calls.
- One bounded rust-analyzer session requests `rust-analyzer/expandMacro` for the
  observed sites, in deterministic source order.
- Each result retains the original invocation anchor separately from expansion
  text, backend macro name, status and failure reason. Invocation coordinates
  must not be presented as generated-code spans.
- The report records analyzer version and the actual backend configuration.
  Cargo features, no-default-features and all-targets come from `[verification]`.
- `identity_resolution = "disabled"` launches no analyzer. In `auto`, missing
  tools/replies remain explicit unavailable observations. `required` rejects an
  incomplete request inventory or missing replies without replacing prior output.
- `identity_build_macros` remains the explicit opt-in for rust-analyzer build
  scripts/procedural macros. Declarative expansion does not enable that permission.
  This is not a project-code sandbox.
- Empty expansion text is distinct from an absent/invalid response. Missing
  helper inventory metadata and recovered syntax cannot certify an empty scope.

## What the result does NOT establish

`response_complete` means the enumerated item/impl requests received matching,
well-shaped backend replies. It does **not** mean the returned Rust has been
parsed/typechecked, fully expanded, or proven applicable to a particular target.

The report always retains `semantic_inventory_complete: false` and partial
measurement/capability confidence. It does not clear syntax/type limitations or
populate authoritative type-health totals/scores. Existing artifacts are not
rewritten using the returned text. There is no expansion cache or reuse contract.

Expression/statement, attribute and derive requests are not comprehensively
inventoried by this first stage. A zero request count therefore says nothing
about their absence. `syntax_target` is a syntax inventory label, not verified
compiler ownership. Backend configuration is not proof of effective cfg values.

## Remaining integration

1. Establish effective cfg and target/feature contexts, including test variants
   and build-script cfg; preserve inactive versus unsupported scope explicitly.
2. Parse/validate expansions with hygiene and generated-source provenance rather
   than treating pretty-printed text as an ordinary source file.
3. Resolve impl owners and merge generated/authored facts without double counting,
   changing score meaning, or inventing source locations/safety documentation.
4. Promote completeness only when all required evidence is covered. Then rerun
   the strict native policy gate, clean CI and the declared platform matrix.

## Validation

`tests/source_expansion.rs` covers missing/disabled backends, preservation of
prior output, no launch for disabled/empty/unrequested work, and live item/impl
expansions with Unicode anchors, repeatability and unchanged type-health output.
Native validation rejects contradictory counters, mismatched macro names, absent
expansion text and claims of complete semantic inventory.

```sh
cargo test --workspace --locked
cargo test --locked --test source_expansion -- --ignored
python3 acceptance/trust_campaign.py --output target/expansion-new --tools source-expansion
```

The campaign keeps the original thirteen-tool default; this extra backend is
opt-in. It preserves daemon configurations and source trees and removes only
`generated_at`/`measured_at` when comparing repeated artifacts. Executed project
macros can depend on external inputs; no universal determinism claim is made for
arbitrary procedural macro/build-script workloads.
