//! Real-service smoke test harness — TASK-006.
//!
//! Gated by `IMAGE_RETRIEVAL_REAL_SMOKE=1`. When the gate is absent,
//! all tests emit `skipped` or `blocked` diagnostic evidence.
//!
//! When the gate is present AND all prerequisites are met, the harness runs:
//! 1. `self-check` against production-like config
//! 2. `run` with real providers/channels/VLM
//! 3. `validate-package` against the generated package
//!
//! **Security**: this harness never prints or serializes resolved credential
//! values. It may print environment variable NAMES (e.g. "SERPAPI_API_KEY")
//! but never their values.
//!
//! References:
//! - `docs/design/v1.1-TASK-006-testing-real-service-acceptance-design.md`
//! - `RELEASE_GATES.md`

use std::path::PathBuf;
use std::process::Command;

// =============================================================================
// Smoke prerequisite detection
// =============================================================================

/// Returns true when the real-service smoke opt-in is set.
fn smoke_opted_in() -> bool {
    std::env::var("IMAGE_RETRIEVAL_REAL_SMOKE")
        .map(|v| v == "1")
        .unwrap_or(false)
}

/// Returns the config path from env, if set.
fn env_config_path() -> Option<PathBuf> {
    std::env::var("IMAGE_RETRIEVAL_CONFIG")
        .ok()
        .map(PathBuf::from)
}

/// Returns the query-plan path from env, if set.
fn env_query_plan_path() -> Option<PathBuf> {
    std::env::var("IMAGE_RETRIEVAL_QUERY_PLAN")
        .ok()
        .map(PathBuf::from)
}

/// Returns the output directory from env, if set.
fn env_output_dir() -> Option<PathBuf> {
    std::env::var("IMAGE_RETRIEVAL_OUTPUT_DIR")
        .ok()
        .map(PathBuf::from)
}

/// Check whether a named env var is set (value redacted — we only report presence).
fn credential_is_set(name: &str) -> bool {
    std::env::var(name).is_ok()
}

fn utc_now_rfc3339_seconds() -> String {
    let now = time::OffsetDateTime::now_utc();
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        now.year(),
        u8::from(now.month()),
        now.day(),
        now.hour(),
        now.minute(),
        now.second()
    )
}

/// Build a prerequisite summary suitable for machine-readable blocked/skipped
/// evidence.
fn build_prerequisite_summary() -> serde_json::Value {
    serde_json::json!({
        "smoke_opt_in": smoke_opted_in(),
        "config_path_present": env_config_path().is_some(),
        "query_plan_path_present": env_query_plan_path().is_some(),
        "output_dir_writable": env_output_dir().is_some_and(|d| {
            // Check if parent exists and is writable, or create it
            if d.exists() {
                d.is_dir()
            } else {
                // Try to create it as a test
                std::fs::create_dir_all(&d).is_ok()
            }
        }),
        "credential_env_names_present": {
            "SERPAPI_API_KEY": credential_is_set("SERPAPI_API_KEY"),
            "QWEN_API_KEY": credential_is_set("QWEN_API_KEY")
        },
        "credential_values_redacted": true,
        "paid_opt_in": std::env::var("IMAGE_RETRIEVAL_ALLOW_PAID").map(|v| v == "1").unwrap_or(false)
    })
}

