//! image-retrieval v1.1 CLI entry point.
//!
//! Commands:
//! - `run`: execute the full image retrieval workflow — admission, search,
//!   candidate quality, retrieval, image acceptance, package build,
//!   validation, review, and handoff.
//! - `self-check`: v1.1 readiness checks for SerpApi, retrieval channels,
//!   Qwen 3.5 VLM, policy, output, credentials, validator, and release
//!   blockers.
//! - `validate-package`: deterministic package validation against the v1.1
//!   canonical package contract.
//! - `inspect-package`: read-only package inspection.
//!
//! Exit codes:
//! - 0: Success / passed
//! - 2: Input error
//! - 3: Config error
//! - 4: Readiness blocked
//! - 5: Partial delivery
//! - 6: Delivery blocked
//! - 7: Package validation failed
//! - 70: Internal error

use clap::{Parser, Subcommand};
use image_retrieval::delivery::build_canonical_package;
use image_retrieval::domain::config::{
    RetrievalChannelKind, RuntimeConfig, SearchProviderKind, VlmEvaluatorKind,
};
use image_retrieval::domain::delivery::{ExecutionMode, PackageStatus, RunRequest};
use image_retrieval::domain::query_plan::{validate_query_plan, QueryPlanInput};
use image_retrieval::domain::search::{ProviderReadiness, ProviderReadinessStatus};
use image_retrieval::orchestrator::RunOrchestrator;
use image_retrieval::pipeline::build_provider_registry;
use image_retrieval::self_check::{
    format_self_check_v11, run_self_check_v11, ChannelReadinessEntry, ProviderReadinessEntry,
    SelfCheckRequestV11,
};
use image_retrieval::validation::{
    validate_package_dir_with_mode, PackageValidationRequest, PackageValidator,
};
use std::path::PathBuf;
use std::process;

// ---------------------------------------------------------------------------
// CLI structure
// ---------------------------------------------------------------------------

#[derive(Parser)]
#[command(
    name = "image-retrieval",
    about = "General-purpose image search, retrieval, validation, and delivery packaging CLI — v1.1",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Execute a full image retrieval task from a QueryPlan and runtime config.
    Run {
        /// Path to the QueryPlan JSON file.
        #[arg(long, default_value = "query-plan.json")]
        query_plan: String,

        /// Path to the runtime config file (TOML or JSON).
        #[arg(long, default_value = "config.toml")]
        config: String,

        /// Output directory for the delivery package.
        #[arg(long, default_value = "out")]
        output_dir: String,

        /// Execution mode: production, fixture, or dry-run.
        #[arg(long, default_value = "production")]
        mode: String,

        /// Output format: human or json.
        #[arg(long, default_value = "human")]
        format: String,

        /// Alias for --query-plan (for backward compatibility).
        #[arg(short, long)]
        plan: Option<String>,

        /// Return non-zero exit code for partial delivery.
        #[arg(long)]
        fail_on_partial: bool,

        /// Allow fixture in production mode (for testing only).
        #[arg(long)]
        allow_fixture: bool,
    },
    /// Run readiness self-checks (no search, retrieval, or delivery).
    SelfCheck {
        /// Path to the runtime config file.
        #[arg(long, default_value = "config.toml")]
        config: String,

        /// Path to the QueryPlan JSON file for validation.
        #[arg(long)]
        query_plan: Option<String>,

        /// Output format: human or json.
        #[arg(long, default_value = "human")]
        format: String,
    },
    /// Validate an existing delivery package.
    ValidatePackage {
        /// Path to the package directory.
        #[arg(long)]
        package_dir: String,

        /// Validation mode: production, fixture, or dry-run.
        #[arg(long, default_value = "production")]
        execution_mode: String,

        /// Output format: human or json.
        #[arg(long, default_value = "human")]
        format: String,

        /// Rewrite validation.json with current validator result.
        #[arg(long)]
        write_report: bool,
    },
    /// Inspect an existing delivery package (read-only).
    InspectPackage {
        /// Path to the package directory.
        #[arg(long)]
        package_dir: String,

        /// Output format: human or json.
        #[arg(long, default_value = "human")]
        format: String,

        /// Section to display: manifest, coverage, validation, handoff, or all.
        #[arg(long, default_value = "all")]
        show: String,
    },
}

