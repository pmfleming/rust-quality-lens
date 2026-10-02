# Source expansion observations (experimental)

This implements expansion responses and bounded generated-syntax observations toward the required self-analysis scope,
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
  incomplete request inventory, missing replies or unparsed fragments without
  replacing prior output.
- `identity_build_macros` remains the explicit opt-in for rust-analyzer build
  scripts/procedural macros. Declarative expansion does not enable that permission.
  This is not a project-code sandbox.
- Empty expansion text is distinct from an absent/invalid response. Missing
  helper inventory metadata and recovered syntax cannot certify an empty scope.

## Generated syntax observations

Each successful reply now contains `generated_syntax`. `parsed: true` means `syn`
accepted the complete fragment in its item or impl-item context. Declarations
record struct/enum names, observed field counts, free functions, impl methods and
trait methods. They are **observations**, not a compiled inventory or lower bound.
Unions, aliases and macro token bodies are not declaration records in this view.

`start_byte`/`end_byte` are half-open UTF-8 ranges in **expanded_source**, including
any declaration attributes. They are never offsets in the authored file. The impl
parser uses a synthetic enclosing impl, subtracts its prefix from ranges and never
exports that artificial owner. Text cannot escape this wrapper into sibling items.
Native validation reparses the text and rejects changed observations or ranges.

Parsing is bounded to 65,536 bytes and 128 conservative lexical chunks (an
alphanumeric run or a non-whitespace punctuation character). Comments and literals
also count: this deliberately rejects some valid large expansions. `parsed: false`
retains an explicit syntax/budget reason and empty observations mean **unknown**,
not zero declarations. Missing backend text instead has `generated_syntax: null`.
A valid empty expansion is distinct: it parses successfully with no declarations.

No ownership joins, source safety rationales or pressure scores are inferred from
this text. The existing type-health artifact remains untouched. Remeasure older
successful source-expansion artifacts: they lack required syntax observations.

## What the result does NOT establish

`response_complete` means the enumerated item/impl requests received matching,
well-shaped backend replies. It does **not** mean the returned Rust has been
parsed/typechecked, fully expanded, or proven applicable to a particular target.

The report always retains `semantic_inventory_complete: false` and partial
measurement/capability confidence. It does not clear syntax/type limitations or
populate authoritative type-health totals/scores. Existing artifacts are not
rewritten using the returned text. There is no expansion cache or reuse contract.

Expression/statement, attribute and derive requests are not comprehensively
inventoried by this profile. A zero request count therefore says nothing
about their absence. `syntax_target` is a syntax inventory label, not verified
compiler ownership. Backend configuration is not proof of effective cfg values.

## Remaining integration

1. Establish effective cfg and target/feature contexts, including test variants
   and build-script cfg; preserve inactive versus unsupported scope explicitly.
2. Extend bounded syntactic observations to compiler-backed hygiene and origin
   maps; handle recursive and currently declined expansions without treating
   pretty-printed text as an ordinary authored source file.
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