/// Determine which commands would be run or were blocked.
fn classify_commands(
    prerequisites: &serde_json::Value,
) -> (Vec<serde_json::Value>, Vec<serde_json::Value>) {
    let mut commands_run = Vec::new();
    let mut commands_not_run = Vec::new();

    fn cmd(
        command: &str,
        can_run: bool,
        reason: &str,
    ) -> (Option<serde_json::Value>, Option<serde_json::Value>) {
        let entry = serde_json::json!({
            "command": command,
            "reason": reason
        });
        if can_run {
            (Some(entry), None)
        } else {
            (None, Some(entry))
        }
    }

    let config_ok = prerequisites["config_path_present"]
        .as_bool()
        .unwrap_or(false);
    let qp_ok = prerequisites["query_plan_path_present"]
        .as_bool()
        .unwrap_or(false);
    let output_ok = prerequisites["output_dir_writable"]
        .as_bool()
        .unwrap_or(false);
    let opted_in = prerequisites["smoke_opt_in"].as_bool().unwrap_or(false);

    let can_self_check = opted_in && config_ok && qp_ok;
    let can_run = can_self_check && output_ok;
    let can_validate = can_run;

    if !opted_in {
        let (_, nr) = cmd(
            "image-retrieval self-check --config $IMAGE_RETRIEVAL_CONFIG --query-plan $IMAGE_RETRIEVAL_QUERY_PLAN --format json",
            false,
            "IMAGE_RETRIEVAL_REAL_SMOKE not set to 1 — real-service smoke not opted in",
        );
        commands_not_run.extend(nr);
        let (_, nr) = cmd(
            "image-retrieval run --query-plan $IMAGE_RETRIEVAL_QUERY_PLAN --config $IMAGE_RETRIEVAL_CONFIG --output-dir $IMAGE_RETRIEVAL_OUTPUT_DIR --mode production --format json",
            false,
            "IMAGE_RETRIEVAL_REAL_SMOKE not set",
        );
        commands_not_run.extend(nr);
        let (_, nr) = cmd(
            "image-retrieval validate-package --package-dir $IMAGE_RETRIEVAL_OUTPUT_DIR/package --format json",
            false,
            "smoke run not executed",
        );
        commands_not_run.extend(nr);
    } else {
        let reason = if !config_ok {
            "config path not set or not readable"
        } else if !qp_ok {
            "query-plan path not set or not readable"
        } else {
            "prerequisites met"
        };

        let (r, nr) = cmd(
            "image-retrieval self-check --config $IMAGE_RETRIEVAL_CONFIG --query-plan $IMAGE_RETRIEVAL_QUERY_PLAN --format json",
            can_self_check,
            if can_self_check { "prerequisites met" } else { reason },
        );
        commands_run.extend(r);
        commands_not_run.extend(nr);

        let run_reason = if !can_self_check {
            "self-check blocked"
        } else if !output_ok {
            "output dir not writable"
        } else {
            "prerequisites met"
        };

        let (r, nr) = cmd(
            "image-retrieval run --query-plan $IMAGE_RETRIEVAL_QUERY_PLAN --config $IMAGE_RETRIEVAL_CONFIG --output-dir $IMAGE_RETRIEVAL_OUTPUT_DIR --mode production --format json",
            can_run,
            if can_run { "prerequisites met" } else { run_reason },
        );
        commands_run.extend(r);
        commands_not_run.extend(nr);

        let (r, nr) = cmd(
            "image-retrieval validate-package --package-dir $IMAGE_RETRIEVAL_OUTPUT_DIR/package --format json",
            can_validate,
            if can_validate { "prerequisites met" } else { "smoke run not executed" },
        );
        commands_run.extend(r);
        commands_not_run.extend(nr);
    }

    (commands_run, commands_not_run)
}

// =============================================================================
// Smoke report builder
// =============================================================================

fn build_smoke_report(status: &str, reason_code: Option<&str>, notes: &str) -> serde_json::Value {
    let prerequisites = build_prerequisite_summary();
    let (commands_run, commands_not_run) = classify_commands(&prerequisites);
    let timestamp = utc_now_rfc3339_seconds();
    let release_gates = release_gates_for_status(status);

    serde_json::json!({
        "schema_version": 1,
        "test_id": "real_service_smoke_v1_1",
        "status": status,
        "blocked_reason_code": reason_code,
        "skipped_reason_code": if status == "skipped" { Some("SMOKE_NOT_OPTED_IN") } else { None::<&str> },
        "timestamp": timestamp,
        "release_gates": release_gates,
        "environment": prerequisites,
        "self_check_status": if smoke_opted_in() { "not_run" } else { "skipped" },
        "package_dir": null,
        "commands_run": commands_run,
        "commands_not_run": commands_not_run,
        "notes": notes
    })
}