// ---------------------------------------------------------------------------
// Exit codes
// ---------------------------------------------------------------------------

mod exit_code {
    pub const SUCCESS: i32 = 0;
    pub const INPUT_ERROR: i32 = 2;
    #[allow(dead_code)]
    pub const CONFIG_ERROR: i32 = 3;
    pub const READINESS_BLOCKED: i32 = 4;
    pub const PARTIAL_DELIVERY: i32 = 5;
    pub const DELIVERY_BLOCKED: i32 = 6;
    pub const PACKAGE_VALIDATION_FAILED: i32 = 7;
    pub const INTERNAL_ERROR: i32 = 70;
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() {
    let cli = Cli::parse();

    let result = match &cli.command {
        Command::Run {
            query_plan,
            config,
            output_dir,
            mode,
            format,
            plan,
            fail_on_partial,
            allow_fixture,
        } => {
            let qp_path = plan.as_ref().unwrap_or(query_plan);
            cmd_run(
                qp_path,
                config,
                output_dir,
                mode,
                format,
                *fail_on_partial,
                *allow_fixture,
            )
        }
        Command::SelfCheck {
            config,
            query_plan,
            format,
        } => cmd_self_check(config, query_plan, format),
        Command::ValidatePackage {
            package_dir,
            execution_mode,
            format,
            write_report,
        } => cmd_validate_package(package_dir, execution_mode, format, *write_report),
        Command::InspectPackage {
            package_dir,
            format,
            show,
        } => cmd_inspect_package(package_dir, format, show),
    };

    let code = match &result {
        Ok(Some(code)) => *code,
        Ok(None) => exit_code::SUCCESS,
        Err(code) => *code,
    };
    process::exit(code);
}

// ===========================================================================
// run command
// ===========================================================================

fn cmd_run(
    query_plan_path: &str,
    config_path: &str,
    output_dir: &str,
    mode: &str,
    format: &str,
    fail_on_partial: bool,
    allow_fixture: bool,
) -> Result<Option<i32>, i32> {
    if allow_fixture {
        print_error(
            "--allow-fixture is deprecated and unsupported; use --mode fixture for fixture-only runs.",
            format,
        );
        return Err(exit_code::INPUT_ERROR);
    }

    // 1. Load QueryPlan
    let input = load_query_plan(query_plan_path)?;

    // 2. Validate QueryPlan
    let validated = match validate_query_plan(input) {
        image_retrieval::domain::query_plan::ValidationOutcome::Valid { plan, warnings } => {
            if format == "human" {
                for w in &warnings {
                    eprintln!("[WARN] {}: {}", w.field, w.reason);
                }
            }
            plan
        }
        image_retrieval::domain::query_plan::ValidationOutcome::Rejected(rejection) => {
            let msg = format!("QueryPlan rejected: {}", rejection.summary);
            print_error(&msg, format);
            return Err(exit_code::INPUT_ERROR);
        }
    };

    // 3. Determine execution mode
    let execution_mode = parse_execution_mode(mode, format)?;

    // 4. Build run request
    let run_id = format!(
        "run-{}",
        uuid::Uuid::new_v4()
            .to_string()
            .split('-')
            .next()
            .unwrap_or("0")
    );
    let query_plan_id = validated.description.chars().take(8).collect::<String>();
    let request = RunRequest {
        query_plan_id,
        description: validated.description.clone(),
        required_image_count: validated.required_count,
        retry_limit: validated.retry_limit as u8,
        candidate_target: validated.required_count.saturating_mul(20),
        retrieval_batch_target: validated.required_count.saturating_mul(2),
        execution_mode,
        output_dir: PathBuf::from(output_dir),
        run_id,
    };

    // 5. Create orchestrator
    let mut orchestrator = RunOrchestrator::new(request);

    // 6. Dry-run: skip actual search/retrieval
    if execution_mode == ExecutionMode::DryRun {
        let msg = format!(
            "Dry-run mode: QueryPlan '{}' validated, {} images requested, {} candidate target, {} retrieval batch target. No search or retrieval executed.",
            validated.description,
            validated.required_count,
            validated.required_count.saturating_mul(20),
            validated.required_count.saturating_mul(2),
        );
        print_output(
            &msg,
            &serde_json::json!({"status": "dry_run_complete"}),
            format,
        );
        return Ok(None);
    }

    // 7. Full pipeline loop.
    //
    // Production mode runs the real end-to-end pipeline (search → candidate
    // quality gate + Qwen VLM → retrieval → image acceptance gate + Qwen VLM),
    // recording accepted images and coverage gaps into the orchestrator.
    // Fixture and other non-dry-run modes stay on the attempt-tracking path
    // until their fixtures are wired through this module.
    let production_config = if execution_mode == ExecutionMode::Production {
        Some(load_runtime_config(config_path, format)?)
    } else {
        None
    };

    loop {
        let accepted_before = orchestrator.accepted_count();
        let gaps_before = orchestrator.gaps.len() as u32;
        let diagnostics_before = orchestrator.diagnostics.len();
        let mut attempt = orchestrator.start_attempt();

        if let Some(config) = production_config.as_ref() {
            let summary = image_retrieval::pipeline::execute_production_attempt(
                config,
                &validated,
                &mut orchestrator,
            )?;
            attempt.search_candidate_count = summary.search_candidate_count as u32;
            attempt.retrievable_candidate_count = summary.retrievable_candidate_count as u32;
            attempt.retrieval_job_count = summary.retrieval_job_count as u32;
            attempt.retrieval_complete_count = summary.retrieval_complete_count as u32;
        } else {
            orchestrator.record_gap(
                image_retrieval::domain::delivery::CoverageGapType::ExternalDecisionBlocked,
                validated.required_count,
                image_retrieval::domain::delivery::WorkflowFailureCode::BlockedDelivery,
                image_retrieval::domain::delivery::PipelineStage::ProviderReadiness,
                "Non-production fixture mode does not create production delivery evidence.",
                true,
            );
        }

        attempt.accepted_delta_count = orchestrator
            .accepted_count()
            .saturating_sub(accepted_before);
        attempt.gap_delta_count = (orchestrator.gaps.len() as u32).saturating_sub(gaps_before);
        orchestrator.finish_attempt(attempt);

        if orchestrator.target_met() {
            break;
        }
        if attempt_has_non_retryable_blocker(
            &orchestrator,
            gaps_before as usize,
            diagnostics_before,
        ) {
            break;
        }
        if !orchestrator.advance_to_retry() {
            orchestrator.record_diagnostic(
                image_retrieval::domain::delivery::WorkflowDiagnostic::blocker(
                    image_retrieval::domain::delivery::WorkflowFailureCode::RetryExhausted,
                    image_retrieval::domain::delivery::PipelineStage::CoverageCheck,
                    "Retries exhausted without meeting required image count.",
                ),
            );
            break;
        }
    }

    // 8. Determine final state
    orchestrator.state.update_status();

    // 9. Build canonical package
    let package_result = build_canonical_package(
        &orchestrator,
        &validated.description,
        validated.required_count.saturating_mul(20),
        validated.required_count.saturating_mul(2),
        &PathBuf::from(output_dir),
    );

    let package_dir = match package_result {
        Ok(dir) => dir,
        Err(e) => {
            let msg = format!("Package build failed: {}", e);
            print_error(&msg, format);
            return Err(exit_code::INTERNAL_ERROR);
        }
    };

    // 10. Run package validator
    let validation_result = validate_package_dir_with_mode(&package_dir, execution_mode);
    let validation_passed = match &validation_result {
        Ok(report) => report.status == image_retrieval::domain::delivery::ValidationStatus::Pass,
        Err(_) => false,
    };
    if let Ok(report) = &validation_result {
        let val_path = package_dir.join("validation.json");
        if let Ok(content) = serde_json::to_string_pretty(report) {
            let _ = std::fs::write(&val_path, content);
        }
    }

    if let Ok(report) = &validation_result {
        if format == "human" {
            println!("Package validation: {:?}", report.status);
            println!(
                "  {} file checks, {} artifact checks, {} redaction checks",
                report.file_checks.len(),
                report.artifact_checks.len(),
                report.redaction_checks.len()
            );
        }
    }

    // 11. Build outcome
    let mut outcome = orchestrator.build_outcome();
    outcome.package_dir = Some(package_dir.display().to_string());
    outcome.validation_status = Some(match &validation_result {
        Ok(report) => match report.status {
            image_retrieval::domain::delivery::ValidationStatus::Pass => "pass".into(),
            image_retrieval::domain::delivery::ValidationStatus::Fail => "fail".into(),
            image_retrieval::domain::delivery::ValidationStatus::Blocked => "blocked".into(),
        },
        Err(_) => "blocked".into(),
    });

    // 12. Output
    let summary = format!(
        "Run '{}' completed: status={}, accepted={}/{}, attempts={}, retries={}",
        outcome.run_id,
        outcome.status.label(),
        outcome.accepted_image_count,
        outcome.required_image_count,
        outcome.full_attempt_count,
        outcome.retry_count,
    );
    print_output(&summary, &outcome, format);

    // 13. Determine exit code
    match run_exit_decision(outcome.status, validation_passed, fail_on_partial) {
        RunExitDecision::Success => Ok(None),
        RunExitDecision::SoftFailure(code) => Ok(Some(code)),
        RunExitDecision::HardFailure(code) => Err(code),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RunExitDecision {
    Success,
    SoftFailure(i32),
    HardFailure(i32),
}

fn run_exit_decision(
    status: PackageStatus,
    validation_passed: bool,
    fail_on_partial: bool,
) -> RunExitDecision {
    if !validation_passed && matches!(status, PackageStatus::Passed | PackageStatus::Partial) {
        return RunExitDecision::HardFailure(exit_code::PACKAGE_VALIDATION_FAILED);
    }

    match status {
        PackageStatus::Passed => RunExitDecision::Success,
        PackageStatus::Partial => {
            if fail_on_partial {
                RunExitDecision::HardFailure(exit_code::DELIVERY_BLOCKED)
            } else {
                RunExitDecision::SoftFailure(exit_code::PARTIAL_DELIVERY)
            }
        }
        PackageStatus::Blocked => RunExitDecision::HardFailure(exit_code::DELIVERY_BLOCKED),
    }
}

fn attempt_has_non_retryable_blocker(
    orchestrator: &RunOrchestrator,
    gaps_before: usize,
    diagnostics_before: usize,
) -> bool {
    let gap_blocked = orchestrator
        .gaps
        .iter()
        .skip(gaps_before)
        .any(|gap| !gap.retryable && gap.missing_count > 0);
    let diagnostic_blocked = orchestrator
        .diagnostics
        .iter()
        .skip(diagnostics_before)
        .any(|diagnostic| {
            !diagnostic.retryable
                && diagnostic.severity
                    == image_retrieval::domain::delivery::WorkflowSeverity::Blocker
        });
    gap_blocked || diagnostic_blocked
}

// ===========================================================================
// self-check command
// ===========================================================================

fn cmd_self_check(
    config_path: &str,
    query_plan_path: &Option<String>,
    format: &str,
) -> Result<Option<i32>, i32> {
    // Load QueryPlan if provided
    let query_plan_input = if let Some(path) = query_plan_path {
        load_query_plan(path)?
    } else {
        QueryPlanInput {
            description: "self-check only — no QueryPlan provided".into(),
            ..Default::default()
        }
    };

    let config = load_runtime_config(config_path, format)?;

    let request = SelfCheckRequestV11 {
        query_plan_input,
        providers: self_check_provider_entries(&config),
        channels: self_check_channel_entries(&config),
        vlm_provider_id: config.vlm_evaluation.provider_id.clone(),
        vlm_available: vlm_available(&config),
        vlm_credential_configured: vlm_credential_configured(&config),
        vlm_endpoint_configured: vlm_endpoint_configured(&config),
        paid_channel_confirmed: config.policy.allow_paid_channels,
        output_dir_writable: true,
        policy_risks: vec![],
    };

    let report = run_self_check_v11(request);

    match format {
        "json" => {
            let json = serde_json::to_string_pretty(&report).unwrap_or_default();
            println!("{}", json);
        }
        _ => {
            println!("{}", format_self_check_v11(&report));
        }
    }

    match report.status {
        image_retrieval::self_check::SelfCheckStatusV11::Ready => Ok(None),
        image_retrieval::self_check::SelfCheckStatusV11::Warning => Ok(Some(exit_code::SUCCESS)),
        image_retrieval::self_check::SelfCheckStatusV11::Blocked => {
            Err(exit_code::READINESS_BLOCKED)
        }
    }
}

// ===========================================================================
// validate-package command
// ===========================================================================

fn cmd_validate_package(
    package_dir: &str,
    execution_mode: &str,
    format: &str,
    write_report: bool,
) -> Result<Option<i32>, i32> {
    let execution_mode = parse_execution_mode(execution_mode, format)?;
    let validator = PackageValidator::new();
    let request = PackageValidationRequest {
        package_dir: PathBuf::from(package_dir),
        execution_mode,
        expected_query_plan_id: None,
    };

    let report = match validator.validate(&request) {
        Ok(r) => r,
        Err(e) => {
            let msg = format!("Validation error: {}", e);
            print_error(&msg, format);
            return Err(exit_code::PACKAGE_VALIDATION_FAILED);
        }
    };

    // Optionally rewrite validation.json
    if write_report {
        let val_path = PathBuf::from(package_dir).join("validation.json");
        if let Ok(content) = serde_json::to_string_pretty(&report) {
            let _ = std::fs::write(&val_path, content);
        }
    }

    match format {
        "json" => {
            let json = serde_json::to_string_pretty(&report).unwrap_or_default();
            println!("{}", json);
        }
        _ => {
            println!("Package Validation Report");
            println!("=========================");
            println!("Package: {}", package_dir);
            println!(
                "Status: {}",
                match report.status {
                    image_retrieval::domain::delivery::ValidationStatus::Pass => "PASS",
                    image_retrieval::domain::delivery::ValidationStatus::Fail => "FAIL",
                    image_retrieval::domain::delivery::ValidationStatus::Blocked => "BLOCKED",
                }
            );
            println!("Validator version: {}", report.validator_version);
            println!();
            if !report.issues.is_empty() {
                println!("Issues ({}):", report.issues.len());
                for issue in &report.issues {
                    println!("  [{}] {} - {}", issue.code, issue.subject, issue.message);
                }
                println!();
            }
            println!(
                "File checks: {} ok / {} total",
                report.file_checks.iter().filter(|f| f.exists).count(),
                report.file_checks.len()
            );
            println!(
                "Artifact checks: {} ok / {} total",
                report.artifact_checks.iter().filter(|a| a.exists).count(),
                report.artifact_checks.len()
            );
            println!(
                "Redaction checks: {} passed / {} total",
                report.redaction_checks.iter().filter(|r| r.passed).count(),
                report.redaction_checks.len()
            );
        }
    }

    match report.status {
        image_retrieval::domain::delivery::ValidationStatus::Pass => Ok(None),
        _ => Err(exit_code::PACKAGE_VALIDATION_FAILED),
    }
}

// ===========================================================================
// inspect-package command
// ===========================================================================

fn cmd_inspect_package(package_dir: &str, format: &str, show: &str) -> Result<Option<i32>, i32> {
    let pkg = PathBuf::from(package_dir);
    if !pkg.exists() || !pkg.is_dir() {
        let msg = format!("Package directory does not exist: {}", package_dir);
        print_error(&msg, format);
        return Err(exit_code::INPUT_ERROR);
    }

    // Collect available files
    let canonical_files = [
        "image-recalls.json",
        "retrieved-images.json",
        "coverage-report.json",
        "retrieval-manifest.json",
        "package-summary.json",
        "delivery-report.json",
        "validation.json",
        "review.json",
        "handoff-report.json",
    ];

    let mut available = Vec::new();
    let mut missing = Vec::new();
    for f in &canonical_files {
        if pkg.join(f).exists() {
            available.push(*f);
        } else {
            missing.push(*f);
        }
    }

    let show_section = |name: &str| {
        let path = pkg.join(name);
        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&content) {
                    println!("── {} ──", name);
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&value).unwrap_or_default()
                    );
                    println!();
                }
            }
        }
    };

    match format {
        "json" => {
            let summary = serde_json::json!({
                "package_dir": package_dir,
                "available_files": available,
                "missing_files": missing,
            });
            println!(
                "{}",
                serde_json::to_string_pretty(&summary).unwrap_or_default()
            );
        }
        _ => {
            println!("Package Inspection: {}", package_dir);
            println!("=================================");
            println!("Available files ({}):", available.len());
            for f in &available {
                let size = std::fs::metadata(pkg.join(f)).map(|m| m.len()).unwrap_or(0);
                println!("  {} ({} bytes)", f, size);
            }
            if !missing.is_empty() {
                println!("Missing files ({}):", missing.len());
                for f in &missing {
                    println!("  {}", f);
                }
            }
            println!();

            match show {
                "all" => {
                    show_section("retrieval-manifest.json");
                    show_section("package-summary.json");
                    show_section("coverage-report.json");
                    show_section("validation.json");
                    show_section("handoff-report.json");
                    show_section("review.json");
                }
                "manifest" => {
                    show_section("retrieval-manifest.json");
                    show_section("package-summary.json");
                }
                "coverage" => {
                    show_section("coverage-report.json");
                }
                "validation" => {
                    show_section("validation.json");
                }
                "handoff" => {
                    show_section("handoff-report.json");
                    show_section("review.json");
                }
                other => {
                    eprintln!("Unknown section: {}", other);
                }
            }
        }
    }

    Ok(None)
}

