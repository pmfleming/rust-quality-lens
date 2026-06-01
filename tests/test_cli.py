from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path
import tempfile
import textwrap
import unittest


ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "tests" / "fixtures" / "mini_rust_project"
GOLDEN_FIXTURE = ROOT / "tests" / "fixtures" / "golden_rust_project"


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
        self.assertIn("quality.escape_hatches", {task["id"] for task in payload["tasks"]})
        escape_task = next(task for task in payload["tasks"] if task["id"] == "quality.escape_hatches")
        self.assertIn("quality.escape-hatches", escape_task["aliases"])
        self.assertEqual(escape_task["commands"], [["rqlens", "measure", "escape-hatches"]])
        self.assertIn("correctness.catalog", {task["id"] for task in payload["tasks"]})
        self.assertIn("map.architecture", {task["id"] for task in payload["tasks"]})

    def test_config_paths_are_relative_to_config_file(self) -> None:
        env = os.environ.copy()
        env["PYTHONPATH"] = str(ROOT / "src")
        result = subprocess.run(
            [
                sys.executable,
                "-m",
                "rust_quality_lens.cli",
                "catalog",
                "--config",
                str(FIXTURE / "rqlens.toml"),
            ],
            cwd=ROOT,
            env=env,
            text=True,
            capture_output=True,
        )

        self.assertEqual(result.returncode, 0, result.stderr)
        payload = json.loads(result.stdout)
        self.assertEqual(payload["analysis_root"], str(FIXTURE / "target" / "analysis"))

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
        self.assertEqual(payload["meta"]["project_name"], "mini-rust-project")
        self.assertEqual(payload["meta"]["source_roots"], ["src"])
        self.assertEqual(payload["meta"]["risk_model_id"], "rqlens.architecture_risk")
        self.assertEqual(payload["meta"]["risk_model_version"], 1)
        self.assertIn("maintainability", payload["meta"]["risk_model_weights"])
        self.assertIn("type_health", payload["meta"]["risk_model_tool_scores"])
        self.assertEqual(payload["meta"]["layer_ruleset"]["id"], "rqlens.generic_layers")

    def test_type_health_runs_on_fixture_project(self) -> None:
        result = self.run_lens("measure", "type-health")

        self.assertEqual(result.returncode, 0, result.stderr)
        output = FIXTURE / "target" / "analysis" / "type_health.json"
        self.assertTrue(output.exists())
        payload = json.loads(output.read_text(encoding="utf-8"))
        self.assertIsInstance(payload, list)

    def test_syn_type_health_keeps_same_named_types_separate(self) -> None:
        with tempfile.TemporaryDirectory() as raw_temp:
            root = Path(raw_temp)
            (root / "src").mkdir()
            (root / "src" / "a.rs").write_text(
                "pub struct Config { pub first: i32 }\nimpl Config { pub fn one(&self) {} }\n",
                encoding="utf-8",
            )
            (root / "src" / "b.rs").write_text(
                "pub struct Config { pub second: i32 }\nimpl Config { pub fn one(&self) {} pub fn two(&self) {} }\n",
                encoding="utf-8",
            )
            config = self.write_temp_config(root)

            result = self.run_lens_with_config(root, config, "measure", "type-health")

            self.assertEqual(result.returncode, 0, result.stderr)
            payload = json.loads((root / "target" / "analysis" / "type_health.json").read_text(encoding="utf-8"))
            methods = {item["qualified_name"]: item["method_count"] for item in payload}
            self.assertEqual(methods["a::Config"], 1)
            self.assertEqual(methods["b::Config"], 2)

    def test_golden_type_health_covers_same_names_and_tuple_structs(self) -> None:
        result = self.run_lens_for_fixture(GOLDEN_FIXTURE, "measure", "type-health")

        self.assertEqual(result.returncode, 0, result.stderr)
        payload = json.loads(
            (GOLDEN_FIXTURE / "target" / "analysis" / "type_health.json").read_text(
                encoding="utf-8"
            )
        )
        by_name = {item["qualified_name"]: item for item in payload}
        self.assertEqual(by_name["alpha::Config"]["method_count"], 1)
        self.assertEqual(by_name["beta::Config"]["method_count"], 2)
        self.assertEqual(by_name["domain::TupleConfig"]["kind"], "struct")
        self.assertEqual(by_name["domain::TupleConfig"]["shape"], "tuple")
        self.assertEqual(by_name["domain::TupleConfig"]["field_count"], 2)
        self.assertEqual(by_name["domain::Marker"]["kind"], "struct")
        self.assertEqual(by_name["domain::Marker"]["shape"], "unit")
        self.assertEqual(by_name["domain::Marker"]["field_count"], 0)
        self.assertEqual(by_name["domain::RichEnum"]["kind"], "enum")
        self.assertEqual(by_name["domain::RichEnum"]["variant_count"], 16)
        self.assertEqual(by_name["domain::RichEnum"]["variant_field_count"], 6)
        self.assertEqual(by_name["domain::RichEnum"]["field_count"], 6)
        self.assertEqual(by_name["domain::inline_a::Shared"]["method_count"], 1)
        self.assertEqual(by_name["domain::inline_b::Shared"]["method_count"], 2)
        self.assertEqual(by_name["domain::RichEnum"]["risk_model_id"], "rqlens.architecture_risk")
        self.assertEqual(by_name["domain::RichEnum"]["risk_model_version"], 1)
        self.assertEqual(by_name["domain::RichEnum"]["risk_calibration"], "type_health")

    def test_syn_map_captures_direct_module_path_dependency(self) -> None:
        with tempfile.TemporaryDirectory() as raw_temp:
            root = Path(raw_temp)
            (root / "src").mkdir()
            (root / "src" / "lib.rs").write_text(
                "pub mod math;\npub fn public_entry() -> i32 { math::wide_branch(5) }\n",
                encoding="utf-8",
            )
            (root / "src" / "math.rs").write_text(
                "pub fn wide_branch(value: i32) -> i32 { value + 1 }\n",
                encoding="utf-8",
            )
            config = self.write_temp_config(root)

            result = self.run_lens_with_config(root, config, "measure", "map")

            self.assertEqual(result.returncode, 0, result.stderr)
            payload = json.loads((root / "target" / "analysis" / "map.json").read_text(encoding="utf-8"))
            edges = {(edge["data"]["source"], edge["data"]["target"]) for edge in payload["graph"]["edges"]}
            self.assertIn(("lib", "math"), edges)
            lib_node = next(
                node["data"]
                for node in payload["graph"]["nodes"]
                if node["data"].get("id") == "lib"
            )
            self.assertIsNone(lib_node["maintainability_risk"])
            self.assertIn("maintainability_risk", lib_node["unknown_metrics"])

    def test_golden_map_covers_grouped_imports_pub_use_and_direct_crate_calls(self) -> None:
        result = self.run_lens_for_fixture(GOLDEN_FIXTURE, "measure", "map")

        self.assertEqual(result.returncode, 0, result.stderr)
        payload = json.loads(
            (GOLDEN_FIXTURE / "target" / "analysis" / "map.json").read_text(
                encoding="utf-8"
            )
        )
        edges = {
            (edge["data"]["source"], edge["data"]["target"])
            for edge in payload["graph"]["edges"]
        }
        self.assertIn(("lib", "alpha"), edges)
        self.assertIn(("lib", "beta"), edges)
        self.assertIn(("lib", "domain"), edges)
        self.assertIn(("lib", "nested"), edges)
        self.assertIn(("lib", "service"), edges)
        self.assertIn(("lib", "wired"), edges)
        self.assertIn(("nested", "domain"), edges)
        self.assertIn(("service", "alpha"), edges)
        self.assertIn(("service", "beta"), edges)
        self.assertIn(("service", "domain"), edges)
        self.assertIn(("service", "service::local"), edges)
        self.assertIn(("service", "external:serde_json"), edges)
        self.assertIn(("service", "external:tracing"), edges)
        node_ids = {node["data"]["id"] for node in payload["graph"]["nodes"]}
        self.assertIn("wired", node_ids)
        self.assertNotIn("attributed", node_ids)
        service = next(
            node["data"]
            for node in payload["graph"]["nodes"]
            if node["data"].get("id") == "service"
        )
        self.assertEqual(service["external_dependencies"], ["serde_json", "tracing"])
        unsupported = payload["measurement_confidence"]["unsupported_pattern"]
        self.assertTrue(
            any("macro-generated module wiring" in item for item in unsupported)
        )

    def test_syn_correctness_discovers_proc_macro_style_tests(self) -> None:
        with tempfile.TemporaryDirectory() as raw_temp:
            root = Path(raw_temp)
            (root / "src").mkdir()
            (root / "src" / "lib.rs").write_text(
                "#[rstest]\nfn parses_input() {}\n",
                encoding="utf-8",
            )
            config = self.write_temp_config(root)

            result = self.run_lens_with_config(root, config, "measure", "correctness")

            self.assertEqual(result.returncode, 0, result.stderr)
            payload = json.loads((root / "target" / "analysis" / "correctness_review.json").read_text(encoding="utf-8"))
            self.assertEqual(payload["summary"]["test_count"], 1)

    def test_golden_correctness_covers_duplicate_names_and_macro_tests(self) -> None:
        result = self.run_lens_for_fixture(GOLDEN_FIXTURE, "measure", "correctness")

        self.assertEqual(result.returncode, 0, result.stderr)
        payload = json.loads(
            (GOLDEN_FIXTURE / "target" / "analysis" / "correctness_review.json").read_text(
                encoding="utf-8"
            )
        )
        tests = payload["tests"]
        duplicate_tests = [item for item in tests if item["name"] == "duplicate_name"]
        self.assertEqual(len(duplicate_tests), 2)
        self.assertEqual({item["path"] for item in duplicate_tests}, {
            "tests/duplicate_a.rs",
            "tests/duplicate_b.rs",
        })
        self.assertIn("macro_case_test", {item["name"] for item in tests})
        self.assertIn("macro_style_test", {item["name"] for item in tests})
        self.assertIn("namespaced_macro_test", {item["name"] for item in tests})
        self.assertIn("bench_smoke_test", {item["name"] for item in tests})
        self.assertIn("example_smoke_test", {item["name"] for item in tests})
        by_name = {item["name"]: item for item in tests}
        self.assertEqual(by_name["bench_smoke_test"]["kind"], "benchmark")
        self.assertEqual(by_name["example_smoke_test"]["kind"], "example")
        self.assertEqual(payload["summary"]["benchmark_count"], 1)
        self.assertEqual(payload["summary"]["example_count"], 1)
        unsupported = payload["measurement_confidence"]["unsupported_pattern"]
        self.assertTrue(any("macro-generated tests" in item for item in unsupported))

    def test_syn_escape_hatches_count_unsafe_blocks(self) -> None:
        with tempfile.TemporaryDirectory() as raw_temp:
            root = Path(raw_temp)
            (root / "src").mkdir()
            (root / "src" / "lib.rs").write_text(
                "pub unsafe fn raw_escape(ptr: *const i32) -> i32 { unsafe { *ptr } }\n",
                encoding="utf-8",
            )
            config = self.write_temp_config(root)

            result = self.run_lens_with_config(root, config, "measure", "escape-hatches")

            self.assertEqual(result.returncode, 0, result.stderr)
            payload = json.loads((root / "target" / "analysis" / "rust_escape_hatches.json").read_text(encoding="utf-8"))
            self.assertEqual(payload[0]["unsafe_count"], 2)
            self.assertTrue(payload[0]["measurement_confidence"]["complete"])

    def test_golden_escape_hatches_do_not_double_count_clippy_suppressions(self) -> None:
        result = self.run_lens_for_fixture(GOLDEN_FIXTURE, "measure", "escape-hatches")

        self.assertEqual(result.returncode, 0, result.stderr)
        payload = json.loads(
            (GOLDEN_FIXTURE / "target" / "analysis" / "rust_escape_hatches.json").read_text(
                encoding="utf-8"
            )
        )
        domain = next(item for item in payload if item["module_key"] == "domain")
        self.assertEqual(domain["clippy_suppression_count"], 1)
        self.assertEqual(domain["lint_suppression_count"], 0)
        self.assertEqual(domain["allow_attribute_count"], 1)
        self.assertEqual(domain["total_count"], 1)
        self.assertEqual(domain["escape_hatch_score"], 3.0)
        self.assertEqual(domain["scoring_counts"]["clippy_suppression"], 1)
        self.assertEqual(domain["scoring_counts"]["lint_suppression"], 0)
        self.assertEqual(domain["risk_model_id"], "rqlens.architecture_risk")
        self.assertEqual(domain["risk_model_version"], 1)
        self.assertEqual(domain["risk_calibration"], "escape_hatches")

    def test_measurement_confidence_reports_missing_source_input(self) -> None:
        with tempfile.TemporaryDirectory() as raw_temp:
            root = Path(raw_temp)
            (root / "src").mkdir()
            config = self.write_temp_config(root)

            result = self.run_lens_with_config(root, config, "measure", "correctness")

            self.assertEqual(result.returncode, 0, result.stderr)
            payload = json.loads((root / "target" / "analysis" / "correctness_review.json").read_text(encoding="utf-8"))
            confidence = payload["measurement_confidence"]
            self.assertFalse(confidence["complete"])
            self.assertTrue(confidence["partial"])
            self.assertTrue(confidence["missing_input"])

    def test_measurement_confidence_reports_unsupported_syntax(self) -> None:
        with tempfile.TemporaryDirectory() as raw_temp:
            root = Path(raw_temp)
            (root / "src").mkdir()
            (root / "src" / "lib.rs").write_text("pub fn broken( {\n", encoding="utf-8")
            config = self.write_temp_config(root)

            result = self.run_lens_with_config(root, config, "measure", "correctness")

            self.assertEqual(result.returncode, 0, result.stderr)
            payload = json.loads((root / "target" / "analysis" / "correctness_review.json").read_text(encoding="utf-8"))
            confidence = payload["measurement_confidence"]
            self.assertFalse(confidence["complete"])
            self.assertTrue(confidence["unsupported_pattern"])

    def test_measurement_confidence_reports_stale_map_inputs(self) -> None:
        with tempfile.TemporaryDirectory() as raw_temp:
            root = Path(raw_temp)
            (root / "src").mkdir()
            source = root / "src" / "lib.rs"
            source.write_text("pub fn answer() -> i32 { 42 }\n", encoding="utf-8")
            analysis = root / "target" / "analysis"
            analysis.mkdir(parents=True)
            artifacts = [
                analysis / "hotspots.json",
                analysis / "correctness_review.json",
                analysis / "locality_metrics.json",
                analysis / "leverage_metrics.json",
            ]
            artifacts[0].write_text("[]\n", encoding="utf-8")
            artifacts[1].write_text('{"tests": []}\n', encoding="utf-8")
            artifacts[2].write_text("[]\n", encoding="utf-8")
            artifacts[3].write_text("[]\n", encoding="utf-8")
            old_time = source.stat().st_mtime - 60
            for artifact in artifacts:
                os.utime(artifact, (old_time, old_time))
            config = self.write_temp_config(root)

            result = self.run_lens_with_config(root, config, "measure", "map")

            self.assertEqual(result.returncode, 0, result.stderr)
            payload = json.loads((analysis / "map.json").read_text(encoding="utf-8"))
            confidence = payload["measurement_confidence"]
            self.assertFalse(confidence["complete"])
            self.assertTrue(confidence["stale_input"])
            node = next(
                item["data"]
                for item in payload["graph"]["nodes"]
                if item["data"].get("id") == "lib"
            )
            self.assertIsNone(node["maintainability_risk"])
            self.assertIsNone(node["correctness_risk"])
            self.assertIsNone(node["locality_score"])
            self.assertIsNone(node["leverage_score"])
            self.assertEqual(payload["meta"]["summary"]["unknown_modules"], 1)

    def test_test_support_ignores_name_only_false_positives(self) -> None:
        with tempfile.TemporaryDirectory() as raw_temp:
            root = Path(raw_temp)
            (root / "src").mkdir()
            (root / "tests").mkdir()
            (root / "src" / "foo.rs").write_text(
                "pub fn answer() -> i32 { 42 }\n",
                encoding="utf-8",
            )
            (root / "tests" / "foo_tests.rs").write_text(
                "// Mentions foo by name, but does not depend on crate::foo.\n"
                "#[test]\nfn smoke() { assert!(true); }\n",
                encoding="utf-8",
            )
            config = self.write_temp_config(root)

            result = self.run_lens_with_config(root, config, "measure", "locality")

            self.assertEqual(result.returncode, 0, result.stderr)
            payload = json.loads((root / "target" / "analysis" / "locality_metrics.json").read_text(encoding="utf-8"))
            foo = next(item for item in payload if item["module_key"] == "foo")
            self.assertEqual(foo["external_test_refs"], 0)
            self.assertEqual(foo["test_locality"], "none")

    def test_test_support_counts_syntax_resolved_test_dependencies(self) -> None:
        with tempfile.TemporaryDirectory() as raw_temp:
            root = Path(raw_temp)
            (root / "src").mkdir()
            (root / "tests").mkdir()
            (root / "src" / "foo.rs").write_text(
                "pub fn answer() -> i32 { 42 }\n",
                encoding="utf-8",
            )
            (root / "tests" / "foo_tests.rs").write_text(
                "use crate::foo;\n#[test]\nfn smoke() { assert_eq!(foo::answer(), 42); }\n",
                encoding="utf-8",
            )
            config = self.write_temp_config(root)

            result = self.run_lens_with_config(root, config, "measure", "locality")

            self.assertEqual(result.returncode, 0, result.stderr)
            payload = json.loads((root / "target" / "analysis" / "locality_metrics.json").read_text(encoding="utf-8"))
            foo = next(item for item in payload if item["module_key"] == "foo")
            self.assertEqual(foo["external_test_refs"], 1)
            self.assertEqual(foo["test_locality"], "external")

    def run_lens_with_config(self, cwd: Path, config: Path, *args: str) -> subprocess.CompletedProcess[str]:
        env = os.environ.copy()
        env["PYTHONPATH"] = str(ROOT / "src")
        return subprocess.run(
            [
                sys.executable,
                "-m",
                "rust_quality_lens.cli",
                *args,
                "--config",
                str(config),
            ],
            cwd=cwd,
            env=env,
            text=True,
            capture_output=True,
        )

    def run_lens_for_fixture(self, fixture: Path, *args: str) -> subprocess.CompletedProcess[str]:
        return self.run_lens_with_config(fixture, fixture / "rqlens.toml", *args)

    def write_temp_config(self, root: Path) -> Path:
        config = root / "rqlens.toml"
        helper_manifest = (ROOT / "rust_helpers" / "Cargo.toml").as_posix()
        config.write_text(
            textwrap.dedent(
                f"""
                project_name = "syn-fixture"
                project_root = "."
                source_roots = ["src"]
                output_dir = "target/analysis"

                [rust]
                helper_manifest = "{helper_manifest}"
                """
            ).strip()
            + "\n",
            encoding="utf-8",
        )
        return config


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

    def test_correctness_statuses_attach_by_test_id_for_duplicate_names(self) -> None:
        from rust_quality_lens.tools import test_catalog

        tests = [
            {
                "id": "tests/duplicate_a.rs::duplicate_name",
                "name": "duplicate_name",
                "qualified_name": "duplicate_name",
                "path": "tests/duplicate_a.rs",
                "last_status": "unknown",
                "last_duration": None,
            },
            {
                "id": "tests/duplicate_b.rs::duplicate_name",
                "name": "duplicate_name",
                "qualified_name": "duplicate_name",
                "path": "tests/duplicate_b.rs",
                "last_status": "unknown",
                "last_duration": None,
            },
        ]
        statuses = test_catalog.parse_cargo_test_statuses(
            "\n".join(
                [
                    "     Running tests/duplicate_a.rs (target/debug/deps/duplicate_a-123)",
                    "test duplicate_name ... ok",
                    "     Running tests/duplicate_b.rs (target/debug/deps/duplicate_b-456)",
                    "test duplicate_name ... FAILED",
                ]
            ),
            returncode=101,
        )

        test_catalog.attach_statuses(tests, statuses)

        self.assertEqual(tests[0]["last_status"], "passed")
        self.assertEqual(tests[1]["last_status"], "failed")

    def test_clippy_suppression_scoring_is_deduplicated(self) -> None:
        from rust_quality_lens.tools.rust_escape_hatches import RustEscapeHatchAnalyzer

        counts = {
            "clippy_suppression": 1,
            "lint_suppression": 1,
            "unsafe_block": 1,
        }

        scoring_counts = RustEscapeHatchAnalyzer._scoring_counts(counts)

        self.assertEqual(scoring_counts["clippy_suppression"], 1)
        self.assertEqual(scoring_counts["lint_suppression"], 0)
        self.assertEqual(scoring_counts["unsafe_block"], 1)

    def test_risk_calibration_is_shared_across_map_and_tools(self) -> None:
        from rust_quality_lens.tools import risk_model
        from rust_quality_lens.tools.map import ArchitectureMapper
        from rust_quality_lens.tools.rust_escape_hatches import WEIGHTS

        values = {
            "complexity": 10.0,
            "sloc": 100.0,
            "public_api": 3,
            "outbound": 2,
            "inbound": 4,
            "churn": 60.0,
            "commit_count": 2,
            "contributors": 1,
            "defect_commits": 0,
            "has_correctness_tests": True,
            "failed_tests": 0,
            "unknown_tests": 1,
            "skipped_tests": 0,
            "perf_score": 0.0,
            "perf_mean_ms": 0.0,
            "perf_variance": 0.0,
            "layer_violations": 1,
            "cycle_member": False,
            "unknown_categories": set(),
        }

        self.assertEqual(ArchitectureMapper._risk_scores(values), risk_model.architecture_risk_scores(values))
        self.assertEqual(WEIGHTS, risk_model.tool_score_weights("escape_hatches"))
        self.assertIn("locality", risk_model.RISK_MODEL["tool_scores"])
        self.assertIn("leverage", risk_model.RISK_MODEL["tool_scores"])

    def test_correctness_statuses_attach_by_qualified_inline_name(self) -> None:
        from rust_quality_lens.tools import test_catalog

        tests = [
            {
                "id": "src/lib.rs::alpha::same_name",
                "name": "same_name",
                "qualified_name": "alpha::same_name",
                "path": "src/lib.rs",
                "last_status": "unknown",
                "last_duration": None,
            },
            {
                "id": "src/lib.rs::beta::same_name",
                "name": "same_name",
                "qualified_name": "beta::same_name",
                "path": "src/lib.rs",
                "last_status": "unknown",
                "last_duration": None,
            },
        ]
        statuses = test_catalog.parse_cargo_test_statuses(
            "\n".join(
                [
                    "     Running unittests src/lib.rs (target/debug/deps/example-123)",
                    "test alpha::same_name ... ok",
                    "test beta::same_name ... ignored",
                ]
            ),
            returncode=0,
        )

        test_catalog.attach_statuses(tests, statuses)

        self.assertEqual(tests[0]["last_status"], "passed")
        self.assertEqual(tests[1]["last_status"], "skipped")

    def test_correctness_uses_generic_layer_rules(self) -> None:
        from rust_quality_lens.tools import test_catalog

        self.assertEqual(test_catalog.layer_for_path(Path("tests/tab_tests.rs")), "Tests")
        self.assertEqual(test_catalog.layer_for_path(Path("src/domain/buffer.rs")), "Domain")
        self.assertEqual(test_catalog.layer_for_path(Path("src/search/index.rs")), "Unclassified")

    def test_map_uses_generic_layer_rules(self) -> None:
        from rust_quality_lens.tools.map import ArchitectureMapper

        mapper = ArchitectureMapper(project_name="generic", source_roots=("src",))

        self.assertEqual(mapper.layer_name("app::ui::panel"), "Interface")
        self.assertEqual(mapper.layer_name("domain::model"), "Domain")
        self.assertEqual(mapper.layer_name("search::index"), "Unclassified")

    def test_clone_measure_uses_catalog_promised_engines(self) -> None:
        from rust_quality_lens.cli import tool_argv
        from rust_quality_lens.config import LensConfig

        config = LensConfig(
            project_name="fixture",
            project_root=FIXTURE,
            source_roots=("src",),
            output_dir=FIXTURE / "target" / "analysis",
            helper_manifest=ROOT / "rust_helpers" / "Cargo.toml",
        )

        argv = tool_argv("clones", config.output_dir / "clones.json", config)

        self.assertIn("--engine", argv)
        self.assertEqual(argv[argv.index("--engine") + 1], "all")

    def test_correctness_measure_uses_configured_paths(self) -> None:
        from rust_quality_lens.cli import tool_argv
        from rust_quality_lens.config import LensConfig

        config = LensConfig(
            project_name="fixture",
            project_root=FIXTURE,
            source_roots=("crates/core/src", "integration"),
            output_dir=FIXTURE / "target" / "analysis",
            helper_manifest=ROOT / "rust_helpers" / "Cargo.toml",
        )

        argv = tool_argv("correctness", config.output_dir / "correctness_review.json", config)

        self.assertIn("--paths", argv)
        self.assertEqual(argv[argv.index("--paths") + 1 :], ["crates/core/src", "integration"])

if __name__ == "__main__":
    unittest.main()
