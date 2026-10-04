//! Stable source anchors shared by artifacts, review and exporters.
pub(crate) mod practices;
use crate::config::{LensConfig, PolicyWaiver};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs};

/// Policy disposition is distinct from impact. Legacy records retain native severity.
pub(crate) fn finding_level(finding: &Value) -> &'static str {
    match finding["disposition"].as_str() {
        Some("block") => "error",
        Some("warn") => "warning",
        Some("review" | "info") => "note",
        _ => match finding["severity"].as_str() {
            Some("error") => "error",
            Some("warning") => "warning",
            _ => "note",
        },
    }
}

pub(crate) fn is_waivable(finding: &Value) -> bool {
    !matches!(
        finding["status"].as_str(),
        Some("unavailable" | "timed-out" | "failed_execution" | "incomplete")
    )
}

pub(crate) const SOURCE_FILES: [(&str, &str); 3] = [
    ("reliability_findings.json", "/records"),
    ("architecture_rules.json", "/data/violations"),
    ("test_quality.json", "/data/findings"),
];

pub(crate) fn source(config: &LensConfig, file: &str, document: Option<&Value>) -> Vec<Value> {
    let mut rows = SOURCE_FILES
        .iter()
        .find(|(name, _)| *name == file)
        .and_then(|(_, pointer)| document?.pointer(pointer)?.as_array())
        .cloned()
        .unwrap_or_default();
    normalize_records(config, &mut rows);
    policy_context(config, &mut rows);
    for (index, row) in rows.iter_mut().enumerate() {
        row["origin"] = json!({"artifact": file, "record_index": index});
        row["policy_selected"] = json!(
            file != "reliability_findings.json"
                || (row["severity"] != "advisory" && row["scope"] == "production")
        );
    }
    rows
}

pub(crate) fn policy_context(config: &LensConfig, rows: &mut [Value]) {
    for row in rows {
        row["policy_selected"] = json!(true);
        row["policy_scope"] = Value::Null;
        if let Some(rule) = row["rule_id"]
            .as_str()
            .and_then(|id| config.policy.rules.get(id))
        {
            row["policy_scope"] = json!({"included": rule.includes(row["path"].as_str(), row["package_name"].as_str()), "configuration": rule});
        }
    }
}

pub(crate) fn active_waiver<'a>(
    config: &'a LensConfig,
    finding: &Value,
) -> Option<&'a PolicyWaiver> {
    config
        .policy
        .active_waiver(finding["rule_id"].as_str()?, finding["path"].as_str())
        .filter(|_| is_waivable(finding))
}

pub(crate) fn normalize(config: &LensConfig, value: &mut Value) {
    visit(config, value, &mut BTreeMap::new());
}

pub(crate) fn normalize_records(config: &LensConfig, records: &mut [Value]) {
    let mut occurrences = BTreeMap::new();
    for record in records {
        visit(config, record, &mut occurrences);
    }
}

fn visit(config: &LensConfig, value: &mut Value, occurrences: &mut BTreeMap<String, u64>) {
    match value {
        Value::Array(items) => {
            for item in items {
                visit(config, item, occurrences);
            }
        }
        Value::Object(item) => {
            if let Some(rule) = item
                .get("rule_id")
                .and_then(Value::as_str)
                .map(str::to_string)
            {
                let path = item.get("path").and_then(Value::as_str).unwrap_or("");
                let line = item.get("line").and_then(Value::as_u64);
                let anchor = line
                    .and_then(|line| usize::try_from(line).ok())
                    .and_then(|line| {
                        fs::read_to_string(config.project_root.join(path))
                            .ok()
                            .and_then(|text| {
                                text.lines().nth(line.saturating_sub(1)).map(str::to_string)
                            })
                    })
                    .unwrap_or_else(|| {
                        item.get("message")
                            .or_else(|| item.get("title"))
                            .and_then(Value::as_str)
                            .unwrap_or(&rule)
                            .to_string()
                    });
                let anchor = anchor.split_whitespace().collect::<Vec<_>>().join(" ");
                let subject = format!("rust-quality-lens\0{rule}\0{path}\0{anchor}");
                let occurrence = occurrences.entry(subject.clone()).or_default();
                *occurrence += 1;
                item.insert(
                    "id".into(),
                    json!(format!(
                        "rqlens:{}:{}",
                        crate::util::stable_hash(&subject),
                        occurrence
                    )),
                );
                item.insert("identity_version".into(), json!("source-anchor-v1"));
            }
            for child in item.values_mut() {
                visit(config, child, occurrences);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::normalize;
    use crate::test_support::source_project;
    use serde_json::json;
    use std::fs;
    #[test]
    fn identity_survives_line_shifts_and_distinguishes_occurrences() -> anyhow::Result<()> {
        let (root, config) = source_project(Some("fn f() { panic!(); }\nfn f() { panic!(); }\n"))?;
        let file = root.path().join("src/lib.rs");
        let finding = |line| json!({"rule_id": "rust.panic", "path": "src/lib.rs", "line": line});
        let mut before = json!([finding(1), finding(2)]);
        normalize(&config, &mut before);
        assert_ne!(before[0]["id"], before[1]["id"]);
        fs::write(
            file,
            "// moved\nfn f() { panic!(); }\nfn f() { panic!(); }\n",
        )?;
        let mut after = json!([finding(2), finding(3)]);
        normalize(&config, &mut after);
        assert_eq!(before[0]["id"], after[0]["id"]);
        assert_eq!(before[1]["id"], after[1]["id"]);
        Ok(())
    }
}
