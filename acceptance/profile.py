#!/usr/bin/env python3
"""Repeat a native fixture benchmark; never treat absent/partial samples as passing."""
import json
import math
import pathlib
import platform
import subprocess


def documents(text):
    decoder = json.JSONDecoder()
    while text.strip():
        value, end = decoder.raw_decode(text.lstrip())
        yield value
        text = text.lstrip()[end:]


def main():
    root = pathlib.Path(__file__).resolve().parents[1]
    if platform.system() != "Linux" or platform.machine() != "x86_64":
        raise SystemExit("Unsupported resource profile; native RSS units must be reviewed first")
    plan = json.loads((root / "acceptance/resource-budgets.json").read_text())
    if type(plan["repetitions"]) is not int or plan["repetitions"] < 3 or type(plan["records_per_repetition"]) is not int or plan["records_per_repetition"] < 1:
        raise ValueError("resource profiles require repeated, nonempty observations")
    for key in ("max_duration_ms", "max_peak_rss_kib"):
        if type(plan[key]) not in (int, float) or not math.isfinite(plan[key]) or plan[key] <= 0:
            raise ValueError("resource ceilings must be finite and positive")
    samples = []
    for repetition in range(plan["repetitions"]):
        run = subprocess.run(plan["command"], cwd=root, text=True, capture_output=True, timeout=240)
        if run.returncode != 0:
            raise RuntimeError(f"benchmark failed ({run.returncode}): {run.stderr[-8192:]}")
        values = list(documents(run.stdout))
        records = [record for value in values for record in value.get("results", [value])]
        if len(records) != plan["records_per_repetition"]:
            raise ValueError("missing or unexpected benchmark samples")
        for record in records:
            for field, ceiling in (("duration_ms", plan["max_duration_ms"]), (plan["rss_field"], plan["max_peak_rss_kib"])):
                value = record[field]
                if type(value) not in (int, float) or not math.isfinite(value) or not 0 <= value <= ceiling:
                    raise ValueError(f"resource ceiling exceeded or invalid: {field}={value}")
            if record.get("status", "complete") != "complete":
                raise ValueError("partial benchmark evidence")
        environment = {key: values[0][key] for key in ("benchmark", "node", "platform", "scope") if key in values[0]}
        samples.append({"repetition": repetition + 1, "environment": environment, "records": records})
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True, timeout=5).strip()
    print(json.dumps({"status": "passed", "revision": revision, "platform": platform.platform(), "budgets": plan, "samples": samples}, indent=2))


if __name__ == "__main__":
    main()
