//! image-retrieval CLI entry point.
//!
//! Supports two command paths:
//! - `run`: execute a full image retrieval task from a QueryPlan.
//! - `self-check`: run readiness checks before a formal task.
//!
//! Both paths share the same QueryPlan input validation logic provided by
//! `image_retrieval::domain::query_plan::validate_query_plan`.

use clap::{Parser, Subcommand};
use image_retrieval::domain::query_plan::{
    validate_query_plan, InputDiagnostic, QueryPlanInput, TaskPlan, ValidationOutcome,
};
use image_retrieval::domain::retrieval::RetrievalChannelReadiness;
use image_retrieval::domain::search::ProviderReadiness;
use image_retrieval::self_check::{
    ChannelReadinessEntry, PolicyRiskEntry, ProviderReadinessEntry, SelfCheckRequest,
};
use serde::Deserialize;
use std::process;

#[derive(Parser)]
#[command(
    name = "image-retrieval",
    about = "General-purpose image search, retrieval, validation, and delivery packaging CLI",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Execute a full image retrieval task from a QueryPlan.
    Run {
        /// Path to the QueryPlan JSON file.
        #[arg(short, long, default_value = "query_plan.json")]
        plan: String,
    },
    /// Run readiness self-checks (no search, retrieval, or delivery).
    SelfCheck {
        /// Path to the QueryPlan JSON file for validation.
        #[arg(short, long, default_value = "query_plan.json")]
        plan: String,

        /// Optional path to a provider configuration JSON file.
        #[arg(long)]
        provider_config: Option<String>,

        /// Optional path to a channel configuration JSON file.
        #[arg(long)]
        channel_config: Option<String>,
    },
}

// ---------------------------------------------------------------------------
// CLI JSON config schemas (minimal, stable, documented)
// ---------------------------------------------------------------------------

/// Shape of a provider configuration JSON file.
#[derive(Debug, Deserialize)]
struct ProviderConfigFile {
    #[serde(default)]
    providers: Vec<ProviderConfigEntry>,
}

#[derive(Debug, Deserialize)]
struct ProviderConfigEntry {
    provider_id: String,
    display_name: String,
    #[serde(default = "default_true")]
    enabled: bool,
    #[serde(default = "default_weight_one")]
    weight: i32,
    /// Readiness: "ready", "disabled", "missing_credentials", "misconfigured",
    /// "rate_limited", "unavailable".
    #[serde(default = "default_ready_str")]
    readiness: String,
    #[serde(default)]
    reason: Option<String>,
}

fn default_true() -> bool {
    true
}
fn default_weight_one() -> i32 {
    1
}
fn default_ready_str() -> String {
    "ready".into()
}

/// Shape of a channel configuration JSON file.
#[derive(Debug, Deserialize)]
struct ChannelConfigFile {
    #[serde(default)]
    channels: Vec<ChannelConfigEntry>,
}

#[derive(Debug, Deserialize)]
struct ChannelConfigEntry {
    channel_id: String,
    display_name: String,
    tier: String,
    #[serde(default = "default_true")]
    enabled: bool,
    /// Readiness: "ready", "disabled", "missing_dependency", "misconfigured",
    /// "paid_unconfirmed".
    #[serde(default = "default_ready_str")]
    readiness: String,
    #[serde(default)]
    reason: Option<String>,
}

fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Command::Run { plan } => {
            let outcome = match load_and_validate(plan) {
                Ok(outcome) => outcome,
                Err(msg) => {
                    eprintln!("错误：{}", msg);
                    process::exit(1);
                }
            };
            handle_run(outcome);
        }
        Command::SelfCheck {
            plan,
            provider_config,
            channel_config,
        } => {
            let query_plan_input = match load_query_plan_input(plan) {
                Ok(input) => input,
                Err(msg) => {
                    eprintln!("错误：{}", msg);
                    process::exit(1);
                }
            };

            let providers = load_provider_config(provider_config);
            let channels = load_channel_config(channel_config);

            handle_self_check(query_plan_input, providers, channels);
        }
    }
}