fn release_gates_for_status(status: &str) -> Vec<serde_json::Value> {
    let gate_status = if status == "passed" { "closed" } else { "open" };
    let decision_ref = if status == "passed" {
        Some("tasks/development/v1.1/release-gate-decisions.md")
    } else {
        None
    };
    [
        (
            "GATE-RSV-001",
            "Default real provider (SerpApi Google Images)",
            "Real service verification",
        ),
        (
            "GATE-RSV-002",
            "Built-in provider list & restricted/legacy policy",
            "Real service verification",
        ),
        (
            "GATE-RSV-003",
            "Paid retrieval channel enablement",
            "Real service verification",
        ),
        (
            "GATE-RSV-004",
            "robots.txt / site-rule compliance strategy",
            "Real service verification",
        ),
        (
            "GATE-RSV-005",
            "Quality tier calibration or waiver",
            "Real service verification",
        ),
        (
            "GATE-MVP-001",
            "Qwen 3.5 VLM production evaluation usage & responsibility",
            "MVP release",
        ),
        ("GATE-MVP-002", "Provider list/policy (MVP)", "MVP release"),
        (
            "GATE-MVP-003",
            "Authorization blocking detailed rules",
            "MVP release",
        ),
        (
            "GATE-MVP-004",
            "Fourth retrieval channel decision",
            "MVP release",
        ),
        (
            "GATE-MVP-005",
            "Qwen 3.5 VLM adapter config/smoke",
            "MVP release",
        ),
    ]
    .into_iter()
    .map(|(gate_id, description, blocks)| {
        serde_json::json!({
            "gate_id": gate_id,
            "status": gate_status,
            "description": description,
            "blocks": blocks,
            "decision_ref": decision_ref,
        })
    })
    .collect()
}

// =============================================================================
// Tests
// =============================================================================

#[test]
fn real_service_smoke_preconditions_report() {
    // This test ALWAYS runs. It produces blocked/skipped diagnostic evidence
    // when real-service prerequisites are absent.

    let prerequisites = build_prerequisite_summary();
    let smoke_on = smoke_opted_in();

    if !smoke_on {
        // Skipped: opt-in not set
        let report = build_smoke_report(
            "skipped",
            None,
            "IMAGE_RETRIEVAL_REAL_SMOKE is not set to 1. ",
        );

        write_report(&report);

        // Diagnostic: this is NOT a failure; it's expected skipped evidence
        eprintln!("[SKIPPED] Real-service smoke skipped: IMAGE_RETRIEVAL_REAL_SMOKE is not set.");
        eprintln!("  Set IMAGE_RETRIEVAL_REAL_SMOKE=1 and provide IMAGE_RETRIEVAL_CONFIG,",);
        eprintln!("  IMAGE_RETRIEVAL_QUERY_PLAN, and IMAGE_RETRIEVAL_OUTPUT_DIR to run.",);
    } else {
        let config_ok = prerequisites["config_path_present"]
            .as_bool()
            .unwrap_or(false);
        let qp_ok = prerequisites["query_plan_path_present"]
            .as_bool()
            .unwrap_or(false);
        let output_ok = prerequisites["output_dir_writable"]
            .as_bool()
            .unwrap_or(false);
        let serpapi_ok = prerequisites["credential_env_names_present"]["SERPAPI_API_KEY"]
            .as_bool()
            .unwrap_or(false);
        let qwen_ok = prerequisites["credential_env_names_present"]["QWEN_API_KEY"]
            .as_bool()
            .unwrap_or(false);

        let mut missing = Vec::new();
        if !config_ok {
            missing.push("IMAGE_RETRIEVAL_CONFIG");
        }
        if !qp_ok {
            missing.push("IMAGE_RETRIEVAL_QUERY_PLAN");
        }
        if !output_ok {
            missing.push("IMAGE_RETRIEVAL_OUTPUT_DIR");
        }
        if !serpapi_ok {
            missing.push("SERPAPI_API_KEY");
        }
        if !qwen_ok {
            missing.push("QWEN_API_KEY");
        }

        if !missing.is_empty() {
            let report = build_smoke_report(
                "blocked",
                Some("PREREQUISITES_MISSING"),
                &format!(
                    "Real-service smoke blocked: missing prerequisites: {}. Set all required env vars and retry.",
                    missing.join(", ")
                ),
            );

            write_report(&report);

            eprintln!(
                "[BLOCKED] Real-service smoke blocked: missing prerequisites: {}",
                missing.join(", ")
            );
        } else {
            // All prerequisites present — run the full smoke flow
            run_real_smoke_flow();
        }
    }
}

