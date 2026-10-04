"""Inventory integrity and campaign-oracle regressions; not E2 rule certification."""
import json
from pathlib import Path
import re
import unittest

from evidence_contract_campaign import FILES, cases, expected, mutate

ROOT = Path(__file__).resolve().parent.parent


class EvidenceContracts(unittest.TestCase):
    def setUp(self):
        self.contracts = json.loads((ROOT / "contracts/blocking-rules.json").read_text())

    def test_inventory_covers_rules_gates_and_explicit_limitations(self):
        self.assertEqual(self.contracts["schema_version"], 1)
        self.assertNotEqual(self.contracts["coverage_status"], "passed")
        for group in ["families", "dynamic_families", "gates", "automatic_gates"]:
            for name, contract in self.contracts[group].items():
                for field in ["prerequisites", "applicability", "scope", "rationale", "limitations"]:
                    self.assertTrue(contract[field], (name, field))
                self.assertEqual(set(contract["cases"]), {"positive", "negative", "unavailable"})
                for refs in contract["cases"].values():
                    for ref in refs:
                        path, anchor = ref.split("#")
                        self.assertIn(anchor, (ROOT / path).read_text())
        for family in self.contracts["rules"].values():
            self.assertIn(self.contracts["families"][family]["artifact"], FILES)

        paths = ["rust_helpers/src/bin/rust_facts.rs", "src/producers/practices.rs",
                 "src/producers/test_quality.rs", "src/toolchain.rs"]
        for path in paths:
            for rule in re.findall(r'"(rust\.[a-z0-9_.-]+)"', (ROOT / path).read_text()):
                if not rule.startswith(("rust.compiler.", "rust.clippy.")):
                    self.assertIn(rule, self.contracts["rules"], (path, rule))
        helper = (ROOT / paths[0]).read_text()
        for pattern in [r'matches!\(method.as_str\(\), ([^)]*)\)', r'matches!\(macro_name, ([^)]*)\)']:
            group, = re.findall(pattern, helper)
            for suffix in re.findall(r'"([a-z]+)"', group):
                self.assertIn(f"rust.reliability.{suffix}", self.contracts["rules"])
        self.assertNotIn("rust.reliability.unreachable", self.contracts["rules"])

        source = (ROOT / "src/policy.rs").read_text()
        names = re.findall(r'Self::\w+ => "([a-z-]+)"', source)
        self.assertEqual(set(names), set(self.contracts["gates"]))
        self.assertEqual(len(names), 7)

    def test_campaign_distinguishes_unavailable_disabled_and_clean(self):
        self.assertEqual(expected("error", "skipped"), ("incomplete", False))
        self.assertEqual(expected("off", "missing"), ("disabled", True))
        self.assertEqual(expected("warning", "skipped"), ("incomplete", True))
        self.assertEqual(expected("error", "clean"), ("passed", True))
        self.assertEqual(expected("error", "violation"), ("exceeded", False))
        self.assertEqual(len(cases()), len(set(cases())))
        docs = {"rust_practices.json": {"data": {"checks": []}}}
        mutate(docs, "rust.security.rustsec", "rust_practices.json", "mixed-skipped")
        self.assertEqual([row["status"] for row in docs["rust_practices.json"]["data"]["checks"]],
                         ["passed", "skipped"])


if __name__ == "__main__":
    unittest.main()
