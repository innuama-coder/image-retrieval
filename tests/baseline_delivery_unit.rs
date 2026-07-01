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
fn baseline_delivery_unit_fixtures_meet_thresholds() {
    let catalog = catalog();
    let root = fixture_root();
    let mut checked = 0usize;

    for case in catalog["cases"].as_array().expect("cases array") {
        if case["unit"] != "delivery" {
            continue;
        }
        checked += 1;
        let case_id = case["case_id"].as_str().expect("case_id");
        let fixture_dir = case["unit_fixtures"]["retrieved_results"]
            .as_str()
            .unwrap_or_else(|| panic!("{} missing retrieved_results fixture", case_id));
        let fixture = read_json(&root.join(fixture_dir).join("metrics.json"));
        let actual = &fixture["metrics"];
        let expected = &case["expected_metrics"]["unit"];

        assert_eq!(
            metric_f64(actual, "accepted_count"),
            metric_f64(expected, "accepted_count"),
            "{} accepted count mismatch",
            case_id
        );
        assert!(
            metric_f64(actual, "delivery_recall") >= metric_f64(expected, "delivery_recall_min"),
            "{} delivery recall below threshold",
            case_id
        );
        assert!(
            metric_f64(actual, "delivery_precision")
                >= metric_f64(expected, "delivery_precision_min"),
            "{} delivery precision below threshold",
            case_id
        );
        assert_eq!(
            metric_f64(actual, "wrong_image_delivered_count"),
            metric_f64(expected, "wrong_image_delivered_count"),
            "{} wrong image delivered count mismatch",
            case_id
        );
        if let Some(expected_missing) = expected
            .get("missing_expected_image_count")
            .and_then(|v| v.as_f64())
        {
            assert_eq!(
                metric_f64(actual, "missing_expected_image_count"),
                expected_missing,
                "{} missing expected image count mismatch",
                case_id
            );
        }

        let validation_expected = expected["package_validation_passed"]
            .as_bool()
            .expect("package_validation_passed bool");
        let validation_actual = actual["package_validation_passed"]
            .as_bool()
            .expect("package_validation_passed bool");
        assert_eq!(
            validation_actual, validation_expected,
            "{} package validation expectation mismatch",
            case_id
        );
    }

    assert_eq!(checked, 5, "delivery baseline must cover 5 cases");
}
