//! Prevent stale inventory references and accidentally omitted new regressions.
use std::collections::BTreeSet;

#[test]
fn inventory_references_real_tests_and_distinguishes_existing_coverage() {
    let inventory: serde_json::Value = serde_json::from_str(include_str!("coverage.json")).unwrap();
    assert_eq!(inventory["schema_version"], 1);
    let sources = [
        ("resilience/mod.rs", include_str!("mod.rs")),
        ("resilience/crashes.rs", include_str!("crashes.rs")),
        ("resilience/storage.rs", include_str!("storage.rs")),
        ("resilience/backup.rs", include_str!("backup.rs")),
        ("resilience/adversarial.rs", include_str!("adversarial.rs")),
        ("finance.rs", include_str!("../finance.rs")),
        ("admission.rs", include_str!("../admission.rs")),
        ("worker_recovery.rs", include_str!("../worker_recovery.rs")),
        ("orchestration.rs", include_str!("../orchestration.rs")),
        (
            "migration_replay.rs",
            include_str!("../migration_replay.rs"),
        ),
        (
            "../../plan-validator/tests/validation.rs",
            include_str!("../../../plan-validator/tests/validation.rs"),
        ),
    ];
    let mut ids = BTreeSet::new();
    let mut references = BTreeSet::new();
    let mut fixtures = BTreeSet::new();
    let mut kills = BTreeSet::new();
    for case in inventory["cases"].as_array().unwrap() {
        assert!(ids.insert(case["id"].as_str().unwrap()));
        assert_eq!(case["outcome"], "local_pass");
        assert!(!case["boundary_limit"].as_str().unwrap().is_empty());
        for reference in case["references"].as_array().unwrap() {
            let reference = reference.as_str().unwrap();
            let (file, name) = reference.split_once('#').unwrap();
            let (_, source) = sources.iter().find(|(path, _)| *path == file).unwrap();
            assert!(source.contains(&format!("fn {name}()")), "{reference}");
            references.insert(reference.to_owned());
        }
        for fixture in case["fixtures"].as_array().unwrap() {
            fixtures.insert(fixture.as_str().unwrap());
        }
        for point in case["kill_points"].as_array().unwrap() {
            assert!(kills.insert(point.as_str().unwrap()));
            assert_eq!(case["restart"], true);
            assert_eq!(case["origin"], "R10");
        }
    }
    for (file, source) in sources
        .iter()
        .filter(|(file, _)| file.starts_with("resilience/"))
    {
        let lines: Vec<_> = source.lines().map(str::trim).collect();
        for pair in lines.windows(2).filter(|pair| pair[0] == "#[tokio::test]") {
            let name = pair[1]
                .strip_prefix("async fn ")
                .unwrap()
                .split('(')
                .next()
                .unwrap();
            assert!(
                references.contains(&format!("{file}#{name}")),
                "unlisted regression: {name}"
            );
        }
    }
    assert_eq!(kills.len(), 20);
    for number in 1..=13 {
        assert!(fixtures.contains(format!("F{number}").as_str()));
    }
    assert!(!inventory["unverified"].as_array().unwrap().is_empty());
}
