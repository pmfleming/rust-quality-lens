#!/usr/bin/env bash
set -euo pipefail

baseline=quality-baseline.json
analysis=target/analysis

metric() { jq -r ".$1" "$baseline"; }
require() {
  local file=$1 expression=$2 message=$3
  if ! jq -e "$expression" "$file" >/dev/null; then
    echo "self-metric regression: $message" >&2
    exit 1
  fi
}

max_hotspot=$(metric max_function_hotspot_score)
max_ast_clones=$(metric max_ast_clone_records)
max_clones=$(metric max_token_clone_records)
max_duplication=$(metric max_duplication_percent)
min_locality=$(metric min_map_locality_score)
min_leverage=$(metric min_map_leverage_score)

require "$analysis/hotspots.json" \
  "([.records[] | select(.kind == \"function\") | .score] | max // 0) <= $max_hotspot" \
  "function hotspot exceeds $max_hotspot"
require "$analysis/rust_escape_hatches.json" \
  '.summary.record_count == 0' 'escape hatch detected'
require "$analysis/clones.json" \
  "[.records[] | select(.engine == \"ast\")] | length <= $max_ast_clones" \
  "AST clone count exceeds $max_ast_clones"
require "$analysis/clones.json" \
  "[.records[] | select(.engine == \"token\")] | length <= $max_clones" \
  "token clone count exceeds $max_clones"
require "$analysis/clones.json" \
  ".summary.duplication_percent <= $max_duplication" \
  "duplicated-line percentage exceeds $max_duplication"
require "$analysis/locality_metrics.json" \
  "[.records[] | select(.module_key == \"producers::map\") | .locality_score][0] >= $min_locality" \
  "map locality fell below $min_locality"
require "$analysis/leverage_metrics.json" \
  "[.records[] | select(.module_key == \"producers::map\") | .leverage_score][0] >= $min_leverage" \
  "map leverage fell below $min_leverage"

echo "Self metrics passed."
