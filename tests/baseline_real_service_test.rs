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

fn value_u64(value: &serde_json::Value, field: &str) -> Result<u64, String> {
    value
        .get(field)
        .and_then(|v| v.as_u64())
        .ok_or_else(|| format!("{} missing numeric field '{}'", value, field))
}

fn value_str<'a>(value: &'a serde_json::Value, field: &str) -> Result<&'a str, String> {
    value
        .get(field)
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("{} missing string field '{}'", value, field))
}

fn evaluate_real_service_thresholds(
    case: &serde_json::Value,
    exit_code: i32,
    parsed: &serde_json::Value,
) -> Result<(), String> {
    let case_id = case["case_id"].as_str().unwrap_or("<unknown>");
    let thresholds = case
        .get("expected_metrics")
        .and_then(|m| m.get("scenario_real_service"))
        .ok_or_else(|| format!("{} missing expected_metrics.scenario_real_service", case_id))?;

    if let Some(expected_exit) = thresholds.get("cli_exit_code").and_then(|v| v.as_i64()) {
        if exit_code as i64 != expected_exit {
            return Err(format!(
                "{} exit code {} did not match expected {}",
                case_id, exit_code, expected_exit
            ));
        }
    }

    if let Some(expected_status) = thresholds.get("status").and_then(|v| v.as_str()) {
        let actual_status = value_str(parsed, "status")?;
        if actual_status != expected_status {
            return Err(format!(
                "{} status '{}' did not match expected '{}'",
                case_id, actual_status, expected_status
            ));
        }
    }

    let query_required = case
        .get("query_plan")
        .and_then(|q| q.get("required_image_count"))
        .and_then(|v| v.as_u64())
        .unwrap_or(1);
    let accepted_min = thresholds
        .get("accepted_count_min")
        .and_then(|v| v.as_u64())
        .unwrap_or(query_required);
    let accepted = value_u64(parsed, "accepted_image_count")?;
    if accepted < accepted_min {
        return Err(format!(
            "{} accepted_image_count {} was below threshold {}",
            case_id, accepted, accepted_min
        ));
    }

    let reported_required = value_u64(parsed, "required_image_count")?;
    if reported_required != query_required {
        return Err(format!(
            "{} required_image_count {} did not match QueryPlan required count {}",
            case_id, reported_required, query_required
        ));
    }

    if thresholds
        .get("package_validation_passed")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        let validation_status = value_str(parsed, "validation_status")?;
        if validation_status != "pass" {
            return Err(format!(
                "{} validation_status '{}' did not match expected 'pass'",
                case_id, validation_status
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod threshold_tests {
    use super::*;

    fn case_with_scenario_thresholds() -> serde_json::Value {
        json!({
            "case_id": "threshold-case",
            "query_plan": {
                "description": "red apple on white background",
                "required_image_count": 2
            },
            "expected_metrics": {
                "scenario_real_service": {
                    "cli_exit_code": 0,
                    "status": "passed",
                    "accepted_count_min": 2,
                    "package_validation_passed": true
                }
            }
        })
    }

    #[test]
    fn real_service_thresholds_reject_failed_cli_exit() {
        let parsed = json!({
            "status": "passed",
            "accepted_image_count": 2,
            "required_image_count": 2,
            "validation_status": "pass"
        });

        let result = evaluate_real_service_thresholds(&case_with_scenario_thresholds(), 2, &parsed);

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("exit code"));
    }

    #[test]
    fn real_service_thresholds_reject_insufficient_delivery() {
        let parsed = json!({
            "status": "passed",
            "accepted_image_count": 1,
            "required_image_count": 2,
            "validation_status": "pass"
        });

        let result = evaluate_real_service_thresholds(&case_with_scenario_thresholds(), 0, &parsed);

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("accepted_image_count"));
    }

    #[test]
    fn real_service_thresholds_reject_package_validation_failure() {
        let parsed = json!({
            "status": "passed",
            "accepted_image_count": 2,
            "required_image_count": 2,
            "validation_status": "fail"
        });

        let result = evaluate_real_service_thresholds(&case_with_scenario_thresholds(), 0, &parsed);

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("validation_status"));
    }

    #[test]
    fn real_service_thresholds_accept_valid_delivery() {
        let parsed = json!({
            "status": "passed",
            "accepted_image_count": 2,
            "required_image_count": 2,
            "validation_status": "pass"
        });

        let result = evaluate_real_service_thresholds(&case_with_scenario_thresholds(), 0, &parsed);

        assert!(result.is_ok());
    }
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
        let parsed = parsed.expect("parseable CLI output");
        let exit_code = output.status.code().unwrap_or(-1);
        evaluate_real_service_thresholds(case, exit_code, parsed).unwrap_or_else(|e| {
            panic!(
                "{} real CLI run did not meet baseline thresholds: {}; see {}",
                case_id,
                e,
                case_dir.display()
            )
        });

        case_results.push(json!({
            "case_id": case_id,
            "unit": case["unit"],
            "test_type": "scenario_real_service",
            "command_exit_code": exit_code,
            "stdout_json_parseable": parsed_stdout.is_some(),
            "stderr_json_parseable": parsed_stderr.is_some(),
            "threshold_status": "pass",
            "status": parsed.get("status").cloned(),
            "accepted_image_count": parsed.get("accepted_image_count").cloned(),
            "required_image_count": parsed.get("required_image_count").cloned(),
            "validation_status": parsed.get("validation_status").cloned(),
            "package_dir": parsed.get("package_dir").cloned(),
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
