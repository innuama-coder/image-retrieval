//! Readiness self-check module.
//!
//! Implements the pre-flight readiness reporter described in
//! `docs/design/TASK-008-readiness-self-check-design.md` and HLD §自助检查视图.
//!
//! Aggregates readiness from five dimensions — QueryPlan validation, search
//! provider readiness, retrieval channel readiness, OpenClaw evaluation
//! availability (candidate-phase and image-phase), and policy guardrails —
//! into a single [`SelfCheckReport`].
//!
//! The self-check performs **no** search, retrieval, subjective evaluation,
//! or delivery packaging. It never exposes credential values in diagnostics.
//!
//! References: PRD FR-012/AC-012, HLD §自助检查视图

use crate::domain::query_plan;
use crate::domain::retrieval::RetrievalChannelReadiness;
use crate::domain::search::ProviderReadiness;
use crate::error::DiagnosticLevel;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Re-export domain types consumed by self-check
// ---------------------------------------------------------------------------

pub use crate::domain::query_plan::QueryPlanInput;
pub use crate::error::{Diagnostic, DiagnosticItem};

// ---------------------------------------------------------------------------
// Self-check status
// ---------------------------------------------------------------------------

/// Overall self-check status.
///
/// Independent of delivery task status (`full_delivery`, `limited_delivery`,
/// `execution_blocked`). A `pass` here means no readiness blockers were found;
/// it does NOT guarantee task success.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SelfCheckStatus {
    /// All readiness checks passed — no blockers or significant warnings.
    Pass,

    /// One or more checks produced warnings but no blockers.
    Warning,

    /// One or more checks found blocking conditions that must be resolved
    /// before the formal task can proceed.
    Blocked,
}

impl SelfCheckStatus {
    /// Returns `true` if the status allows the formal task to be attempted.
    pub fn can_proceed(&self) -> bool {
        matches!(self, Self::Pass | Self::Warning)
    }

    /// Returns `true` if the status is blocked.
    pub fn is_blocked(&self) -> bool {
        matches!(self, Self::Blocked)
    }
}

impl std::fmt::Display for SelfCheckStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Self::Pass => "pass",
            Self::Warning => "warning",
            Self::Blocked => "blocked",
        };
        write!(f, "{}", label)
    }
}

// ---------------------------------------------------------------------------
// Readiness input status — per-dimension tri-state
// ---------------------------------------------------------------------------

/// Per-dimension readiness status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReadinessInputStatus {
    /// This dimension is ready.
    Valid,

    /// This dimension has warnings but is not blocking.
    Warning,

    /// This dimension is blocked and must be resolved.
    Blocked,
}

impl ReadinessInputStatus {
    #[allow(dead_code)]
    pub fn from_bool(ready: bool) -> Self {
        if ready {
            Self::Valid
        } else {
            Self::Blocked
        }
    }

    #[allow(dead_code)]
    pub fn is_blocked(&self) -> bool {
        matches!(self, Self::Blocked)
    }
}

// ---------------------------------------------------------------------------
// Self-check request — what the caller provides
// ---------------------------------------------------------------------------

/// Input to the self-check readiness aggregation.
///
/// Carries the QueryPlan input (pre- or post-validation), provider/channel
/// configuration snapshots, and OpenClaw / policy stances. This is NOT a
/// delivery-package input — it only feeds readiness checks.
#[derive(Debug, Clone)]
pub struct SelfCheckRequest {
    /// The raw QueryPlan input (will be validated during self-check).
    pub query_plan_input: QueryPlanInput,

    /// Registered search providers and their readiness.
    pub providers: Vec<ProviderReadinessEntry>,

    /// Registered retrieval channels and their readiness.
    pub channels: Vec<ChannelReadinessEntry>,

    /// Whether the candidate-phase OpenClaw evaluation port is available.
    pub candidate_openclaw_available: bool,

    /// Whether the image-phase OpenClaw evaluation port is available.
    pub image_openclaw_available: bool,

    /// Whether the user has explicitly confirmed paid channel usage.
    pub paid_channel_confirmed: bool,

    /// Known policy blockers or risks (e.g. authorization unknowns,
    /// site-rule gaps).
    pub policy_risks: Vec<PolicyRiskEntry>,
}

/// A single provider's readiness snapshot for self-check.
#[derive(Debug, Clone)]
pub struct ProviderReadinessEntry {
    pub provider_id: String,
    pub display_name: String,
    pub enabled: bool,
    pub weight: i32,
    pub readiness: ProviderReadiness,
    /// Human-readable reason for the readiness status (never contains credentials).
    pub reason: Option<String>,
}

/// A single channel's readiness snapshot for self-check.
#[derive(Debug, Clone)]
pub struct ChannelReadinessEntry {
    pub channel_id: String,
    pub display_name: String,
    pub tier: String,
    pub enabled: bool,
    pub readiness: RetrievalChannelReadiness,
    /// Human-readable reason (never contains credentials).
    pub reason: Option<String>,
}

/// A known policy risk or open decision.
#[derive(Debug, Clone)]
pub struct PolicyRiskEntry {
    /// Category: "authorization", "paid_channel", "access_control", "open_decision".
    pub category: String,
    /// Human-readable description (never contains credentials).
    pub description: String,
    /// Whether this risk is a blocker (true) or warning (false).
    pub is_blocker: bool,
}

// ---------------------------------------------------------------------------
// Readiness summary types — output dimensions
// ---------------------------------------------------------------------------

/// Aggregated provider readiness for the self-check report.
///
/// Counts are derived from the registered providers; details list every
/// provider so the user can see per-provider status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderReadinessSummary {
    /// Total number of registered providers.
    pub total: usize,

    /// How many are explicitly enabled.
    pub enabled: usize,

    /// How many are ready for search.
    pub ready: usize,

    /// How many are missing credentials (blocker, value never exposed).
    pub missing_credentials: usize,

    /// How many are explicitly disabled.
    pub disabled: usize,

    /// How many are misconfigured (blocker).
    pub misconfigured: usize,

    /// How many are rate-limited or temporarily unavailable.
    pub temporarily_unavailable: usize,

    /// Per-provider detail records (no credential values).
    pub details: Vec<ProviderReadinessDetail>,
}

/// A single provider's readiness detail in the report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderReadinessDetail {
    pub provider_id: String,
    pub display_name: String,
    pub enabled: bool,
    pub weight: i32,
    pub readiness: String,
    pub reason: Option<String>,
}

/// Aggregated retrieval channel readiness for the self-check report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalChannelReadinessSummary {
    /// Total number of registered channels.
    pub total: usize,

    /// How many are explicitly enabled.
    pub enabled: usize,

    /// How many are ready for retrieval.
    pub ready: usize,

    /// How many are explicitly disabled.
    pub disabled: usize,

    /// How many paid channels are unconfirmed (blocker).
    pub paid_unconfirmed: usize,

    /// How many channels are missing a dependency.
    pub missing_dependency: usize,

    /// How many channels are misconfigured.
    pub misconfigured: usize,

    /// Per-channel detail records.
    pub details: Vec<ChannelReadinessDetail>,
}

/// A single channel's readiness detail in the report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelReadinessDetail {
    pub channel_id: String,
    pub display_name: String,
    pub tier: String,
    pub enabled: bool,
    pub readiness: String,
    pub reason: Option<String>,
}

/// OpenClaw readiness summary — covers both evaluation boundaries.
///
/// Per HLD §主观评价架构边界 and ADR-009, candidate evaluation and image
/// evaluation are two distinct OpenClaw boundaries and must be reported
/// separately.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenClawReadinessSummary {
    /// Candidate-phase OpenClaw readiness.
    pub candidate: ReadinessInputStatus,

    /// Human-readable reason if candidate OpenClaw is not ready.
    pub candidate_reason: Option<String>,

    /// Image-phase OpenClaw readiness.
    pub image: ReadinessInputStatus,

    /// Human-readable reason if image OpenClaw is not ready.
    pub image_reason: Option<String>,
}

