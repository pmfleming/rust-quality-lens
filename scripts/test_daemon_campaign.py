import importlib.util
from pathlib import Path
import tempfile
import tomllib
import unittest
from unittest.mock import patch
import sys

spec = importlib.util.spec_from_file_location("campaign", Path(__file__).with_name("daemon-campaign.py"))
campaign = importlib.util.module_from_spec(spec)
spec.loader.exec_module(campaign)


class CampaignTests(unittest.TestCase):
    def test_overrides_preserve_architecture_and_verification_policy(self):
        text = 'project_root = "."\n[rust]\nidentity_resolution = "auto"\n[verification]\nlocked = false\naudit = true\n[[architecture.rules]]\nid = "test"\nfrom = ["a"]\ndeny = ["b"]\n'
        text = campaign.override(text, "", "project_root", "/tmp/a project")
        text = campaign.override(text, "", "output_dir", "/tmp/output")
        text = campaign.override(text, "verification", "locked", True)
        config = tomllib.loads(text)
        self.assertEqual(config["project_root"], "/tmp/a project")
        self.assertTrue(config["verification"]["locked"])
        self.assertTrue(config["verification"]["audit"])
        self.assertEqual(config["architecture"]["rules"][0]["deny"], ["b"])

    def test_new_verification_table_is_valid(self):
        config = tomllib.loads(campaign.override('project_root = "."\n[rust]\nidentity_resolution = "auto"\n', "verification", "locked", True))
        self.assertTrue(config["verification"]["locked"])
        self.assertNotIn("locked", config["rust"])

    def test_missing_numeric_evidence_does_not_become_zero(self):
        self.assertIsNone(campaign.distribution([], "score"))
        self.assertIsNone(campaign.distribution([{"score": None}], "score"))
        self.assertEqual(campaign.distribution([{"score": 0}], "score")["max"], 0)

    def test_profile_output_is_isolated_from_source(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            result = campaign.command(
                [sys.executable, "-c", "import os; print(os.environ['LLVM_PROFILE_FILE'])"],
                root, root / "verify.log",
            )
            self.assertEqual(result["exit_code"], 0)
            self.assertEqual((root / "verify.log").read_text().strip(), str(root / "profiles/%m_%p.profraw"))
            self.assertTrue((root / "profiles").is_dir())

    def test_unstable_capture_cannot_be_compared(self):
        with patch.object(campaign, "summarize", return_value={"run": {"source_stable": False}}):
            with self.assertRaisesRegex(RuntimeError, "source changed"):
                campaign.compare(Path("before"), Path("after"))

    def test_test_and_vendor_code_are_not_production_functions(self):
        root = Path("/project")
        for path, name in [("src/tests.rs", "test"), ("src/a.rs", "a::tests::helper"), ("vendor/a/src/lib.rs", "run")]:
            self.assertFalse(campaign.production_function(root, {"path": str(root / path), "name": name}))
        self.assertTrue(campaign.production_function(root, {"path": "/project/src/domain.rs", "name": "domain::run"}))


if __name__ == "__main__":
    unittest.main()
