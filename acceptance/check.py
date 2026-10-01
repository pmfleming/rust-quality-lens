#!/usr/bin/env python3
"""Structural acceptance gate, not proof of a claim's substantive truth.

Required criteria cannot disappear; validation also applies under python -O.
Review evidence against each criterion, not merely a passing test count.
"""
import argparse
import json
import pathlib

GROUPS = {"T": (0, 4), "E": (1, 6), "P": (2, 4), "C": (3, 4), "O": (4, 7)}
EXPECTED = {f"{prefix}{number}": phase for prefix, (phase, count) in GROUPS.items() for number in range(1, count + 1)}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def check(root, release=False):
    ledger = json.loads((root / "acceptance/criteria.json").read_text())
    require(ledger["version"] == 1, "unsupported ledger version")
    ids = set()
    for row in ledger["criteria"]:
        identity = row["id"]
        require(identity in EXPECTED and identity not in ids, f"unknown/duplicate criterion {identity}")
        ids.add(identity)
        require(type(row["phase"]) is int and row["phase"] == EXPECTED[identity] and row["criterion"].strip(), f"invalid phase/criterion {identity}")
        require(row["status"] in ("open", "blocked", "failed", "passed"), f"invalid status {identity}")
        if row["status"] in ("blocked", "failed"):
            require(row.get("reason", "").strip(), f"{identity}: missing reason")
        if row["status"] == "passed":
            require(row["evidence"], f"{identity}: passed without evidence")
        for evidence in row["evidence"]:
            require(all(evidence[key].strip() for key in ("command", "result", "scope", "revision")), f"{identity}: incomplete evidence metadata")
            require(evidence["tests"], f"{identity}: no reproducible tests")
            for name in evidence["tests"]:
                path = (root / name).resolve()
                require(path.is_relative_to(root) and path.is_file(), f"invalid evidence path: {name}")
        print(f"{identity:3} {row['status']:7} {row['criterion']}")
    require(ids == set(EXPECTED), f"missing required criteria: {sorted(set(EXPECTED) - ids)}")
    remaining = sum(row["status"] != "passed" for row in ledger["criteria"])
    print(f"Unresolved required criteria: {remaining}/{len(ids)}")
    return 1 if release and remaining else 0


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--release", action="store_true")
    args = parser.parse_args()
    raise SystemExit(check(pathlib.Path(__file__).resolve().parents[1], args.release))
