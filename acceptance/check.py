#!/usr/bin/env python3
"""Structural acceptance audit; --release fails closed on any unresolved criterion.

Passing this checker is necessary, not proof that a review claim is substantively true.
Evidence must be reviewed against the criterion, not merely a passing test count.
"""
import argparse
import json
import pathlib


def check(root, release=False):
    ledger = json.loads((root / "acceptance/criteria.json").read_text())
    assert ledger["version"] == 1
    ids = set()
    for row in ledger["criteria"]:
        assert row["id"] not in ids, f"duplicate criterion {row['id']}"
        ids.add(row["id"])
        assert row["phase"] in range(5) and row["criterion"].strip()
        assert row["status"] in ("open", "blocked", "failed", "passed")
        if row["status"] in ("blocked", "failed"):
            assert row.get("reason", "").strip(), f"{row['id']}: missing reason"
        if row["status"] == "passed":
            assert row["evidence"], f"{row['id']}: passed without evidence"
        for evidence in row["evidence"]:
            assert evidence["command"].strip() and evidence["result"].strip()
            assert evidence["scope"].strip() and evidence["revision"].strip()
            for name in evidence["tests"]:
                path = (root / name).resolve()
                assert path.is_relative_to(root) and path.is_file(), f"invalid evidence path: {name}"
        print(f"{row['id']:3} {row['status']:7} {row['criterion']}")
    assert {row["phase"] for row in ledger["criteria"]} == set(range(5))
    remaining = sum(row["status"] != "passed" for row in ledger["criteria"])
    print(f"Unresolved required criteria: {remaining}/{len(ids)}")
    return 1 if release and remaining else 0


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--release", action="store_true")
    args = parser.parse_args()
    raise SystemExit(check(pathlib.Path(__file__).resolve().parents[1], args.release))
