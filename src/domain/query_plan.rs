//! QueryPlan domain types.
//!
//! Covers the input lifecycle:
//! `QueryPlanInput` → validation → `ValidatedQueryPlan` → `TaskPlan`
//!
//! Validation produces either a [`ValidationOutcome::Valid`] (with optional
//! warnings) or an [`InputRejection`] when the input is unusable.
//!
//! References: PRD §QueryPlan 产品设计, HLD §QueryPlan Planner,
//! `docs/design/TASK-002-queryplan-cli-input-planning-design.md`

use crate::error::DiagnosticLevel;
use serde::{Deserialize, Serialize};

/// Raw user-facing QueryPlan before validation and default-value application.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryPlanInput {
    /// Semantic description of the desired image(s). Required; task is
    /// rejected when missing or empty.
    #[serde(default)]
    pub description: String,

    /// Number of qualified images the user wants. Defaults to 1.
    #[serde(default = "default_required_count")]
    pub required_count: u32,

    /// Quality tier preference.
    #[serde(default)]
    pub quality_tier: QualityTier,

    /// Content constraints: must-include / must-avoid hints.
    #[serde(default)]
    pub content_constraints: ContentConstraints,

    /// Authorization risk preference.
    #[serde(default)]
    pub authorization_preference: AuthorizationPreference,

    /// Output preference: human-readable vs automation-consumable.
    #[serde(default)]
    pub output_preference: OutputPreference,

    /// Maximum number of retries after the initial attempt.
    /// Constitution allows at most 3 (initial attempt + up to 3 retries).
    #[serde(default = "default_retry_limit")]
    pub retry_limit: u32,
}

impl Default for QueryPlanInput {
    fn default() -> Self {
        Self {
            description: String::new(),
            required_count: default_required_count(),
            quality_tier: QualityTier::default(),
            content_constraints: ContentConstraints::default(),
            authorization_preference: AuthorizationPreference::default(),
            output_preference: OutputPreference::default(),
            retry_limit: default_retry_limit(),
        }
    }
}

fn default_required_count() -> u32 {
    1
}

fn default_retry_limit() -> u32 {
    3
}

/// Quality tier expressing user expectations about image usability.
///
/// Per PRD: 通用质量 / 较高质量 / 严格质量.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum QualityTier {
    /// General quality: suitable for regular use; no obvious corruption,
    /// unrecognizable subject, extreme blur, or clear mismatch.
    #[default]
    #[serde(rename = "general")]
    General,

    /// Higher quality: clear subject, usable composition, low visual noise,
    /// stable relevance to the intended use.
    #[serde(rename = "high")]
    High,

    /// Strict quality: conservative pass; boundary / uncertain images
    /// should be rejected or deprioritised.
    #[serde(rename = "strict")]
    Strict,
}

/// Content constraints the user may optionally supply.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContentConstraints {
    /// Elements that should be present in the image.
    #[serde(default)]
    pub must_include: Vec<String>,

    /// Elements that should be avoided.
    #[serde(default)]
    pub must_avoid: Vec<String>,
}

/// User stance on authorization / licensing risk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AuthorizationPreference {
    /// Unknown authorization is retained with risk warnings;
    /// explicitly prohibited sources are blocked.
    #[default]
    #[serde(rename = "default")]
    Default,
}

/// How the delivery result will be consumed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum OutputPreference {
    /// Human-oriented explanations.
    #[default]
    #[serde(rename = "human")]
    Human,

    /// Automation-oriented: stable status codes, machine-readable fields.
    #[serde(rename = "automation")]
    Automation,
}

// ---------------------------------------------------------------------------
// Validated query plan
// ---------------------------------------------------------------------------

/// A QueryPlan that has passed input validation and has all defaults applied.
#[derive(Debug, Clone)]
pub struct ValidatedQueryPlan {
    pub description: String,
    pub required_count: u32,
    pub quality_tier: QualityTier,
    pub content_constraints: ContentConstraints,
    pub authorization_preference: AuthorizationPreference,
    pub output_preference: OutputPreference,
    pub retry_limit: u32,
}

