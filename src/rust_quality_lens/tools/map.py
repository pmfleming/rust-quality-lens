import argparse
import json
import re
import subprocess
import sys
from collections import defaultdict
from pathlib import Path
from typing import Any, DefaultDict, Dict, Iterable, List, Optional, Sequence, Set, Tuple

from report_modes import add_mode_argument, emit_report

HOTSPOT_CMD = [".venv/Scripts/python.exe", "scripts/hotspots.py"]
SLOWSPOT_CMD = [".venv/Scripts/python.exe", "scripts/slowspots.py", "--skip-bench"]
DEFAULT_OUTPUT = Path("map.json")
VISIBILITY_OUTPUT = Path("target/analysis/map.json")
CORRECTNESS_PATH = Path("target/analysis/correctness_review.json")

AREA_COLORS = {
    "chrome": "#569cd6",
    "domain": "#4ec9b0",
    "services": "#d7ba7d",
    "ui": "#c586c0",
    "default": "#808080",
}

LAYER_ORDER = {
    "chrome": 0,
    "ui": 1,
    "app_state": 2,
    "services": 2,
    "domain": 3,
    "default": 2,
}

DEFECT_KEYWORDS = ("fix", "bug", "regress", "panic", "crash", "issue", "fault")
RISK_CATEGORIES = (
    "maintainability",
    "change",
    "performance",
    "correctness",
    "architectural",
)


def group_id(mod_name: Optional[str]) -> Optional[str]:
    if mod_name is None:
        return None
    return f"group:{mod_name}"


