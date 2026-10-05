# Rust Halstead measurement

`hotspots.json` reports model **`rqlens.rust_halstead`, version `1`** for files
and extracted functions. This is a syntax-token measurement, not an estimate
from lines of code or cyclomatic complexity. It runs in the bundled Rust
extractor; no external executable or compiler execution is needed.

## Research and design

Halstead starts with four counts, not an effort formula fitted to source size:

| Symbol | JSON field inside `halstead` | Meaning |
| --- | --- | --- |
| n1 | `distinct_operators` | Distinct operator identities |
| N1 | `total_operators` | Operator occurrences |
| n2 | `distinct_operands` | Distinct operand identities |
| N2 | `total_operands` | Operand occurrences |

The definitions and implementation were checked against:

- [Verifysoft / Testwell's Halstead measurement reference](https://www.verifysoft.com/en_halstead_metrics.html): the four counts and the standard length, vocabulary, volume, difficulty and effort formulas. Its language-specific rules also illustrate why a counting convention must be stated.
- Mozilla rust-code-analysis, pinned to commit [`37e5d83c056c8cbf827223d5814a93c5218df1a9`](https://github.com/mozilla/rust-code-analysis/tree/37e5d83c056c8cbf827223d5814a93c5218df1a9): [formulas and independently tabulated Rust example](https://github.com/mozilla/rust-code-analysis/blob/37e5d83c056c8cbf827223d5814a93c5218df1a9/src/metrics/halstead.rs), and [Rust operator/operand classification](https://github.com/mozilla/rust-code-analysis/blob/37e5d83c056c8cbf827223d5814a93c5218df1a9/src/getter.rs).

There is no single universally interchangeable Rust counting convention.
RQLens uses the standard equations with the explicit convention below, rather
than claiming numeric compatibility with every other Halstead implementation.
The Mozilla arithmetic/`println!` example is an independent regression oracle:
`n1=10, N1=23, n2=9, N2=15`, volume `161.42124551085624`, difficulty
`8.333333333333334`, effort `1345.177045923802`.

## Equations and numerical behavior

```text
length     N = N1 + N2
vocabulary n = n1 + n2
volume     V = N * log2(n)
difficulty D = (n1 / 2) * (N2 / n2)
effort     E = D * V
```

The counts are integers. Derived values retain floating-point precision; they
are not rounded before multiplication. No human-time or defect-count estimate
is emitted. Effort is a descriptive complexity indicator, **not measured work,
developer hours, defect probability, or a verified quality gate**. Thresholds
from other languages/tools should not be imported without validation.

An empty scope has zero counts, volume, difficulty and effort by convention.
For a nonempty scope with no operands, difficulty and effort are `null` (the
formula is undefined); counts and volume remain available. No NaN/infinity is
serialized. A zero value is distinct from unavailable extraction.

## Rust counting convention v1

The extractor traverses `syn`'s token representation of successfully parsed
Rust. It does not scan words or operators with a regex.

- **Operators:** strict/reserved Rust keywords (including `async`, `await`,
  `unsafe`, and `gen`), except `self`, `Self`, `true`, `false`; punctuation;
  and paired delimiters `()`, `{}`, `[]`, counted once per pair.
- **Operands:** identifiers, primitive/user-defined type names, function/method
  names, `self`, `Self`, boolean values, `_`, lifetimes/labels, and literals.
  Weak/contextual keywords such as `union` and `raw` use identifier treatment.
- Operator identity is its spelling: unary and binary `-`, for example, share
  one vocabulary entry. Calls count their name as an operand and their
  parentheses as an operator, rather than inventing a distinct operator for
  each callee. Named fields and type names are not silently discarded.
- Compound syntax operators (`>>=`, `&&`, `::`, `..=`, etc.) are atomic.
  `syn` preserves their syntactic role: generic closing angles are separate
  `>` tokens, and `&&x` as two references is two `&` operators, not logical AND.
  Opaque macro token streams use longest recognized joint punctuation instead.
- Every literal is one operand, including raw/byte/C strings, characters,
  numeric suffixes, and embedded punctuation. Identity uses spelling, not
  evaluated value: `1`, `1u8`, and `0x1` differ. Identifiers retain spelling too
  (`r#name` is distinct from `name`). Operand categories distinguish identifiers,
  literals, and lifetimes; `'a` is one lifetime operand, not an apostrophe
  operator plus an identifier. Transparent token groups add no operator.
- Whitespace and ordinary/doc comments contribute nothing. Outer/inner
  attributes are excluded, including their contents. Attribute-shaped token
  groups inside opaque macro input are excluded by the same rule.
- Macros are measured **as written**: invocation path/name, `!`, delimiters and
  input tokens; macro definitions likewise contribute their written tokens.
  Their input need not be Rust expressions. No generated/expanded code is
  inferred. Conditional branches are not evaluated or filtered by `cfg`.

These rules deliberately differ from rust-code-analysis in some areas, such as
primitive type classification and retaining field/type identifiers. The new
model ID/version and scope must match before comparing measurements. Changes
to counting rules require a Halstead model-version change, independently of the
architecture-risk and standard-complexity models.

## Scope and aggregation

- Function scope (`function_signature_and_body`) includes the signature and
  body, including argument and return types, generics, closures, and nested
  declarations. It excludes the function's visibility and attributes. It uses
  the existing function inventory (free functions and impl methods), not a new
  promise to inventory macro-generated functions or all anonymous closures.
- Module rows use scope `file`: the entire parsed source file, including
  declarations outside functions and inline modules. File visibility tokens
  are included. A file with no functions can therefore still have effort.
- File counts are collected directly, **not by summing function efforts or
  distinct counts**. The vocabulary is a union over the file's tokens and the
  equations are recomputed from the file counts.
- Enclosing-function measurements include nested functions and closures. Do not
  sum all function rows and describe that as non-overlapping project effort.
  Likewise, a sum of file efforts is not Halstead effort for the union of the
  project; obtaining that would require a project-wide operator/operand union.

## Output and confidence

Each hotspot row contains:

- `halstead_metric_model_id`, `halstead_metric_model_version`, `halstead_scope`;
- `halstead_status`: `measured` or `unavailable`;
- `halstead`: the four counts plus `length`, `vocabulary`, `volume`, `difficulty`,
  and `effort`, or `null` if unavailable;
- `halstead_effort`: a convenience projection of `halstead.effort`.

Unreadable/unparsed files are not silently treated as zero. Text/tree-sitter
recovery rows have unavailable Halstead metrics, even if raw control-flow
complexity could be recovered. Older/custom helpers without Halstead output
also produce `unavailable`, not zero.

`measured` describes the stated syntax scope, not compiler-verified completeness.
Existing `measurement_confidence` still reports macro extraction limitations,
missing inputs and other partial evidence. Halstead does not clear those flags
or repair semantic dependency resolution.

The calibrated hotspot score, architecture-risk model and existing self-metric
thresholds are unchanged. No standard maintainability index is inferred from
this new metric without a separately documented formula and scope.

## Historical compatibility

The original Python producer read Halstead effort from `rust-code-analysis-cli`.
Commit `4e0d7bb` replaced that integration with `sloc * cyclomatic * 25.0` during
the Rust rewrite; `7b972f0` later removed those proxy fields. Neither those proxy
values nor the old analyzer's undocumented-for-this-consumer scope should be
compared numerically with this model. Regenerate both sides of a baseline with
the same model and extraction scope.

Implementation: `rust_helpers/src/halstead.rs`. Tests cover the independent Rust
oracle, hand counts, syntax-token edge cases, aggregation, degenerate formulas,
and end-to-end helper/artifact serialization and recovery abstention.
