//! QueryPlan domain types.
//!
//! Covers the input lifecycle:
//! `QueryPlanInput` → validation/admission → `NormalizedQueryPlan`
//!
//! v1.1 canonical types:
//! - [`QueryPlanInput`]: user-facing serde DTO with `required_image_count`
//!   (canonical) and `required_count` (alias).
//! - [`NormalizedQueryPlan`]: downstream-consumable plan with all defaults
//!   applied, derived targets, and admission diagnostics.
//! - [`AdmissionOutcome`]: either accepted with warnings or rejected with
//!   machine-readable diagnostics.
//!
//! References: PRD §QueryPlan, HLD §domain, LLD §NormalizedQueryPlan,
//! `docs/design/v1.1-TASK-001-queryplan-config-policy-design.md`

use crate::error::DiagnosticLevel;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Raw user-facing QueryPlan input
// ---------------------------------------------------------------------------

/// Raw user-facing QueryPlan before validation and default-value application.
///
/// v1.1 canonical JSON uses `required_image_count`; the v1.0 `required_count`
/// field is accepted as a serde alias during transition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryPlanInput {
    /// Semantic description of the desired image(s). Required; task is
    /// rejected when missing or empty.
    #[serde(default)]
    pub description: String,

    /// Expanded query texts derived from the description and optional
    /// user-supplied variants. Defaults to `[description]` after admission.
    #[serde(default)]
    pub query_texts: Vec<String>,

    /// Number of qualified images the user wants. Defaults to 1.
    /// Canonical field name in v1.1; accepts `required_count` as input alias.
    #[serde(default = "default_required_image_count", alias = "required_count")]
    pub required_image_count: u32,

    /// Quality tier preference. Supported values: `general`, `high`, `strict`.
    /// Accepts `quality_tier` as input alias for backward compatibility.
    #[serde(default, alias = "quality_tier")]
    pub quality: QualityTier,

    /// Structured quality requirements that override tier-derived defaults.
    #[serde(default)]
    pub quality_requirements: QualityRequirements,

    /// Content constraints: must-include / must-avoid hints (backward compat).
    #[serde(default)]
    pub content_constraints: ContentConstraints,

    /// Material type preferences (e.g. "photo", "illustration", "diagram").
    #[serde(default)]
    pub material_types: Vec<String>,

    /// Visual requirements (e.g. "high contrast", "no text overlay").
    #[serde(default)]
    pub visual_requirements: Vec<String>,

    /// Negative scope — concepts/styles/elements to exclude.
    #[serde(default)]
    pub negative_scope: Vec<String>,

    /// Desired minimum number of distinct sources. Must be between 1 and
    /// `required_image_count` when present.
    #[serde(default)]
    pub source_diversity_requirement: Option<u32>,

    /// Authorization risk preference.
    #[serde(default)]
    pub authorization_preference: AuthorizationPreference,

    /// Output preference: human-readable vs automation-consumable.
    #[serde(default)]
    pub output_preference: OutputPreference,

    /// Provider-level policy constraints.
    #[serde(default)]
    pub provider_policy: QueryProviderPolicy,

    /// Retrieval-level policy constraints.
    #[serde(default)]
    pub retrieval_policy: QueryRetrievalPolicy,

    /// Maximum number of retries after the initial attempt.
    /// Constitution allows at most 3 (initial attempt + up to 3 retries).
    #[serde(default = "default_retry_limit")]
    pub retry_limit: u8,
}

impl Default for QueryPlanInput {
    fn default() -> Self {
        Self {
            description: String::new(),
            query_texts: Vec::new(),
            required_image_count: default_required_image_count(),
            quality: QualityTier::default(),
            quality_requirements: QualityRequirements::default(),
            content_constraints: ContentConstraints::default(),
            material_types: Vec::new(),
            visual_requirements: Vec::new(),
            negative_scope: Vec::new(),
            source_diversity_requirement: None,
            authorization_preference: AuthorizationPreference::default(),
            output_preference: OutputPreference::default(),
            provider_policy: QueryProviderPolicy::default(),
            retrieval_policy: QueryRetrievalPolicy::default(),
            retry_limit: default_retry_limit(),
        }
    }
}

fn default_required_image_count() -> u32 {
    1
}

fn default_retry_limit() -> u8 {
    3
}

// ---------------------------------------------------------------------------
// Quality types
// ---------------------------------------------------------------------------

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

/// Structured quality requirements that override tier-derived defaults.
///
/// v1.1 adds this so downstream quality tasks can use explicit thresholds.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct QualityRequirements {
    /// Minimum acceptable image width in pixels.
    #[serde(default)]
    pub minimum_width: Option<u32>,

    /// Minimum acceptable image height in pixels.
    #[serde(default)]
    pub minimum_height: Option<u32>,

    /// Whether watermarked images are acceptable.
    #[serde(default)]
    pub allow_watermark: Option<bool>,

    /// Whether thumbnail-only candidates (no full image) are acceptable.
    /// Must default to false: metadata-only/thumbnail-only delivery is forbidden.
    #[serde(default)]
    pub allow_thumbnail_only: bool,

    /// Minimum visual relevance score for subjective evaluation.
    #[serde(default)]
    pub min_visual_relevance_score: Option<f32>,
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

// ---------------------------------------------------------------------------
// Policy types embedded in QueryPlan
// ---------------------------------------------------------------------------

/// Provider policy constraints supplied by the QueryPlan.
///
/// These narrow — never broaden — runtime provider policy.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct QueryProviderPolicy {
    /// Provider kinds allowed for this query; empty means all configured.
    #[serde(default)]
    pub allowed_provider_kinds: Vec<String>,

    /// Provider kinds explicitly excluded.
    #[serde(default)]
    pub excluded_provider_kinds: Vec<String>,

    /// Whether fixture providers are allowed for this query.
    #[serde(default)]
    pub allow_fixture: bool,
}

/// Retrieval policy constraints supplied by the QueryPlan.
///
/// These narrow — never broaden — runtime retrieval policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryRetrievalPolicy {
    /// Whether paid retrieval channels are allowed for this query.
    #[serde(default)]
    pub allow_paid: bool,

    /// Whether to respect robots.txt and site rules.
    #[serde(default = "default_true")]
    pub respect_robots: bool,

    /// Whether login-required sources can be accessed.
    #[serde(default)]
    pub allow_login: bool,

    /// Whether paywalled sources can be accessed.
    #[serde(default)]
    pub allow_paywalled: bool,
}

impl Default for QueryRetrievalPolicy {
    fn default() -> Self {
        Self {
            allow_paid: false,
            respect_robots: true,
            allow_login: false,
            allow_paywalled: false,
        }
    }
}

fn default_true() -> bool {
    true
}

// ---------------------------------------------------------------------------
// User stance types
// ---------------------------------------------------------------------------

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
// Normalized query plan — downstream-consumable
// ---------------------------------------------------------------------------

