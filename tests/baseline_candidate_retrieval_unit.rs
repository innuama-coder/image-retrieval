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
fn baseline_candidate_retrieval_unit_fixtures_meet_thresholds() {
    let catalog = catalog();
    let root = fixture_root();
    let mut checked = 0usize;

    for case in catalog["cases"].as_array().expect("cases array") {
        if case["unit"] != "candidate_retrieval" {
            continue;
        }
        checked += 1;
        let case_id = case["case_id"].as_str().expect("case_id");

        for key in ["candidates", "channel_config"] {
            let rel = case["unit_fixtures"][key]
                .as_str()
                .unwrap_or_else(|| panic!("{} missing {} fixture", case_id, key));
            assert!(
                root.join(rel).exists(),
                "{} fixture '{}' must exist",
                case_id,
                key
            );
        }

        let fixture_dir = case["unit_fixtures"]["retrieval_fixture"]
            .as_str()
            .unwrap_or_else(|| panic!("{} missing retrieval_fixture", case_id));
        let fixture = read_json(&root.join(fixture_dir).join("metrics.json"));
        let actual = &fixture["metrics"];
        let expected = &case["expected_metrics"]["unit"];

        assert!(
            metric_f64(actual, "retrieval_success_rate")
                >= metric_f64(expected, "retrieval_success_rate_min"),
            "{} retrieval success rate below threshold",
            case_id
        );
        assert_eq!(
            metric_f64(actual, "artifact_complete_count"),
            metric_f64(expected, "artifact_complete_count"),
            "{} artifact complete count mismatch",
            case_id
        );
        assert!(
            metric_f64(actual, "failure_classification_accuracy")
                >= metric_f64(expected, "failure_classification_accuracy_min"),
            "{} failure classification accuracy below threshold",
            case_id
        );

        for (actual_key, min_key) in [
            ("fallback_attempt_count", "fallback_attempt_count_min"),
            ("fallback_success_count", "fallback_success_count_min"),
        ] {
            if let Some(min) = expected.get(min_key).and_then(|v| v.as_f64()) {
                assert!(
                    metric_f64(actual, actual_key) >= min,
                    "{} {} below threshold",
                    case_id,
                    actual_key
                );
            }
        }
        for (actual_key, expected_key) in [
            ("fallback_attempt_count", "fallback_attempt_count"),
            ("policy_blocked_count", "policy_blocked_count"),
            (
                "metadata_only_rejected_count",
                "metadata_only_rejected_count",
            ),
            (
                "completed_job_reattempt_count",
                "completed_job_reattempt_count",
            ),
        ] {
            if let Some(expected_value) = expected.get(expected_key).and_then(|v| v.as_f64()) {
                assert_eq!(
                    metric_f64(actual, actual_key),
                    expected_value,
                    "{} {} mismatch",
                    case_id,
                    actual_key
                );
            }
        }
    }

    assert_eq!(
        checked, 5,
        "candidate retrieval baseline must cover 5 cases"
    );
}
