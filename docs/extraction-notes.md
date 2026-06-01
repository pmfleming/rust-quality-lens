# Extraction Notes

`rust-quality-lens` currently carries the reusable JSON-producing part of
Scratchpad's measurement layer.

Included:

- `hotspots`
- `clones`
- `escape-hatches`
- `type-health`
- `locality`
- `leverage`
- `correctness`
- `map`
- shared catalog and artifact-writing support
- Rust helper binaries for AST clone hashing and leverage style counts

Not included:

- dashboard/server/UI assets
- Scratchpad-specific app package diagnostics

Left in Scratchpad for now:

- performance probes
- capacity/resource probes
- Scratchpad-specific profile binaries

The next cleanup step is to split the Rust CLI into smaller modules. For the
first Rust pass, preserving the command and artifact contracts matters more than
polishing internal structure.
