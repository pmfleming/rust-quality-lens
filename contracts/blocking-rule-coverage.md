# Blocking-rule evidence: initial E2/E3 milestone

E2, E3, E4 and E6 **remain open**. This milestone fixes configured-policy false
negatives; it does not certify all rule detectors, input consumers or reporters.

## Inventory and applicability

`blocking-rules.json` is embedded in `rqlens catalog` as
`blocking_rule_contracts`. It lists admitted native rule IDs, their producer
families, CLI and automatic gates, prerequisites, applicability, scope, rationale,
limitations and case references. Empty case lists and pending coverage are
intentional, visible gaps—not successful zero-case certification.

Runtime configured-rule evaluation uses this inventory and the actual input
state. Architecture rules additionally require an exact enabled configuration
entry; collisions with registered native IDs abstain. Practice rules require at
least one matching check and **every** matching execution to have a known
`passed` or `failed` status. One passed execution does not hide a skipped sibling.
Existing conservative all-finding-input requirements for error-level configured
rules remain in force.

Unknown rule IDs, arbitrary compiler/Clippy diagnostic IDs and external rule IDs
cannot establish a negative from an empty artifact. The latter two require
rule-enablement and evaluation-scope evidence not yet supported by their adapters.
Imported observations are still retained; declaring an error-level external rule
now abstains rather than treating an empty report as per-rule verification.

## Policy report changes

- `status: incomplete` is distinct from `exceeded` and `disabled`.
- `evaluation_complete` is false for incomplete and disabled evaluations.
- `observed` and `excess` are null unless evaluation is complete.
- `observed_findings` retains the count of **policy-selected** observations after
  waivers and exclusions. It is not an authoritative whole-source total and does
  not claim parity with reporters that select additional observations.
- `reason` explains incomplete evaluation. The configured level explains disabled
  evaluation.
- Incomplete error-level rules block `configured-rules`, but are not placed in
  `policy_rule_violations` as fabricated ordinary violations. Warning/advisory
  levels retain their nonblocking disposition without becoming verified-clean.
- Missing artifact placeholders are no longer emitted as ordinary warning
  findings; artifact status and required-input failures remain explicit.

These are additive report fields plus a deliberate stricter policy behavior.
Consumers must handle null counts and `incomplete`; `passed` is not inferred from
an empty finding collection. All 19 native artifact schemas remain unchanged.

## Executed cases

`acceptance/evidence_contract_campaign.py` creates current native fingerprints and
synthetic native report payloads. Its 28 cases cover clean/violating inputs,
missing/corrupt/stale/partial/unsupported-schema reports, missing/skipped/timed-out/
unavailable checks, mixed passed/skipped checks, unsupported IDs, disabled or
colliding architecture rules, and disabled/nonblocking policy levels.

The candidate passes all 28 cases, including CLI stdout/published JSON agreement;
28 replays of the same fixtures preserve the complete reports with timestamp-only
normalization. This is **policy-ingestion** evidence, not proof that every backend
correctly detects its positive/negative source cases. Baseline results are retained
separately; missing new report fields must not be described as preexisting bugs.

## Baseline E4/E6 counterexample (`8d61005`)

The practice-specific gap below is now repaired by the shared projection described
in `practice-projection.md`. The original baseline evidence remains retained;
product-wide E4/E6 and compatible-engine scope semantics remain open.

`acceptance/probe_reporter_gaps.py` supplies the same nested diagnostic through two
invocation records. On the baseline, policy did not count the nested diagnostic. SARIF emitted one
result but dropped the second invocation's corroborating evidence. At this initial milestone the policy rule
correctly abstained instead of claiming zero verified findings, but projection
agreement and multi-engine semantics were **not fixed**.

Next: a shared typed observation/projection boundary that retains producer/run,
rule identity, subject, scope, exclusions, evidence and disposition. Deduplication
must preserve contributing observations. Absence in a second engine is not
necessarily disagreement; first establish compatible applicability and coverage.
Then expand rule-specific positive/negative/unavailable cases and reporter parity,
including waived, excluded and conflicting observations. Do not close E2/E3 from
family-level contracts or E4/E6 from structural JSON validation alone.

## Reproduction

In the pinned Nix shell, after `cargo build --locked --bin rqlens`:

```sh
python3 acceptance/evidence_contract_campaign.py --output target/evidence-new
python3 acceptance/probe_reporter_gaps.py --output target/reporter-probe-new
python3 -m unittest discover -s acceptance -p 'test_*.py'
```

Outputs must be fresh directories. Results and validation are summarized in
`acceptance/evidence-contracts-results.json`; raw evidence is in
`target/evidence-contracts/`. Unchanged self gates pass at 22,498 source lines,
69.22 maximum hotspot and 20/11 maximum cyclomatic/cognitive complexity. Small
supply-chain-check and fixture deduplications preserve headroom without changing
scope, weights, thresholds or completeness requirements. Nothing is released.