// ---------------------------------------------------------------------------
// Run path
// ---------------------------------------------------------------------------

fn handle_run(outcome: ValidationOutcome) {
    match outcome {
        ValidationOutcome::Valid { plan, warnings } => {
            print_warnings(&warnings);
            let task = TaskPlan::from_validated(plan);
            print_task_summary(&task);
            println!("状态：规划完成，等待下游搜索和抓取实现 (TASK-003+)");
        }
        ValidationOutcome::Rejected(rejection) => {
            print_rejection(&rejection);
            process::exit(1);
        }
    }
}

// ---------------------------------------------------------------------------
// Self-check path
// ---------------------------------------------------------------------------

fn handle_self_check(
    query_plan_input: QueryPlanInput,
    providers: Vec<ProviderReadinessEntry>,
    channels: Vec<ChannelReadinessEntry>,
) {
    // Build the self-check request.
    //
    // OpenClaw availability and paid-channel confirmation are currently
    // derived from the provider/channel registrations rather than external
    // flags. In production, these would come from user configuration or
    // environment variables.
    let candidate_openclaw_available = has_any_ready_provider(&providers);
    let image_openclaw_available = has_any_ready_provider(&providers);
    let paid_channel_confirmed = channels.iter().any(|c| {
        c.tier.to_lowercase() == "paid"
            && c.enabled
            && c.readiness == RetrievalChannelReadiness::Ready
    });

    // Collect policy risks from the readiness state
    let mut policy_risks: Vec<PolicyRiskEntry> = Vec::new();
    if !candidate_openclaw_available {
        policy_risks.push(PolicyRiskEntry {
            category: "openclaw_candidate".into(),
            description: "候选评价 OpenClaw 不可用 — 正式任务将进入 execution_blocked。".into(),
            is_blocker: true,
        });
    }
    if !image_openclaw_available {
        policy_risks.push(PolicyRiskEntry {
            category: "openclaw_image".into(),
            description: "图片评价 OpenClaw 不可用 — 正式任务将进入 execution_blocked。".into(),
            is_blocker: true,
        });
    }

    let request = SelfCheckRequest {
        query_plan_input,
        providers,
        channels,
        candidate_openclaw_available,
        image_openclaw_available,
        paid_channel_confirmed,
        policy_risks,
    };

    let report = image_retrieval::self_check::run_self_check(request);

    // Print human-readable report
    println!("{}", report.format_human_readable());

    // Also print machine-readable JSON to stderr when output_preference is automation
    // (detected from the original QueryPlan — for now, always print JSON summary).
    eprintln!(
        "机器可读状态：{}",
        serde_json::to_string(&report.status).unwrap_or_default()
    );

    if report.status.is_blocked() {
        eprintln!("self-check 结果：存在阻塞项，正式任务无法启动。请修复以上 blocker 后重试。");
        process::exit(1);
    } else if report.status == image_retrieval::self_check::SelfCheckStatus::Warning {
        eprintln!("self-check 结果：存在警告项，正式任务可继续但存在风险。");
    } else {
        eprintln!("self-check 结果：全部通过，可以启动正式任务。");
    }
}

fn has_any_ready_provider(providers: &[ProviderReadinessEntry]) -> bool {
    providers
        .iter()
        .any(|p| p.enabled && p.readiness == ProviderReadiness::Ready)
}

// ---------------------------------------------------------------------------
// Config loading
// ---------------------------------------------------------------------------

/// Load a QueryPlanInput from a JSON file path.
fn load_query_plan_input(path: &str) -> Result<QueryPlanInput, String> {
    let content =
        std::fs::read_to_string(path).map_err(|e| format!("无法读取文件 '{}': {}", path, e))?;
    serde_json::from_str(&content).map_err(|e| format!("无法解析 QueryPlan JSON: {}", e))
}

