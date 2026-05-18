from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "tests" / "fixtures" / "mini_rust_project"


class CliTests(unittest.TestCase):
    def run_lens(self, *args: str) -> subprocess.CompletedProcess[str]:
        env = os.environ.copy()
        env["PYTHONPATH"] = str(ROOT / "src")
        return subprocess.run(
            [
                sys.executable,
                "-m",
                "rust_quality_lens.cli",
                *args,
                "--config",
                str(FIXTURE / "rqlens.toml"),
            ],
            cwd=FIXTURE,
            env=env,
            text=True,
            capture_output=True,
        )

    def test_catalog_lists_quality_tasks(self) -> None:
        result = self.run_lens("catalog")

        self.assertEqual(result.returncode, 0, result.stderr)
        payload = json.loads(result.stdout)
        self.assertEqual(payload["project_name"], "mini-rust-project")
        self.assertIn("quality.escape-hatches", {task["id"] for task in payload["tasks"]})
        self.assertIn("correctness.catalog", {task["id"] for task in payload["tasks"]})
        self.assertIn("map.architecture", {task["id"] for task in payload["tasks"]})

    def test_escape_hatches_runs_on_fixture_project(self) -> None:
        result = self.run_lens("measure", "escape-hatches")

        self.assertEqual(result.returncode, 0, result.stderr)
        output = FIXTURE / "target" / "analysis" / "rust_escape_hatches.json"
        self.assertTrue(output.exists())
        payload = json.loads(output.read_text(encoding="utf-8"))
        self.assertIsInstance(payload, list)

    def test_correctness_runs_on_fixture_project(self) -> None:
        result = self.run_lens("measure", "correctness")

        self.assertEqual(result.returncode, 0, result.stderr)
        review = FIXTURE / "target" / "analysis" / "correctness_review.json"
        catalog = FIXTURE / "target" / "analysis" / "test_catalog.json"
        self.assertTrue(review.exists())
        self.assertTrue(catalog.exists())
        payload = json.loads(review.read_text(encoding="utf-8"))
        self.assertEqual(payload["summary"]["test_count"], 0)

    def test_map_runs_on_fixture_project(self) -> None:
        self.run_lens("measure", "hotspots")
        result = self.run_lens("measure", "map")

        self.assertEqual(result.returncode, 0, result.stderr)
        output = FIXTURE / "target" / "analysis" / "map.json"
        self.assertTrue(output.exists())
        payload = json.loads(output.read_text(encoding="utf-8"))
        self.assertEqual(payload["meta"]["source_root"], "src")

    def test_type_health_runs_on_fixture_project(self) -> None:
        result = self.run_lens("measure", "type-health")

        self.assertEqual(result.returncode, 0, result.stderr)
        output = FIXTURE / "target" / "analysis" / "type_health.json"
        self.assertTrue(output.exists())
        payload = json.loads(output.read_text(encoding="utf-8"))
        self.assertIsInstance(payload, list)


class TestCatalogLogicTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        sys.path.insert(0, str(ROOT / "src"))
        sys.path.insert(0, str(ROOT / "src" / "rust_quality_lens" / "tools"))

    def test_failed_requested_correctness_run_requests_nonzero_exit(self) -> None:
        from rust_quality_lens.tools import test_catalog

        payload = {"summary": {"last_run": {"status": "failed"}}}

        self.assertTrue(test_catalog.requested_run_failed(payload, run=True))

    def test_catalog_only_payload_does_not_fail_without_run(self) -> None:
        from rust_quality_lens.tools import test_catalog

        payload = {"summary": {"last_run": None}}

        self.assertFalse(test_catalog.requested_run_failed(payload, run=False))

    def test_passed_requested_correctness_run_is_successful(self) -> None:
        from rust_quality_lens.tools import test_catalog

        payload = {"summary": {"last_run": {"status": "passed"}}}

        self.assertFalse(test_catalog.requested_run_failed(payload, run=True))

if __name__ == "__main__":
    unittest.main()
