import unittest

from source_reporter_campaign import imported, source_rows


class SourceReporterOracleTests(unittest.TestCase):
    def test_out_of_scope_and_waived_observations_are_not_dropped(self):
        policy = {field: [] for field in ["reliability_findings", "reliability_warnings", "reliability_out_of_scope",
                                         "architecture_violations", "architecture_warnings", "test_quality_findings", "test_quality_warnings"]}
        policy["reliability_out_of_scope"] = [{"id": "test-only"}]
        policy["reliability_warnings"] = [{"id": "production"}]
        policy["waived_findings"] = [{"finding": {"id": "waived"}, "waiver": {"reason": "fixture"}}]
        self.assertEqual({row["id"] for row in source_rows(policy)}, {"test-only", "production", "waived"})

    def test_import_fixture_retains_scope_and_attempts_to_spoof_selection(self):
        raw = imported({"input_fingerprint": {"digest": "fixture"}})
        self.assertEqual(raw["scope"]["excluded"], ["tests"])
        self.assertFalse(raw["findings"][0]["policy_selected"])
        self.assertFalse(raw["findings"][0]["policy_scope"]["included"])
        self.assertEqual(raw["findings"][0]["id"], "original")
        self.assertEqual(raw["tool"]["version"], "1")


if __name__ == "__main__":
    unittest.main()