// ===========================================================================
// Helpers
// ===========================================================================

fn load_runtime_config(path: &str, format: &str) -> Result<RuntimeConfig, i32> {
    match std::fs::read_to_string(path) {
        Ok(s) => match toml::from_str::<RuntimeConfig>(&s) {
            Ok(cfg) => Ok(cfg),
            Err(e) => {
                let msg = format!("Cannot parse runtime config '{}': {}", path, e);
                print_error(&msg, format);
                Err(exit_code::CONFIG_ERROR)
            }
        },
        Err(e) => {
            let msg = format!("Cannot read runtime config '{}': {}", path, e);
            print_error(&msg, format);
            Err(exit_code::CONFIG_ERROR)
        }
    }
}

fn self_check_provider_entries(config: &RuntimeConfig) -> Vec<ProviderReadinessEntry> {
    let fixture_mode = config
        .providers
        .iter()
        .any(|provider| provider.provider_kind == SearchProviderKind::Fixture);
    let previous_fixture_mode = std::env::var("IMAGE_RETRIEVAL_FIXTURE_MODE").ok();
    if fixture_mode {
        std::env::set_var("IMAGE_RETRIEVAL_FIXTURE_MODE", "1");
    }

    let registry = build_provider_registry(config);
    let mut entries = registry
        .evaluate_readiness()
        .into_iter()
        .map(|report| ProviderReadinessEntry {
            provider_id: report.provider_id.to_string(),
            display_name: report.display_name,
            enabled: report.status != ProviderReadinessStatus::Disabled,
            weight: report.configured_weight as i32,
            readiness: legacy_provider_readiness(&report.status),
            reason: report
                .evidence
                .first()
                .map(|e| e.message.clone())
                .or_else(|| Some(report.status.to_string())),
        })
        .collect::<Vec<_>>();

    match previous_fixture_mode {
        Some(value) => std::env::set_var("IMAGE_RETRIEVAL_FIXTURE_MODE", value),
        None if fixture_mode => std::env::remove_var("IMAGE_RETRIEVAL_FIXTURE_MODE"),
        None => {}
    }

    entries.sort_by(|a, b| a.provider_id.cmp(&b.provider_id));
    entries
}