/// A QueryPlan that has passed admission, with all defaults applied and
/// execution targets derived.
///
/// This is the canonical type consumed by TASK-002 through TASK-005.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedQueryPlan {
    /// Stable identifier for this query plan (generated at admission if absent).
    pub query_plan_id: QueryPlanId,

    /// Semantic description (trimmed, non-empty).
    pub description: String,

    /// Normalized query texts (at minimum `[description]`).
    pub query_texts: Vec<String>,

    /// Required number of qualified images (≥ 1 after admission).
    pub required_image_count: u32,

    /// Quality tier.
    pub quality: QualityTier,

    /// Structured quality requirements (tier defaults applied).
    pub quality_requirements: QualityRequirements,

    /// Material type preferences.
    pub material_types: Vec<String>,

    /// Visual requirements.
    pub visual_requirements: Vec<String>,

    /// Negative scope.
    pub negative_scope: Vec<String>,

    /// Desired source diversity, if any.
    pub source_diversity_requirement: Option<u32>,

    /// Derived candidate target: `required_image_count * 20`.
    pub candidate_target: u32,

    /// Derived retrieval batch target: `required_image_count * 2`.
    pub retrieval_batch_target: u32,

    /// Retry limit (≤ 3).
    pub retry_limit: u8,

    /// Full attempt limit: `1 + retry_limit`.
    pub full_attempt_limit: u8,

    /// Narrowed provider policy.
    pub provider_policy: QueryProviderPolicy,

    /// Narrowed retrieval policy.
    pub retrieval_policy: QueryRetrievalPolicy,

    /// Non-blocking diagnostics produced during admission.
    pub admission_diagnostics: Vec<AdmissionDiagnostic>,
}

// ---------------------------------------------------------------------------
// QueryPlan identity
// ---------------------------------------------------------------------------

/// Opaque identifier for a query plan.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct QueryPlanId(pub String);

impl QueryPlanId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Generate a new unique id.
    pub fn generate() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        use std::time::{SystemTime, UNIX_EPOCH};
        // A process-local monotonic counter guarantees uniqueness even when two
        // calls land in the same nanosecond (the timestamp alone collides under
        // load — see the regression this fixes).
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
        Self(format!("qp-{:016x}-{:x}", nanos, seq))
    }
}

impl std::fmt::Display for QueryPlanId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ---------------------------------------------------------------------------
// Attempt counter state
// ---------------------------------------------------------------------------

/// Runtime attempt-counter state for downstream serialization.
///
/// Invariants:
/// - Before first attempt: `full_attempt_count = 1`, `retry_count = 0`.
/// - During attempt N: `retry_count = full_attempt_count - 1`.
/// - `full_attempt_limit = 1 + retry_limit`.
/// - Default `retry_limit = 3`, default `full_attempt_limit = 4`.
/// - Validation fails when `retry_count != full_attempt_count - 1`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttemptCounterState {
    /// Current full attempt (1-based, starts at 1).
    pub full_attempt_count: u8,

    /// Retries so far (0-based).
    pub retry_count: u8,

    /// Maximum full attempts allowed.
    pub full_attempt_limit: u8,

    /// Maximum retries allowed.
    pub retry_limit: u8,
}

impl AttemptCounterState {
    /// Create a new counter state before the first attempt.
    pub fn initial(retry_limit: u8) -> Self {
        Self {
            full_attempt_count: 1,
            retry_count: 0,
            full_attempt_limit: 1 + retry_limit,
            retry_limit,
        }
    }

    /// Advance to the next attempt. Returns `None` if the limit is reached.
    pub fn advance(&mut self) -> Option<()> {
        if self.full_attempt_count >= self.full_attempt_limit {
            return None;
        }
        self.full_attempt_count += 1;
        self.retry_count = self.full_attempt_count - 1;
        Some(())
    }

    /// Verify the `retry_count == full_attempt_count - 1` invariant.
    pub fn invariant_holds(&self) -> bool {
        self.retry_count == self.full_attempt_count - 1
    }
}

// ---------------------------------------------------------------------------
// Admission diagnostics
// ---------------------------------------------------------------------------

/// Severity level for admission diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiagnosticSeverity {
    /// Informational — normal operation.
    #[serde(rename = "info")]
    Info,

    /// Warning — something worth attention but not blocking.
    #[serde(rename = "warning")]
    Warning,

    /// Error — admission is blocked.
    #[serde(rename = "error")]
    Error,

    /// Blocker — external dependency or policy prevents execution.
    #[serde(rename = "blocker")]
    Blocker,
}

/// Machine-readable admission failure codes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdmissionFailureCode {
    /// Description is absent or whitespace-only.
    #[serde(rename = "INPUT_DESCRIPTION_MISSING")]
    InputDescriptionMissing,

    /// Explicit zero was replaced with default one.
    #[serde(rename = "REQUIRED_COUNT_ZERO_DEFAULTED")]
    RequiredCountZeroDefaulted,

    /// Required image count exceeds configured local limit.
    #[serde(rename = "REQUIRED_COUNT_LIMIT_EXCEEDED")]
    RequiredCountLimitExceeded,

    /// Quality value is outside `general/high/strict`.
    #[serde(rename = "QUALITY_UNSUPPORTED")]
    QualityUnsupported,

    /// Retry limit exceeds 3.
    #[serde(rename = "RETRY_LIMIT_EXCEEDED")]
    RetryLimitExceeded,

    /// Derived candidate or retrieval target overflows.
    #[serde(rename = "TARGET_DERIVATION_OVERFLOW")]
    TargetDerivationOverflow,

    /// Enabled provider has zero weight.
    #[serde(rename = "CONFIG_PROVIDER_WEIGHT_INVALID")]
    ConfigProviderWeightInvalid,

    /// Required environment variable is missing by name.
    #[serde(rename = "CONFIG_CREDENTIAL_ENV_MISSING")]
    ConfigCredentialEnvMissing,

    /// Paid channel is required but not explicitly allowed.
    #[serde(rename = "POLICY_PAID_UNCONFIRMED")]
    PolicyPaidUnconfirmed,

    /// Robots/site-rule behavior is unknown under current config.
    #[serde(rename = "POLICY_ROBOTS_UNDECIDED")]
    PolicyRobotsUndecided,

    /// Credential-like input was detected and redacted.
    #[serde(rename = "SENSITIVE_INPUT_REDACTED")]
    SensitiveInputRedacted,

    /// Production subjective evaluation cannot run.
    #[serde(rename = "VLM_EVALUATION_UNAVAILABLE")]
    VlmEvaluationUnavailable,

    /// Query text entry was empty and ignored.
    #[serde(rename = "QUERY_TEXT_EMPTY_IGNORED")]
    QueryTextEmptyIgnored,

    /// Source diversity exceeds required image count.
    #[serde(rename = "SOURCE_DIVERSITY_EXCEEDS_REQUIRED")]
    SourceDiversityExceedsRequired,

    /// Paid channel enabled in query but blocked by config policy.
    #[serde(rename = "POLICY_PAID_BLOCKED_BY_CONFIG")]
    PolicyPaidBlockedByConfig,

    /// Query policy attempts to broaden a config restriction.
    #[serde(rename = "POLICY_BROADENING_BLOCKED")]
    PolicyBroadeningBlocked,

    /// Robots unknown posture is configured as block.
    #[serde(rename = "POLICY_ROBOTS_BLOCKED")]
    PolicyRobotsBlocked,
}

