use std::collections::{BTreeMap, BTreeSet};

fn catalog() -> serde_json::Value {
    serde_json::from_str(include_str!("fixtures/v1_1/baseline/case-catalog.json"))
        .expect("baseline case catalog must be valid JSON")
}

#[test]
fn baseline_catalog_has_expected_units_and_case_count() {
    let catalog = catalog();
    let cases = catalog["cases"].as_array().expect("cases array");

    assert_eq!(catalog["suite_id"], "baseline_recall_v1_1");
    assert_eq!(cases.len(), 15, "first baseline should have 15 cases");

    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for case in cases {
        let unit = case["unit"].as_str().expect("case unit");
        *counts.entry(unit.to_string()).or_default() += 1;
    }

    assert_eq!(counts.get("candidate_recall"), Some(&5));
    assert_eq!(counts.get("candidate_retrieval"), Some(&5));
    assert_eq!(counts.get("delivery"), Some(&5));
}

#[test]
fn baseline_scenarios_require_real_execution_only() {
    let catalog = catalog();
    let policy = &catalog["execution_policy"]["scenario_real_service"];

    assert_eq!(policy["fixtures_allowed"], false);
    assert_eq!(policy["mocks_allowed"], false);
    assert_eq!(policy["fakes_allowed"], false);
    assert_eq!(policy["requires_real_search_provider"], true);
    assert_eq!(policy["requires_real_retrieval_channels"], true);
    assert_eq!(policy["requires_real_vlm_evaluator"], true);
    assert_eq!(policy["requires_cli_execution"], true);

    for case in catalog["cases"].as_array().expect("cases array") {
        let types = case["supported_test_types"]
            .as_array()
            .expect("supported_test_types");
        let type_set: BTreeSet<&str> = types
            .iter()
            .map(|v| v.as_str().expect("test type string"))
            .collect();

        assert!(
            type_set.contains("unit"),
            "{} must support unit tests",
            case["case_id"]
        );
        assert!(
            type_set.contains("scenario_real_service"),
            "{} must support real scenario tests",
            case["case_id"]
        );
        assert!(
            !type_set.contains("scenario_fixture"),
            "{} must not define fixture scenarios",
            case["case_id"]
        );
        assert!(
            !type_set.contains("e2e_fixture"),
            "{} must not define fixture e2e tests",
            case["case_id"]
        );
    }
}

#[test]
fn real_scenario_cases_have_query_plan_inputs() {
    let catalog = catalog();

    for case in catalog["cases"].as_array().expect("cases array") {
        let case_id = case["case_id"].as_str().expect("case_id");
        let query_plan = case
            .get("query_plan")
            .unwrap_or_else(|| panic!("{} missing query_plan", case_id));
        let description = query_plan["description"]
            .as_str()
            .unwrap_or_else(|| panic!("{} missing query_plan.description", case_id));
        let required = query_plan["required_image_count"]
            .as_u64()
            .unwrap_or_else(|| panic!("{} missing query_plan.required_image_count", case_id));

        assert!(
            !description.trim().is_empty(),
            "{} query description must not be empty",
            case_id
        );
        assert!(required >= 1, "{} required count must be >= 1", case_id);
    }
}

#[test]
fn baseline_query_plans_do_not_encode_source_or_license_requirements() {
    let catalog = catalog();
    let forbidden_terms = [
        "public domain",
        "license",
        "licensed",
        "copyright",
        "royalty",
        "stock photo",
        "source diversity",
        "high quality",
        "high resolution",
        "distinct photos",
        "different photos",
        "multiple photos",
        "multiple images",
        "several photos",
        "photos of",
        "images of",
        "pictures of",
    ];

    for case in catalog["cases"].as_array().expect("cases array") {
        let case_id = case["case_id"].as_str().expect("case_id");
        let query_plan = case
            .get("query_plan")
            .unwrap_or_else(|| panic!("{} missing query_plan", case_id));
        for field in [
            "quality",
            "quality_requirements",
            "material_types",
            "source_diversity_requirement",
            "authorization_preference",
            "output_preference",
            "provider_policy",
            "retrieval_policy",
            "retry_limit",
        ] {
            assert!(
                query_plan.get(field).is_none(),
                "{} QueryPlan must not contain non-content requirement field '{}'",
                case_id,
                field
            );
        }
        let serialized = serde_json::to_string(query_plan).expect("serialize query plan");
        let lower = serialized.to_lowercase();

        for term in forbidden_terms {
            assert!(
                !lower.contains(term),
                "{} QueryPlan must not encode source/license requirement '{}'",
                case_id,
                term
            );
        }
    }
}
