# Source and imported findings: bounded reporter agreement

Policy and SARIF now use the same source projection for reliability, architecture
and test-quality findings. Imported findings share the same policy-context and
waiver interpretation. **E2/E3/E4/E6 remain open** beyond this bounded scope.

## Identity, evidence and selection

Source identities are normalized over the **whole artifact array**, before policy
selection or waivers. This prevents occurrence numbering from drifting when test
or advisory findings precede a production finding. Original records and identities
remain in `source_evidence`, equal in policy JSON and SARIF run properties. An
`origin` containing `artifact` and zero-based `record_index` links each projected
finding back to its original record.

The prior production-only, non-advisory reliability selection is unchanged. Its
result is explicit as `policy_selected`. Unselected active rows remain visible in
`reliability_out_of_scope`; waived rows remain visible in `waived_findings`. They
do not silently disappear or change configured-rule counts. SARIF includes all
observations, with the same canonical record in `properties.rqlens_finding`.

`policy_scope` separately records configured path/package inclusion and the
applied rule configuration, or null when no rule is configured. Exclusion is not
suppression. Incoming `policy_selected`, `policy_scope` and `origin` fields are
not trusted as RQLens decisions: projections recompute them. The unmodified input
remains available in the evidence table. Active-waiver lookup is shared between
policy and SARIF; disposition remains distinct from impact severity.

## Imported evidence

`external_evidence`, also equal across reporters, retains each admitted import's
whole envelope: original findings/IDs, producer/version, declared scope and
exclusions, confidence, source fingerprint, metrics and import provenance. Each
projected row points to its adapter and original record index. Separate adapter
namespaces remain separate; matching text is not merged across engines.

Adapter names now require nonempty ASCII letters, digits, `-` or `_`. Invalid
names are **rejected**, not sanitized into another adapter's filename. The reader
checks envelope version and exact adapter/filename agreement. Tool name/version
must be nonempty. Finding identity, severity/disposition, message and optional
coordinates use the same validator as native diagnostics, before publication.
A bad import does not replace a prior valid report.

Only completed, declared-complete, current source-linked imports enter the finding
projection. Corrupt/stale/partial/unsupported/misnamed reports produce explicit
reasons in both reporters, not verified zero findings. Valid reports from other
adapters remain available. This preserves the previous admission restriction;
it does not newly certify external per-rule coverage. Configured external rules
still abstain because rule enablement and negative coverage are unsupported.

## Incomplete inputs

Missing source artifacts appear as null in `source_evidence`. Valid partial source
artifacts retain observations and confidence rather than being promoted to clean.
Policy rejects their use for required complete evaluations. SARIF refuses corrupt,
unsupported or stale native reports without replacing the previous export.

A self-export initially rejected the old `rust_practices.json` as stale. Explicit
`verify` refreshed it; no artifact was removed and no guard bypassed. Subsequent
self-analysis compared **127 source findings**, including **125 outside reliability
policy selection**, with equal identities, records and source evidence. Strict
policy still failed on incomplete evidence; no stale artifacts remained. Category
names such as `test-failure` are not assertions of executed test failures.

## Validation and limits

`acceptance/source_reporter_campaign.py` runs six source/import agreement fixtures,
30 rejection/abstention cases, and twelve byte-identical same-input reporter
replays. Cases include waiver expiry, explicit selection/exclusion, input spoofing,
partial source evidence, adapter collisions, malformed imports, incompatible
versions, stale/corrupt/missing reports and prior-artifact preservation.

The earlier 19 practice-projection cases, ten malformed-diagnostic cases and 28
configured-policy cases still pass. Also passed: 193 workspace tests, two live
fixtures, 24 Python tests, strict Clippy, rustdoc, package smoke, helper parity,
self `verify` and unchanged self-quality gates. Source lines: **22,499/22,500**;
maximum hotspot **69.22**, cyclomatic/cognitive **20/11**.

Reproduce in the pinned shell after building `rqlens`:

```sh
python3 acceptance/source_reporter_campaign.py --output target/source-reporters-new
```

Use a fresh output directory. Evidence: `acceptance/source-reporters-results.json`
and `target/source-reporters/`.

Still open: review/change-snapshot projection and product-wide policy-disposition
agreement, exhaustive per-rule detector cases, compatible engine applicability,
semantic disagreement and exclusion-scope modelling, and large-report resource
certification. Preserved producer claims are not proof of applicability or scope.
No scoring rules, source scopes, thresholds or budgets were weakened; T4 and
release remain uncertified.
