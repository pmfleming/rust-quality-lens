//! Stable source anchors shared by artifacts, review and exporters.
use crate::config::LensConfig;
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs};

pub(crate) fn normalize(config: &LensConfig, value: &mut Value) {
    visit(config, value, &mut BTreeMap::new());
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
    use super::*;
    #[test]
    fn identity_survives_line_shifts_and_distinguishes_occurrences() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        fs::create_dir(root.path().join("src"))?;
        let file = root.path().join("src/lib.rs");
        fs::write(&file, "fn f() { panic!(); }\nfn f() { panic!(); }\n")?;
        let config_path = root.path().join("rqlens.toml");
        fs::write(&config_path, "source_roots = ['src']\n")?;
        let config = LensConfig::load(Some(config_path))?;
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
