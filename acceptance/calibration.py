#!/usr/bin/env python3
"""Verify the frozen regression candidates, without claiming independent calibration."""
import argparse
import hashlib
import json
import pathlib


def check(root, release=False):
    plan = json.loads((root / "acceptance/calibration-plan.json").read_text())
    if plan["version"] != 1 or not plan["frozen_candidates"]:
        raise ValueError("missing frozen calibration plan")
    seen = set()
    for item in plan["frozen_candidates"]:
        path = (root / item["path"]).resolve()
        if not path.is_relative_to(root) or path in seen:
            raise ValueError("duplicate or out-of-repository candidate")
        seen.add(path)
        actual = hashlib.sha256(path.read_bytes()).hexdigest()
        if actual != item["sha256"]:
            raise ValueError(f"Frozen candidate changed: {item['path']}; review and version the corpus, do not silently tune it")
    if release:
        # Deliberately not unlockable by changing a JSON approval flag. Independent
        # labels and their native result adapters have not been supplied/reviewed.
        print("BLOCKED: independent labels, intent review and held-out native ranking results are still required")
        return 1
    print("Frozen candidate hashes verified; independent calibration acceptance is NOT established")
    return 0


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--release", action="store_true")
    args = parser.parse_args()
    raise SystemExit(check(pathlib.Path(__file__).resolve().parents[1], args.release))
