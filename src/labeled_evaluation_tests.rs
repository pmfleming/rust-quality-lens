use super::{Observation, evaluate};
use serde::Deserialize;
use serde_json::{Value, json};

#[test]
fn shared_calibration_conformance() -> anyhow::Result<()> {
    let corpus: Value = serde_json::from_str(include_str!("../contracts/calibration-cases.json"))?;
    let observations: Vec<Observation> = serde_json::from_value(corpus["observations"].clone())?;
    assert_eq!(
        serde_json::to_value(evaluate(&observations)?)?,
        json!([corpus["expected"]])
    );
    let mut duplicate = observations.clone();
    duplicate.extend(observations);
    assert!(evaluate(&duplicate).is_err());
    Ok(())
}

#[derive(Deserialize)]
struct Case {
    #[serde(flatten)]
    observation: Observation,
    source: String,
}

fn corpus_project(cases: &[Case]) -> anyhow::Result<tempfile::TempDir> {
    let root = tempfile::tempdir()?;
    std::fs::create_dir(root.path().join("src"))?;
    std::fs::write(root.path().join("rqlens.toml"), "source_roots = ['src']\n")?;
    for case in cases {
        for mutation in [false, true] {
            let source = if mutation {
                format!("// moved\n{}", case.source.replace("value", "renamed"))
            } else {
                case.source.clone()
            };
            std::fs::write(
                root.path()
                    .join(format!("src/{}-{mutation}.rs", case.observation.id)),
                source,
            )?;
        }
    }
    Ok(root)
}

#[test]
fn rust_idiom_corpus_survives_identifier_and_location_mutations() -> anyhow::Result<()> {
    let cases: Vec<Case> = serde_json::from_str(include_str!("../contracts/quality-corpus.json"))?;
    let root = corpus_project(&cases)?;
    let config = crate::config::LensConfig::load(Some(root.path().join("rqlens.toml")))?;
    let context = crate::facts::RunContext::new(&config, &[crate::tool::MeasureTool::Reliability])?;
    let mut observations = Vec::new();
    for case in cases {
        for mutation in [false, true] {
            let name = format!("{}-{mutation}.rs", case.observation.id);
            let fact = context
                .source_facts
                .iter()
                .find(|fact| {
                    std::path::Path::new(&fact.path)
                        .file_name()
                        .is_some_and(|file| file == name.as_str())
                })
                .ok_or_else(|| anyhow::anyhow!("missing corpus fact {name}"))?;
            let mut observation = case.observation.clone();
            observation.id = name;
            observation.observed = (fact.parse_status == "ok").then(|| {
                fact.items
                    .quality_findings
                    .iter()
                    .any(|finding| finding.rule_id == observation.rule)
            });
            assert_eq!(
                observation.observed,
                Some(observation.expected),
                "{}",
                observation.id
            );
            observations.push(observation);
        }
    }
    println!("{}", serde_json::to_string(&evaluate(&observations)?)?);
    Ok(())
}
