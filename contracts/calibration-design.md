# Labeled evaluation and anti-gaming checks

All three lenses use the shared `calibration-cases.json` conformance example. Language-specific corpora run through real analyzers in the normal test suite and include identifier/location mutations. Counts are per explicitly labeled case/rule, not per arbitrary unlabeled finding.

Definitions:
- Precision = TP / (TP + FP); no predicted positives means unknown, not 100%.
- Recall = TP / labeled positives. An abstained positive remains a missed detection.
- Evaluation coverage and abstention rate are reported separately.
- An abstained negative is not a true negative.
- Development and holdout partitions are never pooled implicitly.

The committed corpora are small regression corpora. Their `holdout` partition protects selected cases from being mixed into development reports, but it is **not independent real-world validation**: cases were authored during this upgrade from known failure modes. No population accuracy, defect probability, or calibrated ranking claim follows from passing them.

Release regression gates: no unexpected positives/negatives in the committed corpus; no silent abstentions where evaluation is required; line/identifier mutations preserve verdicts; unused public surface cannot improve observed reuse. Existing compiler/test failures still determine correctness independently of maintainability scores.

Before enabling a new blocking heuristic: freeze an independently labeled corpus and thresholds before tuning; include at least one credible negative and one positive per rule and language profile; publish applicability, precision/recall, abstention, cold/warm time and memory; retain a genuinely unseen corpus. Necessary complexity and behavior-preserving before/after examples require human-reviewed intent, not automatic labels.

Leverage output now separates observed consumers, public surface, implementation size and dependency pressure. Overall/legacy pressure models remain explicitly advisory and uncalibrated. Function extraction, more exports, or superficially similar syntax is not proof of an improvement.