/// A diagnostic produced during QueryPlan admission.
///
/// Diagnostics are machine-readable and human-readable. They never contain
/// raw credential values, tokens, cookies, or private keys.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdmissionDiagnostic {
    /// Machine-readable failure code.
    pub code: AdmissionFailureCode,

    /// Severity level.
    pub severity: DiagnosticSeverity,

    /// The field or config path being diagnosed.
    pub field_path: String,

    /// Human-readable message (credential-safe).
    pub message: String,

    /// Suggested remediation, if any.
    pub remediation: Option<String>,

    /// What default was applied, if any.
    pub default_applied: Option<String>,

    /// Whether sensitive content was redacted from this diagnostic.
    #[serde(default)]
    pub redacted: bool,
}

impl AdmissionDiagnostic {
    pub fn error(
        code: AdmissionFailureCode,
        field_path: impl Into<String>,
        message: impl Into<String>,
        remediation: Option<impl Into<String>>,
    ) -> Self {
        Self {
            code,
            severity: DiagnosticSeverity::Error,
            field_path: field_path.into(),
            message: message.into(),
            remediation: remediation.map(|s| s.into()),
            default_applied: None,
            redacted: false,
        }
    }

    pub fn warning(
        code: AdmissionFailureCode,
        field_path: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code,
            severity: DiagnosticSeverity::Warning,
            field_path: field_path.into(),
            message: message.into(),
            remediation: None,
            default_applied: None,
            redacted: false,
        }
    }

    pub fn info(
        code: AdmissionFailureCode,
        field_path: impl Into<String>,
        message: impl Into<String>,
        default_applied: impl Into<String>,
    ) -> Self {
        Self {
            code,
            severity: DiagnosticSeverity::Info,
            field_path: field_path.into(),
            message: message.into(),
            remediation: None,
            default_applied: Some(default_applied.into()),
            redacted: false,
        }
    }

    pub fn blocker(
        code: AdmissionFailureCode,
        field_path: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code,
            severity: DiagnosticSeverity::Blocker,
            field_path: field_path.into(),
            message: message.into(),
            remediation: None,
            default_applied: None,
            redacted: false,
        }
    }

    pub fn with_redacted(mut self) -> Self {
        self.redacted = true;
        self
    }
}

// ---------------------------------------------------------------------------
// Admission outcome
// ---------------------------------------------------------------------------

/// Outcome of admitting a [`QueryPlanInput`] with a [`RuntimeConfig`].
///
/// A successful admission produces a [`NormalizedQueryPlan`] that downstream
/// tasks consume. Rejection prevents any search/retrieval/delivery work.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum AdmissionOutcome {
    /// QueryPlan passed all blocking checks. May carry non-blocking warnings.
    Accepted {
        query_plan: NormalizedQueryPlan,
        /// Non-blocking warnings (large count, sensitive patterns, etc.).
        warnings: Vec<AdmissionDiagnostic>,
    },
    /// QueryPlan failed one or more blocking checks. Must not proceed.
    Rejected {
        diagnostics: Vec<AdmissionDiagnostic>,
    },
}

impl AdmissionOutcome {
    /// Returns `true` if the outcome is accepted.
    pub fn is_accepted(&self) -> bool {
        matches!(self, Self::Accepted { .. })
    }

    /// Extract the normalized plan, panicking if rejected.
    pub fn unwrap(self) -> NormalizedQueryPlan {
        match self {
            Self::Accepted { query_plan, .. } => query_plan,
            Self::Rejected { diagnostics } => {
                panic!(
                    "called `AdmissionOutcome::unwrap()` on a `Rejected` value with {} diagnostic(s)",
                    diagnostics.len()
                )
            }
        }
    }

    /// Return all diagnostics (warnings from accepted, errors from rejected).
    pub fn diagnostics(&self) -> &[AdmissionDiagnostic] {
        match self {
            Self::Accepted { warnings, .. } => warnings.as_slice(),
            Self::Rejected { diagnostics } => diagnostics.as_slice(),
        }
    }
}

// ---------------------------------------------------------------------------
// Config fingerprint
// ---------------------------------------------------------------------------

/// A non-secret hash of config shape and env var names.
/// Never includes credential values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigFingerprint(pub String);

impl ConfigFingerprint {
    pub fn new(hash: impl Into<String>) -> Self {
        Self(hash.into())
    }
}

impl std::fmt::Display for ConfigFingerprint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ---------------------------------------------------------------------------
// Admission logic
// ---------------------------------------------------------------------------

/// Configuration for admission behaviour.
pub struct AdmissionConfig {
    /// Maximum allowed `required_image_count`. Requests above this are rejected.
    pub max_required_image_count: u32,
}

impl Default for AdmissionConfig {
    fn default() -> Self {
        Self {
            max_required_image_count: 1000,
        }
    }
}

