# Closure step 4 — Executable candidates and a frozen review plan

Added executable behavior-pair candidates, not independently reviewed labels:

- TS: sampled numeric behavior, including signed zero/NaN/infinities. A smaller Math.min/Math.max replacement loses signed zero. Actual configured test execution fails on that implementation and also when the required runner is removed; neither becomes a clean audit with findings absent.
- QML: the same signed-zero counterexample and behavior-preserving candidate execute in the live Qt Test runtime, not just Node or a parser surrogate. The installed CMake package campaign also runs the new fixture.
- Rust: rustc builds and executes clamp pairs, demonstrating that max/min loses NaN behavior. An owned-result pair preserves the caller's input; removing necessary ownership fails compilation with the expected type mismatch rather than an unavailable-tool result.

All existing labeled idiom/movement/rename/anti-gaming tests remain. SHA-256 manifests freeze both those regression corpora and new candidates. `python3 acceptance/calibration.py` verifies the freeze; `--release` deliberately remains nonzero. Changing a JSON approval flag cannot unlock it. Independent labels and a reviewed native held-out result adapter are still needed.

## Predeclared study floor (before any ranking tuning)

Use at least 20 independent refactor families from at least three independently selected projects per language. Do not count rename/movement variants as independent families. Obtain intent/readability labels from reviewers not authoring/tuning the model, adjudicate disagreements, and freeze source inputs, language/tool profiles and labels before evaluation. Require at least 80% pairwise agreement on those reviewed families, plus zero unexpected positives, missed declared positives or supported-case abstentions in each declared blocking-rule regression set. Each blocking rule needs a relevant positive and negative; a clone example cannot establish the specificity of an unrelated unwrap rule.

These are a predeclared acceptance floor, **not** population precision estimates or permission to call rankings calibrated defect probabilities. Report per-rule applicability/abstention, partitioned precision/recall, mutations, runtime and memory. Add unavailable, unsupported, necessary-complexity and test-removal cases. Never pool the internally authored regression corpus into independent validation. A threshold change after inspection requires new held-out data. No ranking was tuned in this step.

## Results and remaining blocker

TS: 64 tests passed. QML: 112 tests, live Qt integration and installed CMake package smoke passed. The first Qt run correctly reported qmlformat drift in the new fixture; native formatting was applied and the campaign rerun. Rust: 170 workspace tests passed, one normally ignored; strict Clippy passed. Python freeze regression verifies tamper rejection and that an approval string cannot establish calibration.

**Blocked:** no independent corpus selection, label/intent review or ranking results have been supplied. This iteration does not close acceptance criteria C1/C3. Logs: `/tmp/{ts,qml,rust}-closure-step4.log`, `/tmp/qml-closure-step4-qt.log`.
