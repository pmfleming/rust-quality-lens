import argparse
import json
import os
import re
import subprocess
import sys
from collections import defaultdict
from pathlib import Path
from typing import Any, DefaultDict, Dict, Iterable, List, Optional, Sequence, Set, Tuple

from common import (
    confidence_for_artifacts,
    measurement_confidence,
    merge_confidences,
    rust_facts_for_paths,
    source_confidence,
)
from lens_rules import (
    RULESET_ID,
    RULESET_VERSION,
    classify_module,
    layer_color,
    layer_rank,
)
from report_modes import add_mode_argument, analysis_path, emit_report
from risk_model import RISK_MODEL, architecture_risk_scores, model_classification, model_id, model_version, model_weights

HOTSPOT_CMD = [sys.executable, str(Path(__file__).with_name("hotspots.py"))]
DEFAULT_OUTPUT = Path("map.json")
VISIBILITY_OUTPUT = Path("target/analysis/map.json")
CORRECTNESS_PATH = Path("target/analysis/correctness_review.json")
HOTSPOTS_PATH = Path("target/analysis/hotspots.json")
SLOWSPOTS_PATH = Path("target/analysis/slowspots.json")

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
    def __init__(self, *, project_name: str, source_roots: Sequence[str]) -> None:
        self.project_name = project_name
        self.source_roots = tuple(source_roots)
        self.dependencies: Dict[str, Set[str]] = {}
        self.external_dependencies: Dict[str, Set[str]] = {}
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
        self.rust_facts: Dict[str, Dict[str, object]] = {}
        self.confidence_inputs: List[Dict[str, object]] = []
        self.artifact_status: Dict[str, Dict[str, object]] = {}
        self.cycle_members: Set[str] = set()
        self.risk_breakdown: Dict[str, Dict[str, object]] = {}

    def extract_dependencies(self) -> None:
        for source_root in self.source_roots:
            root = Path(source_root)
            if root.exists():
                self._discover_modules(root)
        facts = rust_facts_for_paths(self.source_roots)
        self.confidence_inputs.append(source_confidence(self.source_roots, facts=facts))
        unsupported = [
            str(pattern)
            for fact in facts
            if isinstance(fact, dict)
            for pattern in fact.get("unsupported_patterns", [])
        ]
        if unsupported:
            self.confidence_inputs.append(
                measurement_confidence(unsupported_pattern=unsupported)
            )
        self._apply_module_file_overrides(facts)
        self.rust_facts = {
            str(fact.get("module_key", "")): fact
            for fact in facts
            if isinstance(fact, dict) and fact.get("parse_status") == "ok"
        }
        for file_path, mod_name in self.file_to_mod.items():
            content = Path(file_path).read_text(encoding="utf-8")
            self.module_sources[mod_name] = content
            fact = self.rust_facts.get(mod_name, {})
            self.public_api_counts[mod_name] = int(
                fact.get("public_api_count", self._count_public_api(content))
            )
            self.dependencies.setdefault(mod_name, set())
            self.external_dependencies.setdefault(mod_name, set())
            self.dependencies[mod_name].update(self._extract_fact_dependencies(mod_name, fact))
            self.dependencies[mod_name].update(
                self._extract_child_modules(fact, mod_name)
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
            self.external_dependencies.setdefault(mod_name, set())

    def _apply_module_file_overrides(self, facts: Sequence[Dict[str, object]]) -> None:
        for fact in facts:
            if not isinstance(fact, dict):
                continue
            parent_module = str(fact.get("module_key", ""))
            for module_file in fact.get("module_files", []):
                if not isinstance(module_file, dict):
                    continue
                module_key = self._normalize_declared_module_key(
                    str(module_file.get("module_key", "")),
                    parent_module,
                )
                raw_path = str(module_file.get("path", ""))
                if not module_key or not raw_path:
                    continue
                file_path = str(Path(raw_path).resolve())
                old_module = self.file_to_mod.get(file_path)
                if old_module and old_module != module_key:
                    self.module_paths.discard(old_module)
                    self.mod_to_file.pop(old_module, None)
                    self.dependencies.pop(old_module, None)
                    self.external_dependencies.pop(old_module, None)
                self.file_to_mod[file_path] = module_key
                self.mod_to_file[module_key] = file_path
                self.module_paths.add(module_key)
                self.dependencies.setdefault(module_key, set())
                self.external_dependencies.setdefault(module_key, set())

    def _normalize_declared_module_key(self, raw_key: str, parent_module: str) -> str:
        raw_key = raw_key.strip()
        if not raw_key:
            return ""
        parts = [part for part in raw_key.split("::") if part]
        if parts and parts[0] in {"lib", "main"}:
            parts = parts[1:]
        if parts:
            return "::".join(parts)
        if parent_module in {"lib", "main"}:
            return raw_key
        return f"{parent_module}::{raw_key}"

    def _extract_fact_dependencies(self, mod_name: str, fact: Dict[str, object]) -> Set[str]:
        dependencies: Set[str] = set()
        for raw in fact.get("dependencies", []) if isinstance(fact, dict) else []:
            raw_dependency = str(raw)
            dependency = self._resolve_dependency(raw_dependency, mod_name)
            if dependency and dependency != mod_name:
                dependencies.add(dependency)
                continue
            external_dependency = self._external_dependency_name(raw_dependency, mod_name)
            if external_dependency:
                self.external_dependencies.setdefault(mod_name, set()).add(external_dependency)
        return dependencies

    def _resolve_dependency(self, raw_dependency: str, source_module: str) -> Optional[str]:
        raw = raw_dependency.strip().rstrip(":")
        if raw.endswith("::*"):
            raw = raw[:-3]
        if not raw:
            return None

        parts = [part for part in raw.split("::") if part]
        if not parts:
            return None

        if parts[0] == "crate":
            candidate = "::".join(parts[1:])
        elif parts[0] == "self":
            candidate = "::".join([part for part in [source_module, *parts[1:]] if part])
        elif parts[0] == "super":
            base = source_module.split("::")
            index = 0
            while index < len(parts) and parts[index] == "super":
                if base:
                    base.pop()
                index += 1
            candidate = "::".join([*base, *parts[index:]])
        else:
            candidate = raw

        candidate_parts = [part for part in candidate.split("::") if part]
        if candidate_parts and candidate_parts[0] in {source_module, "lib", "main"}:
            tail_resolved = self._resolve_module_prefix("::".join(candidate_parts[1:]))
            if tail_resolved is not None:
                return tail_resolved
        return self._resolve_module_prefix(candidate)

    def _external_dependency_name(self, raw_dependency: str, source_module: str) -> Optional[str]:
        raw = raw_dependency.strip().rstrip(":")
        if raw.endswith("::*"):
            raw = raw[:-3]
        parts = [part for part in raw.split("::") if part]
        if len(parts) < 2:
            return None
        first = parts[0]
        if first in {"crate", "self", "super", source_module, "lib", "main"}:
            return None
        if self._resolve_module_prefix(raw) is not None:
            return None
        if first in self.module_paths:
            return None
        if first and first[0].isupper():
            return None
        return first

    def _resolve_module_prefix(self, candidate: str) -> Optional[str]:
        parts = candidate.split("::")
        for length in range(len(parts), 0, -1):
            prefix = "::".join(parts[:length])
            if prefix in self.module_paths:
                return prefix
        return None

    def _extract_child_modules(self, fact: Dict[str, object], mod_name: str) -> Set[str]:
        children = set()
        for raw_child in fact.get("child_modules", []) if isinstance(fact, dict) else []:
            child_mod = self._resolve_dependency(str(raw_child), mod_name)
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
        hotspots_path = analysis_path(HOTSPOTS_PATH)
        self.artifact_status["hotspots"] = self._artifact_status(hotspots_path, required=True)
        self.confidence_inputs.append(
            confidence_for_artifacts(
                [hotspots_path],
                source_paths=self.source_roots,
            )
        )
        if not hotspots_path.exists():
            return
        try:
            payload = json.loads(hotspots_path.read_text(encoding="utf-8"))
            results = payload if isinstance(payload, list) else payload.get("items", [])
            for item in results:
                if not isinstance(item, dict):
                    continue
                mod_name = self._metric_module_name(item["name"])
                if mod_name:
                    self.metrics[mod_name] = item
        except Exception as exc:
            self.confidence_inputs.append(
                measurement_confidence(unsupported_pattern=[f"hotspots: {exc}"])
            )
            print(f"Warning: Could not load hotspot metrics: {exc}", file=sys.stderr)

    def _metric_module_name(self, metric_name: str) -> Optional[str]:
        normalized_name = str(Path(metric_name).resolve())
        if normalized_name in self.file_to_mod:
            return self.file_to_mod[normalized_name]

        metric_path = Path(metric_name)
        rel_path = None
        for source_root in self.source_roots:
            try:
                rel_path = metric_path.relative_to(source_root)
                break
            except ValueError:
                continue
        if rel_path is None:
            return None

        mod_name = rel_path.as_posix().replace("/", "::").replace(".rs", "")
        if mod_name.endswith("::mod"):
            mod_name = mod_name[:-5]
        return mod_name

    def gather_performance(self) -> None:
        slowspots_path = analysis_path(SLOWSPOTS_PATH)
        self.artifact_status["slowspots"] = self._artifact_status(slowspots_path, required=False)
        self.confidence_inputs.append(
            confidence_for_artifacts(
                [slowspots_path],
                source_paths=self.source_roots,
                required=False,
            )
        )
        if not slowspots_path.exists():
            return
        try:
            payload = json.loads(slowspots_path.read_text(encoding="utf-8"))
            rows = payload if isinstance(payload, list) else payload.get("items", [])
            if not isinstance(rows, list):
                return
            for item in rows:
                if not isinstance(item, dict):
                    continue
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
        except (OSError, json.JSONDecodeError) as exc:
            self.confidence_inputs.append(
                measurement_confidence(unsupported_pattern=[f"slowspots: {exc}"])
            )
            print(f"Warning: Could not load performance metrics: {exc}", file=sys.stderr)

    def _benchmark_score(self, item: Dict) -> float:
        return float(item["mean_ns"]) / 100_000.0

    def _benchmark_variance(self, item: Dict) -> float:
        mean_ns = float(item.get("mean_ns", 0.0))
        if mean_ns <= 0:
            return 0.0
        return float(item.get("std_dev_ns", 0.0)) / mean_ns

    def gather_test_support(self) -> None:
        test_roots = [
            root
            for root in (Path("tests"), Path("benches"), Path("examples"))
            if root.exists()
        ]
        test_facts = rust_facts_for_paths(test_roots) if test_roots else []
        syntax_refs: DefaultDict[str, Set[str]] = defaultdict(set)
        for fact in test_facts:
            if not isinstance(fact, dict) or fact.get("parse_status") != "ok":
                continue
            test_path = str(fact.get("path", ""))
            source_module = str(fact.get("module_key", ""))
            for raw_dependency in fact.get("dependencies", []):
                dependency = self._resolve_dependency(str(raw_dependency), source_module)
                if dependency in self.module_paths:
                    syntax_refs[dependency].add(test_path)

        for mod_name, file_path in self.mod_to_file.items():
            fact = self.rust_facts.get(mod_name, {})
            has_inline_tests = bool(fact.get("has_inline_tests", False))
            references = sorted(syntax_refs.get(mod_name, set()))

            self.test_support[mod_name] = {
                "has_inline_tests": has_inline_tests,
                "external_refs": references,
                "support_detection": "syntax_dependencies",
                "coverage_hint": has_inline_tests or bool(references),
            }

    def gather_correctness(self) -> None:
        correctness_path = analysis_path(CORRECTNESS_PATH)
        self.artifact_status["correctness"] = self._artifact_status(correctness_path, required=True)
        self.confidence_inputs.append(
            confidence_for_artifacts(
                [correctness_path],
                source_paths=self.source_roots,
            )
        )
        if not correctness_path.exists():
            return
        try:
            payload = json.loads(correctness_path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as exc:
            self.confidence_inputs.append(
                measurement_confidence(unsupported_pattern=[f"correctness: {exc}"])
            )
            return
        if isinstance(payload, dict) and isinstance(payload.get("measurement_confidence"), dict):
            self.confidence_inputs.append(payload["measurement_confidence"])

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
        return None

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
            *self.source_roots,
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
        locality_path = analysis_path(Path("target/analysis/locality_metrics.json"))
        leverage_path = analysis_path(Path("target/analysis/leverage_metrics.json"))
        self.artifact_status["locality"] = self._artifact_status(locality_path, required=True)
        self.artifact_status["leverage"] = self._artifact_status(leverage_path, required=True)
        self.confidence_inputs.append(
            confidence_for_artifacts(
                [locality_path, leverage_path],
                source_paths=self.source_roots,
            )
        )
        self.locality_metrics = self._load_module_metric_artifact(
            Path("target/analysis/locality_metrics.json")
        )
        self.leverage_metrics = self._load_module_metric_artifact(
            Path("target/analysis/leverage_metrics.json")
        )

    def _load_module_metric_artifact(self, path: Path) -> Dict[str, Dict[str, object]]:
        path = analysis_path(path)
        if not path.exists():
            return {}
        try:
            payload = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as exc:
            self.confidence_inputs.append(
                measurement_confidence(unsupported_pattern=[f"{path}: {exc}"])
            )
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

    def _artifact_status(self, path: Path, *, required: bool) -> Dict[str, object]:
        source_files = [Path(item) for item in self.file_to_mod]
        newest_source = max(
            (source.stat().st_mtime for source in source_files if source.exists()),
            default=None,
        )
        exists = path.exists()
        stale = bool(exists and newest_source is not None and path.stat().st_mtime < newest_source)
        if not required and not exists:
            state = "not_provided"
        elif not exists:
            state = "missing"
        elif stale:
            state = "stale"
        else:
            state = "complete"
        return {
            "path": str(path),
            "required": required,
            "exists": exists,
            "stale": stale,
            "state": state,
        }

    def _artifact_unknown(self, name: str) -> bool:
        return self.artifact_status.get(name, {}).get("state") in {"missing", "stale"}

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
        return classify_module(mod_name)

    def _count_layer_violations(self, mod_name: str) -> int:
        source_layer = self.layer_name(mod_name)
        source_rank = layer_rank(source_layer)
        violations = 0
        for dependency in self.dependencies.get(mod_name, set()):
            target_layer = self.layer_name(dependency)
            target_rank = layer_rank(target_layer)
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
        hotspots_unknown = self._artifact_unknown("hotspots")
        correctness_unknown = self._artifact_unknown("correctness")
        return {
            "metric": metric,
            "perf": perf,
            "git": git,
            "tests": tests,
            "correctness": correctness,
            "outbound": outbound,
            "inbound": inbound,
            "public_api": self.public_api_counts.get(mod_name, 0),
            "sloc": None if hotspots_unknown else float(metric.get("sloc", 0.0)),
            "complexity": None if hotspots_unknown else float(metric.get("score", 0.0)),
            "churn": float(git.get("churn", 0)),
            "contributors": int(git.get("contributor_count", 0)),
            "defect_commits": int(git.get("defect_commits", 0)),
            "commit_count": int(git.get("commits", 0)),
            "test_count": None if correctness_unknown else test_count,
            "failed_tests": None if correctness_unknown else int(correctness.get("failed_tests", 0)),
            "unknown_tests": None if correctness_unknown else int(correctness.get("unknown_tests", 0)),
            "skipped_tests": None if correctness_unknown else int(correctness.get("skipped_tests", 0)),
            "has_correctness_tests": None
            if correctness_unknown
            else bool(tests.get("coverage_hint", False)) or test_count > 0,
            "perf_score": float(perf.get("score", 0.0)),
            "perf_mean_ms": float(perf.get("mean_ms", 0.0)),
            "perf_variance": float(perf.get("variance", 0.0)),
            "layer_violations": self._count_layer_violations(mod_name),
            "cycle_member": mod_name in self.cycle_members,
            "external_dependencies": sorted(self.external_dependencies.get(mod_name, set())),
            "unknown_categories": self._unknown_categories(),
        }

    def _unknown_categories(self) -> Set[str]:
        unknown = set()
        if self._artifact_unknown("hotspots"):
            unknown.add("maintainability")
        if self._artifact_unknown("correctness"):
            unknown.update({"change", "correctness"})
        return unknown

    @staticmethod
    def _risk_scores(values: Dict[str, Any]) -> Dict[str, Optional[float]]:
        return architecture_risk_scores(values)

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
        unknown_categories = values["unknown_categories"]
        rules = {
            "maintainability": [
                ("maintainability" in unknown_categories, "unknown: missing or stale hotspot metrics"),
                (
                    values["complexity"] is not None and values["complexity"] >= 300,
                    f"high internal complexity {values['complexity'] or 0:.0f}",
                ),
                (
                    values["sloc"] is not None and values["sloc"] >= 150,
                    f"large module {int(values['sloc'] or 0)} sloc",
                ),
                (values["public_api"] >= 10, f"broad interface {values['public_api']} public items"),
                (
                    values["outbound"] >= 10 or values["inbound"] >= 20,
                    f"high coupling in={values['inbound']} out={values['outbound']}",
                ),
            ],
            "change": [
                ("change" in unknown_categories, "unknown: missing or stale correctness catalog"),
                (values["has_correctness_tests"] is False, "low test evidence"),
                (values["churn"] >= 200, f"high churn {int(values['churn'])} lines"),
                (values["contributors"] >= 3, f"many contributors {values['contributors']}"),
                (
                    values["defect_commits"] >= 1,
                    f"defect history {values['defect_commits']} fix commits",
                ),
            ],
            "performance": [
                (values["perf_mean_ms"] > 0, f"runtime cost {values['perf_mean_ms']:.2f} ms"),
                (
                    values["perf_variance"] >= 0.15,
                    f"instability variance {values['perf_variance']:.2f}",
                ),
                (not values["perf"].get("items"), "no benchmark mapping"),
            ],
            "correctness": [
                ("correctness" in unknown_categories, "unknown: missing or stale correctness catalog"),
                (bool(values["failed_tests"]), f"failing tests {values['failed_tests']}"),
                (bool(values["unknown_tests"]), f"unknown tests {values['unknown_tests']}"),
                (bool(values["skipped_tests"]), f"skipped tests {values['skipped_tests']}"),
                (values["has_correctness_tests"] is False, "no direct tests"),
            ],
            "architectural": [
                (values["layer_violations"] >= 1, f"layer violations {values['layer_violations']}"),
                (values["cycle_member"], "circular dependency"),
                (values["inbound"] >= 6, f"oversized hub inbound {values['inbound']}"),
                (values["sloc"] is not None and values["sloc"] >= 250, "oversized module"),
            ],
        }
        for category, category_rules in rules.items():
            for condition, message in category_rules:
                self._add_signal(signals, category, condition, message)
        return {key: value or ["stable"] for key, value in signals.items()}

    @staticmethod
    def _risk_evidence(values: Dict[str, Any]) -> Dict[str, Any]:
        return {
            "complexity_score": values["complexity"],
            "sloc": int(values["sloc"]) if values["sloc"] is not None else None,
            "public_api_count": values["public_api"],
            "outbound_dependencies": values["outbound"],
            "inbound_dependencies": values["inbound"],
            "external_dependencies": values["external_dependencies"],
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
            "unknown_categories": sorted(values["unknown_categories"]),
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
        classification = model_classification()
        if score >= classification["bad_color_score"]:
            return "#f44747"
        if score >= classification["warn_color_score"]:
            return "#d7ba7d"
        return "#b5cea8"

    def get_group_style(self, mod_name: str) -> Dict[str, str]:
        parts = mod_name.split("::")
        base_color = layer_color(self.layer_name(mod_name))
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

    @staticmethod
    def _float_or_none(value: object) -> Optional[float]:
        if value is None:
            return None
        return float(value)

    @staticmethod
    def _int_or_none(value: object) -> Optional[int]:
        if value is None:
            return None
        return int(value)

    def _module_node(self, mod_name: str) -> Dict[str, Dict[str, object]]:
        perf_data = self.performance.get(mod_name, {})
        perf_items = perf_data.get("items", [])
        metric = self.metrics.get(mod_name, {})
        risk = self.risk_breakdown.get(mod_name, {})
        evidence = risk.get("evidence", {})
        category_signals = risk.get("signals", {})
        locality = self.locality_metrics.get(mod_name, {})
        leverage = self.leverage_metrics.get(mod_name, {})
        locality_unknown = self._artifact_unknown("locality")
        leverage_unknown = self._artifact_unknown("leverage")
        leverage_score = None
        if not leverage_unknown:
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
                "comp_score": None if self._artifact_unknown("hotspots") else float(metric.get("score", 0.0)),
                "perf_score": float(perf_data.get("score", 0.0)),
                "quality_risk": self._float_or_none(
                    risk.get("quality_risk", risk.get("maintainability_risk"))
                ),
                "maintainability_risk": self._float_or_none(risk.get("maintainability_risk")),
                "correctness_risk": self._float_or_none(risk.get("correctness_risk")),
                "change_risk": self._float_or_none(risk.get("change_risk")),
                "performance_risk": self._float_or_none(risk.get("performance_risk")),
                "architectural_risk": self._float_or_none(risk.get("architectural_risk")),
                "locality_score": None
                if locality_unknown
                else float(locality.get("locality_score", 0.0)),
                "locality_risk": None
                if locality_unknown
                else float(locality.get("non_locality_risk", locality.get("locality_risk", 0.0))),
                "non_locality_risk": None
                if locality_unknown
                else float(locality.get("non_locality_risk", locality.get("locality_risk", 0.0))),
                "leverage_score": leverage_score,
                "leverage_risk": None
                if leverage_unknown
                else float(leverage.get("leverage_risk", 100.0 - (leverage_score or 0.0))),
                "total_score": self._float_or_none(risk.get("total_score")),
                "sloc": None if self._artifact_unknown("hotspots") else int(metric.get("sloc", 0)),
                "signals": self._flat_signals(category_signals),
                "category_signals": category_signals,
                "risk_colors": self.risk_colors(risk),
                "evidence": evidence,
                "locality_metrics": None if locality_unknown else locality,
                "leverage_metrics": None if leverage_unknown else leverage,
                "unknown_metrics": self._unknown_metrics(),
                "external_dependencies": sorted(
                    self.external_dependencies.get(mod_name, set())
                ),
                "is_slow": bool(perf_items),
                "perf_benchmarks": self._perf_benchmark_rows(perf_items),
                "perf_kind": ", ".join(
                    sorted({item.get("benchmark_kind", "unmapped") for item in perf_items})
                ),
            }
        }

    def build_graph_payload(self) -> Dict[str, List[Dict]]:
        nodes = [self._group_node(group) for group in sorted(self._graph_groups())]
        external_names = sorted(
            {
                dependency
                for dependencies in self.external_dependencies.values()
                for dependency in dependencies
            }
        )
        nodes.extend(self._external_node(name) for name in external_names)
        nodes.extend(self._module_node(mod_name) for mod_name in sorted(self.dependencies))
        edges = [
            {"data": {"source": source, "target": target}}
            for source, targets in sorted(self.dependencies.items())
            for target in sorted(targets)
            if source != target
        ]
        edges.extend(
            {
                "data": {
                    "source": source,
                    "target": self._external_node_id(target),
                    "is_external": True,
                }
            }
            for source, targets in sorted(self.external_dependencies.items())
            for target in sorted(targets)
        )
        return {"nodes": nodes, "edges": edges}

    @staticmethod
    def _external_node_id(name: str) -> str:
        return f"external:{name}"

    def _external_node(self, name: str) -> Dict[str, Dict[str, object]]:
        return {
            "data": {
                "id": self._external_node_id(name),
                "module": name,
                "label": name,
                "layer": "External",
                "is_external": True,
            }
        }

    def risk_colors(self, risk: Dict[str, object]) -> Dict[str, str]:
        return {
            category: (
                "#808080"
                if risk.get(f"{category}_risk") is None
                else self.risk_color(float(risk.get(f"{category}_risk", 0.0)))
            )
            for category in RISK_CATEGORIES
        }

    def _unknown_metrics(self) -> List[str]:
        unknown = []
        if self._artifact_unknown("hotspots"):
            unknown.extend(["comp_score", "sloc", "maintainability_risk", "quality_risk"])
        if self._artifact_unknown("correctness"):
            unknown.extend(["correctness_risk", "change_risk", "test evidence"])
        if self._artifact_unknown("locality"):
            unknown.extend(["locality_score", "locality_risk"])
        if self._artifact_unknown("leverage"):
            unknown.extend(["leverage_score", "leverage_risk"])
        return unknown

    def meta_summary(self) -> Dict[str, object]:
        measured_modules = len(self.risk_breakdown)
        good = warn = bad = 0
        unknown = 0
        for item in self.risk_breakdown.values():
            raw_score = item.get("total_score", item.get("total_risk"))
            if raw_score is None:
                unknown += 1
                continue
            score = float(raw_score)
            classification = model_classification()
            if score >= classification["bad_total_score"]:
                bad += 1
            elif score >= classification["warn_total_score"]:
                warn += 1
            else:
                good += 1
        category_totals = {
            category: self._category_total(category)
            for category in RISK_CATEGORIES
        }
        return {
            "measured_modules": measured_modules,
            "unknown_modules": unknown,
            "cycle_members": len(self.cycle_members),
            "modules_without_test_evidence": sum(
                1
                for item in self.risk_breakdown.values()
                if item["evidence"]["has_tests"] is False
            ),
            "category_totals": category_totals,
            "artifact_status": self.artifact_status,
            "unknown_metrics": self._unknown_metrics(),
            "good": good,
            "warn": warn,
            "bad": bad,
        }

    def _category_total(self, category: str) -> Optional[float]:
        values = [item.get(f"{category}_risk") for item in self.risk_breakdown.values()]
        if any(value is None for value in values):
            return None
        return round(sum(float(value) for value in values), 2)

    def viewer_payload(self) -> Dict:
        graph = self.build_graph_payload()
        confidence = self.measurement_confidence()
        return {
            "meta": {
                "title": f"{self.project_name} Architecture Risk Map",
                "generated_from": "rust_quality_lens.tools.map",
                "project_name": self.project_name,
                "source_roots": list(self.source_roots),
                "node_count": len(graph["nodes"]),
                "edge_count": len(graph["edges"]),
                "risk_model": list(RISK_CATEGORIES),
                "risk_model_id": model_id(),
                "risk_model_version": model_version(),
                "risk_model_weights": model_weights(),
                "risk_model_tool_scores": RISK_MODEL["tool_scores"],
                "risk_model_contract": RISK_MODEL["raw_fact_contract"],
                "layer_ruleset": {"id": RULESET_ID, "version": RULESET_VERSION},
                "summary": self.meta_summary(),
                "measurement_confidence": confidence,
            },
            "graph": graph,
            "measurement_confidence": confidence,
        }

    def measurement_confidence(self) -> Dict[str, object]:
        return merge_confidences(*self.confidence_inputs)


def refresh_analysis_inputs() -> None:
    commands = [
        HOTSPOT_CMD
        + [
            "--mode",
            "analysis",
            "--paths",
            *[
                root
                for root in os.environ.get("RQLENS_SOURCE_ROOTS", "src").split(os.pathsep)
                if root
            ],
        ],
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
        and not node.get("data", {}).get("is_external")
    ]
    top = sorted(
        modules,
        key=lambda item: -float(item.get("total_score") or -1.0),
    )[:10]
    lines = ["Architecture Risk Map"]
    for index, item in enumerate(top, start=1):
        lines.append(
            f"{index:>2}. {item.get('id', '<unknown>')} | total={format_risk(item.get('total_score'))} | maintainability={format_risk(item.get('maintainability_risk'))} | change={format_risk(item.get('change_risk'))} | architectural={format_risk(item.get('architectural_risk'))}"
        )
    if not top:
        lines.append("No modules found.")
    return "\n".join(lines)


def format_risk(value: object) -> str:
    return "unknown" if value is None else f"{float(value):.2f}"


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

    source_roots = [
        root
        for root in os.environ.get("RQLENS_SOURCE_ROOTS", "src").split(os.pathsep)
        if root
    ]
    mapper = ArchitectureMapper(
        project_name=os.environ.get("RQLENS_PROJECT_NAME", "Rust Project"),
        source_roots=source_roots or ["src"],
    )
    mapper.extract_dependencies()
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