/// Admit a [`QueryPlanInput`] and produce a [`NormalizedQueryPlan`] or rejection.
///
/// # Blocking checks
///
/// - `description` is empty or whitespace-only.
/// - `retry_limit` exceeds the constitution maximum of 3.
/// - `required_image_count` exceeds `admission_config.max_required_image_count`.
/// - Target derivation overflows `u32`.
/// - `source_diversity_requirement > required_image_count`.
///
/// # Non-blocking checks
///
/// - `required_image_count` is zero (defaulted to 1).
/// - `required_image_count` is large (≥ 100).
/// - Empty `query_texts` entries are removed.
/// - Description or text fields contain suspected credentials.
pub fn admit_query_plan(
    input: QueryPlanInput,
    admission_config: &AdmissionConfig,
) -> AdmissionOutcome {
    let mut diagnostics: Vec<AdmissionDiagnostic> = Vec::new();

    // --- blocking: description ---
    let trimmed = input.description.trim();
    if trimmed.is_empty() {
        return AdmissionOutcome::Rejected {
            diagnostics: vec![AdmissionDiagnostic::error(
                AdmissionFailureCode::InputDescriptionMissing,
                "description",
                "Semantic description is missing. A description of the desired image(s) is required.",
                Some("Provide a description, e.g. \"sunset over mountains\"."),
            )],
        };
    }

    // --- blocking: retry_limit ---
    const MAX_RETRY: u8 = 3;
    if input.retry_limit > MAX_RETRY {
        diagnostics.push(AdmissionDiagnostic::error(
            AdmissionFailureCode::RetryLimitExceeded,
            "retry_limit",
            format!(
                "Retry limit {} exceeds the maximum allowed ({}).",
                input.retry_limit, MAX_RETRY
            ),
            Some(format!("Set retry_limit to at most {}.", MAX_RETRY)),
        ));
    }

    // --- blocking: required_image_count limit ---
    if input.required_image_count > admission_config.max_required_image_count {
        diagnostics.push(AdmissionDiagnostic::error(
            AdmissionFailureCode::RequiredCountLimitExceeded,
            "required_image_count",
            format!(
                "Required image count {} exceeds the configured limit ({}).",
                input.required_image_count, admission_config.max_required_image_count
            ),
            Some(format!(
                "Reduce required_image_count to at most {}.",
                admission_config.max_required_image_count
            )),
        ));
    }

    // Collect errors so far; reject now to avoid deriving targets from bad input.
    let errors: Vec<AdmissionDiagnostic> = diagnostics
        .iter()
        .filter(|d| d.severity == DiagnosticSeverity::Error)
        .cloned()
        .collect();
    if !errors.is_empty() {
        return AdmissionOutcome::Rejected {
            diagnostics: errors,
        };
    }

    // --- non-blocking: required_image_count zero ---
    let applied_count = if input.required_image_count == 0 {
        diagnostics.push(AdmissionDiagnostic::warning(
            AdmissionFailureCode::RequiredCountZeroDefaulted,
            "required_image_count",
            "Required image count was zero; defaulting to 1.",
        ));
        1u32
    } else {
        input.required_image_count
    };

    // --- non-blocking: large count risk ---
    const LARGE_COUNT_THRESHOLD: u32 = 100;
    if applied_count >= LARGE_COUNT_THRESHOLD {
        diagnostics.push(AdmissionDiagnostic::warning(
            AdmissionFailureCode::RequiredCountLimitExceeded,
            "required_image_count",
            format!(
                "Large request: {} images requested. Candidate target will be {}. This may take significant time.",
                applied_count,
                applied_count.saturating_mul(20)
            ),
        ));
    }

    // --- derive targets ---
    let candidate_target = match applied_count.checked_mul(20) {
        Some(v) => v,
        None => {
            return AdmissionOutcome::Rejected {
                diagnostics: vec![AdmissionDiagnostic::error(
                    AdmissionFailureCode::TargetDerivationOverflow,
                    "candidate_target",
                    format!(
                        "Candidate target derivation overflowed for required_image_count={}.",
                        applied_count
                    ),
                    Some("Reduce required_image_count."),
                )],
            };
        }
    };

    let retrieval_batch_target = match applied_count.checked_mul(2) {
        Some(v) => v,
        None => {
            return AdmissionOutcome::Rejected {
                diagnostics: vec![AdmissionDiagnostic::error(
                    AdmissionFailureCode::TargetDerivationOverflow,
                    "retrieval_batch_target",
                    format!(
                        "Retrieval batch target derivation overflowed for required_image_count={}.",
                        applied_count
                    ),
                    Some("Reduce required_image_count."),
                )],
            };
        }
    };

    // --- non-blocking: source diversity ---
    if let Some(diversity) = input.source_diversity_requirement {
        if diversity > applied_count {
            diagnostics.push(AdmissionDiagnostic::warning(
                AdmissionFailureCode::SourceDiversityExceedsRequired,
                "source_diversity_requirement",
                format!(
                    "Source diversity requirement ({}) exceeds required image count ({}).",
                    diversity, applied_count
                ),
            ));
        }
    }

    // --- non-blocking: sensitive content (before query_texts move) ---
    check_sensitive_patterns_v1(&input, &mut diagnostics);

    // --- normalize query texts ---
    let mut query_texts: Vec<String> = input
        .query_texts
        .into_iter()
        .filter(|t| {
            let trimmed = t.trim();
            if trimmed.is_empty() {
                diagnostics.push(AdmissionDiagnostic::warning(
                    AdmissionFailureCode::QueryTextEmptyIgnored,
                    "query_texts",
                    "Empty query text entry was ignored.",
                ));
                false
            } else {
                true
            }
        })
        .collect();

    if query_texts.is_empty() {
        query_texts.push(trimmed.to_string());
    }

    // --- build normalized plan ---
    let retry_limit = input.retry_limit.min(MAX_RETRY);
    let full_attempt_limit = 1 + retry_limit;

    let normalized = NormalizedQueryPlan {
        query_plan_id: QueryPlanId::generate(),
        description: trimmed.to_string(),
        query_texts,
        required_image_count: applied_count,
        quality: input.quality,
        quality_requirements: input.quality_requirements,
        material_types: input.material_types,
        visual_requirements: input.visual_requirements,
        negative_scope: input.negative_scope,
        source_diversity_requirement: input.source_diversity_requirement,
        candidate_target,
        retrieval_batch_target,
        retry_limit,
        full_attempt_limit,
        provider_policy: input.provider_policy,
        retrieval_policy: input.retrieval_policy,
        admission_diagnostics: diagnostics.clone(),
    };

    AdmissionOutcome::Accepted {
        query_plan: normalized,
        warnings: diagnostics,
    }
}

// ---------------------------------------------------------------------------
// Policy narrowing helpers
// ---------------------------------------------------------------------------

/// Narrow a QueryPlan policy against the runtime config policy.
///
/// The query plan policy may only restrict (never broaden) the runtime policy.
/// Returns a list of diagnostics for any broadening attempts detected.
pub fn narrow_policy(
    query_policy: &QueryRetrievalPolicy,
    config_allow_paid: bool,
    config_respect_robots: bool,
    config_allow_login: bool,
    config_allow_paywalled: bool,
) -> Vec<AdmissionDiagnostic> {
    let mut diags = Vec::new();

    // Paid: query cannot enable if config disables
    if query_policy.allow_paid && !config_allow_paid {
        diags.push(AdmissionDiagnostic::blocker(
            AdmissionFailureCode::PolicyPaidBlockedByConfig,
            "retrieval_policy.allow_paid",
            "Query plan requests paid retrieval, but paid channels are disabled in runtime config.",
        ));
    }

    // Robots: config respect is minimum; query cannot override to non-respect
    if !query_policy.respect_robots && config_respect_robots {
        diags.push(AdmissionDiagnostic::warning(
            AdmissionFailureCode::PolicyBroadeningBlocked,
            "retrieval_policy.respect_robots",
            "Query plan attempts to disable robots respect, but config requires it.",
        ));
    }

    // Login: query cannot enable if config disables
    if query_policy.allow_login && !config_allow_login {
        diags.push(AdmissionDiagnostic::warning(
            AdmissionFailureCode::PolicyBroadeningBlocked,
            "retrieval_policy.allow_login",
            "Query plan requests login-required sources, disabled by config policy.",
        ));
    }

    // Paywalled: query cannot enable if config disables
    if query_policy.allow_paywalled && !config_allow_paywalled {
        diags.push(AdmissionDiagnostic::warning(
            AdmissionFailureCode::PolicyBroadeningBlocked,
            "retrieval_policy.allow_paywalled",
            "Query plan requests paywalled sources, disabled by config policy.",
        ));
    }

    diags
}

