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

    def test_escape_hatches_runs_on_fixture_project(self) -> None:
        result = self.run_lens("measure", "escape-hatches")

        self.assertEqual(result.returncode, 0, result.stderr)
        output = FIXTURE / "target" / "analysis" / "rust_escape_hatches.json"
        self.assertTrue(output.exists())
        payload = json.loads(output.read_text(encoding="utf-8"))
        self.assertIsInstance(payload, list)

    def test_type_health_runs_on_fixture_project(self) -> None:
        result = self.run_lens("measure", "type-health")

        self.assertEqual(result.returncode, 0, result.stderr)
        output = FIXTURE / "target" / "analysis" / "type_health.json"
        self.assertTrue(output.exists())
        payload = json.loads(output.read_text(encoding="utf-8"))
        self.assertIsInstance(payload, list)


if __name__ == "__main__":
    unittest.main()
