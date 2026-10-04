import copy
import unittest

from reporter_campaign import checks, reconstruct, scenarios
from probe_reporter_gaps import projection_agrees


class ReporterCampaignTests(unittest.TestCase):
    def fixture(self):
        raw = {"rule_id": "rust.compiler.diagnostic", "id": "original", "message": "finding"}
        findings = {"projected": {"observations": [{"check_index": 2, "diagnostic_index": 0, "diagnostic": raw}]}}
        evidence = {"artifact_with_diagnostic_refs": {"data": {"checks": [
            {"diagnostics": []}, {}, {"diagnostics": [{"finding_id": "projected", "observation_index": 0}]}]}}}
        return raw, findings, evidence

    def test_lossless_projection_and_invalid_observations(self):
        raw, findings, evidence = self.fixture()
        before = copy.deepcopy(evidence)
        actual = reconstruct(evidence, findings)["data"]["checks"]
        self.assertEqual(actual, [{"diagnostics": []}, {}, {"diagnostics": [raw]}])
        self.assertEqual(evidence, before)

        findings["projected"]["observations"][0]["check_index"] = 0
        with self.assertRaises(AssertionError):
            reconstruct(evidence, findings)
        findings["projected"]["observations"] = []
        with self.assertRaises(IndexError):
            reconstruct(evidence, findings)

        self.assertFalse(projection_agrees([], [None]))
        self.assertFalse(projection_agrees([{"id": "a"}], [None]))
        self.assertTrue(projection_agrees([], []))
        self.assertTrue(projection_agrees([{"id": "a"}], [{"id": "a"}]))

    def test_each_mutation_has_independent_inputs(self):
        rows = scenarios()
        self.assertEqual(len(rows), len({row[0] for row in rows}))
        data = checks()
        data[0]["diagnostics"][0]["severity"] = "warning"
        self.assertEqual(data[1]["diagnostics"][0]["severity"], "error")
        self.assertEqual(checks()[0]["diagnostics"][0]["severity"], "error")
        self.assertIn("no-inventory", [row[0] for row in rows])
        self.assertIn("empty-observations", [row[0] for row in rows])


if __name__ == "__main__":
    unittest.main()