/// Resolve the effective (narrowed) retrieval policy from query plan and config.
pub fn effective_retrieval_policy(
    query_policy: &QueryRetrievalPolicy,
    config_allow_paid: bool,
    config_respect_robots: bool,
    config_allow_login: bool,
    config_allow_paywalled: bool,
) -> QueryRetrievalPolicy {
    QueryRetrievalPolicy {
        allow_paid: query_policy.allow_paid && config_allow_paid,
        respect_robots: query_policy.respect_robots || config_respect_robots,
        allow_login: query_policy.allow_login && config_allow_login,
        allow_paywalled: query_policy.allow_paywalled && config_allow_paywalled,
    }
}

// ---------------------------------------------------------------------------
// Legacy types (backward compatibility with existing consumers)
// ---------------------------------------------------------------------------

/// A QueryPlan that has passed input validation and has all defaults applied.
///
/// **Deprecated for v1.1**: prefer [`NormalizedQueryPlan`].
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

/// Execution plan derived from a validated QueryPlan.
///
/// **Deprecated for v1.1**: target derivation is now part of
/// [`NormalizedQueryPlan`].
#[derive(Debug, Clone)]
pub struct TaskPlan {
    /// The validated QueryPlan this plan was derived from.
    pub query_plan: ValidatedQueryPlan,

    /// Target number of candidates to search for (≈ required_count × 20).
    pub candidate_target: u32,

    /// Target number of candidates per retrieval batch (= required_count × 2).
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

/// A field-level diagnostic produced during QueryPlan input validation.
///
/// **Deprecated for v1.1**: prefer [`AdmissionDiagnostic`].
#[derive(Debug, Clone)]
pub struct InputDiagnostic {
    pub field: String,
    pub severity: DiagnosticLevel,
    pub reason: String,
    pub default_applied: Option<String>,
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

/// Collection of diagnostic findings that together make a QueryPlan invalid.
///
/// **Deprecated for v1.1**: prefer [`AdmissionOutcome::Rejected`].
#[derive(Debug, Clone)]
pub struct InputRejection {
    pub diagnostics: Vec<InputDiagnostic>,
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

/// Outcome of validating a [`QueryPlanInput`].
///
/// **Deprecated for v1.1**: prefer [`AdmissionOutcome`].
#[derive(Debug, Clone)]
pub enum ValidationOutcome {
    Valid {
        plan: ValidatedQueryPlan,
        warnings: Vec<InputDiagnostic>,
    },
    Rejected(InputRejection),
}

impl ValidationOutcome {
    pub fn is_valid(&self) -> bool {
        matches!(self, Self::Valid { .. })
    }

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

    pub fn diagnostics(&self) -> &[InputDiagnostic] {
        match self {
            Self::Valid { warnings, .. } => warnings.as_slice(),
            Self::Rejected(rejection) => rejection.diagnostics.as_slice(),
        }
    }
}

// ---------------------------------------------------------------------------
// Legacy validation (backward compat)
// ---------------------------------------------------------------------------

/// Validate a [`QueryPlanInput`] and produce a [`ValidatedQueryPlan`] or
/// an [`InputRejection`].
///
/// **Deprecated for v1.1**: prefer [`admit_query_plan`].
pub fn validate_query_plan(input: QueryPlanInput) -> ValidationOutcome {
    let mut diagnostics: Vec<InputDiagnostic> = Vec::new();

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

    const MAX_RETRY: u32 = 3;
    if (input.retry_limit as u32) > MAX_RETRY {
        diagnostics.push(InputDiagnostic::error(
            "retry_limit",
            format!(
                "重试策略越界：retry_limit 为 {}，但宪法允许最多 {} 次重试。",
                input.retry_limit, MAX_RETRY
            ),
            format!("请将 retry_limit 设置为不超过 {} 的值。", MAX_RETRY),
        ));
    }

    let errors: Vec<InputDiagnostic> = diagnostics
        .iter()
        .filter(|d| d.severity == DiagnosticLevel::Error)
        .cloned()
        .collect();
    if !errors.is_empty() {
        let summary = format!("输入被拒绝：{} 个问题需要修复。", errors.len());
        return ValidationOutcome::Rejected(InputRejection::new(errors, summary));
    }

    let applied_count = if input.required_image_count == 0 {
        diagnostics.push(InputDiagnostic::warning(
            "required_count",
            "交付数量为 0：不会搜索候选或抓取图片。",
            Some("如需获得图片，请设置 required_count 为至少 1。"),
        ));
        1u32
    } else {
        input.required_image_count
    };

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

    check_sensitive_patterns_legacy(&input, &mut diagnostics);

    let plan = ValidatedQueryPlan {
        description: trimmed.to_string(),
        required_count: applied_count,
        quality_tier: input.quality,
        content_constraints: input.content_constraints,
        authorization_preference: input.authorization_preference,
        output_preference: input.output_preference,
        retry_limit: (input.retry_limit as u32).min(MAX_RETRY),
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

/// Check description and query texts for suspected credential patterns (v1.1).
fn check_sensitive_patterns_v1(input: &QueryPlanInput, diags: &mut Vec<AdmissionDiagnostic>) {
    let fields_to_check: Vec<(&str, &str)> = {
        let mut fields = vec![("description", input.description.as_str())];
        for (i, qt) in input.query_texts.iter().enumerate() {
            fields.push((
                // Leak of temporary is fine since we push into Vec
                Box::leak(format!("query_texts[{}]", i).into_boxed_str()),
                qt.as_str(),
            ));
        }
        fields
    };

    for (field_path, text) in &fields_to_check {
        let lower = text.to_lowercase();
        for (pattern, label) in SENSITIVE_PATTERNS {
            let lower_pat = pattern.to_lowercase();
            if lower.contains(&lower_pat) {
                diags.push(
                    AdmissionDiagnostic::warning(
                        AdmissionFailureCode::SensitiveInputRedacted,
                        *field_path,
                        format!(
                            "Suspected sensitive input: {} may contain {}. Specific values are not echoed in diagnostics.",
                            field_path, label
                        ),
                    )
                    .with_redacted(),
                );
                break;
            }
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
                diags.push(
                    AdmissionDiagnostic::warning(
                        AdmissionFailureCode::SensitiveInputRedacted,
                        "content_constraints",
                        format!(
                            "Suspected sensitive input in content constraints: may contain {}. Values are not echoed.",
                            label
                        ),
                    )
                    .with_redacted(),
                );
                return;
            }
        }
    }
}

/// Legacy sensitive-pattern checker (backward compat).
fn check_sensitive_patterns_legacy(input: &QueryPlanInput, diags: &mut Vec<InputDiagnostic>) {
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
            break;
        }
    }

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
                return;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // Default value tests
    // =========================================================================