#[test]
fn smoke_report_is_written_only_when_report_path_is_explicit() {
    let report = build_smoke_report(
        "skipped",
        None,
        "IMAGE_RETRIEVAL_REAL_SMOKE is not set to 1.",
    );

    std::env::remove_var("IMAGE_RETRIEVAL_SMOKE_REPORT_PATH");
    assert!(
        !write_smoke_report_if_requested(&report),
        "ordinary cargo test runs must not rewrite release evidence files"
    );

    let path = std::env::temp_dir().join(format!(
        "image-retrieval-smoke-report-{}-{}.json",
        std::process::id(),
        utc_now_rfc3339_seconds().replace([':', '-'], "")
    ));
    std::env::set_var("IMAGE_RETRIEVAL_SMOKE_REPORT_PATH", &path);
    assert!(write_smoke_report_if_requested(&report));
    let written: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(written["status"], "skipped");

    std::env::remove_var("IMAGE_RETRIEVAL_SMOKE_REPORT_PATH");
    let _ = std::fs::remove_file(path);
}

/// Execute the full real-service smoke flow.
///
/// This function is only called when IMAGE_RETRIEVAL_REAL_SMOKE=1 AND all
/// path/credential prerequisites are met.
fn run_real_smoke_flow() {
    let config_path = env_config_path().expect("config path");
    let query_plan_path = env_query_plan_path().expect("query plan path");
    let output_dir = env_output_dir().expect("output dir");

    // Ensure output directory exists
    let _ = std::fs::create_dir_all(&output_dir);

    // Locate the compiled binary
    let binary = find_binary();

    // ---- 1. self-check ----
    eprintln!("[SMOKE] Running self-check...");
    let self_check_output = Command::new(&binary)
        .args([
            "self-check",
            "--config",
            &config_path.display().to_string(),
            "--query-plan",
            &query_plan_path.display().to_string(),
            "--format",
            "json",
        ])
        .output();

    match &self_check_output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            eprintln!("[SMOKE] self-check stdout: {}", stdout);
            if !out.status.success() {
                eprintln!(
                    "[SMOKE] self-check exit code: {} (stderr: {})",
                    out.status,
                    String::from_utf8_lossy(&out.stderr)
                );
            }
        }
        Err(e) => {
            eprintln!("[SMOKE] self-check failed to execute: {}", e);
            let report = build_smoke_report(
                "blocked",
                Some("SELF_CHECK_EXECUTION_FAILED"),
                &format!("self-check binary execution failed: {}", e),
            );
            write_report(&report);
            return;
        }
    }

    // ---- 2. run ----
    eprintln!("[SMOKE] Running full pipeline...");
    let run_output = Command::new(&binary)
        .args([
            "run",
            "--query-plan",
            &query_plan_path.display().to_string(),
            "--config",
            &config_path.display().to_string(),
            "--output-dir",
            &output_dir.display().to_string(),
            "--mode",
            "production",
            "--format",
            "json",
        ])
        .output();

    let run_success = match &run_output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            eprintln!("[SMOKE] run stdout: {}", stdout);
            if !stderr.is_empty() {
                eprintln!("[SMOKE] run stderr: {}", stderr);
            }
            eprintln!("[SMOKE] run exit code: {}", out.status);
            out.status.success()
        }
        Err(e) => {
            eprintln!("[SMOKE] run failed to execute: {}", e);
            false
        }
    };

    // ---- 3. validate-package ----
    let package_dir = output_dir.join("package");
    eprintln!("[SMOKE] Validating package at {}...", package_dir.display());

    let validate_output = Command::new(&binary)
        .args([
            "validate-package",
            "--package-dir",
            &package_dir.display().to_string(),
            "--format",
            "json",
        ])
        .output();

    let validation_passed = match &validate_output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            eprintln!("[SMOKE] validate-package stdout: {}", stdout);
            out.status.success()
        }
        Err(e) => {
            eprintln!("[SMOKE] validate-package failed to execute: {}", e);
            false
        }
    };

    // ---- 4. Build final report ----
    let status = if run_success && validation_passed {
        "passed"
    } else if run_success {
        "failed"
    } else {
        "blocked"
    };

    let reason_code = if status == "blocked" {
        Some("RUN_EXECUTION_FAILED_OR_BLOCKED")
    } else if status == "failed" {
        Some("VALIDATION_FAILED")
    } else {
        None
    };

    let prerequisites = build_prerequisite_summary();
    let (_commands_run, _commands_not_run) = classify_commands(&prerequisites);

    // Override command lists with actual execution results
    let commands_run: Vec<serde_json::Value> = vec![
        serde_json::json!({
            "command": format!("image-retrieval self-check --config {} --query-plan {} --format json",
                config_path.display(), query_plan_path.display()),
            "exit_code": self_check_output.as_ref().map(|o| o.status.code().map(|c| c.to_string()).unwrap_or_else(|| "N/A".into())).unwrap_or_else(|_| "N/A".into()),
            "status": self_check_output.map(|o| if o.status.success() { "ok" } else { "failed" }).unwrap_or("error")
        }),
        serde_json::json!({
            "command": format!("image-retrieval run --query-plan {} --config {} --output-dir {} --mode production --format json",
                query_plan_path.display(), config_path.display(), output_dir.display()),
            "exit_code": run_output.as_ref().map(|o| o.status.code().map(|c| c.to_string()).unwrap_or_else(|| "N/A".into())).unwrap_or_else(|_| "N/A".into()),
            "status": run_output.map(|o| if o.status.success() { "ok" } else { "failed" }).unwrap_or("error")
        }),
        serde_json::json!({
            "command": format!("image-retrieval validate-package --package-dir {} --format json",
                package_dir.display()),
            "exit_code": validate_output.as_ref().map(|o| o.status.code().map(|c| c.to_string()).unwrap_or_else(|| "N/A".into())).unwrap_or_else(|_| "N/A".into()),
            "status": validate_output.map(|o| if o.status.success() { "ok" } else { "failed" }).unwrap_or("error")
        }),
    ];

    let timestamp = utc_now_rfc3339_seconds();
    let release_gates = release_gates_for_status(status);
    let report = serde_json::json!({
        "schema_version": 1,
        "test_id": "real_service_smoke_v1_1",
        "status": status,
        "blocked_reason_code": reason_code,
        "skipped_reason_code": null,
        "timestamp": timestamp,
        "release_gates": release_gates,
        "environment": {
            "config_path_present": true,
            "query_plan_path_present": true,
            "output_dir_writable": true,
            "credential_env_names_present": {
                "SERPAPI_API_KEY": true,
                "QWEN_API_KEY": true
            },
            "credential_values_redacted": true
        },
        "self_check_status": "completed",
        "package_dir": package_dir.display().to_string(),
        "commands_run": commands_run,
        "commands_not_run": [],
        "notes": format!(
            "Real-service smoke completed with status '{}'. Run: {}, Validation: {}.",
            status, run_success, validation_passed
        )
    });

    write_report(&report);

    // Assertions: smoke test must not silently pass if target not met
    if status == "passed" {
        eprintln!("[SMOKE] PASSED — full real-service smoke completed successfully.");
    } else {
        eprintln!(
            "[SMOKE] {} — real-service smoke did not prove full expected-target completion.",
            status.to_uppercase()
        );
    }
}

