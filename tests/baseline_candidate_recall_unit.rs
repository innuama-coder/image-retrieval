use std::path::{Path, PathBuf};

fn catalog() -> serde_json::Value {
    serde_json::from_str(include_str!("fixtures/v1_1/baseline/case-catalog.json"))
        .expect("baseline case catalog must be valid JSON")
}

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("v1_1")
}

fn read_json(path: &Path) -> serde_json::Value {
    let content = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("read fixture {}: {}", path.display(), e));
    serde_json::from_str(&content)
        .unwrap_or_else(|e| panic!("parse fixture {}: {}", path.display(), e))
}

fn metric_f64(value: &serde_json::Value, key: &str) -> f64 {
    value[key]
        .as_f64()
        .unwrap_or_else(|| panic!("metric '{}' must be numeric in {}", key, value))
}

#[test]
fn baseline_candidate_recall_unit_fixtures_meet_thresholds() {
    let catalog = catalog();
    let mut checked = 0usize;

    for case in catalog["cases"].as_array().expect("cases array") {
        if case["unit"] != "candidate_recall" {
            continue;
        }
        checked += 1;
        let case_id = case["case_id"].as_str().expect("case_id");
        let provider_response = case["unit_fixtures"]["provider_response"]
            .as_str()
            .unwrap_or_else(|| panic!("{} missing provider_response fixture", case_id));
        let fixture = read_json(&fixture_root().join(provider_response));
        let actual = &fixture["metrics"];
        let expected = &case["expected_metrics"]["unit"];

        assert!(
            metric_f64(actual, "unique_candidate_count")
                >= metric_f64(expected, "min_unique_candidate_count"),
            "{} unique candidate count below threshold",
            case_id
        );
        assert!(
            metric_f64(actual, "expected_candidate_recall")
                >= metric_f64(expected, "expected_candidate_recall_min"),
            "{} expected candidate recall below threshold",
            case_id
        );
        assert!(
            metric_f64(actual, "precision_at_10") >= metric_f64(expected, "precision_at_10_min"),
            "{} precision@10 below threshold",
            case_id
        );
        assert!(
            metric_f64(actual, "duplicate_rate") <= metric_f64(expected, "duplicate_rate_max"),
            "{} duplicate rate above threshold",
            case_id
        );

        if let Some(max) = expected.get("score_mae_max").and_then(|v| v.as_f64()) {
            assert!(
                metric_f64(actual, "score_mae") <= max,
                "{} score MAE above threshold",
                case_id
            );
        }
        if let Some(max) = expected
            .get("false_accept_count_max")
            .and_then(|v| v.as_f64())
        {
            assert!(
                metric_f64(actual, "false_accept_count") <= max,
                "{} false accept count above threshold",
                case_id
            );
        }
        if let Some(min) = expected
            .get("min_distinct_sources")
            .and_then(|v| v.as_f64())
        {
            assert!(
                metric_f64(actual, "distinct_sources") >= min,
                "{} distinct source count below threshold",
                case_id
            );
        }
    }

    assert_eq!(checked, 5, "candidate recall baseline must cover 5 cases");
}