    #[test]
    fn default_required_image_count_is_1() {
        assert_eq!(default_required_image_count(), 1);
    }

    #[test]
    fn default_retry_limit_is_3() {
        assert_eq!(default_retry_limit(), 3);
    }

    #[test]
    fn quality_tier_default_is_general() {
        assert_eq!(QualityTier::default(), QualityTier::General);
    }

    #[test]
    fn quality_requirements_default_thumbnail_only_is_false() {
        let qr = QualityRequirements::default();
        assert!(!qr.allow_thumbnail_only);
    }

    #[test]
    fn query_provider_policy_default_allow_fixture_is_false() {
        let pp = QueryProviderPolicy::default();
        assert!(!pp.allow_fixture);
    }

    #[test]
    fn query_retrieval_policy_default_paid_disabled() {
        let rp = QueryRetrievalPolicy::default();
        assert!(!rp.allow_paid);
        assert!(rp.respect_robots);
        assert!(!rp.allow_login);
        assert!(!rp.allow_paywalled);
    }

    // =========================================================================
    // Admission — acceptance tests
    // =========================================================================

    #[test]
    fn admit_minimal_input_produces_normalized_plan() {
        let input = QueryPlanInput {
            description: "sunset over mountains".into(),
            ..Default::default()
        };
        let outcome = admit_query_plan(input, &AdmissionConfig::default());
        assert!(outcome.is_accepted());
        let plan = outcome.unwrap();
        assert_eq!(plan.description, "sunset over mountains");
        assert_eq!(plan.required_image_count, 1);
        assert_eq!(plan.quality, QualityTier::General);
        assert_eq!(plan.retry_limit, 3);
        assert_eq!(plan.full_attempt_limit, 4);
        assert_eq!(plan.candidate_target, 20);
        assert_eq!(plan.retrieval_batch_target, 2);
        assert_eq!(plan.query_texts, vec!["sunset over mountains"]);
    }

    #[test]
    fn admit_missing_description_rejected() {
        let input = QueryPlanInput {
            description: "".into(),
            ..Default::default()
        };
        let outcome = admit_query_plan(input, &AdmissionConfig::default());
        assert!(!outcome.is_accepted());
        match outcome {
            AdmissionOutcome::Rejected { diagnostics } => {
                assert_eq!(diagnostics.len(), 1);
                assert_eq!(
                    diagnostics[0].code,
                    AdmissionFailureCode::InputDescriptionMissing
                );
            }
            _ => panic!("expected rejected"),
        }
    }

    #[test]
    fn admit_whitespace_only_description_rejected() {
        let input = QueryPlanInput {
            description: "   \n  \t  ".into(),
            ..Default::default()
        };
        let outcome = admit_query_plan(input, &AdmissionConfig::default());
        assert!(!outcome.is_accepted());
    }

    #[test]
    fn admit_retry_limit_exceeds_max_rejected() {
        let input = QueryPlanInput {
            description: "test".into(),
            retry_limit: 10,
            ..Default::default()
        };
        let outcome = admit_query_plan(input, &AdmissionConfig::default());
        assert!(!outcome.is_accepted());
    }

    #[test]
    fn admit_required_count_exceeds_limit_rejected() {
        let config = AdmissionConfig {
            max_required_image_count: 50,
        };
        let input = QueryPlanInput {
            description: "test".into(),
            required_image_count: 100,
            ..Default::default()
        };
        let outcome = admit_query_plan(input, &config);
        assert!(!outcome.is_accepted());
    }

    #[test]
    fn admit_zero_count_defaulted_with_warning() {
        let input = QueryPlanInput {
            description: "test".into(),
            required_image_count: 0,
            ..Default::default()
        };
        let outcome = admit_query_plan(input, &AdmissionConfig::default());
        assert!(outcome.is_accepted());
        match outcome {
            AdmissionOutcome::Accepted {
                query_plan,
                warnings,
            } => {
                assert_eq!(query_plan.required_image_count, 1);
                assert!(warnings
                    .iter()
                    .any(|d| d.code == AdmissionFailureCode::RequiredCountZeroDefaulted));
            }
            _ => panic!("expected accepted"),
        }
    }

    #[test]
    fn admit_query_texts_defaults_to_description() {
        let input = QueryPlanInput {
            description: "cats playing".into(),
            query_texts: vec![],
            ..Default::default()
        };
        let outcome = admit_query_plan(input, &AdmissionConfig::default());
        let plan = outcome.unwrap();
        assert_eq!(plan.query_texts, vec!["cats playing"]);
    }

    #[test]
    fn admit_empty_query_text_entries_ignored() {
        let input = QueryPlanInput {
            description: "dogs running".into(),
            query_texts: vec!["valid query".into(), "".into(), "  ".into()],
            ..Default::default()
        };
        let outcome = admit_query_plan(input, &AdmissionConfig::default());
        let plan = outcome.unwrap();
        assert_eq!(plan.query_texts, vec!["valid query"]);
    }

    #[test]
    fn admit_source_diversity_exceeds_required_warns() {
        let input = QueryPlanInput {
            description: "test".into(),
            required_image_count: 2,
            source_diversity_requirement: Some(5),
            ..Default::default()
        };
        let outcome = admit_query_plan(input, &AdmissionConfig::default());
        assert!(outcome.is_accepted());
        match outcome {
            AdmissionOutcome::Accepted { warnings, .. } => {
                assert!(warnings
                    .iter()
                    .any(|d| d.code == AdmissionFailureCode::SourceDiversityExceedsRequired));
            }
            _ => panic!("expected accepted"),
        }
    }

    // =========================================================================
    // Derivation tests
    // =========================================================================

    #[test]
    fn candidate_target_is_20n() {
        for n in &[1, 2, 3, 5, 10] {
            let input = QueryPlanInput {
                description: "test".into(),
                required_image_count: *n,
                ..Default::default()
            };
            let outcome = admit_query_plan(input, &AdmissionConfig::default());
            let plan = outcome.unwrap();
            assert_eq!(
                plan.candidate_target,
                n * 20,
                "candidate_target for n={}",
                n
            );
        }
    }

    #[test]
    fn retrieval_batch_target_is_2n() {
        for n in &[1, 2, 3, 5, 10] {
            let input = QueryPlanInput {
                description: "test".into(),
                required_image_count: *n,
                ..Default::default()
            };
            let outcome = admit_query_plan(input, &AdmissionConfig::default());
            let plan = outcome.unwrap();
            assert_eq!(
                plan.retrieval_batch_target,
                n * 2,
                "retrieval_batch_target for n={}",
                n
            );
        }
    }

    #[test]
    fn full_attempt_limit_is_one_plus_retry_limit() {
        let input = QueryPlanInput {
            description: "test".into(),
            retry_limit: 3,
            ..Default::default()
        };
        let outcome = admit_query_plan(input, &AdmissionConfig::default());
        let plan = outcome.unwrap();
        assert_eq!(plan.full_attempt_limit, 4);
    }