class ArchitectureMapper:
    def __init__(self) -> None:
        self.dependencies: Dict[str, Set[str]] = {}
        self.reverse_dependencies: DefaultDict[str, Set[str]] = defaultdict(set)
        self.metrics: Dict[str, Dict] = {}
        self.performance: Dict[str, Dict] = {}
        self.file_to_mod: Dict[str, str] = {}
        self.mod_to_file: Dict[str, str] = {}
        self.module_paths: Set[str] = set()
        self.module_sources: Dict[str, str] = {}
        self.public_api_counts: Dict[str, int] = {}
        self.test_support: Dict[str, Dict[str, object]] = {}
        self.correctness: Dict[str, Dict[str, object]] = {}
        self.git_history: Dict[str, Dict[str, object]] = {}
        self.locality_metrics: Dict[str, Dict[str, object]] = {}
        self.leverage_metrics: Dict[str, Dict[str, object]] = {}
        self.cycle_members: Set[str] = set()
        self.risk_breakdown: Dict[str, Dict[str, object]] = {}

    def extract_dependencies(self, root_dir: str) -> None:
        root = Path(root_dir)
        self._discover_modules(root)
        for file_path, mod_name in self.file_to_mod.items():
            content = Path(file_path).read_text(encoding="utf-8")
            self.module_sources[mod_name] = content
            self.public_api_counts[mod_name] = self._count_public_api(content)
            self.dependencies.setdefault(mod_name, set())
            self.dependencies[mod_name].update(self._extract_use_dependencies(content))
            self.dependencies[mod_name].update(
                self._extract_child_modules(content, mod_name)
            )

        for source, targets in self.dependencies.items():
            for target in targets:
                self.reverse_dependencies[target].add(source)

        self.cycle_members = self._find_cycle_members()

    def _discover_modules(self, root_dir: Path) -> None:
        for file_path in root_dir.rglob("*.rs"):
            rel_path = file_path.relative_to(root_dir)
            mod_name = rel_path.as_posix().replace("/", "::").replace(".rs", "")
            if mod_name.endswith("::mod"):
                mod_name = mod_name[:-5]

            normalized_path = str(file_path.resolve())
            self.file_to_mod[normalized_path] = mod_name
            self.mod_to_file[mod_name] = normalized_path
            self.module_paths.add(mod_name)
            self.dependencies.setdefault(mod_name, set())

    def _extract_use_dependencies(self, content: str) -> Set[str]:
        dependencies: Set[str] = set()
        use_statements = re.findall(r"^\s*use\s+crate::([^;]+);", content, re.MULTILINE)
        for raw_use in use_statements:
            dependency = self._normalize_use_dependency(raw_use.strip())
            if dependency and dependency in self.module_paths:
                dependencies.add(dependency)
        return dependencies

    def _normalize_use_dependency(self, raw_use: str) -> Optional[str]:
        candidate = raw_use.split(" as ")[0].strip()
        if "::{" in candidate:
            candidate = candidate.split("::{", 1)[0]
        elif "{" in candidate:
            candidate = candidate.split("{", 1)[0].rstrip(":")

        candidate = candidate.split(",")[0].strip().rstrip(":")
        if not candidate:
            return None
        return self._resolve_module_prefix(candidate)

    def _resolve_module_prefix(self, candidate: str) -> Optional[str]:
        parts = candidate.split("::")
        for length in range(len(parts), 0, -1):
            prefix = "::".join(parts[:length])
            if prefix in self.module_paths:
                return prefix
        return None

    def _extract_child_modules(self, content: str, mod_name: str) -> Set[str]:
        children = set()
        declared_mods = re.findall(
            r"^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+([a-zA-Z0-9_]+)\s*;",
            content,
            re.MULTILINE,
        )
        for child in declared_mods:
            child_mod = f"{mod_name}::{child}"
            if child_mod in self.module_paths:
                children.add(child_mod)
        return children

    def _count_public_api(self, content: str) -> int:
        patterns = [
            r"^\s*pub(?:\([^)]*\))?\s+fn\s+",
            r"^\s*pub(?:\([^)]*\))?\s+struct\s+",
            r"^\s*pub(?:\([^)]*\))?\s+enum\s+",
            r"^\s*pub(?:\([^)]*\))?\s+trait\s+",
            r"^\s*pub(?:\([^)]*\))?\s+mod\s+",
            r"^\s*pub(?:\([^)]*\))?\s+(?:const|static|type)\s+",
        ]
        return sum(
            len(re.findall(pattern, content, re.MULTILINE)) for pattern in patterns
        )

    def gather_metrics(self) -> None:
        from hotspots import HotspotAnalyzer
        from dataclasses import asdict
        try:
            analyzer = HotspotAnalyzer(top=None, scope="all", include_anonymous=False)
            results = analyzer.run(["src"])
            for metric in results:
                item = asdict(metric)
                mod_name = self._metric_module_name(item["name"])
                if mod_name:
                    self.metrics[mod_name] = item
        except Exception as exc:
            print(f"Warning: Could not gather complexity metrics: {exc}", file=sys.stderr)

    def _metric_module_name(self, metric_name: str) -> Optional[str]:
        normalized_name = str(Path(metric_name).resolve())
        if normalized_name in self.file_to_mod:
            return self.file_to_mod[normalized_name]

        metric_path = Path(metric_name)
        try:
            rel_path = metric_path.relative_to("src")
        except ValueError:
            return None

        mod_name = rel_path.as_posix().replace("/", "::").replace(".rs", "")
        if mod_name.endswith("::mod"):
            mod_name = mod_name[:-5]
        if mod_name == "lib":
            return "scratchpad"
        return mod_name

    def gather_performance(self) -> None:
        try:
            result = subprocess.run(
                SLOWSPOT_CMD, capture_output=True, text=True, check=True
            )
            for item in json.loads(result.stdout):
                for mod_name in item.get("targets", []):
                    perf_entry = self.performance.setdefault(
                        mod_name,
                        {
                            "score": 0.0,
                            "mean_ms": 0.0,
                            "variance": 0.0,
                            "items": [],
                        },
                    )
                    perf_score = self._benchmark_score(item)
                    perf_entry["score"] = max(perf_entry["score"], perf_score)
                    perf_entry["mean_ms"] = max(
                        perf_entry["mean_ms"], float(item["mean_ns"]) / 1_000_000.0
                    )
                    perf_entry["variance"] = max(
                        perf_entry["variance"], self._benchmark_variance(item)
                    )
                    perf_entry["items"].append(item)
        except Exception as exc:
            print(f"Warning: Could not gather performance metrics: {exc}", file=sys.stderr)

    def _benchmark_score(self, item: Dict) -> float:
        return float(item["mean_ns"]) / 100_000.0

    def _benchmark_variance(self, item: Dict) -> float:
        mean_ns = float(item.get("mean_ns", 0.0))
        if mean_ns <= 0:
            return 0.0
        return float(item.get("std_dev_ns", 0.0)) / mean_ns

    def gather_test_support(self) -> None:
        test_files = list(Path("tests").rglob("*.rs")) if Path("tests").exists() else []
        test_contents = []
        for path in test_files:
            try:
                test_contents.append((str(path), path.read_text(encoding="utf-8")))
            except OSError:
                continue

        for mod_name, file_path in self.mod_to_file.items():
            source = self.module_sources.get(mod_name, "")
            stem = Path(file_path).stem
            path_hint = mod_name.replace("::", "_")
            has_inline_tests = "#[cfg(test)]" in source or "mod tests" in source
            references: List[str] = []

            for test_path, content in test_contents:
                if (
                    mod_name in content
                    or stem in content
                    or path_hint in Path(test_path).stem
                    or stem in Path(test_path).stem
                ):
                    references.append(test_path)

            self.test_support[mod_name] = {
                "has_inline_tests": has_inline_tests,
                "external_refs": sorted(set(references)),
                "coverage_hint": has_inline_tests or bool(references),
            }

    def gather_correctness(self) -> None:
        if not CORRECTNESS_PATH.exists():
            return
        try:
            payload = json.loads(CORRECTNESS_PATH.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            return

        tests = payload.get("tests", []) if isinstance(payload, dict) else []
        for item in tests:
            matched = self._correctness_module_for_item(item)
            if matched is None:
                continue
            self._record_correctness_item(matched, item)

    def _correctness_module_for_item(self, item: Dict[str, object]) -> Optional[str]:
        module = str(item.get("module", ""))
        if not module:
            return None
        for candidate in (module, module.replace("/", "::"), module.replace("\\", "::")):
            if candidate in self.module_paths:
                return candidate
        return self._match_test_to_module(str(item.get("path", "")), module)

    def _record_correctness_item(self, matched: str, item: Dict[str, object]) -> None:
        entry = self.correctness.setdefault(
            matched,
            {
                "test_count": 0,
                "failed_tests": 0,
                "unknown_tests": 0,
                "skipped_tests": 0,
                "tests": [],
            },
        )
        entry["test_count"] = int(entry["test_count"]) + 1
        status = str(item.get("last_status", "unknown"))
        if status in {"failed", "unknown", "skipped"}:
            entry[f"{status}_tests"] = int(entry[f"{status}_tests"]) + 1
        tests_list = entry["tests"]
        if isinstance(tests_list, list):
            tests_list.append(
                {
                    "id": item.get("id"),
                    "name": item.get("name"),
                    "path": item.get("path"),
                    "line": item.get("line"),
                    "status": status,
                    "description": item.get("description"),
                }
            )

    def _match_test_to_module(self, path_text: str, module: str) -> Optional[str]:
        stem = Path(path_text).stem
        hints = [stem, module.split("::")[-1], module.split("/")[-1]]
        for mod_name in self.module_paths:
            tail = mod_name.split("::")[-1]
            if tail in hints or any(hint and hint in mod_name for hint in hints):
                return mod_name
        return None

    @staticmethod
    def _empty_git_record() -> Dict[str, object]:
        return {
            "commits": 0,
            "churn": 0,
            "contributors": set(),
            "defect_commits": 0,
            "cochange_commits": 0,
            "cochange_total": 0,
            "cochanged_modules": set(),
        }

    @staticmethod
    def _record_cochanges(
        records: DefaultDict[str, Dict[str, object]],
        current_modules: Set[str],
    ) -> None:
        if not current_modules:
            return
        peer_count = max(0, len(current_modules) - 1)
        for mod_name in current_modules:
            record = records[mod_name]
            record["cochange_commits"] = int(record["cochange_commits"]) + 1
            record["cochange_total"] = int(record["cochange_total"]) + peer_count
            cast_set = record["cochanged_modules"]
            assert isinstance(cast_set, set)
            cast_set.update(current_modules - {mod_name})

    def _git_module_for_numstat(self, raw_line: str) -> Optional[Tuple[str, int]]:
        parts = raw_line.split("\t")
        if len(parts) != 3:
            return None
        added_text, deleted_text, path_text = parts
        if not path_text.endswith(".rs"):
            return None
        mod_name = self.file_to_mod.get(str(Path(path_text).resolve()))
        if mod_name is None:
            return None
        added = int(added_text) if added_text.isdigit() else 0
        deleted = int(deleted_text) if deleted_text.isdigit() else 0
        return mod_name, added + deleted

    @staticmethod
    def _record_git_touch(
        record: Dict[str, object],
        *,
        churn: int,
        author: str,
        subject: str,
    ) -> None:
        record["commits"] = int(record["commits"]) + 1
        record["churn"] = int(record["churn"]) + churn
        contributors = record["contributors"]
        assert isinstance(contributors, set)
        contributors.add(author)
        if any(keyword in subject for keyword in DEFECT_KEYWORDS):
            record["defect_commits"] = int(record["defect_commits"]) + 1

    @staticmethod
    def _finalize_git_record(record: Dict[str, object]) -> Dict[str, object]:
        contributors = (
            sorted(record["contributors"]) if isinstance(record["contributors"], set) else []
        )
        cochanged_modules = (
            sorted(record["cochanged_modules"])
            if isinstance(record["cochanged_modules"], set)
            else []
        )
        cochange_commits = int(record["cochange_commits"])
        return {
            "commits": int(record["commits"]),
            "churn": int(record["churn"]),
            "contributors": contributors,
            "contributor_count": len(contributors),
            "defect_commits": int(record["defect_commits"]),
            "cochange_commits": cochange_commits,
            "cochange_total": int(record["cochange_total"]),
            "avg_cochanged_modules": (
                float(record["cochange_total"]) / cochange_commits
                if cochange_commits
                else 0.0
            ),
            "cochanged_modules": cochanged_modules,
            "cochanged_module_count": len(cochanged_modules),
        }

    def gather_git_history(self) -> None:
        cmd = [
            "git",
            "log",
            "--format=commit%x09%H%x09%an%x09%s",
            "--numstat",
            "--",
            "src",
            "tests",
        ]
        try:
            result = subprocess.run(
                cmd, capture_output=True, text=True, check=True
            )
        except Exception as exc:
            print(f"Warning: Could not gather git history: {exc}", file=sys.stderr)
            return

        records: DefaultDict[str, Dict[str, object]] = defaultdict(self._empty_git_record)

        current_author = ""
        current_subject = ""
        current_modules: Set[str] = set()

        for raw_line in result.stdout.splitlines():
            if raw_line.startswith("commit\t"):
                self._record_cochanges(records, current_modules)
                current_modules = set()
                parts = raw_line.split("\t", 3)
                current_author = parts[2] if len(parts) > 2 else ""
                current_subject = parts[3].lower() if len(parts) > 3 else ""
                continue

            if not raw_line.strip():
                continue

            parsed = self._git_module_for_numstat(raw_line)
            if parsed is None:
                continue

            mod_name, churn = parsed
            current_modules.add(mod_name)
            self._record_git_touch(
                records[mod_name],
                churn=churn,
                author=current_author,
                subject=current_subject,
            )

        self._record_cochanges(records, current_modules)

        for mod_name in self.module_paths:
            self.git_history[mod_name] = self._finalize_git_record(
                records.get(mod_name, self._empty_git_record())
            )

    def gather_locality_leverage_metrics(self) -> None:
        self.locality_metrics = self._load_module_metric_artifact(
            Path("target/analysis/locality_metrics.json")
        )
        self.leverage_metrics = self._load_module_metric_artifact(
            Path("target/analysis/leverage_metrics.json")
        )

    def _load_module_metric_artifact(self, path: Path) -> Dict[str, Dict[str, object]]:
        if not path.exists():
            return {}
        try:
            payload = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            return {}
        rows = payload if isinstance(payload, list) else payload.get("items", [])
        metrics: Dict[str, Dict[str, object]] = {}
        if not isinstance(rows, list):
            return metrics
        for item in rows:
            if not isinstance(item, dict):
                continue
            key = str(item.get("module_key") or item.get("module_name") or "")
            if key:
                metrics[key] = item
        return metrics

    def _find_cycle_members(self) -> Set[str]:
        visited: Set[str] = set()
        stack: List[str] = []
        on_stack: Set[str] = set()
        cycle_members: Set[str] = set()

        def dfs(node: str) -> None:
            visited.add(node)
            stack.append(node)
            on_stack.add(node)

            for neighbor in sorted(self.dependencies.get(node, set())):
                if neighbor not in visited:
                    dfs(neighbor)
                elif neighbor in on_stack:
                    try:
                        start = stack.index(neighbor)
                    except ValueError:
                        start = 0
                    cycle_members.update(stack[start:])

            stack.pop()
            on_stack.remove(node)

        for mod_name in sorted(self.module_paths):
            if mod_name not in visited:
                dfs(mod_name)
        return cycle_members

    def layer_name(self, mod_name: str) -> str:
        parts = mod_name.split("::")
        if len(parts) > 1 and parts[0] == "app":
            return parts[1]
        return "default"

    def _count_layer_violations(self, mod_name: str) -> int:
        source_layer = self.layer_name(mod_name)
        source_rank = LAYER_ORDER.get(source_layer, LAYER_ORDER["default"])
        violations = 0
        for dependency in self.dependencies.get(mod_name, set()):
            target_layer = self.layer_name(dependency)
            target_rank = LAYER_ORDER.get(target_layer, LAYER_ORDER["default"])
            if source_rank < target_rank:
                violations += 1
        return violations

    def _dependency_density(self, mod_name: str) -> Tuple[int, int]:
        outbound = len(self.dependencies.get(mod_name, set()))
        inbound = len(self.reverse_dependencies.get(mod_name, set()))
        return outbound, inbound

    def _risk_inputs(self, mod_name: str) -> Dict[str, Any]:
        metric = self.metrics.get(mod_name, {})
        perf = self.performance.get(mod_name, {})
        git = self.git_history.get(mod_name, {})
        tests = self.test_support.get(mod_name, {})
        correctness = self.correctness.get(mod_name, {})
        outbound, inbound = self._dependency_density(mod_name)
        test_count = int(correctness.get("test_count", 0))
        return {
            "metric": metric,
            "perf": perf,
            "git": git,
            "tests": tests,
            "correctness": correctness,
            "outbound": outbound,
            "inbound": inbound,
            "public_api": self.public_api_counts.get(mod_name, 0),
            "sloc": float(metric.get("sloc", 0.0)),
            "complexity": float(metric.get("score", 0.0)),
            "churn": float(git.get("churn", 0)),
            "contributors": int(git.get("contributor_count", 0)),
            "defect_commits": int(git.get("defect_commits", 0)),
            "commit_count": int(git.get("commits", 0)),
            "test_count": test_count,
            "failed_tests": int(correctness.get("failed_tests", 0)),
            "unknown_tests": int(correctness.get("unknown_tests", 0)),
            "skipped_tests": int(correctness.get("skipped_tests", 0)),
            "has_correctness_tests": bool(tests.get("coverage_hint", False)) or test_count > 0,
            "perf_score": float(perf.get("score", 0.0)),
            "perf_mean_ms": float(perf.get("mean_ms", 0.0)),
            "perf_variance": float(perf.get("variance", 0.0)),
            "layer_violations": self._count_layer_violations(mod_name),
            "cycle_member": mod_name in self.cycle_members,
        }

    @staticmethod
    def _risk_scores(values: Dict[str, Any]) -> Dict[str, float]:
        maintainability = round(
            values["complexity"]
            + min(70.0, values["sloc"] * 0.12)
            + min(30.0, values["public_api"] * 2.5)
            + min(35.0, values["outbound"] * 4.0 + values["inbound"] * 1.0),
            2,
        )
        change = round(
            min(160.0, values["churn"] / 12.0)
            + min(100.0, values["commit_count"] * 2.5)
            + min(80.0, values["contributors"] * 14.0)
            + min(90.0, values["defect_commits"] * 18.0)
            + (90.0 if not values["has_correctness_tests"] else 0.0),
            2,
        )
        correctness = round(
            (140.0 if values["failed_tests"] else 0.0)
            + min(120.0, values["failed_tests"] * 45.0)
            + min(80.0, values["unknown_tests"] * 4.0)
            + min(40.0, values["skipped_tests"] * 10.0)
            + (90.0 if not values["has_correctness_tests"] else 0.0),
            2,
        )
        performance = round(
            values["perf_score"]
            + min(120.0, values["perf_mean_ms"] * 2.5)
            + min(90.0, values["perf_variance"] * 180.0),
            2,
        )
        architectural = round(
            min(120.0, values["outbound"] * 10.0)
            + min(120.0, values["inbound"] * 8.0)
            + min(120.0, values["layer_violations"] * 32.0)
            + (110.0 if values["cycle_member"] else 0.0)
            + (60.0 if values["sloc"] >= 250 else 0.0),
            2,
        )
        return {
            "maintainability_risk": maintainability,
            "change_risk": change,
            "performance_risk": performance,
            "correctness_risk": correctness,
            "quality_risk": maintainability,
            "architectural_risk": architectural,
            "total_score": round(
                maintainability + change + performance + architectural + correctness, 2
            ),
        }

    @staticmethod
    def _add_signal(
        signals: Dict[str, List[str]],
        category: str,
        condition: bool,
        message: str,
    ) -> None:
        if condition:
            signals[category].append(message)

    def _risk_signals(self, values: Dict[str, Any]) -> Dict[str, List[str]]:
        signals: Dict[str, List[str]] = {category: [] for category in RISK_CATEGORIES}
        self._add_signal(
            signals,
            "maintainability",
            values["complexity"] >= 300,
            f"high internal complexity {values['complexity']:.0f}",
        )
        self._add_signal(
            signals,
            "maintainability",
            values["sloc"] >= 150,
            f"large module {int(values['sloc'])} sloc",
        )
        self._add_signal(
            signals,
            "maintainability",
            values["public_api"] >= 10,
            f"broad interface {values['public_api']} public items",
        )
        self._add_signal(
            signals,
            "maintainability",
            values["outbound"] >= 10 or values["inbound"] >= 20,
            f"high coupling in={values['inbound']} out={values['outbound']}",
        )
        self._add_signal(signals, "change", not values["has_correctness_tests"], "low test evidence")
        self._add_signal(signals, "change", values["churn"] >= 200, f"high churn {int(values['churn'])} lines")
        self._add_signal(signals, "change", values["contributors"] >= 3, f"many contributors {values['contributors']}")
        self._add_signal(signals, "change", values["defect_commits"] >= 1, f"defect history {values['defect_commits']} fix commits")
        self._add_signal(signals, "performance", values["perf_mean_ms"] > 0, f"runtime cost {values['perf_mean_ms']:.2f} ms")
        self._add_signal(signals, "performance", values["perf_variance"] >= 0.15, f"instability variance {values['perf_variance']:.2f}")
        self._add_signal(signals, "performance", not values["perf"].get("items"), "no benchmark mapping")
        self._add_signal(signals, "correctness", bool(values["failed_tests"]), f"failing tests {values['failed_tests']}")
        self._add_signal(signals, "correctness", bool(values["unknown_tests"]), f"unknown tests {values['unknown_tests']}")
        self._add_signal(signals, "correctness", bool(values["skipped_tests"]), f"skipped tests {values['skipped_tests']}")
        self._add_signal(signals, "correctness", not values["has_correctness_tests"], "no direct tests")
        self._add_signal(signals, "architectural", values["layer_violations"] >= 1, f"layer violations {values['layer_violations']}")
        self._add_signal(signals, "architectural", values["cycle_member"], "circular dependency")
        self._add_signal(signals, "architectural", values["inbound"] >= 6, f"oversized hub inbound {values['inbound']}")
        self._add_signal(signals, "architectural", values["sloc"] >= 250, "oversized module")
        return {key: value or ["stable"] for key, value in signals.items()}

    @staticmethod
    def _risk_evidence(values: Dict[str, Any]) -> Dict[str, Any]:
        return {
            "complexity_score": values["complexity"],
            "sloc": int(values["sloc"]),
            "public_api_count": values["public_api"],
            "outbound_dependencies": values["outbound"],
            "inbound_dependencies": values["inbound"],
            "commit_count": values["commit_count"],
            "churn": int(values["churn"]),
            "contributors": values["git"].get("contributors", []),
            "contributor_count": values["contributors"],
            "defect_commits": values["defect_commits"],
            "has_tests": values["has_correctness_tests"],
            "test_refs": values["tests"].get("external_refs", []),
            "test_count": values["test_count"],
            "failed_tests": values["failed_tests"],
            "unknown_tests": values["unknown_tests"],
            "skipped_tests": values["skipped_tests"],
            "correctness_tests": values["correctness"].get("tests", []),
            "layer_violations": values["layer_violations"],
            "cycle_member": values["cycle_member"],
            "perf_mean_ms": values["perf_mean_ms"],
            "perf_variance": values["perf_variance"],
        }

    def compute_risks(self) -> None:
        for mod_name in sorted(self.module_paths):
            values = self._risk_inputs(mod_name)
            self.risk_breakdown[mod_name] = {
                **self._risk_scores(values),
                "signals": self._risk_signals(values),
                "evidence": self._risk_evidence(values),
            }

    def risk_color(self, score: float) -> str:
        if score >= 700:
            return "#f44747"
        if score >= 350:
            return "#d7ba7d"
        return "#b5cea8"

    def get_group_style(self, mod_name: str) -> Dict[str, str]:
        parts = mod_name.split("::")
        area = parts[1] if len(parts) > 1 and parts[0] == "app" else "default"
        base_color = AREA_COLORS.get(area, AREA_COLORS["default"])
        opacity = max(0.1, 0.4 - (len(parts) * 0.08))
        return {"color": base_color, "opacity": opacity}

    def _graph_groups(self) -> Set[str]:
        groups: Set[str] = set()
        for mod_name in self.dependencies:
            parts = mod_name.split("::")
            for depth in range(1, len(parts)):
                groups.add("::".join(parts[:depth]))
        return groups

    def _group_node(self, group: str) -> Dict[str, Dict[str, object]]:
        style = self.get_group_style(group)
        return {
            "data": {
                "id": group_id(group),
                "module": group,
                "label": group.split("::")[-1],
                "parent": group_id("::".join(group.split("::")[:-1]) or None),
                "is_group": True,
                "bg_color": style["color"],
                "bg_opacity": style["opacity"],
            }
        }

    @staticmethod
    def _perf_benchmark_rows(perf_items: List[Dict]) -> List[Dict[str, object]]:
        return [
            {
                "name": item["name"],
                "mean_ms": float(item["mean_ns"]) / 1_000_000.0,
                "dispersion_ms": (
                    float(item["dispersion_ns"]) / 1_000_000.0
                    if item.get("dispersion_ns") is not None
                    else None
                ),
                "dispersion_label": item.get("dispersion_label", "median_abs_dev"),
                "kind": item.get("benchmark_kind", "unmapped"),
                "threshold_ms": item.get("threshold_ms", 50.0),
                "signals": item.get("signals", "nominal"),
            }
            for item in perf_items
        ]

    @staticmethod
    def _flat_signals(category_signals: Dict[str, List[str]]) -> List[str]:
        return list(
            dict.fromkeys(
                signal
                for values in category_signals.values()
                for signal in values
            )
        )

    def _module_node(self, mod_name: str) -> Dict[str, Dict[str, object]]:
        perf_data = self.performance.get(mod_name, {})
        perf_items = perf_data.get("items", [])
        metric = self.metrics.get(mod_name, {})
        risk = self.risk_breakdown.get(mod_name, {})
        evidence = risk.get("evidence", {})
        category_signals = risk.get("signals", {})
        locality = self.locality_metrics.get(mod_name, {})
        leverage = self.leverage_metrics.get(mod_name, {})
        leverage_score = float(
            leverage.get("leverage_score", leverage.get("total_leverage_score", 0.0))
        )
        return {
            "data": {
                "id": mod_name,
                "layer": self.layer_name(mod_name),
                "churn": int(evidence.get("churn", 0)),
                "label": mod_name.split("::")[-1],
                "parent": group_id("::".join(mod_name.split("::")[:-1]) or None),
                "comp_score": float(metric.get("score", 0.0)),
                "perf_score": float(perf_data.get("score", 0.0)),
                "quality_risk": float(
                    risk.get("quality_risk", risk.get("maintainability_risk", 0.0))
                ),
                "maintainability_risk": float(risk.get("maintainability_risk", 0.0)),
                "correctness_risk": float(risk.get("correctness_risk", 0.0)),
                "change_risk": float(risk.get("change_risk", 0.0)),
                "performance_risk": float(risk.get("performance_risk", 0.0)),
                "architectural_risk": float(risk.get("architectural_risk", 0.0)),
                "locality_score": float(locality.get("locality_score", 0.0)),
                "locality_risk": float(
                    locality.get("non_locality_risk", locality.get("locality_risk", 0.0))
                ),
                "non_locality_risk": float(
                    locality.get("non_locality_risk", locality.get("locality_risk", 0.0))
                ),
                "leverage_score": leverage_score,
                "leverage_risk": float(
                    leverage.get("leverage_risk", 100.0 - leverage_score)
                ),
                "total_score": float(risk.get("total_score", 0.0)),
                "sloc": int(metric.get("sloc", 0)),
                "signals": self._flat_signals(category_signals),
                "category_signals": category_signals,
                "risk_colors": self.risk_colors(risk),
                "evidence": evidence,
                "locality_metrics": locality,
                "leverage_metrics": leverage,
                "is_slow": bool(perf_items),
                "perf_benchmarks": self._perf_benchmark_rows(perf_items),
                "perf_kind": ", ".join(
                    sorted({item.get("benchmark_kind", "unmapped") for item in perf_items})
                ),
            }
        }

    def build_graph_payload(self) -> Dict[str, List[Dict]]:
        nodes = [self._group_node(group) for group in sorted(self._graph_groups())]
        nodes.extend(self._module_node(mod_name) for mod_name in sorted(self.dependencies))
        edges = [
            {"data": {"source": source, "target": target}}
            for source, targets in sorted(self.dependencies.items())
            for target in sorted(targets)
            if source != target
        ]
        return {"nodes": nodes, "edges": edges}

    def risk_colors(self, risk: Dict[str, object]) -> Dict[str, str]:
        return {
            category: self.risk_color(float(risk.get(f"{category}_risk", 0.0)))
            for category in RISK_CATEGORIES
        }

    def meta_summary(self) -> Dict[str, object]:
        measured_modules = len(self.risk_breakdown)
        good = warn = bad = 0
        for item in self.risk_breakdown.values():
            score = float(item.get("total_score") or item.get("total_risk") or 0)
            if score >= 600:
                bad += 1
            elif score >= 300:
                warn += 1
            else:
                good += 1
        category_totals = {
            category: round(
                sum(item[f"{category}_risk"] for item in self.risk_breakdown.values()),
                2,
            )
            for category in RISK_CATEGORIES
        }
        return {
            "measured_modules": measured_modules,
            "cycle_members": len(self.cycle_members),
            "modules_without_test_evidence": sum(
                1
                for item in self.risk_breakdown.values()
                if not bool(item["evidence"]["has_tests"])
            ),
            "category_totals": category_totals,
            "good": good,
            "warn": warn,
            "bad": bad,
        }

    def viewer_payload(self) -> Dict:
        graph = self.build_graph_payload()
        return {
            "meta": {
                "title": "Scratchpad Architecture Risk Map",
                "generated_from": "scripts/map.py",
                "source_root": "src",
                "node_count": len(graph["nodes"]),
                "edge_count": len(graph["edges"]),
                "risk_model": list(RISK_CATEGORIES),
                "summary": self.meta_summary(),
            },
            "graph": graph,
        }


def refresh_analysis_inputs() -> None:
    commands = [
        HOTSPOT_CMD + ["--mode", "analysis", "--paths", "src"],
        SLOWSPOT_CMD + ["--mode", "analysis"],
    ]
    for command in commands:
        subprocess.run(command, check=True, capture_output=True, text=True)


def render_cli(payload: object) -> str:
    data = payload if isinstance(payload, dict) else {}
    nodes = data.get("graph", {}).get("nodes", [])
    modules = [
        node.get("data", {})
        for node in nodes
        if not node.get("data", {}).get("is_group")
    ]
    top = sorted(modules, key=lambda item: -float(item.get("total_score", 0.0)))[:10]
    lines = ["Architecture Risk Map"]
    for index, item in enumerate(top, start=1):
        lines.append(
            f"{index:>2}. {item.get('id', '<unknown>')} | total={float(item.get('total_score', 0.0)):.2f} | maintainability={float(item.get('maintainability_risk', 0.0)):.2f} | change={float(item.get('change_risk', 0.0)):.2f} | architectural={float(item.get('architectural_risk', 0.0)):.2f}"
        )
    if not top:
        lines.append("No modules found.")
    return "\n".join(lines)


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Emit architecture dependency and risk map data as JSON"
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=None,
        help=f"Optional output JSON path. Example: {DEFAULT_OUTPUT}",
    )
    parser.add_argument(
        "--refresh",
        action="store_true",
        help="Regenerate hotspot and slowspot inputs before building the map.",
    )
    add_mode_argument(parser)
    args = parser.parse_args()

    if args.refresh:
        refresh_analysis_inputs()

    mapper = ArchitectureMapper()
    mapper.extract_dependencies("src")
    mapper.gather_metrics()
    mapper.gather_performance()
    mapper.gather_test_support()
    mapper.gather_correctness()
    mapper.gather_git_history()
    mapper.gather_locality_leverage_metrics()
    mapper.compute_risks()

    payload = mapper.viewer_payload()
    emit_report(
        payload,
        mode=args.mode,
        output_path=args.output,
        visibility_path=VISIBILITY_OUTPUT,
        cli_renderer=render_cli,
        label="map",
    )


if __name__ == "__main__":
    main()