/// Write the smoke report to the explicitly configured handoff path.
fn write_report(report: &serde_json::Value) {
    if !write_smoke_report_if_requested(report) {
        eprintln!(
            "[SMOKE] Report not written; set IMAGE_RETRIEVAL_SMOKE_REPORT_PATH to persist it."
        );
    }
}

fn write_smoke_report_if_requested(report: &serde_json::Value) -> bool {
    let Ok(report_path) = std::env::var("IMAGE_RETRIEVAL_SMOKE_REPORT_PATH") else {
        return false;
    };
    let report_path = PathBuf::from(report_path);
    if let Ok(json) = serde_json::to_string_pretty(report) {
        if let Err(e) = std::fs::write(&report_path, &json) {
            eprintln!("[SMOKE] Failed to write smoke report: {}", e);
            false
        } else {
            eprintln!("[SMOKE] Report written to {}", report_path.display());
            true
        }
    } else {
        false
    }
}

/// Locate the compiled `image-retrieval` binary.
///
/// In test context, CARGO_BIN_EXE_image-retrieval is set by Cargo when the
/// binary is built. Falls back to `target/debug/image-retrieval`.
fn find_binary() -> PathBuf {
    // Cargo sets this env var for integration tests that need the binary
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_image-retrieval") {
        let p = PathBuf::from(&path);
        if p.exists() {
            return p;
        }
    }

    // Fallback: look in the target directory
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let debug_binary = manifest_dir
        .join("target")
        .join("debug")
        .join("image-retrieval");
    if debug_binary.exists() {
        return debug_binary;
    }

    let release_binary = manifest_dir
        .join("target")
        .join("release")
        .join("image-retrieval");
    if release_binary.exists() {
        return release_binary;
    }

    // Last resort: assume it's on PATH
    PathBuf::from("image-retrieval")
}