    #[test]
    fn full_attempt_limit_with_retry_limit_2() {
        let input = QueryPlanInput {
            description: "test".into(),
            retry_limit: 2,
            ..Default::default()
        };
        let outcome = admit_query_plan(input, &AdmissionConfig::default());
        let plan = outcome.unwrap();
        assert_eq!(plan.full_attempt_limit, 3);
    }

    // =========================================================================
    // Serde compatibility tests
    // =========================================================================

    #[test]
    fn required_image_count_is_canonical() {
        let json = r#"{"description": "test", "required_image_count": 5}"#;
        let parsed: QueryPlanInput = serde_json::from_str(json).expect("deserialize");
        assert_eq!(parsed.required_image_count, 5);
    }

    #[test]
    fn required_count_alias_accepted() {
        let json = r#"{"description": "test", "required_count": 5}"#;
        let parsed: QueryPlanInput = serde_json::from_str(json).expect("deserialize");
        assert_eq!(parsed.required_image_count, 5);
    }

    #[test]
    fn required_image_count_serializes_canonically() {
        let input = QueryPlanInput {
            description: "test".into(),
            required_image_count: 5,
            ..Default::default()
        };
        let serialized = serde_json::to_string(&input).expect("serialize");
        assert!(serialized.contains("required_image_count"));
        // The alias `required_count` must not appear as a serialized output field
        assert!(!serialized.contains("\"required_count\""));
    }

    #[test]
    fn quality_field_accepts_quality_tier_alias() {
        let json = r#"{"description": "test", "quality_tier": "strict"}"#;
        let parsed: QueryPlanInput = serde_json::from_str(json).expect("deserialize");
        assert_eq!(parsed.quality, QualityTier::Strict);
    }

    // =========================================================================
    // AttemptCounterState tests
    // =========================================================================

    #[test]
    fn attempt_counter_initial_state() {
        let state = AttemptCounterState::initial(3);
        assert_eq!(state.full_attempt_count, 1);
        assert_eq!(state.retry_count, 0);
        assert_eq!(state.full_attempt_limit, 4);
        assert_eq!(state.retry_limit, 3);
        assert!(state.invariant_holds());
    }

    #[test]
    fn attempt_counter_advance() {
        let mut state = AttemptCounterState::initial(3);
        assert!(state.advance().is_some());
        assert_eq!(state.full_attempt_count, 2);
        assert_eq!(state.retry_count, 1);
        assert!(state.invariant_holds());
    }

    #[test]
    fn attempt_counter_exhausted_after_limit() {
        let mut state = AttemptCounterState::initial(1); // limit = 2
        assert!(state.advance().is_some()); // count=2, retry=1
        assert!(state.advance().is_none()); // exhausted
        assert_eq!(state.full_attempt_count, 2);
    }

    #[test]
    fn attempt_counter_invariant_fails_on_mismatch() {
        let state = AttemptCounterState {
            full_attempt_count: 2,
            retry_count: 0, // should be 1
            full_attempt_limit: 4,
            retry_limit: 3,
        };
        assert!(!state.invariant_holds());
    }

    // =========================================================================
    // Policy narrowing tests
    // =========================================================================

    #[test]
    fn paid_query_blocked_when_config_disables() {
        let query = QueryRetrievalPolicy {
            allow_paid: true,
            ..Default::default()
        };
        let diags = narrow_policy(&query, false, true, false, false);
        assert!(diags
            .iter()
            .any(|d| d.code == AdmissionFailureCode::PolicyPaidBlockedByConfig));
    }

    #[test]
    fn paid_query_allowed_when_config_enables() {
        let query = QueryRetrievalPolicy {
            allow_paid: true,
            ..Default::default()
        };
        let diags = narrow_policy(&query, true, true, false, false);
        assert!(diags.is_empty());
    }

    #[test]
    fn robots_disable_blocked_when_config_requires() {
        let query = QueryRetrievalPolicy {
            respect_robots: false,
            ..Default::default()
        };
        let diags = narrow_policy(&query, false, true, false, false);
        assert!(diags
            .iter()
            .any(|d| d.code == AdmissionFailureCode::PolicyBroadeningBlocked));
    }

    #[test]
    fn login_query_blocked_when_config_disables() {
        let query = QueryRetrievalPolicy {
            allow_login: true,
            ..Default::default()
        };
        let diags = narrow_policy(&query, false, true, false, false);
        assert!(diags
            .iter()
            .any(|d| d.code == AdmissionFailureCode::PolicyBroadeningBlocked));
    }

    #[test]
    fn effective_policy_narrows_correctly() {
        let query = QueryRetrievalPolicy {
            allow_paid: true,
            respect_robots: false,
            allow_login: true,
            allow_paywalled: true,
        };
        let effective = effective_retrieval_policy(&query, true, true, false, false);
        // Paid: query=true AND config=true → true
        assert!(effective.allow_paid);
        // Robots: query=false OR config=true → true (config overrides)
        assert!(effective.respect_robots);
        // Login: query=true AND config=false → false
        assert!(!effective.allow_login);
        // Paywalled: query=true AND config=false → false
        assert!(!effective.allow_paywalled);
    }

    // =========================================================================
    // Sensitive input tests
    // =========================================================================

    #[test]
    fn sensitive_bearer_token_in_description_warns() {
        let input = QueryPlanInput {
            description: "Bearer eyJhbGciOiJIUzI1NiJ9.test description".into(),
            ..Default::default()
        };
        let outcome = admit_query_plan(input, &AdmissionConfig::default());
        assert!(outcome.is_accepted());
        match outcome {
            AdmissionOutcome::Accepted { warnings, .. } => {
                let sens = warnings
                    .iter()
                    .find(|d| {
                        d.code == AdmissionFailureCode::SensitiveInputRedacted
                            && d.field_path == "description"
                    })
                    .expect("should have sensitive content warning");
                assert!(sens.redacted);
                assert!(!sens.message.contains("eyJhbGci"));
            }
            _ => panic!("expected accepted"),
        }
    }