fn legacy_provider_readiness(status: &ProviderReadinessStatus) -> ProviderReadiness {
    match status {
        ProviderReadinessStatus::Ready => ProviderReadiness::Ready,
        ProviderReadinessStatus::Disabled => ProviderReadiness::Disabled,
        ProviderReadinessStatus::MissingCredentials => ProviderReadiness::MissingCredentials,
        ProviderReadinessStatus::Misconfigured => ProviderReadiness::Misconfigured,
        ProviderReadinessStatus::QuotaExhausted => ProviderReadiness::RateLimited,
        ProviderReadinessStatus::HealthFailed
        | ProviderReadinessStatus::ConstraintUnsupported
        | ProviderReadinessStatus::Retired
        | ProviderReadinessStatus::FixtureOnly
        | ProviderReadinessStatus::Unavailable => ProviderReadiness::Unavailable,
    }
}

fn self_check_channel_entries(config: &RuntimeConfig) -> Vec<ChannelReadinessEntry> {
    config
        .retrieval_channels
        .iter()
        .map(|channel| {
            let credential_configured = channel
                .credential_env
                .as_deref()
                .map(|env| std::env::var(env).is_ok())
                .unwrap_or(true);
            let is_paid = matches!(
                channel.channel_kind,
                RetrievalChannelKind::PaidOnlineService
            ) || channel.tier.is_paid();
            let paid_allowed = !is_paid || config.policy.allow_paid_channels;
            let artifact_capable = matches!(
                channel.channel_kind,
                RetrievalChannelKind::NormalWebFetch | RetrievalChannelKind::Fixture
            );
            let ready =
                channel.enabled && credential_configured && paid_allowed && artifact_capable;
            let readiness = if ready {
                image_retrieval::domain::retrieval::RetrievalChannelReadiness::Ready
            } else if is_paid && !paid_allowed {
                image_retrieval::domain::retrieval::RetrievalChannelReadiness::PaidUnconfirmed
            } else if !channel.enabled {
                image_retrieval::domain::retrieval::RetrievalChannelReadiness::Disabled
            } else if !credential_configured {
                image_retrieval::domain::retrieval::RetrievalChannelReadiness::MissingDependency
            } else {
                image_retrieval::domain::retrieval::RetrievalChannelReadiness::Misconfigured
            };
            let reason = if ready {
                Some(format!("{} artifact retrieval ready", channel.channel_id))
            } else if is_paid && !paid_allowed {
                Some("paid channel requires explicit confirmation".into())
            } else if !artifact_capable {
                Some(format!(
                    "{} is a boundary channel, not artifact-ready",
                    channel.channel_id
                ))
            } else if !credential_configured {
                Some(format!(
                    "{} not set — retrieval channel unavailable",
                    channel
                        .credential_env
                        .as_deref()
                        .unwrap_or("credential env")
                ))
            } else {
                Some(format!("{} is disabled", channel.channel_id))
            };
            ChannelReadinessEntry {
                channel_id: channel.channel_id.clone(),
                display_name: channel.channel_id.clone(),
                tier: channel.tier.to_string(),
                enabled: channel.enabled,
                readiness,
                reason,
            }
        })
        .collect()
}