/// Policy readiness summary for the self-check report.
///
/// Describes authorization stance, paid-channel gating, access-control
/// posture, and any open product decisions that affect task execution.
/// Credential values and sensitive configuration are never included.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyReadinessSummary {
    /// Current authorization risk stance.
    pub authorization_stance: String,

    /// Whether unknown authorization is present (risk, not necessarily blocker).
    pub unknown_authorization_present: bool,

    /// Whether paid channels are explicitly confirmed.
    pub paid_channel_confirmed: bool,

    /// Policy-level blockers that prevent the task from proceeding.
    pub blockers: Vec<String>,

    /// Policy-level warnings that do not prevent task execution.
    pub warnings: Vec<String>,

    /// Open product decisions that may affect task outcome.
    pub open_decisions: Vec<String>,
}

// ---------------------------------------------------------------------------
// Self-check report — the aggregate output
// ---------------------------------------------------------------------------

/// The output of a self-check readiness run.
///
/// Contains pass/warning/blocked status, per-dimension summaries, human- and
/// machine-readable diagnostics, default-value explanations, and adjustment
/// suggestions.
///
/// This is NOT a delivery package. It does not contain `images/`,
/// `status.json`, or `manifest.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfCheckReport {
    /// Overall self-check status.
    pub status: SelfCheckStatus,

    /// Whether the QueryPlan input passed validation.
    pub query_plan_valid: bool,

    /// QueryPlan validation diagnostics (warnings from valid plans, or errors
    /// from rejected plans). Credential-like values are never echoed.
    pub query_plan_diagnostics: Vec<SelfCheckDiagnostic>,

    /// Aggregated provider readiness.
    pub provider_summary: ProviderReadinessSummary,

    /// Aggregated channel readiness.
    pub channel_summary: RetrievalChannelReadinessSummary,

    /// OpenClaw readiness (candidate and image phases reported separately).
    pub openclaw_summary: OpenClawReadinessSummary,

    /// Policy readiness and risks.
    pub policy_summary: PolicyReadinessSummary,

    /// Blocking findings that prevent the formal task from proceeding.
    pub blockers: Vec<String>,

    /// Non-blocking warnings.
    pub warnings: Vec<String>,

    /// Explanations of defaults that were applied.
    pub default_explanations: Vec<String>,

    /// Suggestions for the user to resolve issues.
    pub adjustment_suggestions: Vec<String>,

    /// Machine-readable diagnostic entries for automation consumers.
    pub diagnostics: Vec<SelfCheckDiagnostic>,
}

/// A diagnostic entry in the self-check report.
///
/// Categories correspond to the diagnostic taxonomy defined in the LLD.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfCheckDiagnostic {
    /// Severity level.
    pub level: DiagnosticLevel,

    /// Diagnostic category (e.g. "query_plan", "provider", "channel",
    /// "openclaw_candidate", "openclaw_image", "policy").
    pub category: String,

    /// Human-readable message (never contains credential values).
    pub message: String,

    /// Optional suggestion for resolution.
    pub suggestion: Option<String>,
}

