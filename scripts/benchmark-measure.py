#!/usr/bin/env python3
"""Reproducible source-only first/repeat CLI timing; does not execute project code."""
import json
import pathlib
import platform
import resource
import subprocess
import tempfile
import time


def main():
    binary = pathlib.Path(__file__).resolve().parents[1] / "target/debug/rqlens"
    results = []
    with tempfile.TemporaryDirectory(prefix="rqlens-bench-") as directory:
        root = pathlib.Path(directory)
        for count in (10, 50, 100):
            project = root / str(count)
            source = project / "src"
            source.mkdir(parents=True)
            for index in range(count):
                (source / f"file{index}.rs").write_text(
                    f"pub fn value{index}(flag: bool) -> u32 {{ if flag {{ {index} }} else {{ 0 }} }}\n"
                )
            config = project / "rqlens.toml"
            config.write_text("source_roots = ['src']\n[rust]\nidentity_resolution = 'disabled'\n")
            for run in ("first-process", "repeat-process"):
                started = time.monotonic()
                subprocess.run([str(binary), "measure", "hotspots", "--config", str(config)],
                               check=True, capture_output=True, text=True, timeout=120)
                results.append({"files": count, "run": run,
                                "duration_ms": round((time.monotonic() - started) * 1000),
                                "children_peak_rss_native_units": resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss})
    print(json.dumps({"benchmark": "source-only-hotspots", "platform": platform.platform(),
                      "scope": "prebuilt binary/helper; OS caches may be warm; child RSS peak cumulative (KiB on Linux, bytes on macOS)",
                      "results": results}, indent=2))


if __name__ == "__main__":
    main()