    #[test]
    fn sensitive_api_key_in_description_warns() {
        let input = QueryPlanInput {
            description: "use x-api-key: abc123secret for access".into(),
            ..Default::default()
        };
        let outcome = admit_query_plan(input, &AdmissionConfig::default());
        assert!(outcome.is_accepted());
        match outcome {
            AdmissionOutcome::Accepted { warnings, .. } => {
                let sens = warnings
                    .iter()
                    .find(|d| d.code == AdmissionFailureCode::SensitiveInputRedacted)
                    .expect("should have API key warning");
                assert!(!sens.message.contains("abc123secret"));
            }
            _ => panic!("expected accepted"),
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
        let outcome = admit_query_plan(input, &AdmissionConfig::default());
        assert!(outcome.is_accepted());
        match outcome {
            AdmissionOutcome::Accepted { warnings, .. } => {
                let sens = warnings
                    .iter()
                    .find(|d| {
                        d.code == AdmissionFailureCode::SensitiveInputRedacted
                            && d.field_path == "content_constraints"
                    })
                    .expect("should have constraints warning");
                assert!(!sens.message.contains("secret123"));
            }
            _ => panic!("expected accepted"),
        }
    }

    #[test]
    fn clean_description_no_sensitive_warning() {
        let input = QueryPlanInput {
            description: "a beautiful sunset over mountains with orange sky".into(),
            ..Default::default()
        };
        let outcome = admit_query_plan(input, &AdmissionConfig::default());
        assert!(outcome.is_accepted());
        match outcome {
            AdmissionOutcome::Accepted { warnings, .. } => {
                let has_sensitive = warnings
                    .iter()
                    .any(|d| d.code == AdmissionFailureCode::SensitiveInputRedacted);
                assert!(
                    !has_sensitive,
                    "clean description should not trigger sensitive warning"
                );
            }
            _ => panic!("expected accepted"),
        }
    }

    // =========================================================================
    // Legacy validation backward-compat tests
    // =========================================================================

    #[test]
    fn legacy_validate_valid_with_description() {
        let input = QueryPlanInput {
            description: "sunset over mountains".into(),
            ..Default::default()
        };
        let outcome = validate_query_plan(input);
        assert!(outcome.is_valid());
        let plan = outcome.unwrap();
        assert_eq!(plan.description, "sunset over mountains");
        assert_eq!(plan.required_count, 1);
        assert_eq!(plan.quality_tier, QualityTier::General);
        assert_eq!(plan.retry_limit, 3);
    }

    #[test]
    fn legacy_validate_missing_description_rejected() {
        let input = QueryPlanInput {
            description: "".into(),
            ..Default::default()
        };
        let outcome = validate_query_plan(input);
        assert!(!outcome.is_valid());
    }

    #[test]
    fn legacy_validate_retry_exceeds_max_rejected() {
        let input = QueryPlanInput {
            description: "test".into(),
            retry_limit: 10,
            ..Default::default()
        };
        let outcome = validate_query_plan(input);
        assert!(!outcome.is_valid());
    }

    #[test]
    fn legacy_validate_zero_count_warns() {
        let input = QueryPlanInput {
            description: "test".into(),
            required_image_count: 0,
            ..Default::default()
        };
        let outcome = validate_query_plan(input);
        assert!(outcome.is_valid());
        match outcome {
            ValidationOutcome::Valid { plan, warnings } => {
                assert_eq!(plan.required_count, 1);
                assert!(!warnings.is_empty());
            }
            _ => panic!("expected valid"),
        }
    }

    #[test]
    fn legacy_task_plan_derives_targets() {
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
        assert_eq!(task.candidate_target, 60);
        assert_eq!(task.retrieval_batch_target, 6);
        assert_eq!(task.max_attempts, 4);
    }

    #[test]
    fn legacy_task_plan_single_image() {
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

    // =========================================================================
    // AdmissionDiagnostic constructors
    // =========================================================================

    #[test]
    fn diagnostic_error_has_correct_severity() {
        let d = AdmissionDiagnostic::error(
            AdmissionFailureCode::InputDescriptionMissing,
            "description",
            "missing",
            Some("fix it"),
        );
        assert_eq!(d.severity, DiagnosticSeverity::Error);
        assert_eq!(d.code, AdmissionFailureCode::InputDescriptionMissing);
        assert!(d.remediation.is_some());
    }

    #[test]
    fn diagnostic_warning_has_correct_severity() {
        let d = AdmissionDiagnostic::warning(
            AdmissionFailureCode::RequiredCountZeroDefaulted,
            "required_image_count",
            "was zero",
        );
        assert_eq!(d.severity, DiagnosticSeverity::Warning);
        assert_eq!(d.default_applied, None);
    }

    #[test]
    fn diagnostic_info_records_default() {
        let d = AdmissionDiagnostic::info(
            AdmissionFailureCode::RequiredCountZeroDefaulted,
            "field",
            "applied default",
            "1",
        );
        assert_eq!(d.severity, DiagnosticSeverity::Info);
        assert_eq!(d.default_applied, Some("1".to_string()));
    }

    #[test]
    fn diagnostic_blocker_has_correct_severity() {
        let d = AdmissionDiagnostic::blocker(
            AdmissionFailureCode::VlmEvaluationUnavailable,
            "vlm_evaluation",
            "Qwen not available",
        );
        assert_eq!(d.severity, DiagnosticSeverity::Blocker);
    }

    // =========================================================================
    // QueryPlanId
    // =========================================================================

    #[test]
    fn query_plan_id_generates_unique() {
        let a = QueryPlanId::generate();
        let b = QueryPlanId::generate();
        assert_ne!(a, b);
        assert!(!a.0.is_empty());
    }

    // =========================================================================
    // Quality tier serde
    // =========================================================================

    #[test]
    fn quality_tier_serde_general() {
        let json = "\"general\"";
        let tier: QualityTier = serde_json::from_str(json).expect("deserialize");
        assert_eq!(tier, QualityTier::General);
        assert_eq!(serde_json::to_string(&tier).unwrap(), json);
    }

    #[test]
    fn quality_tier_serde_high() {
        let json = "\"high\"";
        let tier: QualityTier = serde_json::from_str(json).expect("deserialize");
        assert_eq!(tier, QualityTier::High);
    }

    #[test]
    fn quality_tier_serde_strict() {
        let json = "\"strict\"";
        let tier: QualityTier = serde_json::from_str(json).expect("deserialize");
        assert_eq!(tier, QualityTier::Strict);
    }

    // =========================================================================
    // AdmissionOutcome
    // =========================================================================

    #[test]
    fn outcome_diagnostics_returns_warnings_for_accepted() {
        let input = QueryPlanInput {
            description: "test".into(),
            required_image_count: 0,
            ..Default::default()
        };
        let outcome = admit_query_plan(input, &AdmissionConfig::default());
        assert!(outcome.is_accepted());
        let diags = outcome.diagnostics();
        assert!(!diags.is_empty());
    }

    #[test]
    fn outcome_diagnostics_returns_errors_for_rejected() {
        let input = QueryPlanInput {
            description: "".into(),
            ..Default::default()
        };
        let outcome = admit_query_plan(input, &AdmissionConfig::default());
        assert!(!outcome.is_accepted());
        let diags = outcome.diagnostics();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].severity, DiagnosticSeverity::Error);
    }

    #[test]
    #[should_panic]
    fn outcome_unwrap_panics_on_rejected() {
        let input = QueryPlanInput {
            description: "".into(),
            ..Default::default()
        };
        let outcome = admit_query_plan(input, &AdmissionConfig::default());
        outcome.unwrap();
    }
}
