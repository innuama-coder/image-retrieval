use std::path::PathBuf;
use std::process::Command;

fn binary() -> PathBuf {
    std::env::var("CARGO_BIN_EXE_image-retrieval")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("target/debug/image-retrieval"))
}

fn temp_dir(prefix: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("cli-v11-{}-{}-{}", prefix, std::process::id(), n));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn fixture_run_exhausts_initial_attempt_plus_three_retries_before_blocking() {
    let output_dir = temp_dir("fixture-retry");
    let output = Command::new(binary())
        .args([
            "run",
            "--query-plan",
            "tests/fixtures/v1_1/query-plans/query-plan-basic.json",
            "--config",
            "tests/fixtures/v1_1/configs/config-fixture.toml",
            "--output-dir",
            output_dir.to_str().unwrap(),
            "--mode",
            "fixture",
            "--format",
            "json",
        ])
        .output()
        .expect("run CLI");

    assert_eq!(output.status.code(), Some(6));
    let stdout = String::from_utf8(output.stdout).unwrap();
    let value: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(value["status"], "blocked");
    assert_eq!(value["full_attempt_count"], 4);
    assert_eq!(value["retry_count"], 3);
}

#[test]
fn run_rewrites_package_validation_report_with_actual_validator_result() {
    let output_dir = temp_dir("validation-report");
    let output = Command::new(binary())
        .args([
            "run",
            "--query-plan",
            "tests/fixtures/v1_1/query-plans/query-plan-basic.json",
            "--config",
            "tests/fixtures/v1_1/configs/config-fixture.toml",
            "--output-dir",
            output_dir.to_str().unwrap(),
            "--mode",
            "fixture",
            "--format",
            "json",
        ])
        .output()
        .expect("run CLI");

    assert_eq!(output.status.code(), Some(6));
    let stdout = String::from_utf8(output.stdout).unwrap();
    let outcome: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let package_dir = outcome["package_dir"].as_str().expect("package_dir");
    let validation: serde_json::Value = serde_json::from_slice(
        &std::fs::read(PathBuf::from(package_dir).join("validation.json"))
            .expect("read validation report"),
    )
    .expect("parse validation report");

    assert_eq!(validation["status"], outcome["validation_status"]);
    assert!(!validation["file_checks"]
        .as_array()
        .expect("file checks")
        .is_empty());
    assert_eq!(validation["package_dir"], package_dir);
}

#[test]
fn run_defaults_to_production_and_requires_config() {
    let output_dir = temp_dir("default-production");
    let missing_config = output_dir.join("missing-config.toml");
    let output = Command::new(binary())
        .args([
            "run",
            "--query-plan",
            "tests/fixtures/v1_1/query-plans/query-plan-basic.json",
            "--config",
            missing_config.to_str().unwrap(),
            "--output-dir",
            output_dir.to_str().unwrap(),
            "--format",
            "json",
        ])
        .output()
        .expect("run CLI");

    assert_eq!(output.status.code(), Some(3));
    let stderr = String::from_utf8(output.stderr).unwrap();
    let value: serde_json::Value = serde_json::from_str(&stderr).unwrap();
    assert!(value["error"]
        .as_str()
        .unwrap()
        .contains("Cannot read runtime config"));
}

#[test]
fn self_check_uses_fixture_config_instead_of_hardcoded_serpapi_qwen_env() {
    let output = Command::new(binary())
        .args([
            "self-check",
            "--config",
            "tests/fixtures/v1_1/configs/config-fixture.toml",
            "--query-plan",
            "tests/fixtures/v1_1/query-plans/query-plan-basic.json",
            "--format",
            "json",
        ])
        .output()
        .expect("self-check CLI");

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8(output.stdout).unwrap();
    let value: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(value["status"], "ready");
    assert_eq!(
        value["search_provider_readiness"][0]["provider_id"],
        "fixture_search"
    );
    assert_eq!(value["vlm_readiness"]["provider_id"], "fixture_vlm");
}