/// Load provider configuration from a JSON file, or return a sensible default.
fn load_provider_config(path: &Option<String>) -> Vec<ProviderReadinessEntry> {
    match path {
        Some(p) => match std::fs::read_to_string(p) {
            Ok(content) => match serde_json::from_str::<ProviderConfigFile>(&content) {
                Ok(config) => config
                    .providers
                    .into_iter()
                    .map(|e| ProviderReadinessEntry {
                        provider_id: e.provider_id,
                        display_name: e.display_name,
                        enabled: e.enabled,
                        weight: e.weight,
                        readiness: parse_provider_readiness(&e.readiness),
                        reason: e.reason,
                    })
                    .collect(),
                Err(err) => {
                    eprintln!(
                        "警告：无法解析 provider 配置文件 '{}': {}。使用默认配置。",
                        p, err
                    );
                    default_providers()
                }
            },
            Err(err) => {
                eprintln!(
                    "警告：无法读取 provider 配置文件 '{}': {}。使用默认配置。",
                    p, err
                );
                default_providers()
            }
        },
        None => default_providers(),
    }
}

fn default_providers() -> Vec<ProviderReadinessEntry> {
    vec![ProviderReadinessEntry {
        provider_id: "fixture".into(),
        display_name: "Fixture Provider".into(),
        enabled: true,
        weight: 1,
        readiness: ProviderReadiness::Ready,
        reason: Some("fixture provider for testing — 非生产搜索服务".into()),
    }]
}

/// Load channel configuration from a JSON file, or return a sensible default.
fn load_channel_config(path: &Option<String>) -> Vec<ChannelReadinessEntry> {
    match path {
        Some(p) => match std::fs::read_to_string(p) {
            Ok(content) => match serde_json::from_str::<ChannelConfigFile>(&content) {
                Ok(config) => config
                    .channels
                    .into_iter()
                    .map(|e| ChannelReadinessEntry {
                        channel_id: e.channel_id,
                        display_name: e.display_name,
                        tier: e.tier,
                        enabled: e.enabled,
                        readiness: parse_channel_readiness(&e.readiness),
                        reason: e.reason,
                    })
                    .collect(),
                Err(err) => {
                    eprintln!(
                        "警告：无法解析 channel 配置文件 '{}': {}。使用默认配置。",
                        p, err
                    );
                    default_channels()
                }
            },
            Err(err) => {
                eprintln!(
                    "警告：无法读取 channel 配置文件 '{}': {}。使用默认配置。",
                    p, err
                );
                default_channels()
            }
        },
        None => default_channels(),
    }
}

fn default_channels() -> Vec<ChannelReadinessEntry> {
    vec![ChannelReadinessEntry {
        channel_id: "web-fetch-default".into(),
        display_name: "Default Web Fetch".into(),
        tier: "web_fetch".into(),
        enabled: true,
        readiness: RetrievalChannelReadiness::Ready,
        reason: Some("普通 web fetch 最小抓取通道 — 非生产付费服务".into()),
    }]
}

fn parse_provider_readiness(s: &str) -> ProviderReadiness {
    match s.to_lowercase().as_str() {
        "ready" => ProviderReadiness::Ready,
        "disabled" => ProviderReadiness::Disabled,
        "missing_credentials" => ProviderReadiness::MissingCredentials,
        "misconfigured" => ProviderReadiness::Misconfigured,
        "rate_limited" => ProviderReadiness::RateLimited,
        "unavailable" => ProviderReadiness::Unavailable,
        other => {
            eprintln!(
                "警告：未知 provider readiness '{}'，按 unavailable 处理。",
                other
            );
            ProviderReadiness::Unavailable
        }
    }
}