// ---------------------------------------------------------------------------
// Task plan — derived execution parameters
// ---------------------------------------------------------------------------

/// Execution plan derived from a validated QueryPlan.
///
/// Contains the candidate target and batch size derived per the
/// constitution ratios (≈20 candidates per required image;
/// batch target = required_count × 2).
#[derive(Debug, Clone)]
pub struct TaskPlan {
    /// The validated QueryPlan this plan was derived from.
    pub query_plan: ValidatedQueryPlan,

    /// Target number of candidates to search for
    /// (≈ required_count × 20).
    pub candidate_target: u32,

    /// Target number of candidates per retrieval batch
    /// (= required_count × 2).
    pub retrieval_batch_target: u32,

    /// Maximum number of full attempts (1 initial + retry_limit).
    pub max_attempts: u32,
}

impl TaskPlan {
    /// Derive a `TaskPlan` from a validated QueryPlan.
    pub fn from_validated(plan: ValidatedQueryPlan) -> Self {
        let candidate_target = plan.required_count.saturating_mul(20);
        let retrieval_batch_target = plan.required_count.saturating_mul(2);
        let max_attempts = 1 + plan.retry_limit;
        Self {
            query_plan: plan,
            candidate_target,
            retrieval_batch_target,
            max_attempts,
        }
    }
}

// ---------------------------------------------------------------------------
// Input diagnostic — field-level feedback
// ---------------------------------------------------------------------------

/// A field-level diagnostic produced during QueryPlan input validation.
///
/// Diagnostics explain *what* was checked, *why* a value was accepted or
/// rejected, what default was applied, and what the user can do to resolve
/// issues. They never echo suspected credentials or tokens.
#[derive(Debug, Clone)]
pub struct InputDiagnostic {
    /// The field or aspect being diagnosed (e.g. "description", "retry_limit").
    pub field: String,

    /// Severity level of this diagnostic.
    pub severity: DiagnosticLevel,

    /// Why this diagnostic was produced.
    pub reason: String,

    /// What default was applied, if any.
    pub default_applied: Option<String>,

    /// What the user can adjust to resolve the issue.
    pub suggestion: Option<String>,
}

impl InputDiagnostic {
    pub fn error(
        field: impl Into<String>,
        reason: impl Into<String>,
        suggestion: impl Into<String>,
    ) -> Self {
        Self {
            field: field.into(),
            severity: DiagnosticLevel::Error,
            reason: reason.into(),
            default_applied: None,
            suggestion: Some(suggestion.into()),
        }
    }

    pub fn warning(
        field: impl Into<String>,
        reason: impl Into<String>,
        suggestion: Option<impl Into<String>>,
    ) -> Self {
        Self {
            field: field.into(),
            severity: DiagnosticLevel::Warning,
            reason: reason.into(),
            default_applied: None,
            suggestion: suggestion.map(|s| s.into()),
        }
    }

    pub fn info(
        field: impl Into<String>,
        reason: impl Into<String>,
        default_applied: impl Into<String>,
    ) -> Self {
        Self {
            field: field.into(),
            severity: DiagnosticLevel::Info,
            reason: reason.into(),
            default_applied: Some(default_applied.into()),
            suggestion: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Input rejection
// ---------------------------------------------------------------------------

/// Collection of diagnostic findings that together make a QueryPlan invalid.
///
/// An `InputRejection` is produced *before* any search, retrieval, or
/// delivery. It is not a delivery result status — it means the task never
/// started.
#[derive(Debug, Clone)]
pub struct InputRejection {
    /// All diagnostics that led to rejection.
    pub diagnostics: Vec<InputDiagnostic>,

    /// Human-readable summary.
    pub summary: String,
}

impl InputRejection {
    pub fn new(diagnostics: Vec<InputDiagnostic>, summary: impl Into<String>) -> Self {
        Self {
            diagnostics,
            summary: summary.into(),
        }
    }
}

impl std::fmt::Display for InputRejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.summary)
    }
}

// ---------------------------------------------------------------------------
// Validation outcome
// ---------------------------------------------------------------------------

