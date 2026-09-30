import hashlib
import json
import pathlib
import tempfile
import unittest

from calibration import check


class FrozenCandidates(unittest.TestCase):
    def test_freeze_is_not_approval_and_mutation_requires_review(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory).resolve()
            (root / "acceptance").mkdir()
            candidate = root / "candidate.json"
            candidate.write_text("[]")
            item = {"path": "candidate.json", "sha256": hashlib.sha256(candidate.read_bytes()).hexdigest()}
            plan = {"version": 1, "frozen_candidates": [item], "independent_review": {"status": "approved"}}
            (root / "acceptance/calibration-plan.json").write_text(json.dumps(plan))
            self.assertEqual(check(root), 0)
            self.assertEqual(check(root, release=True), 1)
            candidate.write_text("[1]")
            with self.assertRaisesRegex(ValueError, "Frozen candidate changed"):
                check(root)
            item["path"] = "../outside"
            (root / "acceptance/calibration-plan.json").write_text(json.dumps(plan))
            with self.assertRaisesRegex(ValueError, "out-of-repository"):
                check(root)


if __name__ == "__main__":
    unittest.main()