fn parse_channel_readiness(s: &str) -> RetrievalChannelReadiness {
    match s.to_lowercase().as_str() {
        "ready" => RetrievalChannelReadiness::Ready,
        "disabled" => RetrievalChannelReadiness::Disabled,
        "missing_dependency" => RetrievalChannelReadiness::MissingDependency,
        "misconfigured" => RetrievalChannelReadiness::Misconfigured,
        "paid_unconfirmed" => RetrievalChannelReadiness::PaidUnconfirmed,
        other => {
            eprintln!(
                "警告：未知 channel readiness '{}'，按 missing_dependency 处理。",
                other
            );
            RetrievalChannelReadiness::MissingDependency
        }
    }
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Read a JSON file and validate it as a QueryPlan.
fn load_and_validate(path: &str) -> Result<ValidationOutcome, String> {
    let content =
        std::fs::read_to_string(path).map_err(|e| format!("无法读取文件 '{}': {}", path, e))?;

    let input: QueryPlanInput =
        serde_json::from_str(&content).map_err(|e| format!("无法解析 QueryPlan JSON: {}", e))?;

    Ok(validate_query_plan(input))
}

/// Print non-blocking warning diagnostics to stderr.
fn print_warnings(warnings: &[InputDiagnostic]) {
    if warnings.is_empty() {
        return;
    }
    eprintln!("注意：以下非阻塞提示供参考：");
    for w in warnings {
        eprintln!(
            "  [{}] {}: {}",
            severity_label(w.severity),
            w.field,
            w.reason
        );
        if let Some(ref suggestion) = w.suggestion {
            eprintln!("    建议：{}", suggestion);
        }
        if let Some(ref default) = w.default_applied {
            eprintln!("    已应用默认值：{}", default);
        }
    }
    eprintln!();
}

/// Print an input rejection to stderr and exit.
fn print_rejection(rejection: &image_retrieval::domain::query_plan::InputRejection) {
    eprintln!("输入被拒绝：");
    for diag in &rejection.diagnostics {
        eprintln!(
            "  [{}] {}: {}",
            severity_label(diag.severity),
            diag.field,
            diag.reason
        );
        if let Some(ref suggestion) = diag.suggestion {
            eprintln!("    建议：{}", suggestion);
        }
    }
    eprintln!();
    eprintln!("任务未启动。请修复以上问题后重试。");
}

/// Print a validated plan summary.
fn print_plan_summary(plan: &image_retrieval::domain::query_plan::ValidatedQueryPlan) {
    println!("QueryPlan 校验通过：");
    println!("  描述：{}", plan.description);
    println!("  交付数量：{}", plan.required_count);
    println!(
        "  质量档位：{}",
        match plan.quality_tier {
            image_retrieval::domain::query_plan::QualityTier::General => "通用质量",
            image_retrieval::domain::query_plan::QualityTier::High => "较高质量",
            image_retrieval::domain::query_plan::QualityTier::Strict => "严格质量",
        }
    );
    if !plan.content_constraints.must_include.is_empty() {
        println!(
            "  必须包含：{}",
            plan.content_constraints.must_include.join(", ")
        );
    }
    if !plan.content_constraints.must_avoid.is_empty() {
        println!(
            "  必须避免：{}",
            plan.content_constraints.must_avoid.join(", ")
        );
    }
    println!(
        "  授权偏好：{}",
        match plan.authorization_preference {
            image_retrieval::domain::query_plan::AuthorizationPreference::Default => {
                "未知授权（保留风险提示）"
            }
        }
    );
    println!(
        "  输出偏好：{}",
        match plan.output_preference {
            image_retrieval::domain::query_plan::OutputPreference::Human => "面向人工查看",
            image_retrieval::domain::query_plan::OutputPreference::Automation => "面向自动化消费",
        }
    );
    println!("  重试上限：{}", plan.retry_limit);
}

/// Print the derived TaskPlan summary.
fn print_task_summary(task: &TaskPlan) {
    print_plan_summary(&task.query_plan);
    println!();
    println!("派生执行规划 (TaskPlan)：");
    println!(
        "  候选目标：{} ({} × 20)",
        task.candidate_target, task.query_plan.required_count
    );
    println!(
        "  抓取批次目标：{} ({} × 2)",
        task.retrieval_batch_target, task.query_plan.required_count
    );
    println!(
        "  最大尝试次数：{} (1 初始 + {} 重试)",
        task.max_attempts, task.query_plan.retry_limit
    );
}

fn severity_label(level: image_retrieval::error::DiagnosticLevel) -> &'static str {
    use image_retrieval::error::DiagnosticLevel;
    match level {
        DiagnosticLevel::Info => "INFO",
        DiagnosticLevel::Warning => "WARN",
        DiagnosticLevel::Error => "ERROR",
    }
}
