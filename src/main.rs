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
    },
}

fn main() {
    let cli = Cli::parse();

    let plan_path = match &cli.command {
        Command::Run { plan } => plan,
        Command::SelfCheck { plan } => plan,
    };

    let outcome = match load_and_validate(plan_path) {
        Ok(outcome) => outcome,
        Err(msg) => {
            eprintln!("错误：{}", msg);
            process::exit(1);
        }
    };

    match outcome {
        ValidationOutcome::Valid { plan, warnings } => {
            print_warnings(&warnings);

            match &cli.command {
                Command::Run { .. } => {
                    let task = TaskPlan::from_validated(plan);
                    print_task_summary(&task);
                    println!("状态：规划完成，等待下游搜索和抓取实现 (TASK-003+)");
                }
                Command::SelfCheck { .. } => {
                    print_plan_summary(&plan);
                    println!("self-check 状态：输入验证通过，无阻塞问题。");
                }
            }
        }
        ValidationOutcome::Rejected(rejection) => {
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
            process::exit(1);
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
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