/// Outcome of validating a [`QueryPlanInput`].
///
/// A valid outcome may still carry non-blocking warnings (e.g. large
/// request count, suspected sensitive input). An invalid outcome is an
/// [`InputRejection`] that prevents the task from starting.
#[derive(Debug, Clone)]
pub enum ValidationOutcome {
    /// Input passed all blocking checks. The plan can proceed to execution.
    Valid {
        plan: ValidatedQueryPlan,
        /// Non-blocking warnings (large count, sensitive content patterns).
        warnings: Vec<InputDiagnostic>,
    },
    /// Input failed one or more blocking checks. The task must not start.
    Rejected(InputRejection),
}

impl ValidationOutcome {
    /// Returns `true` if the outcome is [`ValidationOutcome::Valid`].
    pub fn is_valid(&self) -> bool {
        matches!(self, Self::Valid { .. })
    }

    /// Extract the validated plan, panicking if the outcome is rejected.
    pub fn unwrap(self) -> ValidatedQueryPlan {
        match self {
            Self::Valid { plan, .. } => plan,
            Self::Rejected(rejection) => {
                panic!(
                    "called `ValidationOutcome::unwrap()` on a `Rejected` value: {}",
                    rejection
                )
            }
        }
    }

    /// Return all diagnostics (warnings from valid, or errors from rejected).
    pub fn diagnostics(&self) -> &[InputDiagnostic] {
        match self {
            Self::Valid { warnings, .. } => warnings.as_slice(),
            Self::Rejected(rejection) => rejection.diagnostics.as_slice(),
        }
    }
}

// ---------------------------------------------------------------------------
// Validation logic
// ---------------------------------------------------------------------------

/// Validate a [`QueryPlanInput`] and produce a [`ValidatedQueryPlan`] or
/// an [`InputRejection`].
///
/// # Blocking checks (produce `Rejected`)
///
/// - `description` is empty or whitespace-only.
/// - `retry_limit` exceeds the constitution maximum of 3.
///
/// # Non-blocking checks (produce warnings in `Valid`)
///
/// - `required_count` is zero.
/// - `required_count` is large (≥ 100) — risk hint, not a hard limit.
/// - Description or content constraints contain suspected credentials
///   (tokens, cookies, API keys).
pub fn validate_query_plan(input: QueryPlanInput) -> ValidationOutcome {
    let mut diagnostics: Vec<InputDiagnostic> = Vec::new();

    // --- blocking: description ---
    let trimmed = input.description.trim();
    if trimmed.is_empty() {
        return ValidationOutcome::Rejected(InputRejection::new(
            vec![InputDiagnostic::error(
                "description",
                "语义描述缺失：必须提供图片语义描述才能开始任务。",
                "请提供一段描述所需图片的文字，例如“夕阳下的山景”。",
            )],
            "输入被拒绝：缺少图片语义描述。",
        ));
    }

    // --- blocking: retry_limit ---
    const MAX_RETRY: u32 = 3;
    if input.retry_limit > MAX_RETRY {
        diagnostics.push(InputDiagnostic::error(
            "retry_limit",
            format!(
                "重试策略越界：retry_limit 为 {}，但宪法允许最多 {} 次重试。",
                input.retry_limit, MAX_RETRY
            ),
            format!("请将 retry_limit 设置为不超过 {} 的值。", MAX_RETRY),
        ));
    }

    // If we have any error-level diagnostics, reject.
    let errors: Vec<InputDiagnostic> = diagnostics
        .iter()
        .filter(|d| d.severity == DiagnosticLevel::Error)
        .cloned()
        .collect();
    if !errors.is_empty() {
        let summary = format!("输入被拒绝：{} 个问题需要修复。", errors.len());
        return ValidationOutcome::Rejected(InputRejection::new(errors, summary));
    }

    // --- non-blocking: required_count ---
    let applied_count = if input.required_count == 0 {
        diagnostics.push(InputDiagnostic::warning(
            "required_count",
            "交付数量为 0：不会搜索候选或抓取图片。",
            Some("如需获得图片，请设置 required_count 为至少 1。"),
        ));
        // Still use the default of 1 for derivation so downstream doesn't
        // see zero. The warning already tells the user what happened.
        1u32
    } else {
        input.required_count
    };

    // --- non-blocking: large count risk ---
    const LARGE_COUNT_THRESHOLD: u32 = 100;
    if applied_count >= LARGE_COUNT_THRESHOLD {
        diagnostics.push(InputDiagnostic::warning(
            "required_count",
            format!(
                "大数量请求：交付数量为 {}，候选目标将为 {}。大量请求可能导致搜索和抓取耗时较长。",
                applied_count,
                applied_count.saturating_mul(20)
            ),
            None::<&str>,
        ));
    }

    // --- non-blocking: sensitive content ---
    check_sensitive_patterns(&input, &mut diagnostics);

    // --- build validated plan ---
    let plan = ValidatedQueryPlan {
        description: trimmed.to_string(),
        required_count: applied_count,
        quality_tier: input.quality_tier,
        content_constraints: input.content_constraints,
        authorization_preference: input.authorization_preference,
        output_preference: input.output_preference,
        retry_limit: input.retry_limit.min(MAX_RETRY),
    };

    ValidationOutcome::Valid {
        plan,
        warnings: diagnostics,
    }
}