#[test]
fn self_check_production_like_config_uses_runtime_qwen_defaults() {
    let output = Command::new(binary())
        .env("SERPAPI_API_KEY", "dummy-serpapi-key")
        .env("QWEN_API_KEY", "dummy-qwen-token")
        .args([
            "self-check",
            "--config",
            "tests/fixtures/v1_1/configs/config-production-like.toml",
            "--query-plan",
            "tests/fixtures/v1_1/query-plans/query-plan-basic.json",
            "--format",
            "json",
        ])
        .output()
        .expect("self-check CLI");

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8(output.stdout).unwrap();
    let value: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(value["status"], "ready");
    assert_eq!(value["vlm_readiness"]["credential_configured"], true);
    assert_eq!(value["vlm_readiness"]["endpoint_configured"], true);
}

#[test]
fn run_rejects_deprecated_allow_fixture_flag() {
    let output_dir = temp_dir("allow-fixture");
    let output = Command::new(binary())
        .args([
            "run",
            "--query-plan",
            "tests/fixtures/v1_1/query-plans/query-plan-basic.json",
            "--config",
            "tests/fixtures/v1_1/configs/config-fixture.toml",
            "--output-dir",
            output_dir.to_str().unwrap(),
            "--mode",
            "fixture",
            "--format",
            "json",
            "--allow-fixture",
        ])
        .output()
        .expect("run CLI");

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    let value: serde_json::Value = serde_json::from_str(&stderr).unwrap();
    assert!(value["error"]
        .as_str()
        .unwrap()
        .contains("--allow-fixture is deprecated"));
}

#[test]
fn self_check_uses_real_provider_registry_for_missing_adapter() {
    let dir = temp_dir("missing-adapter");
    std::fs::create_dir_all(&dir).unwrap();
    let config = dir.join("config.toml");
    std::fs::write(
        &config,
        r#"
[policy]
allow_paid_channels = false
respect_robots = true
allow_login_required_sources = false
allow_paywalled_sources = false
robots_unknown_behavior = "warn"

[[providers]]
provider_id = "custom_without_adapter"
provider_kind = { custom = "not_installed" }
enabled = true
weight = 1

[[retrieval_channels]]
channel_id = "normal_web_fetch"
channel_kind = "normal_web_fetch"
tier = "normal_web_fetch"
enabled = true

[vlm_evaluation]
provider_id = "fixture_vlm"
provider_kind = "fixture"
enabled = true
fixture_mode = true
"#,
    )
    .unwrap();

    let output = Command::new(binary())
        .args([
            "self-check",
            "--config",
            config.to_str().unwrap(),
            "--query-plan",
            "tests/fixtures/v1_1/query-plans/query-plan-basic.json",
            "--format",
            "json",
        ])
        .output()
        .expect("self-check CLI");

    assert_eq!(output.status.code(), Some(4));
    let stdout = String::from_utf8(output.stdout).unwrap();
    let value: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(value["status"], "blocked");
    assert_eq!(
        value["search_provider_readiness"][0]["provider_id"],
        "custom_without_adapter"
    );
    assert_eq!(
        value["search_provider_readiness"][0]["readiness"],
        "not_ready"
    );
    assert!(value["search_provider_readiness"][0]["message"]
        .as_str()
        .unwrap()
        .contains("No adapter registered"));
}

#[test]
fn validate_package_defaults_to_production_and_rejects_fixture_pass_package() {
    let output = Command::new(binary())
        .args([
            "validate-package",
            "--package-dir",
            "tests/fixtures/v1_1/packages/passed_minimal",
            "--format",
            "json",
        ])
        .output()
        .expect("validate-package CLI");

    assert_eq!(output.status.code(), Some(7));
    let stdout = String::from_utf8(output.stdout).unwrap();
    let value: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(value["status"], "fail");
    assert!(value["issues"]
        .as_array()
        .unwrap()
        .iter()
        .any(|issue| { issue["code"] == "PKG_FIXTURE_USED_IN_PRODUCTION_PASS" }));
}

#[test]
fn validate_package_fixture_mode_allows_fixture_pass_package() {
    let output = Command::new(binary())
        .args([
            "validate-package",
            "--package-dir",
            "tests/fixtures/v1_1/packages/passed_minimal",
            "--execution-mode",
            "fixture",
            "--format",
            "json",
        ])
        .output()
        .expect("validate-package CLI");

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8(output.stdout).unwrap();
    let value: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(value["status"], "pass");
}