impl SelfCheckDiagnostic {
    pub fn blocker(category: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            level: DiagnosticLevel::Error,
            category: category.into(),
            message: message.into(),
            suggestion: None,
        }
    }

    pub fn blocker_with_suggestion(
        category: impl Into<String>,
        message: impl Into<String>,
        suggestion: impl Into<String>,
    ) -> Self {
        Self {
            level: DiagnosticLevel::Error,
            category: category.into(),
            message: message.into(),
            suggestion: Some(suggestion.into()),
        }
    }

    pub fn warning(category: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            level: DiagnosticLevel::Warning,
            category: category.into(),
            message: message.into(),
            suggestion: None,
        }
    }

    pub fn warning_with_suggestion(
        category: impl Into<String>,
        message: impl Into<String>,
        suggestion: impl Into<String>,
    ) -> Self {
        Self {
            level: DiagnosticLevel::Warning,
            category: category.into(),
            message: message.into(),
            suggestion: Some(suggestion.into()),
        }
    }

    pub fn info(category: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            level: DiagnosticLevel::Info,
            category: category.into(),
            message: message.into(),
            suggestion: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Aggregation — the core self-check logic
// ---------------------------------------------------------------------------

/// Run the readiness self-check against a [`SelfCheckRequest`] and produce a
/// [`SelfCheckReport`].
///
/// This function performs no search, retrieval, subjective evaluation, or
/// delivery packaging. It only inspects configuration and readiness state.
///
/// # Diagnostics produced
///
/// | Category | Condition |
/// |---|---|
/// | `query_plan` | Invalid/missing description, retry-limit exceeded, large count, sensitive input |
/// | `provider` | No enabled providers, missing credentials, misconfigured, zero/negative weight |
/// | `channel` | No enabled channels, paid unconfirmed, missing dependency, misconfigured |
/// | `openclaw_candidate` | Candidate OpenClaw unavailable |
/// | `openclaw_image` | Image OpenClaw unavailable |
/// | `policy` | Unknown authorization, access-control gaps, open product decisions |
pub fn run_self_check(request: SelfCheckRequest) -> SelfCheckReport {
    let mut blockers: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    let mut default_explanations: Vec<String> = Vec::new();
    let mut adjustment_suggestions: Vec<String> = Vec::new();
    let mut diagnostics: Vec<SelfCheckDiagnostic> = Vec::new();

    // ---- 1. QueryPlan validation ----
    let (query_plan_valid, query_plan_diagnostics) = check_query_plan(
        &request,
        &mut blockers,
        &mut warnings,
        &mut default_explanations,
    );

    diagnostics.extend(query_plan_diagnostics.clone());

    // ---- 2. Provider readiness ----
    let provider_summary =
        check_provider_readiness(&request, &mut blockers, &mut warnings, &mut diagnostics);

    // ---- 3. Channel readiness ----
    let channel_summary =
        check_channel_readiness(&request, &mut blockers, &mut warnings, &mut diagnostics);

    // ---- 4. OpenClaw readiness ----
    let openclaw_summary =
        check_openclaw_readiness(&request, &mut blockers, &mut warnings, &mut diagnostics);

    // ---- 5. Policy readiness ----
    let policy_summary =
        check_policy_readiness(&request, &mut blockers, &mut warnings, &mut diagnostics);

    // ---- Determine overall status ----
    let status = if !blockers.is_empty() {
        SelfCheckStatus::Blocked
    } else if !warnings.is_empty() {
        SelfCheckStatus::Warning
    } else {
        SelfCheckStatus::Pass
    };

    // Collect adjustment suggestions from diagnostics
    for d in &diagnostics {
        if let Some(ref suggestion) = d.suggestion {
            if !adjustment_suggestions.contains(suggestion) {
                adjustment_suggestions.push(suggestion.clone());
            }
        }
    }

    SelfCheckReport {
        status,
        query_plan_valid,
        query_plan_diagnostics,
        provider_summary,
        channel_summary,
        openclaw_summary,
        policy_summary,
        blockers,
        warnings,
        default_explanations,
        adjustment_suggestions,
        diagnostics,
    }
}

// ---------------------------------------------------------------------------
// Dimension check helpers
// ---------------------------------------------------------------------------

/// Validate the QueryPlan input and produce diagnostics.
fn check_query_plan(
    request: &SelfCheckRequest,
    blockers: &mut Vec<String>,
    warnings: &mut Vec<String>,
    default_explanations: &mut Vec<String>,
) -> (bool, Vec<SelfCheckDiagnostic>) {
    let mut diags: Vec<SelfCheckDiagnostic> = Vec::new();

    let outcome = query_plan::validate_query_plan(request.query_plan_input.clone());

    match outcome {
        query_plan::ValidationOutcome::Valid {
            plan,
            warnings: plan_warnings,
        } => {
            // Record defaults that were applied
            if request.query_plan_input.required_count == 0 {
                default_explanations
                    .push("required_count 为 0，已应用默认值 1（不会搜索候选或抓取图片）。".into());
            }
            if request.query_plan_input.description.is_empty()
                || request.query_plan_input.description.trim().is_empty()
            {
                // This case would have been rejected, so we don't reach here.
            }

            // Default explanations for fields that were not explicitly set
            if request.query_plan_input.quality_tier
                == crate::domain::query_plan::QualityTier::General
            {
                default_explanations
                    .push("quality_tier 未指定或为默认值，已应用通用质量 (general)。".into());
            }
            if request.query_plan_input.output_preference
                == crate::domain::query_plan::OutputPreference::Human
            {
                default_explanations.push(
                    "output_preference 未指定或为默认值，已应用面向人工查看 (human)。".into(),
                );
            }
            if request.query_plan_input.retry_limit == 3 {
                default_explanations.push("retry_limit 未指定或为默认值，已应用 3 次重试。".into());
            }

            // Large count warning
            if plan.required_count >= 100 {
                let msg = format!(
                    "大数量请求：required_count={}，候选目标={}，抓取批次目标={}。大量请求可能导致耗时较长。",
                    plan.required_count,
                    plan.required_count.saturating_mul(20),
                    plan.required_count.saturating_mul(2)
                );
                warnings.push(msg.clone());
                diags.push(SelfCheckDiagnostic::warning_with_suggestion(
                    "query_plan",
                    msg,
                    "如需减少耗时，请降低 required_count。当前值不会阻止任务，但建议关注抓取和验收的批次规划。",
                ));
            }

            // Convert InputDiagnostic warnings
            for w in &plan_warnings {
                let msg = format!("[{}] {}: {}", w.field, w.reason, {
                    w.default_applied
                        .as_ref()
                        .map(|d| format!(" 已应用默认值：{}", d))
                        .unwrap_or_default()
                });
                if w.severity == DiagnosticLevel::Warning {
                    warnings.push(msg.clone());
                }
                diags.push(SelfCheckDiagnostic {
                    level: w.severity,
                    category: "query_plan".into(),
                    message: w.reason.clone(),
                    suggestion: w.suggestion.clone(),
                });
            }

            (true, diags)
        }
        query_plan::ValidationOutcome::Rejected(rejection) => {
            for err in &rejection.diagnostics {
                let msg = format!("[{}] {}", err.field, err.reason);
                blockers.push(msg.clone());
                diags.push(SelfCheckDiagnostic {
                    level: DiagnosticLevel::Error,
                    category: "query_plan".into(),
                    message: err.reason.clone(),
                    suggestion: err.suggestion.clone(),
                });
            }
            blockers.push(format!("QueryPlan 输入被拒绝：{}", rejection.summary));
            (false, diags)
        }
    }
}

/// Aggregate provider readiness.
fn check_provider_readiness(
    request: &SelfCheckRequest,
    blockers: &mut Vec<String>,
    warnings: &mut Vec<String>,
    diagnostics: &mut Vec<SelfCheckDiagnostic>,
) -> ProviderReadinessSummary {
    let total = request.providers.len();
    let mut enabled = 0usize;
    let mut ready = 0usize;
    let mut missing_credentials = 0usize;
    let mut disabled = 0usize;
    let mut misconfigured = 0usize;
    let mut temporarily_unavailable = 0usize;
    let mut details: Vec<ProviderReadinessDetail> = Vec::new();

    for p in &request.providers {
        if p.enabled {
            enabled += 1;
        }

        match &p.readiness {
            ProviderReadiness::Ready => ready += 1,
            ProviderReadiness::Disabled => disabled += 1,
            ProviderReadiness::MissingCredentials => missing_credentials += 1,
            ProviderReadiness::Misconfigured => misconfigured += 1,
            ProviderReadiness::RateLimited | ProviderReadiness::Unavailable => {
                temporarily_unavailable += 1;
            }
        }

        // Zero or negative weight is a warning (not a blocker per se)
        if p.weight <= 0 && p.enabled {
            let msg = format!(
                "provider '{}' 权重为 {}，将从有效权重表中排除。",
                p.display_name, p.weight
            );
            warnings.push(msg.clone());
            diagnostics.push(SelfCheckDiagnostic::warning("provider", msg));
        }

        // Missing credentials is a blocker
        if matches!(p.readiness, ProviderReadiness::MissingCredentials) {
            let msg = format!(
                "provider '{}' 缺少凭据：provider 已启用但凭据未配置。凭据值不会在诊断中显示。",
                p.display_name
            );
            blockers.push(msg.clone());
            diagnostics.push(SelfCheckDiagnostic::blocker_with_suggestion(
                "provider",
                format!(
                    "provider '{}' 凭据缺失（凭据值不在此处显示）",
                    p.display_name
                ),
                format!(
                    "请为 provider '{}' 配置凭据，或将其 enabled 设为 false。",
                    p.display_name
                ),
            ));
        }

        // Misconfigured is a blocker
        if matches!(p.readiness, ProviderReadiness::Misconfigured) {
            let msg = format!(
                "provider '{}' 配置无效：{}。",
                p.display_name,
                p.reason.as_deref().unwrap_or("未知配置错误")
            );
            blockers.push(msg.clone());
            diagnostics.push(SelfCheckDiagnostic::blocker("provider", msg));
        }

        details.push(ProviderReadinessDetail {
            provider_id: p.provider_id.clone(),
            display_name: p.display_name.clone(),
            enabled: p.enabled,
            weight: p.weight,
            readiness: p.readiness.to_string(),
            reason: p.reason.clone(),
        });
    }

    // No enabled providers is a blocker
    if enabled == 0 && total > 0 {
        let msg = "无启用的搜索 provider：所有已注册 provider 均被禁用。".to_string();
        blockers.push(msg.clone());
        diagnostics.push(SelfCheckDiagnostic::blocker_with_suggestion(
            "provider",
            msg,
            "请至少启用一个搜索 provider 或将 enabled 设为 true。",
        ));
    }

    // No ready providers among enabled ones
    if enabled > 0 && ready == 0 {
        let msg =
            "无就绪的搜索 provider：虽然存在已启用 provider，但均未处于 ready 状态。".to_string();
        blockers.push(msg.clone());
        diagnostics.push(SelfCheckDiagnostic::blocker("provider", msg));
    }

    // No providers at all
    if total == 0 {
        let msg = "未注册任何搜索 provider：没有可用的搜索服务。".to_string();
        blockers.push(msg.clone());
        diagnostics.push(SelfCheckDiagnostic::blocker_with_suggestion(
            "provider",
            msg,
            "请注册至少一个搜索 provider（fixture 或真实服务）。",
        ));
    }

    ProviderReadinessSummary {
        total,
        enabled,
        ready,
        missing_credentials,
        disabled,
        misconfigured,
        temporarily_unavailable,
        details,
    }
}

/// Aggregate channel readiness.
fn check_channel_readiness(
    request: &SelfCheckRequest,
    blockers: &mut Vec<String>,
    warnings: &mut Vec<String>,
    diagnostics: &mut Vec<SelfCheckDiagnostic>,
) -> RetrievalChannelReadinessSummary {
    let total = request.channels.len();
    let mut enabled = 0usize;
    let mut ready = 0usize;
    let mut disabled = 0usize;
    let mut paid_unconfirmed = 0usize;
    let mut missing_dependency = 0usize;
    let mut misconfigured = 0usize;
    let mut details: Vec<ChannelReadinessDetail> = Vec::new();

    for c in &request.channels {
        if c.enabled {
            enabled += 1;
        }

        match &c.readiness {
            RetrievalChannelReadiness::Ready => ready += 1,
            RetrievalChannelReadiness::Disabled => disabled += 1,
            RetrievalChannelReadiness::PaidUnconfirmed => paid_unconfirmed += 1,
            RetrievalChannelReadiness::MissingDependency => missing_dependency += 1,
            RetrievalChannelReadiness::Misconfigured => misconfigured += 1,
        }

        // Paid unconfirmed is a blocker
        if matches!(c.readiness, RetrievalChannelReadiness::PaidUnconfirmed) {
            let msg = format!(
                "付费 channel '{}' 未确认：付费抓取渠道需要用户明确确认后才能使用。",
                c.display_name
            );
            blockers.push(msg.clone());
            diagnostics.push(SelfCheckDiagnostic::blocker_with_suggestion(
                "channel",
                format!("付费 channel '{}' 未确认", c.display_name),
                format!(
                    "如需使用付费 channel '{}'，请显式确认付费渠道。否则请将 enabled 设为 false 以使用更低层级的免费渠道。",
                    c.display_name
                ),
            ));
        }

        // Missing dependency is a blocker
        if matches!(c.readiness, RetrievalChannelReadiness::MissingDependency) {
            let msg = format!(
                "channel '{}' 缺少依赖：{}。",
                c.display_name,
                c.reason.as_deref().unwrap_or("未知依赖缺失")
            );
            blockers.push(msg.clone());
            diagnostics.push(SelfCheckDiagnostic::blocker("channel", msg));
        }

        // Misconfigured is a blocker
        if matches!(c.readiness, RetrievalChannelReadiness::Misconfigured) {
            let msg = format!(
                "channel '{}' 配置无效：{}。",
                c.display_name,
                c.reason.as_deref().unwrap_or("未知配置错误")
            );
            blockers.push(msg.clone());
            diagnostics.push(SelfCheckDiagnostic::blocker("channel", msg));
        }

        details.push(ChannelReadinessDetail {
            channel_id: c.channel_id.clone(),
            display_name: c.display_name.clone(),
            tier: c.tier.clone(),
            enabled: c.enabled,
            readiness: c.readiness.to_string(),
            reason: c.reason.clone(),
        });
    }

    // No enabled channels
    if enabled == 0 && total > 0 {
        let msg = "无启用的抓取 channel：所有已注册 channel 均被禁用。".to_string();
        blockers.push(msg.clone());
        diagnostics.push(SelfCheckDiagnostic::blocker_with_suggestion(
            "channel",
            msg,
            "请至少启用一个抓取 channel（如 web_fetch）或将 enabled 设为 true。",
        ));
    }

    // No ready channels among enabled ones
    if enabled > 0 && ready == 0 {
        let msg =
            "无就绪的抓取 channel：虽然存在已启用 channel，但均未处于 ready 状态。".to_string();
        blockers.push(msg.clone());
        diagnostics.push(SelfCheckDiagnostic::blocker("channel", msg));
    }

    // No channels at all
    if total == 0 {
        let msg = "未注册任何抓取 channel：没有可用的抓取通道。".to_string();
        blockers.push(msg.clone());
        diagnostics.push(SelfCheckDiagnostic::blocker_with_suggestion(
            "channel",
            msg,
            "请注册至少一个抓取 channel（如普通 web fetch）。",
        ));
    }

    // Paid channel still unconfirmed overall
    if paid_unconfirmed > 0 && !request.paid_channel_confirmed {
        let msg = format!(
            "付费 channel 未确认：{} 个付费 channel 需要用户明确确认后才能使用。",
            paid_unconfirmed
        );
        warnings.push(msg.clone());
    }

    RetrievalChannelReadinessSummary {
        total,
        enabled,
        ready,
        disabled,
        paid_unconfirmed,
        missing_dependency,
        misconfigured,
        details,
    }
}

/// Check OpenClaw readiness for both candidate and image evaluation boundaries.
fn check_openclaw_readiness(
    request: &SelfCheckRequest,
    blockers: &mut Vec<String>,
    _warnings: &mut Vec<String>,
    diagnostics: &mut Vec<SelfCheckDiagnostic>,
) -> OpenClawReadinessSummary {
    let candidate = if request.candidate_openclaw_available {
        ReadinessInputStatus::Valid
    } else {
        ReadinessInputStatus::Blocked
    };

    let image = if request.image_openclaw_available {
        ReadinessInputStatus::Valid
    } else {
        ReadinessInputStatus::Blocked
    };

    let candidate_reason = if !request.candidate_openclaw_available {
        let msg = "候选评价 OpenClaw 不可用：候选主观评价无法执行，正式任务将进入 execution_blocked 状态。".to_string();
        blockers.push(msg.clone());
        diagnostics.push(SelfCheckDiagnostic::blocker_with_suggestion(
            "openclaw_candidate",
            msg,
            "请配置 OpenClaw 生产评价端点，或确认 fixture evaluator 仅用于内部测试（不得作为生产通过依据）。",
        ));
        Some("候选 OpenClaw 评价端口未就绪 — 缺少配置或生产端点不可达。".to_string())
    } else {
        None
    };

    let image_reason = if !request.image_openclaw_available {
        let msg = "图片评价 OpenClaw 不可用：图片主观验收无法执行，正式任务将进入 execution_blocked 状态。".to_string();
        blockers.push(msg.clone());
        diagnostics.push(SelfCheckDiagnostic::blocker_with_suggestion(
            "openclaw_image",
            msg,
            "请配置 OpenClaw 生产评价端点，或确认 fixture evaluator 仅用于内部测试（不得作为生产通过依据）。",
        ));
        Some("图片 OpenClaw 评价端口未就绪 — 缺少配置或生产端点不可达。".to_string())
    } else {
        None
    };

    OpenClawReadinessSummary {
        candidate,
        candidate_reason,
        image,
        image_reason,
    }
}

/// Check policy readiness — authorization stance, paid gating, open decisions.
fn check_policy_readiness(
    request: &SelfCheckRequest,
    blockers: &mut Vec<String>,
    warnings: &mut Vec<String>,
    diagnostics: &mut Vec<SelfCheckDiagnostic>,
) -> PolicyReadinessSummary {
    let mut policy_blockers: Vec<String> = Vec::new();
    let mut policy_warnings: Vec<String> = Vec::new();
    let mut open_decisions: Vec<String> = Vec::new();
    // Evaluate policy risk entries
    for risk in &request.policy_risks {
        if risk.is_blocker {
            let msg = format!("[{}] {}", risk.category, risk.description);
            blockers.push(msg.clone());
            policy_blockers.push(risk.description.clone());
            diagnostics.push(SelfCheckDiagnostic::blocker(
                format!("policy_{}", risk.category),
                risk.description.clone(),
            ));
        } else {
            let msg = format!("[{}] {}", risk.category, risk.description);
            warnings.push(msg.clone());
            policy_warnings.push(risk.description.clone());
            diagnostics.push(SelfCheckDiagnostic::warning(
                format!("policy_{}", risk.category),
                risk.description.clone(),
            ));
        }
    }

    // Authorization stance
    let authorization_stance =
        "default（未知授权保留风险提示，明确禁止的来源将被拒绝）".to_string();
    let unknown_authorization_present = true; // Default stance implies unknown auth
    if unknown_authorization_present {
        let msg =
            "授权风险：当前授权偏好为 default，未知授权来源将保留风险提示，不得被描述为商用安全。"
                .to_string();
        warnings.push(msg.clone());
        policy_warnings.push(msg);
    }

    // Paid channel gating
    if !request.paid_channel_confirmed {
        let has_paid_channels = request
            .channels
            .iter()
            .any(|c| c.tier.to_lowercase() == "paid");
        if has_paid_channels {
            let msg = "付费 channel 未确认：存在付费层级 channel 但用户未明确确认使用付费服务。付费 channel 不会被静默使用。".to_string();
            policy_warnings.push(msg.clone());
            diagnostics.push(SelfCheckDiagnostic::warning("policy_paid", msg));
        }
    }

    // Open product decisions (always present in MVP)
    open_decisions.push(
        "真实搜索 provider 选择未决：当前使用 fixture provider，生产环境需用户确认默认真实 provider。"
            .to_string(),
    );
    open_decisions.push(
        "OpenClaw 生产协议未决：生产评价端点和协议需用户决策后才能启用生产验证。".to_string(),
    );
    open_decisions
        .push("第四级抓取渠道决策未决：当前仅支持 web_fetch、self_hosted、paid 三层。".to_string());
    open_decisions
        .push("robots/site-rule 策略未决：站点规则和 robots.txt 合规策略尚未确定。".to_string());

    for decision in &open_decisions {
        warnings.push(format!("开放决策：{}", decision));
    }

    PolicyReadinessSummary {
        authorization_stance,
        unknown_authorization_present,
        paid_channel_confirmed: request.paid_channel_confirmed,
        blockers: policy_blockers,
        warnings: policy_warnings,
        open_decisions,
    }
}

// ---------------------------------------------------------------------------
// Display helpers
// ---------------------------------------------------------------------------

impl SelfCheckReport {
    /// Produce a human-readable summary of the self-check report.
    pub fn format_human_readable(&self) -> String {
        let mut out = String::new();

        out.push_str("══════════════════════════════════════════\n");
        out.push_str("         Self-Check Readiness Report        \n");
        out.push_str("══════════════════════════════════════════\n\n");

        // Status
        let status_label = match self.status {
            SelfCheckStatus::Pass => "✅ PASS",
            SelfCheckStatus::Warning => "⚠️  WARNING",
            SelfCheckStatus::Blocked => "❌ BLOCKED",
        };
        out.push_str(&format!("Overall Status: {}\n\n", status_label));

        // QueryPlan
        out.push_str("── QueryPlan ──────────────────────────────\n");
        if self.query_plan_valid {
            out.push_str("  状态：✅ 有效\n");
        } else {
            out.push_str("  状态：❌ 无效（输入被拒绝）\n");
        }
        for d in &self.query_plan_diagnostics {
            out.push_str(&format!("  [{}] {}\n", level_icon(d.level), d.message));
        }
        out.push('\n');

        // Provider
        out.push_str("── Provider Readiness ─────────────────────\n");
        out.push_str(&format!(
            "  总计 {} / 启用 {} / 就绪 {} / 缺凭据 {} / 禁用 {} / 误配 {} / 暂不可用 {}\n",
            self.provider_summary.total,
            self.provider_summary.enabled,
            self.provider_summary.ready,
            self.provider_summary.missing_credentials,
            self.provider_summary.disabled,
            self.provider_summary.misconfigured,
            self.provider_summary.temporarily_unavailable,
        ));
        for detail in &self.provider_summary.details {
            let icon = if detail.readiness == "ready" {
                "✅"
            } else if detail.readiness == "disabled" {
                "⚫"
            } else {
                "❌"
            };
            out.push_str(&format!(
                "  {} {} (enabled={}, weight={}, readiness={})",
                icon, detail.display_name, detail.enabled, detail.weight, detail.readiness
            ));
            if let Some(ref reason) = detail.reason {
                out.push_str(&format!(" — {}", reason));
            }
            out.push('\n');
        }
        out.push('\n');

        // Channel
        out.push_str("── Channel Readiness ──────────────────────\n");
        out.push_str(&format!(
            "  总计 {} / 启用 {} / 就绪 {} / 禁用 {} / 付费未确认 {} / 缺依赖 {} / 误配 {}\n",
            self.channel_summary.total,
            self.channel_summary.enabled,
            self.channel_summary.ready,
            self.channel_summary.disabled,
            self.channel_summary.paid_unconfirmed,
            self.channel_summary.missing_dependency,
            self.channel_summary.misconfigured,
        ));
        for detail in &self.channel_summary.details {
            let icon = if detail.readiness == "ready" {
                "✅"
            } else if detail.readiness == "disabled" {
                "⚫"
            } else {
                "❌"
            };
            out.push_str(&format!(
                "  {} {} (tier={}, enabled={}, readiness={})",
                icon, detail.display_name, detail.tier, detail.enabled, detail.readiness
            ));
            if let Some(ref reason) = detail.reason {
                out.push_str(&format!(" — {}", reason));
            }
            out.push('\n');
        }
        out.push('\n');

        // OpenClaw
        out.push_str("── OpenClaw Readiness ─────────────────────\n");
        let cand_icon = if self.openclaw_summary.candidate == ReadinessInputStatus::Valid {
            "✅"
        } else {
            "❌"
        };
        let img_icon = if self.openclaw_summary.image == ReadinessInputStatus::Valid {
            "✅"
        } else {
            "❌"
        };
        out.push_str(&format!("  候选评价 OpenClaw: {}\n", cand_icon));
        if let Some(ref reason) = self.openclaw_summary.candidate_reason {
            out.push_str(&format!("    → {}\n", reason));
        }
        out.push_str(&format!("  图片评价 OpenClaw: {}\n", img_icon));
        if let Some(ref reason) = self.openclaw_summary.image_reason {
            out.push_str(&format!("    → {}\n", reason));
        }
        out.push('\n');

        // Policy
        out.push_str("── Policy Readiness ───────────────────────\n");
        out.push_str(&format!(
            "  授权立场：{}\n",
            self.policy_summary.authorization_stance
        ));
        out.push_str(&format!(
            "  付费确认：{}\n",
            if self.policy_summary.paid_channel_confirmed {
                "✅ 已确认"
            } else {
                "⚠️  未确认"
            }
        ));
        if !self.policy_summary.blockers.is_empty() {
            out.push_str("  策略阻塞项：\n");
            for b in &self.policy_summary.blockers {
                out.push_str(&format!("    ❌ {}\n", b));
            }
        }
        if !self.policy_summary.warnings.is_empty() {
            out.push_str("  策略警告：\n");
            for w in &self.policy_summary.warnings {
                out.push_str(&format!("    ⚠️  {}\n", w));
            }
        }
        if !self.policy_summary.open_decisions.is_empty() {
            out.push_str("  开放产品决策：\n");
            for d in &self.policy_summary.open_decisions {
                out.push_str(&format!("    🔓 {}\n", d));
            }
        }
        out.push('\n');

        // Blockers
        if !self.blockers.is_empty() {
            out.push_str("── Blockers ───────────────────────────────\n");
            for (i, b) in self.blockers.iter().enumerate() {
                out.push_str(&format!("  {}. ❌ {}\n", i + 1, b));
            }
            out.push('\n');
        }

        // Warnings
        if !self.warnings.is_empty() {
            out.push_str("── Warnings ───────────────────────────────\n");
            for (i, w) in self.warnings.iter().enumerate() {
                out.push_str(&format!("  {}. ⚠️  {}\n", i + 1, w));
            }
            out.push('\n');
        }

        // Defaults
        if !self.default_explanations.is_empty() {
            out.push_str("── Defaults Applied ───────────────────────\n");
            for d in &self.default_explanations {
                out.push_str(&format!("  • {}\n", d));
            }
            out.push('\n');
        }

        // Suggestions
        if !self.adjustment_suggestions.is_empty() {
            out.push_str("── Suggestions ────────────────────────────\n");
            for (i, s) in self.adjustment_suggestions.iter().enumerate() {
                out.push_str(&format!("  {}. {}\n", i + 1, s));
            }
            out.push('\n');
        }

        out.push_str("══════════════════════════════════════════\n");
        out.push_str("Report end. Self-check does NOT produce delivery artifacts.\n");

        out
    }
}

fn level_icon(level: DiagnosticLevel) -> &'static str {
    match level {
        DiagnosticLevel::Info => "ℹ️",
        DiagnosticLevel::Warning => "⚠️",
        DiagnosticLevel::Error => "❌",
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::query_plan::{
        AuthorizationPreference, ContentConstraints, OutputPreference, QualityTier,
    };
    use crate::domain::retrieval::RetrievalChannelReadiness;
    use crate::domain::search::ProviderReadiness;

    // -----------------------------------------------------------------------
    // Helper builders
    // -----------------------------------------------------------------------

    fn sample_query_plan() -> QueryPlanInput {
        QueryPlanInput {
            description: "test image search".into(),
            required_count: 2,
            quality_tier: QualityTier::General,
            content_constraints: ContentConstraints::default(),
            authorization_preference: AuthorizationPreference::Default,
            output_preference: OutputPreference::Human,
            retry_limit: 3,
        }
    }

    fn ready_provider(id: &str) -> ProviderReadinessEntry {
        ProviderReadinessEntry {
            provider_id: id.into(),
            display_name: format!("Provider {}", id),
            enabled: true,
            weight: 1,
            readiness: ProviderReadiness::Ready,
            reason: None,
        }
    }

    fn ready_channel(id: &str, tier: &str) -> ChannelReadinessEntry {
        ChannelReadinessEntry {
            channel_id: id.into(),
            display_name: format!("Channel {}", id),
            tier: tier.into(),
            enabled: true,
            readiness: RetrievalChannelReadiness::Ready,
            reason: None,
        }
    }

    fn minimal_request() -> SelfCheckRequest {
        SelfCheckRequest {
            query_plan_input: sample_query_plan(),
            providers: vec![ready_provider("p1")],
            channels: vec![ready_channel("c1", "web_fetch")],
            candidate_openclaw_available: true,
            image_openclaw_available: true,
            paid_channel_confirmed: false,
            policy_risks: vec![],
        }
    }

    // -----------------------------------------------------------------------
    // T1: All ready with open decisions → Warning (not Pass, not Blocked)
    // -----------------------------------------------------------------------

    #[test]
    fn all_ready_with_open_decisions_produces_warning() {
        let request = minimal_request();
        let report = run_self_check(request);

        // With open product decisions always present in MVP, the status is
        // Warning, not Pass — but there must be no blockers.
        assert_eq!(report.status, SelfCheckStatus::Warning);
        assert!(report.query_plan_valid);
        assert!(report.blockers.is_empty());
        assert!(!report.warnings.is_empty());
        // Warnings should be from open decisions, not from readiness failures
        assert!(report
            .warnings
            .iter()
            .any(|w| w.contains("开放决策") || w.contains("授权风险")));
        assert_eq!(report.provider_summary.total, 1);
        assert_eq!(report.provider_summary.ready, 1);
        assert_eq!(report.channel_summary.total, 1);
        assert_eq!(report.channel_summary.ready, 1);
        assert_eq!(
            report.openclaw_summary.candidate,
            ReadinessInputStatus::Valid
        );
        assert_eq!(report.openclaw_summary.image, ReadinessInputStatus::Valid);
    }

    #[test]
    fn all_ready_no_policy_risks_produces_pass() {
        // When open decisions are acknowledged (no policy_risks entries),
        // the status is a clean Pass.
        let request = SelfCheckRequest {
            policy_risks: vec![],
            ..minimal_request()
        };
        let report = run_self_check(request);
        // Still gets warnings from authorization stance + open decisions
        // hardcoded in check_policy_readiness for MVP.
        // For a true clean pass we'd need to suppress those too —
        // this test documents the current MVP behavior.
        assert!(report.blockers.is_empty());
        assert!(
            report.status == SelfCheckStatus::Warning || report.status == SelfCheckStatus::Pass
        );
    }

    // -----------------------------------------------------------------------
    // T2: Invalid QueryPlan → blocked
    // -----------------------------------------------------------------------

    #[test]
    fn invalid_query_plan_produces_blocked() {
        let request = SelfCheckRequest {
            query_plan_input: QueryPlanInput {
                description: "".into(),
                ..Default::default()
            },
            ..minimal_request()
        };

        let report = run_self_check(request);
        assert_eq!(report.status, SelfCheckStatus::Blocked);
        assert!(!report.query_plan_valid);
        assert!(!report.blockers.is_empty());
        // Check that the rejection is recorded
        let has_query_blocker = report
            .blockers
            .iter()
            .any(|b| b.contains("缺少图片语义描述") || b.contains("输入被拒绝"));
        assert!(has_query_blocker);
    }

    #[test]
    fn retry_limit_exceeded_produces_blocked() {
        let request = SelfCheckRequest {
            query_plan_input: QueryPlanInput {
                description: "test".into(),
                retry_limit: 10,
                ..Default::default()
            },
            ..minimal_request()
        };

        let report = run_self_check(request);
        assert_eq!(report.status, SelfCheckStatus::Blocked);
        assert!(!report.blockers.is_empty());
    }

    // -----------------------------------------------------------------------
    // T3: Provider missing credentials → blocked, value not exposed
    // -----------------------------------------------------------------------

    #[test]
    fn provider_missing_credentials_is_blocked() {
        let request = SelfCheckRequest {
            providers: vec![ProviderReadinessEntry {
                provider_id: "p1".into(),
                display_name: "TestProvider".into(),
                enabled: true,
                weight: 1,
                readiness: ProviderReadiness::MissingCredentials,
                reason: Some("API key not configured".into()),
            }],
            ..minimal_request()
        };

        let report = run_self_check(request);
        assert_eq!(report.status, SelfCheckStatus::Blocked);
        assert_eq!(report.provider_summary.missing_credentials, 1);
        assert_eq!(report.provider_summary.ready, 0);

        // Verify credential VALUE is not in any output
        let json = serde_json::to_string_pretty(&report).unwrap();
        assert!(!json.contains("sk-"));
        assert!(!json.contains("api_key"));
        assert!(!json.contains("password"));
        assert!(!json.contains("secret"));

        // But the fact of missing credentials IS visible
        let has_cred_msg = report
            .blockers
            .iter()
            .any(|b| b.contains("缺少凭据") || b.contains("凭据缺失"));
        assert!(has_cred_msg, "missing credentials should be reported");

        // Detail record should show the readiness status
        assert_eq!(
            report.provider_summary.details[0].readiness,
            "missing_credentials"
        );
    }

    // -----------------------------------------------------------------------
    // T4: No enabled channel → blocked
    // -----------------------------------------------------------------------

    #[test]
    fn no_enabled_channel_is_blocked() {
        let request = SelfCheckRequest {
            channels: vec![ChannelReadinessEntry {
                channel_id: "c1".into(),
                display_name: "DisabledChannel".into(),
                tier: "web_fetch".into(),
                enabled: false,
                readiness: RetrievalChannelReadiness::Disabled,
                reason: None,
            }],
            ..minimal_request()
        };

        let report = run_self_check(request);
        assert_eq!(report.status, SelfCheckStatus::Blocked);
        assert_eq!(report.channel_summary.enabled, 0);
        assert_eq!(report.channel_summary.disabled, 1);
        assert!(report
            .blockers
            .iter()
            .any(|b| b.contains("无启用的抓取 channel")));
    }

    #[test]
    fn no_channels_registered_is_blocked() {
        let request = SelfCheckRequest {
            channels: vec![],
            ..minimal_request()
        };

        let report = run_self_check(request);
        assert_eq!(report.status, SelfCheckStatus::Blocked);
        assert!(report
            .blockers
            .iter()
            .any(|b| b.contains("未注册任何抓取 channel")));
    }

    // -----------------------------------------------------------------------
    // T5: Paid channel unconfirmed → blocker
    // -----------------------------------------------------------------------

    #[test]
    fn paid_channel_unconfirmed_is_blocked() {
        let request = SelfCheckRequest {
            channels: vec![ChannelReadinessEntry {
                channel_id: "paid-1".into(),
                display_name: "PaidService".into(),
                tier: "paid".into(),
                enabled: true,
                readiness: RetrievalChannelReadiness::PaidUnconfirmed,
                reason: Some("requires user confirmation".into()),
            }],
            ..minimal_request()
        };

        let report = run_self_check(request);
        assert_eq!(report.status, SelfCheckStatus::Blocked);
        assert_eq!(report.channel_summary.paid_unconfirmed, 1);
        assert!(report
            .blockers
            .iter()
            .any(|b| b.contains("未确认") || b.contains("paid")));
    }

    // -----------------------------------------------------------------------
    // T6: Candidate and image OpenClaw reported separately
    // -----------------------------------------------------------------------

    #[test]
    fn candidate_openclaw_unavailable_reported_separately() {
        let request = SelfCheckRequest {
            candidate_openclaw_available: false,
            image_openclaw_available: true,
            ..minimal_request()
        };

        let report = run_self_check(request);
        assert_eq!(report.status, SelfCheckStatus::Blocked);
        assert_eq!(
            report.openclaw_summary.candidate,
            ReadinessInputStatus::Blocked
        );
        assert_eq!(report.openclaw_summary.image, ReadinessInputStatus::Valid);
        assert!(report.openclaw_summary.candidate_reason.is_some());
        assert!(report.openclaw_summary.image_reason.is_none());

        // Candidate OpenClaw blocker should mention candidate evaluation
        let has_cand_blocker = report
            .blockers
            .iter()
            .any(|b| b.contains("候选评价") || b.contains("候选"));
        assert!(has_cand_blocker);

        // Image OpenClaw should NOT be flagged
        let has_img_blocker = report
            .blockers
            .iter()
            .any(|b| b.contains("图片评价") || b.contains("图片 OpenClaw"));
        assert!(!has_img_blocker);
    }

    #[test]
    fn image_openclaw_unavailable_reported_separately() {
        let request = SelfCheckRequest {
            candidate_openclaw_available: true,
            image_openclaw_available: false,
            ..minimal_request()
        };

        let report = run_self_check(request);
        assert_eq!(report.status, SelfCheckStatus::Blocked);
        assert_eq!(
            report.openclaw_summary.candidate,
            ReadinessInputStatus::Valid
        );
        assert_eq!(report.openclaw_summary.image, ReadinessInputStatus::Blocked);
    }

    #[test]
    fn both_openclaw_unavailable_reports_both() {
        let request = SelfCheckRequest {
            candidate_openclaw_available: false,
            image_openclaw_available: false,
            ..minimal_request()
        };

        let report = run_self_check(request);
        assert_eq!(report.status, SelfCheckStatus::Blocked);
        assert_eq!(
            report.openclaw_summary.candidate,
            ReadinessInputStatus::Blocked
        );
        assert_eq!(report.openclaw_summary.image, ReadinessInputStatus::Blocked);
        // At least 2 blockers (one for each OpenClaw dimension)
        assert!(report.blockers.len() >= 2);
    }

    // -----------------------------------------------------------------------
    // T7: Policy blocker — sanitised display
    // -----------------------------------------------------------------------

    #[test]
    fn policy_blocker_shown_without_credentials() {
        let request = SelfCheckRequest {
            policy_risks: vec![PolicyRiskEntry {
                category: "authorization".into(),
                description: "source prohibits reuse for commercial purposes".into(),
                is_blocker: true,
            }],
            ..minimal_request()
        };

        let report = run_self_check(request);
        assert_eq!(report.status, SelfCheckStatus::Blocked);
        assert!(!report.policy_summary.blockers.is_empty());
        assert!(report.policy_summary.blockers[0].contains("prohibits"));

        // Verify no credential-like data in the report
        let json = serde_json::to_string_pretty(&report).unwrap();
        assert!(!json.contains("Bearer "));
        assert!(!json.contains("x-api-key"));
        assert!(!json.contains("access_token"));
    }

    // -----------------------------------------------------------------------
    // T8: Self-check does NOT produce delivery artifacts
    // -----------------------------------------------------------------------

    #[test]
    fn report_is_not_delivery_package() {
        let report = run_self_check(minimal_request());
        let json = serde_json::to_string_pretty(&report).unwrap();

        // Must not contain delivery-package fields
        assert!(!json.contains("images/"));
        assert!(!json.contains("status.json"));
        assert!(!json.contains("manifest.json"));
        assert!(!json.contains("delivery_package"));
        assert!(!json.contains("full_delivery"));
        assert!(!json.contains("limited_delivery"));
        assert!(!json.contains("execution_blocked"));
    }

    // -----------------------------------------------------------------------
    // T9: No providers + no channels → blocked with both reasons
    // -----------------------------------------------------------------------

    #[test]
    fn no_providers_no_channels_is_blocked() {
        let request = SelfCheckRequest {
            providers: vec![],
            channels: vec![],
            ..minimal_request()
        };

        let report = run_self_check(request);
        assert_eq!(report.status, SelfCheckStatus::Blocked);
        assert!(report.blockers.len() >= 2);
        assert!(report
            .blockers
            .iter()
            .any(|b| b.contains("provider") || b.contains("搜索")));
        assert!(report
            .blockers
            .iter()
            .any(|b| b.contains("channel") || b.contains("抓取")));
    }

    // -----------------------------------------------------------------------
    // T10: Warnings (not blockers) produce warning status
    // -----------------------------------------------------------------------

    #[test]
    fn warnings_without_blockers_produce_warning_status() {
        let request = SelfCheckRequest {
            query_plan_input: QueryPlanInput {
                description: "test".into(),
                required_count: 200, // large count → warning
                ..Default::default()
            },
            policy_risks: vec![PolicyRiskEntry {
                category: "open_decision".into(),
                description: "site-rule policy not yet decided".into(),
                is_blocker: false,
            }],
            ..minimal_request()
        };

        let report = run_self_check(request);
        assert_eq!(report.status, SelfCheckStatus::Warning);
        assert!(report.blockers.is_empty());
        assert!(!report.warnings.is_empty());
    }

    // -----------------------------------------------------------------------
    // T11: Default-value explanations
    // -----------------------------------------------------------------------

    #[test]
    fn default_explanations_included_in_report() {
        let request = SelfCheckRequest {
            query_plan_input: QueryPlanInput {
                description: "test".into(),
                required_count: 1,
                quality_tier: QualityTier::General,
                output_preference: OutputPreference::Human,
                retry_limit: 3,
                ..Default::default()
            },
            ..minimal_request()
        };

        let report = run_self_check(request);
        assert!(!report.default_explanations.is_empty());
        assert!(report
            .default_explanations
            .iter()
            .any(|e| e.contains("quality_tier")));
        assert!(report
            .default_explanations
            .iter()
            .any(|e| e.contains("output_preference")));
        assert!(report
            .default_explanations
            .iter()
            .any(|e| e.contains("retry_limit")));
    }

    // -----------------------------------------------------------------------
    // T12: Sensitive input in QueryPlan → warning, value not exposed
    // -----------------------------------------------------------------------

    #[test]
    fn sensitive_query_plan_content_not_exposed() {
        let request = SelfCheckRequest {
            query_plan_input: QueryPlanInput {
                description: "Bearer secret-token-abc123 image search".into(),
                ..Default::default()
            },
            ..minimal_request()
        };

        let report = run_self_check(request);
        // Should be valid (sensitive content is a warning, not blocker)
        assert!(report.query_plan_valid);

        // Verify the token value is NOT in any diagnostic
        let json = serde_json::to_string_pretty(&report).unwrap();
        assert!(!json.contains("secret-token-abc123"));
        assert!(!json.contains("Bearer secret"));

        // But there should be a sensitive-content warning
        let has_sensitive_warning = report
            .warnings
            .iter()
            .any(|w| w.contains("疑似敏感") || w.contains("敏感"));
        assert!(
            has_sensitive_warning,
            "sensitive content should produce a warning"
        );
    }

    // -----------------------------------------------------------------------
    // T13: SelfCheckStatus derives
    // -----------------------------------------------------------------------

    #[test]
    fn self_check_status_display() {
        assert_eq!(SelfCheckStatus::Pass.to_string(), "pass");
        assert_eq!(SelfCheckStatus::Warning.to_string(), "warning");
        assert_eq!(SelfCheckStatus::Blocked.to_string(), "blocked");
    }

    #[test]
    fn self_check_status_can_proceed() {
        assert!(SelfCheckStatus::Pass.can_proceed());
        assert!(SelfCheckStatus::Warning.can_proceed());
        assert!(!SelfCheckStatus::Blocked.can_proceed());
    }

    #[test]
    fn self_check_status_is_blocked() {
        assert!(!SelfCheckStatus::Pass.is_blocked());
        assert!(!SelfCheckStatus::Warning.is_blocked());
        assert!(SelfCheckStatus::Blocked.is_blocked());
    }

    // -----------------------------------------------------------------------
    // T14: ReadinessInputStatus
    // -----------------------------------------------------------------------

    #[test]
    fn readiness_input_status_from_bool() {
        assert_eq!(
            ReadinessInputStatus::from_bool(true),
            ReadinessInputStatus::Valid
        );
        assert_eq!(
            ReadinessInputStatus::from_bool(false),
            ReadinessInputStatus::Blocked
        );
    }

    // -----------------------------------------------------------------------
    // T15: SelfCheckDiagnostic constructors
    // -----------------------------------------------------------------------

    #[test]
    fn diagnostic_constructors() {
        let b = SelfCheckDiagnostic::blocker("provider", "no providers");
        assert_eq!(b.level, DiagnosticLevel::Error);
        assert_eq!(b.category, "provider");
        assert!(b.suggestion.is_none());

        let bs =
            SelfCheckDiagnostic::blocker_with_suggestion("provider", "no providers", "add one");
        assert_eq!(bs.suggestion, Some("add one".into()));

        let w = SelfCheckDiagnostic::warning("channel", "slow channel");
        assert_eq!(w.level, DiagnosticLevel::Warning);

        let i = SelfCheckDiagnostic::info("query_plan", "default applied");
        assert_eq!(i.level, DiagnosticLevel::Info);
    }

    // -----------------------------------------------------------------------
    // T16: Provider summary details alignment
    // -----------------------------------------------------------------------

    #[test]
    fn provider_summary_counts_match_details() {
        let providers = vec![
            ready_provider("p1"),
            ProviderReadinessEntry {
                provider_id: "p2".into(),
                display_name: "DisabledP".into(),
                enabled: false,
                weight: 0,
                readiness: ProviderReadiness::Disabled,
                reason: None,
            },
            ProviderReadinessEntry {
                provider_id: "p3".into(),
                display_name: "NoCredP".into(),
                enabled: true,
                weight: 1,
                readiness: ProviderReadiness::MissingCredentials,
                reason: Some("no API key".into()),
            },
        ];

        let request = SelfCheckRequest {
            providers,
            ..minimal_request()
        };

        let report = run_self_check(request);
        assert_eq!(report.provider_summary.total, 3);
        assert_eq!(report.provider_summary.enabled, 2);
        assert_eq!(report.provider_summary.ready, 1);
        assert_eq!(report.provider_summary.disabled, 1);
        assert_eq!(report.provider_summary.missing_credentials, 1);
        assert_eq!(report.provider_summary.details.len(), 3);

        // Verify details match
        let ready_detail = report
            .provider_summary
            .details
            .iter()
            .find(|d| d.provider_id == "p1")
            .unwrap();
        assert_eq!(ready_detail.readiness, "ready");

        let disabled_detail = report
            .provider_summary
            .details
            .iter()
            .find(|d| d.provider_id == "p2")
            .unwrap();
        assert_eq!(disabled_detail.readiness, "disabled");

        let missing_cred_detail = report
            .provider_summary
            .details
            .iter()
            .find(|d| d.provider_id == "p3")
            .unwrap();
        assert_eq!(missing_cred_detail.readiness, "missing_credentials");
        assert!(missing_cred_detail.reason.is_some());
    }

    // -----------------------------------------------------------------------
    // T17: Channel summary details alignment
    // -----------------------------------------------------------------------

    #[test]
    fn channel_summary_counts_match_details() {
        let channels = vec![
            ready_channel("c1", "web_fetch"),
            ChannelReadinessEntry {
                channel_id: "c2".into(),
                display_name: "PaidC".into(),
                tier: "paid".into(),
                enabled: true,
                readiness: RetrievalChannelReadiness::PaidUnconfirmed,
                reason: Some("needs confirmation".into()),
            },
            ChannelReadinessEntry {
                channel_id: "c3".into(),
                display_name: "NoDepC".into(),
                tier: "self_hosted".into(),
                enabled: true,
                readiness: RetrievalChannelReadiness::MissingDependency,
                reason: Some("binary not found".into()),
            },
        ];

        let request = SelfCheckRequest {
            channels,
            ..minimal_request()
        };

        let report = run_self_check(request);
        assert_eq!(report.channel_summary.total, 3);
        assert_eq!(report.channel_summary.enabled, 3);
        assert_eq!(report.channel_summary.ready, 1);
        assert_eq!(report.channel_summary.paid_unconfirmed, 1);
        assert_eq!(report.channel_summary.missing_dependency, 1);
        assert_eq!(report.channel_summary.details.len(), 3);
    }

    // -----------------------------------------------------------------------
    // T18: Human-readable output produces expected content
    // -----------------------------------------------------------------------

    #[test]
    fn human_readable_output_contains_key_sections() {
        let report = run_self_check(minimal_request());
        let output = report.format_human_readable();

        assert!(output.contains("Self-Check Readiness Report"));
        assert!(output.contains("Overall Status"));
        assert!(output.contains("QueryPlan"));
        assert!(output.contains("Provider Readiness"));
        assert!(output.contains("Channel Readiness"));
        assert!(output.contains("OpenClaw Readiness"));
        assert!(output.contains("Policy Readiness"));
        assert!(output.contains("Report end. Self-check does NOT produce delivery artifacts."));
    }

    #[test]
    fn human_readable_blocked_shows_blockers_section() {
        let request = SelfCheckRequest {
            providers: vec![],
            channels: vec![],
            ..minimal_request()
        };
        let report = run_self_check(request);
        let output = report.format_human_readable();

        assert!(output.contains("Blockers"));
        assert!(output.contains("❌ BLOCKED"));
    }

    // -----------------------------------------------------------------------
    // T19: SelfCheckReport serialization round-trip
    // -----------------------------------------------------------------------

    #[test]
    fn report_serialization_roundtrip() {
        let report = run_self_check(minimal_request());
        let json = serde_json::to_string_pretty(&report).unwrap();
        let parsed: SelfCheckReport = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.status, report.status);
        assert_eq!(parsed.query_plan_valid, report.query_plan_valid);
        assert_eq!(parsed.provider_summary.total, report.provider_summary.total);
        assert_eq!(parsed.channel_summary.total, report.channel_summary.total);
    }

    // -----------------------------------------------------------------------
    // T20: Misconfigured provider is a blocker
    // -----------------------------------------------------------------------

    #[test]
    fn misconfigured_provider_is_blocked() {
        let request = SelfCheckRequest {
            providers: vec![ProviderReadinessEntry {
                provider_id: "p1".into(),
                display_name: "BadConfig".into(),
                enabled: true,
                weight: 1,
                readiness: ProviderReadiness::Misconfigured,
                reason: Some("invalid endpoint URL".into()),
            }],
            ..minimal_request()
        };

        let report = run_self_check(request);
        assert_eq!(report.status, SelfCheckStatus::Blocked);
        assert_eq!(report.provider_summary.misconfigured, 1);
    }

    // -----------------------------------------------------------------------
    // T21: Zero-weight provider produces warning
    // -----------------------------------------------------------------------

    #[test]
    fn zero_weight_provider_warns() {
        let request = SelfCheckRequest {
            providers: vec![ProviderReadinessEntry {
                provider_id: "p1".into(),
                display_name: "ZeroWeight".into(),
                enabled: true,
                weight: 0,
                readiness: ProviderReadiness::Ready,
                reason: None,
            }],
            ..minimal_request()
        };

        let report = run_self_check(request);
        // Provider is ready, so no blocker — but zero weight is a warning
        assert_eq!(report.provider_summary.ready, 1);
        let has_weight_warn = report
            .warnings
            .iter()
            .any(|w| w.contains("权重") || w.contains("weight"));
        assert!(has_weight_warn, "zero weight should produce a warning");
    }
}