// ---------------------------------------------------------------------------
// Sensitive-input detection
// ---------------------------------------------------------------------------

/// Patterns that suggest the user accidentally pasted credentials or tokens.
const SENSITIVE_PATTERNS: &[(&str, &str)] = &[
    ("Bearer ", "疑似 Bearer token"),
    ("Authorization:", "疑似 Authorization 头"),
    ("Cookie:", "疑似 Cookie 头"),
    ("Set-Cookie:", "疑似 Set-Cookie 头"),
    ("x-api-key:", "疑似 API key 头"),
    ("api_key=", "疑似 API key 参数"),
    ("access_token=", "疑似 access token 参数"),
    ("client_secret=", "疑似 client secret 参数"),
    ("private_key=", "疑似私钥参数"),
];

/// Check the description and content constraints for suspected credential
/// patterns. Diagnostics are appended to `diags`; the original values are
/// never echoed.
fn check_sensitive_patterns(input: &QueryPlanInput, diags: &mut Vec<InputDiagnostic>) {
    let lower_desc = input.description.to_lowercase();

    for (pattern, label) in SENSITIVE_PATTERNS {
        let lower_pat = pattern.to_lowercase();
        if lower_desc.contains(&lower_pat) {
            diags.push(InputDiagnostic::warning(
                "description",
                format!(
                    "疑似敏感输入：描述中可能包含{}。出于安全考虑，具体值不会在诊断中回显。",
                    label
                ),
                Some("请从描述中移除凭据、token 或认证头信息，改用环境变量或配置文件管理凭据。"),
            ));
            break; // one warning is enough for the description
        }
    }

    // Check content constraints
    let all_constraints: Vec<&str> = input
        .content_constraints
        .must_include
        .iter()
        .chain(input.content_constraints.must_avoid.iter())
        .map(|s| s.as_str())
        .collect();

    for constraint in &all_constraints {
        let lower_c = constraint.to_lowercase();
        for (pattern, label) in SENSITIVE_PATTERNS {
            let lower_pat = pattern.to_lowercase();
            if lower_c.contains(&lower_pat) {
                diags.push(InputDiagnostic::warning(
                    "content_constraints",
                    format!(
                        "疑似敏感输入：内容约束中可能包含{}。出于安全考虑，具体值不会在诊断中回显。",
                        label
                    ),
                    Some("请从内容约束中移除凭据或认证信息。"),
                ));
                return; // one warning is enough for constraints
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Existing tests — defaults and derivation
    // -----------------------------------------------------------------------

    #[test]
    fn default_required_count_is_1() {
        assert_eq!(default_required_count(), 1);
    }

    #[test]
    fn default_retry_limit_is_3() {
        assert_eq!(default_retry_limit(), 3);
    }

    #[test]
    fn task_plan_derives_candidate_target_20x() {
        let plan = ValidatedQueryPlan {
            description: "test".into(),
            required_count: 3,
            quality_tier: QualityTier::General,
            content_constraints: ContentConstraints::default(),
            authorization_preference: AuthorizationPreference::Default,
            output_preference: OutputPreference::Human,
            retry_limit: 3,
        };
        let task = TaskPlan::from_validated(plan);
        assert_eq!(task.candidate_target, 60); // 3 × 20
        assert_eq!(task.retrieval_batch_target, 6); // 3 × 2
        assert_eq!(task.max_attempts, 4); // 1 initial + 3 retries
    }

    #[test]
    fn task_plan_derives_candidate_target_for_single_image() {
        let plan = ValidatedQueryPlan {
            description: "single".into(),
            required_count: 1,
            quality_tier: QualityTier::General,
            content_constraints: ContentConstraints::default(),
            authorization_preference: AuthorizationPreference::Default,
            output_preference: OutputPreference::Human,
            retry_limit: 3,
        };
        let task = TaskPlan::from_validated(plan);
        assert_eq!(task.candidate_target, 20);
        assert_eq!(task.retrieval_batch_target, 2);
    }

    #[test]
    fn task_plan_zero_count_saturates() {
        let plan = ValidatedQueryPlan {
            description: "zero".into(),
            required_count: 0,
            quality_tier: QualityTier::General,
            content_constraints: ContentConstraints::default(),
            authorization_preference: AuthorizationPreference::Default,
            output_preference: OutputPreference::Human,
            retry_limit: 0,
        };
        let task = TaskPlan::from_validated(plan);
        assert_eq!(task.candidate_target, 0);
        assert_eq!(task.retrieval_batch_target, 0);
        assert_eq!(task.max_attempts, 1);
    }

    // -----------------------------------------------------------------------
    // Validation — acceptance criteria tests
    // -----------------------------------------------------------------------

    /// AC: 包含语义描述时生成有效规划。
    #[test]
    fn valid_with_description_produces_valid_outcome() {
        let input = QueryPlanInput {
            description: "sunset over mountains".into(),
            ..Default::default()
        };
        let outcome = validate_query_plan(input);
        assert!(outcome.is_valid());
        let plan = outcome.unwrap();
        assert_eq!(plan.description, "sunset over mountains");
        assert_eq!(plan.required_count, 1); // default
        assert_eq!(plan.quality_tier, QualityTier::General); // default
        assert_eq!(plan.retry_limit, 3); // default
    }

    /// AC: 缺少语义描述时输入拒绝且不进入搜索。
    #[test]
    fn missing_description_rejected() {
        let input = QueryPlanInput {
            description: "".into(),
            ..Default::default()
        };
        let outcome = validate_query_plan(input);
        assert!(!outcome.is_valid());
        match outcome {
            ValidationOutcome::Rejected(rejection) => {
                assert!(rejection.summary.contains("缺少图片语义描述"));
                assert_eq!(rejection.diagnostics.len(), 1);
                assert_eq!(rejection.diagnostics[0].field, "description");
                assert_eq!(rejection.diagnostics[0].severity, DiagnosticLevel::Error);
            }
            _ => panic!("expected rejection"),
        }
    }

    /// AC: 空白描述同样拒绝。
    #[test]
    fn whitespace_only_description_rejected() {
        let input = QueryPlanInput {
            description: "   \n  \t  ".into(),
            ..Default::default()
        };
        let outcome = validate_query_plan(input);
        assert!(!outcome.is_valid());
    }

    /// AC: 缺省数量为 1。
    #[test]
    fn default_count_is_applied() {
        let input = QueryPlanInput {
            description: "test".into(),
            required_count: 0, // serde default is 1, but explicit 0 should be flagged
            ..Default::default()
        };
        let outcome = validate_query_plan(input);
        assert!(outcome.is_valid());
        match outcome {
            ValidationOutcome::Valid { plan, warnings } => {
                // required_count 0 → warning, plan uses 1
                assert_eq!(plan.required_count, 1);
                assert!(!warnings.is_empty());
                let count_warn = warnings
                    .iter()
                    .find(|d| d.field == "required_count")
                    .expect("should have a required_count warning");
                assert_eq!(count_warn.severity, DiagnosticLevel::Warning);
            }
            _ => panic!("expected valid"),
        }
    }

    /// AC: 缺省质量为通用质量。
    #[test]
    fn default_quality_tier_is_general() {
        let input = QueryPlanInput {
            description: "test".into(),
            ..Default::default()
        };
        let outcome = validate_query_plan(input);
        assert!(outcome.is_valid());
        let plan = outcome.unwrap();
        assert_eq!(plan.quality_tier, QualityTier::General);
    }

    /// AC: 缺省重试为 3。
    #[test]
    fn default_retry_limit_is_applied() {
        let input = QueryPlanInput {
            description: "test".into(),
            retry_limit: 3,
            ..Default::default()
        };
        let outcome = validate_query_plan(input);
        assert!(outcome.is_valid());
        let plan = outcome.unwrap();
        assert_eq!(plan.retry_limit, 3);
    }

    /// AC: 重试越界拒绝。
    #[test]
    fn retry_limit_exceeds_maximum_rejected() {
        let input = QueryPlanInput {
            description: "test".into(),
            retry_limit: 10,
            ..Default::default()
        };
        let outcome = validate_query_plan(input);
        assert!(!outcome.is_valid());
        match outcome {
            ValidationOutcome::Rejected(rejection) => {
                assert!(rejection.summary.contains("输入被拒绝"));
                let retry_diag = rejection
                    .diagnostics
                    .iter()
                    .find(|d| d.field == "retry_limit")
                    .expect("should have retry_limit diagnostic");
                assert_eq!(retry_diag.severity, DiagnosticLevel::Error);
                assert!(retry_diag.reason.contains("越界"));
            }
            _ => panic!("expected rejection"),
        }
    }

    /// AC: 要求 3 张图片派生约 60 个候选 (3 × 20 = 60)。
    #[test]
    fn three_images_derives_60_candidates() {
        let input = QueryPlanInput {
            description: "cats".into(),
            required_count: 3,
            ..Default::default()
        };
        let outcome = validate_query_plan(input);
        assert!(outcome.is_valid());
        let plan = outcome.unwrap();
        let task = TaskPlan::from_validated(plan);
        assert_eq!(task.candidate_target, 60);
    }

    /// AC: 要求 4 张图片派生 8 个抓取批次 (4 × 2 = 8)。
    #[test]
    fn four_images_derives_8_batch_target() {
        let input = QueryPlanInput {
            description: "dogs".into(),
            required_count: 4,
            ..Default::default()
        };
        let outcome = validate_query_plan(input);
        assert!(outcome.is_valid());
        let plan = outcome.unwrap();
        let task = TaskPlan::from_validated(plan);
        assert_eq!(task.retrieval_batch_target, 8);
    }

    // -----------------------------------------------------------------------
    // Additional validation tests
    // -----------------------------------------------------------------------

    /// AC: 未知授权保持风险提示，不被描述为商用安全。
    #[test]
    fn unknown_authorization_remains_default() {
        let input = QueryPlanInput {
            description: "test".into(),
            authorization_preference: AuthorizationPreference::Default,
            ..Default::default()
        };
        let outcome = validate_query_plan(input);
        assert!(outcome.is_valid());
        let plan = outcome.unwrap();
        // Default is not "commercially safe" — it's "unknown"
        assert_eq!(
            plan.authorization_preference,
            AuthorizationPreference::Default
        );
    }

    #[test]
    fn quality_tier_preserved_on_valid_input() {
        for tier in &[QualityTier::General, QualityTier::High, QualityTier::Strict] {
            let input = QueryPlanInput {
                description: "test".into(),
                quality_tier: *tier,
                ..Default::default()
            };
            let outcome = validate_query_plan(input);
            assert!(outcome.is_valid());
            let plan = outcome.unwrap();
            assert_eq!(plan.quality_tier, *tier);
        }
    }

    #[test]
    fn content_constraints_preserved() {
        let input = QueryPlanInput {
            description: "test".into(),
            content_constraints: ContentConstraints {
                must_include: vec!["tree".into()],
                must_avoid: vec!["people".into()],
            },
            ..Default::default()
        };
        let outcome = validate_query_plan(input);
        assert!(outcome.is_valid());
        let plan = outcome.unwrap();
        assert_eq!(plan.content_constraints.must_include, vec!["tree"]);
        assert_eq!(plan.content_constraints.must_avoid, vec!["people"]);
    }

    #[test]
    fn output_preference_preserved() {
        let input = QueryPlanInput {
            description: "test".into(),
            output_preference: OutputPreference::Automation,
            ..Default::default()
        };
        let outcome = validate_query_plan(input);
        assert!(outcome.is_valid());
        let plan = outcome.unwrap();
        assert_eq!(plan.output_preference, OutputPreference::Automation);
    }

    /// Large count produces a warning but is not rejected.
    #[test]
    fn large_count_warning_not_rejection() {
        let input = QueryPlanInput {
            description: "test".into(),
            required_count: 200,
            ..Default::default()
        };
        let outcome = validate_query_plan(input);
        assert!(outcome.is_valid());
        match outcome {
            ValidationOutcome::Valid { plan, warnings } => {
                assert_eq!(plan.required_count, 200);
                let large_warn = warnings
                    .iter()
                    .find(|d| d.field == "required_count" && d.reason.contains("大数量"))
                    .expect("should have large count warning");
                assert_eq!(large_warn.severity, DiagnosticLevel::Warning);
                // Candidate target is derived correctly
                let task = TaskPlan::from_validated(plan);
                assert_eq!(task.candidate_target, 4000); // 200 × 20
            }
            _ => panic!("expected valid with warnings"),
        }
    }

    // -----------------------------------------------------------------------
    // Sensitive input detection tests
    // -----------------------------------------------------------------------

    #[test]
    fn sensitive_bearer_token_in_description_warns() {
        let input = QueryPlanInput {
            description: "Bearer eyJhbGciOiJIUzI1NiJ9.test description".into(),
            ..Default::default()
        };
        let outcome = validate_query_plan(input);
        assert!(outcome.is_valid());
        match outcome {
            ValidationOutcome::Valid { warnings, .. } => {
                let sens = warnings
                    .iter()
                    .find(|d| d.field == "description" && d.reason.contains("疑似敏感"))
                    .expect("should have sensitive content warning");
                assert_eq!(sens.severity, DiagnosticLevel::Warning);
                // The raw token must NOT appear in the diagnostic
                assert!(!sens.reason.contains("eyJhbGci"));
                assert!(!sens.reason.contains("Bearer eyJ"));
            }
            _ => panic!("expected valid with warnings"),
        }
    }

    #[test]
    fn sensitive_api_key_in_description_warns() {
        let input = QueryPlanInput {
            description: "use x-api-key: abc123secret for access".into(),
            ..Default::default()
        };
        let outcome = validate_query_plan(input);
        assert!(outcome.is_valid());
        match outcome {
            ValidationOutcome::Valid { warnings, .. } => {
                let sens = warnings
                    .iter()
                    .find(|d| d.field == "description" && d.reason.contains("疑似敏感"))
                    .expect("should have API key warning");
                // The raw key must NOT appear
                assert!(!sens.reason.contains("abc123secret"));
            }
            _ => panic!("expected valid with warnings"),
        }
    }

    #[test]
    fn sensitive_content_in_constraints_warns() {
        let input = QueryPlanInput {
            description: "test".into(),
            content_constraints: ContentConstraints {
                must_include: vec!["Authorization: Bearer secret123".into()],
                must_avoid: vec![],
            },
            ..Default::default()
        };
        let outcome = validate_query_plan(input);
        assert!(outcome.is_valid());
        match outcome {
            ValidationOutcome::Valid { warnings, .. } => {
                let sens = warnings
                    .iter()
                    .find(|d| d.field == "content_constraints" && d.reason.contains("疑似敏感"))
                    .expect("should have constraints warning");
                assert!(!sens.reason.contains("secret123"));
            }
            _ => panic!("expected valid with warnings"),
        }
    }

    #[test]
    fn clean_description_no_sensitive_warning() {
        let input = QueryPlanInput {
            description: "a beautiful sunset over mountains with orange sky".into(),
            ..Default::default()
        };
        let outcome = validate_query_plan(input);
        assert!(outcome.is_valid());
        match outcome {
            ValidationOutcome::Valid { warnings, .. } => {
                let has_sensitive = warnings.iter().any(|d| d.reason.contains("疑似敏感"));
                assert!(
                    !has_sensitive,
                    "clean description should not trigger sensitive warning"
                );
            }
            _ => panic!("expected valid"),
        }
    }

    // -----------------------------------------------------------------------
    // Derivation tests (additional)
    // -----------------------------------------------------------------------

    #[test]
    fn max_attempts_equals_one_plus_retry_limit() {
        let input = QueryPlanInput {
            description: "test".into(),
            retry_limit: 2,
            ..Default::default()
        };
        let outcome = validate_query_plan(input);
        assert!(outcome.is_valid());
        let plan = outcome.unwrap();
        assert_eq!(plan.retry_limit, 2);
        let task = TaskPlan::from_validated(plan);
        assert_eq!(task.max_attempts, 3); // 1 initial + 2 retries
    }

    #[test]
    fn candidate_target_for_five_images_is_100() {
        let input = QueryPlanInput {
            description: "test".into(),
            required_count: 5,
            ..Default::default()
        };
        let outcome = validate_query_plan(input);
        assert!(outcome.is_valid());
        let plan = outcome.unwrap();
        let task = TaskPlan::from_validated(plan);
        assert_eq!(task.candidate_target, 100); // 5 × 20
        assert_eq!(task.retrieval_batch_target, 10); // 5 × 2
    }

    // -----------------------------------------------------------------------
    // InputDiagnostic constructors
    // -----------------------------------------------------------------------

    #[test]
    fn diagnostic_error_has_error_level() {
        let d = InputDiagnostic::error("field", "reason", "suggestion");
        assert_eq!(d.severity, DiagnosticLevel::Error);
        assert_eq!(d.field, "field");
        assert!(d.suggestion.is_some());
        assert!(d.default_applied.is_none());
    }

    #[test]
    fn diagnostic_warning_has_warning_level() {
        let d = InputDiagnostic::warning("field", "reason", Some("suggestion"));
        assert_eq!(d.severity, DiagnosticLevel::Warning);
        assert_eq!(d.reason, "reason");
    }

    #[test]
    fn diagnostic_info_records_default() {
        let d = InputDiagnostic::info("field", "applied default", "value");
        assert_eq!(d.severity, DiagnosticLevel::Info);
        assert_eq!(d.default_applied, Some("value".to_string()));
        assert!(d.suggestion.is_none());
    }

    // -----------------------------------------------------------------------
    // ValidationOutcome
    // -----------------------------------------------------------------------

    #[test]
    fn outcome_diagnostics_returns_warnings_for_valid() {
        let input = QueryPlanInput {
            description: "test".into(),
            required_count: 0, // triggers warning
            ..Default::default()
        };
        let outcome = validate_query_plan(input);
        assert!(outcome.is_valid());
        let diags = outcome.diagnostics();
        assert!(!diags.is_empty());
    }

    #[test]
    fn outcome_diagnostics_returns_errors_for_rejected() {
        let input = QueryPlanInput {
            description: "".into(),
            ..Default::default()
        };
        let outcome = validate_query_plan(input);
        assert!(!outcome.is_valid());
        let diags = outcome.diagnostics();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].severity, DiagnosticLevel::Error);
    }

    #[test]
    #[should_panic]
    fn outcome_unwrap_panics_on_rejected() {
        let input = QueryPlanInput {
            description: "".into(),
            ..Default::default()
        };
        let outcome = validate_query_plan(input);
        outcome.unwrap(); // should panic
    }
}
