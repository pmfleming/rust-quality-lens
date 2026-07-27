#!/usr/bin/env bash
set -euo pipefail

version=$(awk -F '"' '/^version = / { print $2; exit }' Cargo.toml)
cargo package --locked --allow-dirty
package_dir="target/package/rust-quality-lens-${version}"
cargo build --manifest-path "$package_dir/Cargo.toml" --bin rqlens

fixture=$(mktemp -d)
trap 'rm -rf "$fixture"' EXIT
mkdir -p "$fixture/src"
cat >"$fixture/Cargo.toml" <<'TOML'
[package]
name = "rqlens-package-smoke"
version = "0.1.0"
edition = "2024"
TOML
cat >"$fixture/src/lib.rs" <<'RS'
/// # Safety
/// `pointer` must be valid for reads.
pub unsafe fn read(pointer: *const i32) -> i32 {
    // SAFETY: the caller guarantees that the pointer is valid.
    unsafe { *pointer }
}
RS
cat >"$fixture/rqlens.toml" <<'TOML'
project_name = "rqlens-package-smoke"
project_root = "."
output_dir = "target/analysis"

[rust]
identity_resolution = "disabled"
TOML

"$package_dir/target/debug/rqlens" measure reliability --config "$fixture/rqlens.toml"
test -f "$fixture/target/analysis/reliability_findings.json"
grep -q '"tool": "reliability"' "$fixture/target/analysis/reliability_findings.json"
echo "Packaged rqlens analyzed an external fixture successfully."