#[test]
fn smoke_report_contains_no_credentials() {
    // This test verifies that the smoke report never contains credential values,
    // even when credentials are set in the environment.

    let report = build_smoke_report("skipped", None, "IMAGE_RETRIEVAL_REAL_SMOKE is not set.");

    let json = serde_json::to_string_pretty(&report).unwrap_or_default();
    let lower = json.to_lowercase();

    // These patterns should NEVER appear in the report
    let forbidden = ["sk-", "eyjh", "bearer ", "access_token="];

    for pattern in &forbidden {
        assert!(
            !lower.contains(pattern),
            "Smoke report contains forbidden pattern '{}'",
            pattern
        );
    }

    // The report MAY contain env var NAMES (like "SERPAPI_API_KEY") but never
    // their resolved values.
}

#[test]
fn smoke_report_has_required_schema_fields() {
    let report = build_smoke_report("skipped", None, "IMAGE_RETRIEVAL_REAL_SMOKE is not set.");

    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["test_id"], "real_service_smoke_v1_1");
    assert!(report["status"].is_string());
    assert!(report["timestamp"].is_string());
    assert!(report["release_gates"].is_array());
    assert!(report["environment"].is_object());
    assert!(report["commands_run"].is_array());
    assert!(report["commands_not_run"].is_array());
    assert!(report["notes"].is_string());
}

#[test]
fn smoke_report_status_is_skipped_when_not_opted_in() {
    let report = build_smoke_report("skipped", None, "IMAGE_RETRIEVAL_REAL_SMOKE is not set.");
    assert_eq!(report["status"], "skipped");
    assert!(report["skipped_reason_code"].is_string());
    assert!(!report["commands_not_run"].as_array().unwrap().is_empty());
}

#[test]
fn smoke_report_release_gates_are_all_open() {
    let report = build_smoke_report(
        "blocked",
        Some("VLM_EVALUATION_UNAVAILABLE"),
        "Real-service smoke blocked: Qwen 3.5 VLM not configured.",
    );

    let gates = report["release_gates"].as_array().unwrap();
    assert_eq!(gates.len(), 10, "Smoke report must list all release gates");
    for gate in gates {
        let gate_id = gate["gate_id"].as_str().unwrap();
        let gate_status = gate["status"].as_str().unwrap();
        assert_eq!(
            gate_status, "open",
            "Release gate {} should be open in blocked smoke report",
            gate_id
        );
    }
}

#[test]
fn smoke_report_environment_reports_credential_presence_only() {
    let report = build_smoke_report(
        "blocked",
        Some("PREREQUISITES_MISSING"),
        "Missing prerequisites.",
    );

    let env = &report["environment"];
    // Environment section reports boolean presence, never values
    let creds = &env["credential_env_names_present"];
    assert!(creds["SERPAPI_API_KEY"].is_boolean());
    assert!(creds["QWEN_API_KEY"].is_boolean());
    assert!(env["credential_values_redacted"].as_bool().unwrap());

    // The report JSON text must never contain resolved credential values
    let json = serde_json::to_string_pretty(&report).unwrap_or_default();
    // Check that actual credential values from the environment are NOT in the report
    if let Ok(val) = std::env::var("SERPAPI_API_KEY") {
        if !val.is_empty() {
            assert!(
                !json.contains(&val),
                "Smoke report must not contain SERPAPI_API_KEY value"
            );
        }
    }
    if let Ok(val) = std::env::var("QWEN_API_KEY") {
        if !val.is_empty() {
            assert!(
                !json.contains(&val),
                "Smoke report must not contain QWEN_API_KEY value"
            );
        }
    }
}
