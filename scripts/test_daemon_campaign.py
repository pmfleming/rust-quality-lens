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

        # An absent table must be created without nesting it under [rust].
        config = tomllib.loads(campaign.override('project_root = "."\n[rust]\nidentity_resolution = "auto"\n', "verification", "locked", True))
        self.assertTrue(config["verification"]["locked"])
        self.assertNotIn("locked", config["rust"])

    def test_default_build_uses_cargos_reported_executable(self):
        message = campaign.json.dumps({"reason": "compiler-artifact", "target": {"name": "rqlens"},
                                      "executable": "/custom-target/debug/rqlens"})
        with patch.object(campaign.subprocess, "check_output", return_value=message) as build:
            self.assertEqual(campaign.build_binary(), Path("/custom-target/debug/rqlens"))
            arguments = build.call_args.args[0]
            self.assertIn("--locked", arguments)
            self.assertEqual(arguments[arguments.index("--target-dir") + 1],
                             str(campaign.LENS / "target/campaign-tool-build"))

    def test_capture_freezes_binary_before_commands_can_replace_it(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "rqlens.toml").write_text('project_root = "."\n')
            binary = root / "built-rqlens"
            binary.write_bytes(b"original executable")
            destination = root / "capture"
            def run(args, cwd, log):
                self.assertEqual(args[0], str(destination / "rqlens"))
                binary.write_bytes(b"replaced executable")
                return {"command": args, "exit_code": 0}
            with patch.object(campaign, "revision", return_value={"head": "fixed", "dirty": False}), \
                 patch.object(campaign, "git", return_value="/repository/.git"), \
                 patch.object(campaign, "command", side_effect=run):
                campaign.collect(root, destination, binary)
            provenance = campaign.json.loads((destination / "run.json").read_text())
            self.assertEqual((destination / "rqlens").read_bytes(), b"original executable")
            self.assertEqual(provenance["binary_sha256"], campaign.hashlib.sha256(b"original executable").hexdigest())

        # Actual command execution must isolate profiling side effects too.
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            result = campaign.command(
                [sys.executable, "-c", "import os; print(os.environ['LLVM_PROFILE_FILE'])"],
                root, root / "verify.log",
            )
            self.assertEqual(result["exit_code"], 0)
            self.assertEqual((root / "verify.log").read_text().strip(), str(root / "profiles/%m_%p.profraw"))
            self.assertTrue((root / "profiles").is_dir())

    def test_unstable_sources_or_changed_policy_cannot_be_compared(self):
        with patch.object(campaign, "summarize", return_value={"run": {"source_stable": False}}):
            with self.assertRaisesRegex(RuntimeError, "source changed"):
                campaign.compare(Path("before"), Path("after"))

        with tempfile.TemporaryDirectory() as directory:
            before, after = [Path(directory) / name for name in ["before", "after"]]
            for root, enabled in [(before, "true"), (after, "false")]:
                root.mkdir()
                (root / "rqlens.toml").write_text(f'[verification]\naudit = {enabled}\n')
            with patch.object(campaign, "summarize", return_value={"run": {"source_stable": True}}):
                with self.assertRaisesRegex(RuntimeError, "configuration differs"):
                    campaign.compare(before, after)

    def test_missing_evidence_and_verification_make_comparison_provisional(self):
        self.assertIsNone(campaign.distribution([], "score"))
        self.assertIsNone(campaign.distribution([{"score": None}], "score"))
        self.assertEqual(campaign.distribution([{"score": 0}], "score")["max"], 0)
        with tempfile.TemporaryDirectory() as directory:
            roots = [Path(directory) / name for name in ["before", "after"]]
            for root in roots:
                (root / "analysis").mkdir(parents=True)
                (root / "rqlens.toml").write_text(f'project_root = "{root}"\noutput_dir = "{root}/analysis"\n')
                (root / "run.json").write_text(campaign.json.dumps({
                    "root": str(root), "repository_id": "/shared/.git",
                    "source_stable": True, "binary_sha256": "same", "commands": [],
                }))
            result = campaign.compare(*roots)
            self.assertEqual(result["comparison_status"], "provisional")
            self.assertIsNone(result["deltas"]["production_panic_findings_in_src"])
            self.assertTrue(any("nonconformant" in warning for warning in result["warnings"]))
            summaries = [campaign.summarize(root) for root in roots]
            for version in [4, 5]:
                for summary in summaries:
                    summary["artifact_status"] = {"hotspots.json": {"fingerprint": {
                        "algorithm": f"fnv1a64-quality-inputs-v{version}", "complete": True,
                        "helper_digest": "same-helper-inputs",
                    }}}
                with patch.object(campaign, "summarize", side_effect=summaries):
                    result = campaign.compare(*roots)
                self.assertEqual(any("project-and-helper" in warning for warning in result["warnings"]),
                                 version != 5)
            summaries[1]["artifact_status"]["hotspots.json"]["fingerprint"]["helper_digest"] = "changed"
            with patch.object(campaign, "summarize", side_effect=summaries):
                with self.assertRaisesRegex(RuntimeError, "Helper inputs differ"):
                    campaign.compare(*roots)

    def test_test_and_vendor_code_are_not_production_functions(self):
        root = Path("/project")
        for path, name in [("src/tests.rs", "test"), ("src/a.rs", "a::tests::helper"),
                           ("src/background_tests.rs", "case"), ("src/a.rs", "a::disk_tests::helper"),
                           ("vendor/a/src/lib.rs", "run")]:
            self.assertFalse(campaign.production_function(root, {"path": str(root / path), "name": name}))
        self.assertTrue(campaign.production_function(root, {"path": "/project/src/domain.rs", "name": "domain::run"}))


if __name__ == "__main__":
    unittest.main()