fn vlm_credential_configured(config: &RuntimeConfig) -> bool {
    if matches!(
        config.vlm_evaluation.provider_kind,
        VlmEvaluatorKind::Fixture
    ) || config.vlm_evaluation.fixture_mode
    {
        return config.vlm_evaluation.enabled;
    }
    config
        .vlm_evaluation
        .credential_env
        .as_deref()
        .map(|env| std::env::var(env).is_ok())
        .unwrap_or(false)
}

fn vlm_endpoint_configured(config: &RuntimeConfig) -> bool {
    matches!(
        config.vlm_evaluation.provider_kind,
        VlmEvaluatorKind::Fixture | VlmEvaluatorKind::Qwen35Vlm
    ) || config.vlm_evaluation.fixture_mode
        || config.vlm_evaluation.base_url.is_some()
        || config.vlm_evaluation.endpoint.is_some()
}

fn vlm_available(config: &RuntimeConfig) -> bool {
    config.vlm_evaluation.enabled
        && vlm_credential_configured(config)
        && vlm_endpoint_configured(config)
}

fn load_query_plan(path: &str) -> Result<QueryPlanInput, i32> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        let msg = format!("Cannot read QueryPlan file '{}': {}", path, e);
        eprintln!("Error: {}", msg);
        exit_code::INPUT_ERROR
    })?;
    serde_json::from_str(&content).map_err(|e| {
        let msg = format!("Cannot parse QueryPlan JSON '{}': {}", path, e);
        eprintln!("Error: {}", msg);
        exit_code::INPUT_ERROR
    })
}

