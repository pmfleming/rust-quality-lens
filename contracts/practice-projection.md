# Shared practice projection (bounded E4/E6 milestone)

Policy and SARIF now consume `findings::practices::project`. This repairs the
nested-compiler-diagnostic counterexample recorded on `8d61005`. **E4 and E6
remain open across the full product**; this is practice-reporting agreement and
observation retention, not engine accuracy or whole-scope certification.

## Findings and identities

The projection contains failed/unavailable/timed-out check findings and observed
nested diagnostics, including diagnostics emitted by a successful invocation.
Both reporters use the same identity, message, severity/disposition and evidence.
Waived observations remain visible, with accepted SARIF suppressions. Configured
path/package exclusions are retained separately as `policy_scope`; exclusion is
not a waiver and does not delete the observation.

Diagnostics are grouped only when their entire payloads match after removing the
producer-local `id` and `identity_version`. Severity, disposition, message,
location, package and any supplied scope fields participate in equality. Different
payloads are **not merged**, so a first observation cannot overwrite a conflicting
severity or disposition. Other differences, including rendered text, can keep
observations separate; this intentionally favors retention over aggressive merging.

A projected diagnostic has identity version `exact-diagnostic-v1`, with an ID
hashed from that payload. The SARIF partial-fingerprint key is
`rqlens/exact-diagnostic-v1`. This is a new, explicitly versioned identity—not the
old source-anchor scheme. It is invariant under invocation reordering but **not**
guaranteed stable across line shifts or changed diagnostic payloads. Original
producer identities are retained in the observations, not silently rewritten.

## Evidence and reference contract

Policy's `practice_evidence` and SARIF's run-level property of the same name are
equal. They contain:

- `projection: practice-v1`;
- `scope_relation: not_established`;
- `artifact_with_diagnostic_refs`: the original practice envelope/context, except
  nested diagnostic arrays contain references into projected findings.

Each reference has a `finding_id` and zero-based `observation_index`. Each
finding's `observations` preserves the original diagnostic and its zero-based
`check_index` / `diagnostic_index`. This permits exact reconstruction of the input
artifact, including original IDs, source fingerprints, producer/toolchain data,
command evidence, status, supplied scope/exclusions and diagnostic ordering.
Invocation context is stored once, not repeated in full for every diagnostic.

An empty diagnostic array stays empty; an absent array stays absent. A missing
source artifact is null. These states are not promoted to verified per-rule
negatives. The transformed artifact is a **projection context**, not a native
artifact to ingest without first resolving its references.

SARIF result properties include `rqlens_finding`, preserving the canonical finding
rather than dropping its evidence when formatting it. Known source columns are
retained. Unknown regions are not replaced with invented line/column 1 locations.

## Abstention and validation

Exact duplicate observations do not prove independent corroboration or compatible
engine scopes. Missing diagnostics in another invocation do not prove disagreement.
The projection neither infers an exclusion-scope union nor upgrades confidence.
Compiler/Clippy rule enablement and negative coverage remain unsupported:
configured-rule authoritative `observed` / `excess` stay null and status remains
`incomplete`, even when useful observations are present.

Native practice input validation now checks nested diagnostics instead of skipping
malformed rows: nonempty rule ID, recognized severity/disposition, string message,
optional string path and optional positive integer coordinates. Present diagnostic
inventories must be arrays of valid records. Invalid input is incompatible for
policy and rejected by SARIF without replacing the previous export. Published
artifact schemas remain broad structural descriptions; native validation is stricter.

## Evidence and remaining work

`acceptance/reporter_campaign.py` verifies 19 native projection cases, including
matching observations, conflicting payloads, missing/empty inventories, successful
checks with warnings, unlocated/unknown-region observations, waiver expiry,
exclusions, incomplete invocations and reordered invocations. It verifies exact
input reconstruction and 38 byte-identical same-input reporter replays. Ten
malformed cases exercise both consumers and prior-SARIF preservation. The previous
28 configured-policy ingestion cases still pass.

Native validation: 193 workspace tests, two live fixtures, 22 Python tests, strict
Clippy, rustdoc, helper parity, package smoke and unchanged self-quality gates.
Source lines: **22,492/22,500**; maximum hotspot **69.22**, cyclomatic/cognitive
**20/11**. No budgets, thresholds, weights or source roots were changed.

Reproduce in the pinned shell after building `rqlens`:

```sh
python3 acceptance/reporter_campaign.py --output target/reporters-new
python3 acceptance/evidence_contract_campaign.py --output target/ingestion-new
```

Directories must be fresh. Summary: `acceptance/reporter-agreement-results.json`;
raw evidence: `target/reporter-agreement/`.

Still required: product-wide source/external/review/policy-disposition parity,
compatible engine applicability and exclusion-scope modelling, explicit semantic
disagreement handling, exhaustive per-rule cases, and resource certification for
large diagnostic inventories. Retaining supplied scope is not proof of that scope.
E2/E3 also remain open; T4 and release are not certified by this milestone.
