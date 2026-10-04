# Acceptance closure

`criteria.json` maps the original review exit criteria (phases 0–4) and their necessary cross-cutting obligations. Closure commits use steps 1–5; they are not acceptance certificates.

Run `python3 acceptance/check.py` to audit the ledger and `python3 acceptance/check.py --release` for the final fail-closed gate. A release requires **every** criterion passed, followed by complete clean-checkout and packaged validation. Never turn a blocked criterion into passed by changing its wording or documenting its limitation. Any scope reduction requires an explicit decision, not silent removal.

Each passed row must cite commands, actual results, source revision, bounded scope and existing regression files. Native evidence locations and reproduction commands are in `validation.md`. A successful broad suite alone does not prove all criteria; end-to-end reporter parity, fault injection and independent label review need their own evidence. Machine checking catches missing evidence, not false claims.

## Current closure result

Steps 1–5 are implementation iterations, not completed acceptance. Six criteria have scoped evidence; **19/25 remain unresolved**. T1 is reconfirmed; T2 and T3 pass the final 182-command/91-comparison campaign on six daemons plus daemon-framework. Unresolved type/semantic scope now explicitly abstains (`trust-t3-review.md`, `trust-t3-results.json`, `../contracts/semantic-abstention.md`). T4 is **not passed**: strict native policy rejects partial self-evidence, and platform/latest-stable certification remains outstanding (`trust-t4-results.json`). Historical clean results are in `step-5.md`; they do not certify the new implementation. Operational recovery/limits are in `../contracts/operational-scope.md`.

The initial E2/E3 milestone inventories rule families and fixes configured-policy false-clean statuses (28 native ingestion cases). Shared practice projection subsequently repairs the nested-diagnostic counterexample with 19 native projection cases and ten malformed-input cases. All E2/E3/E4/E6 criteria remain open beyond these scopes. See `evidence-contracts-results.json`, `reporter-agreement-results.json`, and `../contracts/practice-projection.md`.

Source/imported reporter agreement adds six native agreement fixtures, 30 rejection/abstention cases, twelve exact replays and an actual 127-finding self comparison. Source/import provenance is retained without changing reliability selection or inventing engine-scope completeness. See `source-reporters-results.json` and `../contracts/source-projection.md`.

Additional checks:

```sh
python3 -m unittest discover -s acceptance -p 'test_*.py'
python3 acceptance/calibration.py
python3 acceptance/calibration.py --release  # intentionally blocked pending independent study
python3 acceptance/profile.py               # after native build/helper prewarming
python3 acceptance/trust_campaign.py --output target/trust-new  # fresh directory; built rqlens required
python3 acceptance/check_trust_t3.py target/trust-new            # current incomplete syntax profiles
python3 acceptance/evidence_contract_campaign.py --output target/evidence-new  # fresh native policy fixtures
python3 acceptance/probe_reporter_gaps.py --output target/reporter-probe-new  # bounded projection probe, not certification
python3 acceptance/reporter_campaign.py --output target/reporters-new  # lossless projection and rejection cases
python3 acceptance/source_reporter_campaign.py --output target/source-reporters-new  # source/imported projection
python3 acceptance/trust_campaign.py --output target/expansion-new --tools source-expansion  # experimental, opt-in
```

`resource-budgets.json` fixes the sampled workload ceilings; `resource-results.json` retains all three repetitions and source-snapshot identifiers. These synthetic observations are not a cross-language benchmark or a universal memory guarantee. The structural gate also rejects deleted criteria and remains active under `python -O`.

Known up-front external dependencies: independently reviewed held-out labels and behavior-pair intent review. GitHub authentication is delivery-only and cannot satisfy any acceptance criterion. Do not push until `--release` and the final native profiles pass.
