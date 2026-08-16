# Architecture Risk Model v5 Candidate

RQLens continues to emit `rqlens.architecture_risk` version 4. The newly added
function CRAP, configured architecture-rule, test-quality, clone-percentage,
and module change-entropy evidence is intentionally not part of the aggregate
risk score yet.

A v5 model may be activated only after a pinned multi-project calibration run
includes fresh function coverage, project-specific architecture rules, and
held-out reviewed outcome labels. The candidate must improve ranking lift
without reducing semantic identity confidence or turning missing evidence into
zero risk.

`rqlens calibrate` now reports cyclomatic and cognitive distributions, per-project
change entropy, and an explicit `risk_model_v5_candidate` decision block. Until
the validation criteria are met, its status remains `deferred` and baseline
comparisons continue to require risk model version 4.
