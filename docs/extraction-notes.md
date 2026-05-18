# Extraction Notes

`rust-quality-lens` currently carries the reusable JSON-producing part of
Scratchpad's measurement layer.

Included:

- `hotspots.py`
- `clone_alert.py`
- `rust_escape_hatches.py`
- `type_health.py`
- `locality_bench.py`
- `leverage_metrics.py`
- `test_catalog.py`
- `map.py`
- shared report-mode support
- Rust helper binaries for AST clone hashing and leverage style counts

Not included:

- dashboard/server/UI assets
- Scratchpad-specific app package diagnostics

Left in Scratchpad for now:

- performance probes
- capacity/resource probes
- Scratchpad-specific profile binaries

The next cleanup step is to split the copied analyzer modules into smaller
package modules. For the first extraction pass, preserving existing JSON output
contracts matters more than changing internal structure.
