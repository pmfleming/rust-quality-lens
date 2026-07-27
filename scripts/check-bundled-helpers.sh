#!/usr/bin/env bash
set -euo pipefail

diff -u src/bundled_helpers/lib.rs.txt rust_helpers/src/lib.rs
diff -u src/bundled_helpers/rust_facts.rs.txt rust_helpers/src/bin/rust_facts.rs
diff -u src/bundled_helpers/ast_hasher.rs.txt rust_helpers/src/bin/ast_hasher.rs

echo "Bundled helper sources match the workspace helper crate."
