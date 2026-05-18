# Extraction Notes

`rust-quality-lens` currently carries the quality-focused part of Scratchpad's
measurement layer.

Included:

- `hotspots.py`
- `clone_alert.py`
- `rust_escape_hatches.py`
- `type_health.py`
- `locality_bench.py`
- `leverage_metrics.py`
- shared report-mode support
- Rust helper binaries for AST clone hashing and leverage style counts
- static dashboard assets

Left in Scratchpad for now:

- performance probes
- capacity/resource probes
- correctness catalog
- app package diagnostics
- Scratchpad-specific profile binaries

The next cleanup step is to split the copied analyzer modules into smaller
package modules. For the first extraction pass, preserving existing JSON output
contracts matters more than changing internal structure.
