use serde_json::json;
use std::path::{Path, PathBuf};
use std::process::Command;

fn binary() -> PathBuf {
    std::env::var("CARGO_BIN_EXE_image-retrieval")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("target/debug/image-retrieval"))
}

fn catalog() -> serde_json::Value {
    serde_json::from_str(include_str!("fixtures/v1_1/baseline/case-catalog.json"))
        .expect("baseline case catalog must be valid JSON")
}

fn opt_in_enabled() -> bool {
    std::env::var("IMAGE_RETRIEVAL_REAL_BASELINE")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn selected_case_ids() -> Option<Vec<String>> {
    std::env::var("IMAGE_RETRIEVAL_BASELINE_CASES")
        .ok()
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(ToOwned::to_owned)
                .collect()
        })
        .filter(|ids: &Vec<String>| !ids.is_empty())
}

fn report_dir() -> PathBuf {
    std::env::var("IMAGE_RETRIEVAL_BASELINE_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("target/baseline-reports"))
}

fn config_path() -> PathBuf {
    std::env::var("IMAGE_RETRIEVAL_BASELINE_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from("tests/fixtures/v1_1/configs/config-production-like.toml")
        })
}

fn require_env(name: &str) {
    let value = std::env::var(name).unwrap_or_default();
    assert!(
        !value.trim().is_empty(),
        "{} must be configured when IMAGE_RETRIEVAL_REAL_BASELINE=1",
        name
    );
}

fn assert_config_is_real(path: &Path) {
    let content = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("cannot read baseline config {}: {}", path.display(), e));
    let lower = content.to_lowercase();

    assert!(
        !lower.contains("provider_kind = \"fixture\""),
        "real baseline config must not use fixture providers or evaluators"
    );
    assert!(
        !lower.contains("provider_id = \"fixture_vlm\""),
        "real baseline config must not use the fixture VLM evaluator"
    );
    assert!(
        !lower.contains("fixture_mode = true"),
        "real baseline config must not enable fixture mode"
    );
}

#[test]
fn baseline_real_service_scenarios_execute_real_cli_when_opted_in() {
    if !opt_in_enabled() {
        eprintln!(
            "skipping real baseline scenarios; set IMAGE_RETRIEVAL_REAL_BASELINE=1 to execute"
        );
        return;
    }

    require_env("SERPAPI_API_KEY");
    require_env("QWEN_API_KEY");

    let config = config_path();
    assert_config_is_real(&config);

    let catalog = catalog();
    let selected = selected_case_ids();
    let run_id = format!(
        "baseline-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time")
            .as_secs()
    );
    let report_root = report_dir();
    let run_root = report_root.join(&run_id);
    std::fs::create_dir_all(&run_root).expect("create baseline report dir");

    let mut case_results = Vec::new();
    let mut executed_count = 0usize;

    for case in catalog["cases"].as_array().expect("cases array") {
        let case_id = case["case_id"].as_str().expect("case_id");
        if let Some(selected) = &selected {
            if !selected.iter().any(|id| id == case_id) {
                continue;
            }
        }

        let types = case["supported_test_types"]
            .as_array()
            .expect("supported_test_types");
        if !types
            .iter()
            .any(|v| v.as_str() == Some("scenario_real_service"))
        {
            continue;
        }

        executed_count += 1;
        let case_dir = run_root.join(case_id);
        let output_dir = case_dir.join("output");
        std::fs::create_dir_all(&case_dir).expect("create case dir");
        let query_plan_path = case_dir.join("query-plan.json");
        let query_plan = case
            .get("query_plan")
            .unwrap_or_else(|| panic!("{} missing query_plan", case_id));
        std::fs::write(
            &query_plan_path,
            serde_json::to_vec_pretty(query_plan).expect("serialize query plan"),
        )
        .expect("write query plan");

        let output = Command::new(binary())
            .args([
                "run",
                "--query-plan",
                query_plan_path.to_str().expect("query plan path"),
                "--config",
                config.to_str().expect("config path"),
                "--output-dir",
                output_dir.to_str().expect("output dir"),
                "--mode",
                "production",
                "--format",
                "json",
            ])
            .output()
            .unwrap_or_else(|e| panic!("run real baseline CLI for {}: {}", case_id, e));

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let parsed_stdout = serde_json::from_str::<serde_json::Value>(&stdout).ok();
        let parsed_stderr = serde_json::from_str::<serde_json::Value>(&stderr).ok();
        let parsed = parsed_stdout.as_ref().or(parsed_stderr.as_ref());

        std::fs::write(case_dir.join("stdout.json"), &stdout).expect("write stdout");
        std::fs::write(case_dir.join("stderr.json"), &stderr).expect("write stderr");

        assert!(
            parsed.is_some(),
            "{} real CLI run did not produce parseable JSON; see {}",
            case_id,
            case_dir.display()
        );

        case_results.push(json!({
            "case_id": case_id,
            "unit": case["unit"],
            "test_type": "scenario_real_service",
            "command_exit_code": output.status.code(),
            "stdout_json_parseable": parsed_stdout.is_some(),
            "stderr_json_parseable": parsed_stderr.is_some(),
            "status": parsed.and_then(|v| v.get("status")).cloned(),
            "accepted_image_count": parsed.and_then(|v| v.get("accepted_image_count")).cloned(),
            "required_image_count": parsed.and_then(|v| v.get("required_image_count")).cloned(),
            "validation_status": parsed.and_then(|v| v.get("validation_status")).cloned(),
            "package_dir": parsed.and_then(|v| v.get("package_dir")).cloned(),
            "stdout_path": case_dir.join("stdout.json").display().to_string(),
            "stderr_path": case_dir.join("stderr.json").display().to_string(),
            "output_dir": output_dir.display().to_string()
        }));
    }

    assert!(
        executed_count > 0,
        "no baseline cases executed; check IMAGE_RETRIEVAL_BASELINE_CASES"
    );

    let report = json!({
        "schema_version": 1,
        "suite_id": "baseline_recall_v1_1",
        "run_id": run_id,
        "execution_mode": "real_service",
        "config_path": config.display().to_string(),
        "case_count": executed_count,
        "cases": case_results,
        "notes": [
            "Scenario tests use real CLI execution and real configured services.",
            "Public web results are threshold/trend evidence, not exact fixture assertions."
        ]
    });

    let report_path = report_root.join(format!("{}-baseline-v1.1.json", run_id));
    std::fs::write(
        &report_path,
        serde_json::to_vec_pretty(&report).expect("serialize baseline report"),
    )
    .expect("write baseline report");

    assert!(
        report_path.exists(),
        "baseline report should be written to {}",
        report_path.display()
    );
}