fn print_error(msg: &str, format: &str) {
    match format {
        "json" => {
            let err = serde_json::json!({"error": msg});
            eprintln!("{}", serde_json::to_string(&err).unwrap_or_default());
        }
        _ => {
            eprintln!("Error: {}", msg);
        }
    }
}

fn parse_execution_mode(mode: &str, format: &str) -> Result<ExecutionMode, i32> {
    match mode {
        "production" => Ok(ExecutionMode::Production),
        "fixture" => Ok(ExecutionMode::Fixture),
        "dry-run" => Ok(ExecutionMode::DryRun),
        other => {
            let msg = format!(
                "Invalid execution mode: {} (use production, fixture, or dry-run)",
                other
            );
            print_error(&msg, format);
            Err(exit_code::INPUT_ERROR)
        }
    }
}

fn print_output(summary: &str, payload: &impl serde::Serialize, format: &str) {
    match format {
        "json" => {
            println!(
                "{}",
                serde_json::to_string_pretty(payload).unwrap_or_default()
            );
        }
        _ => {
            println!("{}", summary);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image_retrieval::domain::delivery::{CoverageGapType, PipelineStage, WorkflowFailureCode};

    fn make_orchestrator() -> RunOrchestrator {
        RunOrchestrator::new(RunRequest {
            query_plan_id: "qp-test".into(),
            description: "test image".into(),
            required_image_count: 1,
            retry_limit: 3,
            candidate_target: 20,
            retrieval_batch_target: 2,
            execution_mode: ExecutionMode::Production,
            output_dir: std::env::temp_dir().join("image-retrieval-main-test"),
            run_id: "run-test".into(),
        })
    }

    #[test]
    fn non_retryable_gap_blocks_further_retry() {
        let mut orchestrator = make_orchestrator();
        orchestrator.record_gap(
            CoverageGapType::CandidateQualityExecutionBlocked,
            1,
            WorkflowFailureCode::CandidateQualityBlocked,
            PipelineStage::CandidateQuality,
            "VLM unavailable",
            false,
        );

        assert!(attempt_has_non_retryable_blocker(&orchestrator, 0, 0));
    }

    #[test]
    fn retryable_gap_allows_retry() {
        let mut orchestrator = make_orchestrator();
        orchestrator.record_gap(
            CoverageGapType::SearchRecallShortage,
            1,
            WorkflowFailureCode::SearchShortage,
            PipelineStage::Search,
            "Search returned no candidates.",
            true,
        );

        assert!(!attempt_has_non_retryable_blocker(&orchestrator, 0, 0));
    }

    #[test]
    fn non_retryable_blocker_diagnostic_blocks_retry() {
        let mut orchestrator = make_orchestrator();
        let mut diagnostic = image_retrieval::domain::delivery::WorkflowDiagnostic::blocker(
            WorkflowFailureCode::CandidateQualityBlocked,
            PipelineStage::CandidateQuality,
            "VLM unavailable",
        );
        diagnostic.retryable = false;
        orchestrator.record_diagnostic(diagnostic);

        assert!(attempt_has_non_retryable_blocker(&orchestrator, 0, 0));
    }

    #[test]
    fn partial_package_validation_failure_is_hard_failure() {
        assert_eq!(
            run_exit_decision(PackageStatus::Partial, false, false),
            RunExitDecision::HardFailure(exit_code::PACKAGE_VALIDATION_FAILED)
        );
    }

    #[test]
    fn valid_partial_remains_soft_failure_when_allowed() {
        assert_eq!(
            run_exit_decision(PackageStatus::Partial, true, false),
            RunExitDecision::SoftFailure(exit_code::PARTIAL_DELIVERY)
        );
    }
}
