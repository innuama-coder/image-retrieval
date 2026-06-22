#![allow(clippy::too_many_arguments)]
//! Search domain types.
//!
//! v1.1 canonical types per LLD and
//! `docs/design/v1.1-TASK-002-search-provider-candidate-design.md`.
//!
//! Covers:
//! - SearchRequest / SearchResponse / ProviderRawImageResult
//! - ProviderReadinessReport, failure codes, credential/health/quota status
//! - WeightedProviderEntry, SearchUsageEvent
//! - SearchSessionOutcome, SearchDiagnostic, CandidateShortageReason
//! - SearchSessionState (in-memory state)
//! - SearchError (provider-level errors)
//!
//! Legacy types retained at the bottom for backward compatibility.
//!
//! References: PRD FR-004/FR-005, HLD §Search Scheduler, LLD §Search Provider Contract

use crate::domain::candidate::{CandidateDedupeEvidence, CandidateRecord, ProviderId};
use crate::domain::config::SearchProviderKind;
use crate::domain::query_plan::QueryPlanId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

// ---------------------------------------------------------------------------
// SearchError — provider-level search failures
// ---------------------------------------------------------------------------

/// Errors that can occur during a provider search invocation.
///
/// These are distinct from `crate::error::Error` — they carry structured
/// failure codes for machine consumption and never contain credentials.
#[derive(Debug, Clone)]
pub enum SearchError {
    /// Provider is misconfigured (missing endpoint, invalid params).
    Misconfigured { reason: String },

    /// Required credential environment variable is absent.
    CredentialMissing { env_var: String },

    /// HTTP request failed (network, DNS, TLS).
    HttpError {
        status: Option<u16>,
        message: String,
    },

    /// Provider returned a response that could not be parsed.
    ParseError { message: String },

    /// Provider timed out.
    Timeout { message: String },

    /// Provider is rate-limited.
    RateLimited { message: String },

    /// Provider returned an empty result set (not an error, but no results).
    EmptyResult,

    /// Provider is unavailable for an unknown reason.
    Unavailable { message: String },

    /// Adapter-specific internal error.
    Internal { message: String },
}

impl std::fmt::Display for SearchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Misconfigured { reason } => write!(f, "provider misconfigured: {}", reason),
            Self::CredentialMissing { env_var } => {
                write!(f, "credential env var '{}' is not set", env_var)
            }
            Self::HttpError { status, message } => {
                if let Some(s) = status {
                    write!(f, "HTTP {} error: {}", s, message)
                } else {
                    write!(f, "HTTP error: {}", message)
                }
            }
            Self::ParseError { message } => write!(f, "parse error: {}", message),
            Self::Timeout { message } => write!(f, "timeout: {}", message),
            Self::RateLimited { message } => write!(f, "rate limited: {}", message),
            Self::EmptyResult => write!(f, "empty result set"),
            Self::Unavailable { message } => write!(f, "unavailable: {}", message),
            Self::Internal { message } => write!(f, "internal error: {}", message),
        }
    }
}

impl std::error::Error for SearchError {}

impl SearchError {
    pub fn credential_missing(env_var: impl Into<String>) -> Self {
        Self::CredentialMissing {
            env_var: env_var.into(),
        }
    }

    pub fn misconfigured(reason: impl Into<String>) -> Self {
        Self::Misconfigured {
            reason: reason.into(),
        }
    }

    pub fn http(status: Option<u16>, message: impl Into<String>) -> Self {
        Self::HttpError {
            status,
            message: message.into(),
        }
    }

    pub fn parse(message: impl Into<String>) -> Self {
        Self::ParseError {
            message: message.into(),
        }
    }

    pub fn timeout(message: impl Into<String>) -> Self {
        Self::Timeout {
            message: message.into(),
        }
    }

    pub fn rate_limited(message: impl Into<String>) -> Self {
        Self::RateLimited {
            message: message.into(),
        }
    }

    pub fn unavailable(message: impl Into<String>) -> Self {
        Self::Unavailable {
            message: message.into(),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal {
            message: message.into(),
        }
    }

    /// Classify this error into a [`ProviderFailureCode`].
    pub fn to_failure_code(&self) -> ProviderFailureCode {
        match self {
            Self::CredentialMissing { .. } => ProviderFailureCode::ProviderCredentialMissing,
            Self::Misconfigured { .. } => ProviderFailureCode::ProviderWeightInvalid,
            Self::HttpError { status, .. } => match status {
                Some(429) => ProviderFailureCode::ProviderQuotaExhausted,
                _ => ProviderFailureCode::ProviderUnavailable,
            },
            Self::Timeout { .. } => ProviderFailureCode::ProviderUnavailable,
            Self::RateLimited { .. } => ProviderFailureCode::ProviderQuotaExhausted,
            Self::ParseError { .. } => ProviderFailureCode::ProviderUnavailable,
            Self::EmptyResult => ProviderFailureCode::ProviderUnavailable,
            Self::Unavailable { .. } => ProviderFailureCode::ProviderUnavailable,
            Self::Internal { .. } => ProviderFailureCode::ProviderUnavailable,
        }
    }
}

// ---------------------------------------------------------------------------
// Provider readiness status
// ---------------------------------------------------------------------------

/// Readiness status of a provider after inspection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderReadinessStatus {
    /// Provider is ready to serve search requests.
    Ready,

    /// Provider is explicitly disabled in configuration.
    Disabled,

    /// Required credential environment variable is absent.
    MissingCredentials,

    /// Provider configuration is invalid (e.g. malformed endpoint).
    Misconfigured,

    /// Provider health check failed.
    HealthFailed,

    /// Provider quota or rate limit is exhausted.
    QuotaExhausted,

    /// Provider cannot satisfy QueryPlan/provider policy constraints.
    ConstraintUnsupported,

    /// Provider is known to be retired / deprecated.
    Retired,

    /// Fixture provider attempted in production mode.
    FixtureOnly,

    /// Unknown or transient provider unavailability.
    Unavailable,
}

impl ProviderReadinessStatus {
    /// Returns `true` if the provider can be included in the effective weight table.
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready)
    }

    /// Returns `true` if the status is terminal for this run (requires config change).
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Disabled
                | Self::MissingCredentials
                | Self::Misconfigured
                | Self::Retired
                | Self::FixtureOnly
        )
    }
}

impl std::fmt::Display for ProviderReadinessStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Self::Ready => "ready",
            Self::Disabled => "disabled",
            Self::MissingCredentials => "missing_credentials",
            Self::Misconfigured => "misconfigured",
            Self::HealthFailed => "health_failed",
            Self::QuotaExhausted => "quota_exhausted",
            Self::ConstraintUnsupported => "constraint_unsupported",
            Self::Retired => "retired",
            Self::FixtureOnly => "fixture_only",
            Self::Unavailable => "unavailable",
        };
        write!(f, "{}", label)
    }
}

// ---------------------------------------------------------------------------
// Provider failure codes
// ---------------------------------------------------------------------------

/// Machine-readable failure codes for provider readiness and execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderFailureCode {
    /// Config explicitly disables provider.
    #[serde(rename = "PROVIDER_DISABLED")]
    ProviderDisabled,

    /// Config names a provider kind not compiled or registered.
    #[serde(rename = "PROVIDER_ADAPTER_MISSING")]
    ProviderAdapterMissing,

    /// Required credential env var is absent.
    #[serde(rename = "PROVIDER_CREDENTIAL_MISSING")]
    ProviderCredentialMissing,

    /// Enabled provider has zero or invalid weight.
    #[serde(rename = "PROVIDER_WEIGHT_INVALID")]
    ProviderWeightInvalid,

    /// Adapter health check failed.
    #[serde(rename = "PROVIDER_HEALTH_FAILED")]
    ProviderHealthFailed,

    /// Provider indicates quota/rate limit exhaustion.
    #[serde(rename = "PROVIDER_QUOTA_EXHAUSTED")]
    ProviderQuotaExhausted,

    /// Provider cannot satisfy constraints.
    #[serde(rename = "PROVIDER_CONSTRAINT_UNSUPPORTED")]
    ProviderConstraintUnsupported,

    /// Provider is known unavailable / deprecated.
    #[serde(rename = "PROVIDER_RETIRED")]
    ProviderRetired,

    /// Fixture adapter selected in production mode.
    #[serde(rename = "PROVIDER_FIXTURE_NOT_PRODUCTION")]
    ProviderFixtureNotProduction,

    /// Unknown or transient unavailability.
    #[serde(rename = "PROVIDER_UNAVAILABLE")]
    ProviderUnavailable,
}

impl std::fmt::Display for ProviderFailureCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::ProviderDisabled => "PROVIDER_DISABLED",
            Self::ProviderAdapterMissing => "PROVIDER_ADAPTER_MISSING",
            Self::ProviderCredentialMissing => "PROVIDER_CREDENTIAL_MISSING",
            Self::ProviderWeightInvalid => "PROVIDER_WEIGHT_INVALID",
            Self::ProviderHealthFailed => "PROVIDER_HEALTH_FAILED",
            Self::ProviderQuotaExhausted => "PROVIDER_QUOTA_EXHAUSTED",
            Self::ProviderConstraintUnsupported => "PROVIDER_CONSTRAINT_UNSUPPORTED",
            Self::ProviderRetired => "PROVIDER_RETIRED",
            Self::ProviderFixtureNotProduction => "PROVIDER_FIXTURE_NOT_PRODUCTION",
            Self::ProviderUnavailable => "PROVIDER_UNAVAILABLE",
        };
        write!(f, "{}", s)
    }
}

// ---------------------------------------------------------------------------
// Credential / health / quota status
// ---------------------------------------------------------------------------

/// Status of a provider's credential.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CredentialStatus {
    /// Credential env var is present and appears valid.
    Present,
    /// Credential env var is not set.
    Missing { env_var: String },
    /// Credential was not checked (e.g. provider does not require one).
    NotRequired,
}

/// Status of a provider health check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HealthCheckStatus {
    /// Health check passed.
    Healthy,
    /// Health check failed.
    Failed { reason: String },
    /// Health check was not performed.
    NotChecked,
}

/// Status of a provider's quota.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum QuotaStatus {
    /// Quota is sufficient.
    Ok,
    /// Quota is near exhaustion.
    NearExhaustion,
    /// Quota is exhausted.
    Exhausted,
    /// Quota status unknown.
    Unknown,
}

// ---------------------------------------------------------------------------
// Provider constraint support
// ---------------------------------------------------------------------------

/// Constraints a provider declares it can (or cannot) support.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProviderConstraintSupport {
    /// Maximum results per request the provider can return.
    pub max_results_per_request: Option<u32>,

    /// Supported image content types.
    pub supported_content_types: Vec<String>,

    /// Whether the provider supports quality-tier filtering.
    pub supports_quality_filter: bool,

    /// Whether the provider supports license filtering.
    pub supports_license_filter: bool,

    /// Whether the provider supports dimension filtering.
    pub supports_dimension_filter: bool,
}

// ---------------------------------------------------------------------------
// Provider evidence
// ---------------------------------------------------------------------------

/// A piece of evidence about a provider's readiness.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderEvidence {
    /// Machine-readable evidence code.
    pub code: String,

    /// Human-readable message (redacted, no credentials).
    pub message: String,

    /// Severity: info, warning, error, blocker.
    pub severity: String,
}

// ---------------------------------------------------------------------------
// Provider readiness report
// ---------------------------------------------------------------------------

/// Full readiness report for a configured provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderReadinessReport {
    /// Stable provider identifier.
    pub provider_id: ProviderId,

    /// Adapter family.
    pub provider_kind: SearchProviderKind,

    /// Human-readable display name.
    pub display_name: String,

    /// Readiness status.
    pub status: ProviderReadinessStatus,

    /// Whether the provider is available for scheduling.
    pub available: bool,

    /// Whether the provider appears in the effective weight table.
    pub included_in_weight_table: bool,

    /// Weight from config.
    pub configured_weight: u32,

    /// Weight used in scheduling (if included).
    pub effective_weight: Option<u32>,

    /// Credential readiness.
    pub credential_status: CredentialStatus,

    /// Health check result.
    pub health_check_status: HealthCheckStatus,

    /// Quota status.
    pub quota_status: QuotaStatus,

    /// Constraint support declaration.
    pub constraint_support: ProviderConstraintSupport,

    /// Failure code if not ready.
    pub failure_code: Option<ProviderFailureCode>,

    /// When readiness was checked (ISO 8601 string).
    pub checked_at: String,

    /// Supporting evidence items.
    pub evidence: Vec<ProviderEvidence>,

    /// Whether redaction was applied to any evidence messages.
    pub redaction_applied: bool,
}

impl ProviderReadinessReport {
    pub fn ready(
        provider_id: ProviderId,
        provider_kind: SearchProviderKind,
        display_name: &str,
    ) -> Self {
        Self {
            provider_id,
            provider_kind,
            display_name: display_name.into(),
            status: ProviderReadinessStatus::Ready,
            available: true,
            included_in_weight_table: true,
            configured_weight: 1,
            effective_weight: Some(1),
            credential_status: CredentialStatus::NotRequired,
            health_check_status: HealthCheckStatus::NotChecked,
            quota_status: QuotaStatus::Unknown,
            constraint_support: ProviderConstraintSupport::default(),
            failure_code: None,
            checked_at: String::new(),
            evidence: Vec::new(),
            redaction_applied: false,
        }
    }

    pub fn not_ready(
        provider_id: ProviderId,
        provider_kind: SearchProviderKind,
        display_name: &str,
        status: ProviderReadinessStatus,
        failure_code: ProviderFailureCode,
        evidence: Vec<ProviderEvidence>,
    ) -> Self {
        Self {
            provider_id,
            provider_kind,
            display_name: display_name.into(),
            status,
            available: false,
            included_in_weight_table: false,
            configured_weight: 0,
            effective_weight: None,
            credential_status: CredentialStatus::NotRequired,
            health_check_status: HealthCheckStatus::NotChecked,
            quota_status: QuotaStatus::Unknown,
            constraint_support: ProviderConstraintSupport::default(),
            failure_code: Some(failure_code),
            checked_at: String::new(),
            evidence,
            redaction_applied: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Search request (v1.1)
// ---------------------------------------------------------------------------

/// A search request passed to a provider adapter.
///
/// Package-safe: contains no credential values.
#[derive(Debug, Clone)]
pub struct SearchRequest {
    /// Unique identifier for this request.
    pub search_request_id: String,

    /// Query plan that originated this search.
    pub query_plan_id: QueryPlanId,

    /// Full attempt count from the orchestrator.
    pub full_attempt_count: u8,

    /// Retry count (= full_attempt_count - 1).
    pub retry_count: u8,

    /// Which search round within the session (1-based).
    pub search_round: u32,

    /// Which provider this request is dispatched to.
    pub provider_id: ProviderId,

    /// The query text to search for.
    pub query_text: String,

    /// Human-readable description for context.
    pub semantic_description: String,

    /// Maximum results to return from this call.
    pub max_results: u32,

    /// Request tags for traceability (no credentials).
    pub request_tags: Vec<(String, String)>,
}

impl SearchRequest {
    pub fn new(
        query_plan_id: QueryPlanId,
        provider_id: ProviderId,
        query_text: impl Into<String>,
        max_results: u32,
        search_round: u32,
        full_attempt_count: u8,
    ) -> Self {
        let retry_count = full_attempt_count.saturating_sub(1);
        let search_request_id = format!("sr-{}-{}-r{}", query_plan_id, provider_id, search_round);
        Self {
            search_request_id,
            query_plan_id,
            full_attempt_count,
            retry_count,
            search_round,
            provider_id,
            query_text: query_text.into(),
            semantic_description: String::new(),
            max_results,
            request_tags: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Search response
// ---------------------------------------------------------------------------

/// Status of a search response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SearchResponseStatus {
    /// All requested results were returned.
    Complete,

    /// Some results returned but not all requested.
    Partial,

    /// No results returned (but no error).
    Empty,

    /// The provider returned an error.
    Failed,
}

/// Normalized response from a provider search invocation.
#[derive(Debug, Clone)]
pub struct SearchResponse {
    /// Echoes the request id.
    pub search_request_id: String,

    /// Which provider produced this response.
    pub provider_id: ProviderId,

    /// Adapter family.
    pub provider_kind: SearchProviderKind,

    /// Query plan id.
    pub query_plan_id: QueryPlanId,

    /// Which search round.
    pub search_round: u32,

    /// Overall status.
    pub status: SearchResponseStatus,

    /// Normalized candidate records.
    pub candidates: Vec<CandidateRecord>,

    /// How many raw results the provider returned.
    pub raw_result_count: u32,

    /// How many candidates were successfully normalized.
    pub normalized_count: u32,

    /// Whether the provider has more pages available.
    pub provider_next_page_token_present: bool,

    /// Whether the provider is exhausted for this query.
    pub exhausted: bool,

    /// Diagnostics from this invocation.
    pub diagnostics: Vec<SearchDiagnostic>,

    /// Whether redaction was applied.
    pub redaction_applied: bool,
}

impl SearchResponse {
    pub fn empty(
        request: &SearchRequest,
        provider_id: ProviderId,
        provider_kind: SearchProviderKind,
    ) -> Self {
        Self {
            search_request_id: request.search_request_id.clone(),
            provider_id,
            provider_kind,
            query_plan_id: request.query_plan_id.clone(),
            search_round: request.search_round,
            status: SearchResponseStatus::Empty,
            candidates: Vec::new(),
            raw_result_count: 0,
            normalized_count: 0,
            provider_next_page_token_present: false,
            exhausted: true,
            diagnostics: Vec::new(),
            redaction_applied: false,
        }
    }

    pub fn failed(
        request: &SearchRequest,
        provider_id: ProviderId,
        provider_kind: SearchProviderKind,
        diagnostics: Vec<SearchDiagnostic>,
    ) -> Self {
        Self {
            search_request_id: request.search_request_id.clone(),
            provider_id,
            provider_kind,
            query_plan_id: request.query_plan_id.clone(),
            search_round: request.search_round,
            status: SearchResponseStatus::Failed,
            candidates: Vec::new(),
            raw_result_count: 0,
            normalized_count: 0,
            provider_next_page_token_present: false,
            exhausted: false,
            diagnostics,
            redaction_applied: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Provider raw image result (internal adapter type)
// ---------------------------------------------------------------------------

/// A raw image result from a provider, before normalization into
/// `CandidateRecord`. Used internally by adapters.
#[derive(Debug, Clone)]
pub struct ProviderRawImageResult {
    /// Provider's native identifier for this result.
    pub provider_raw_id: Option<String>,

    /// 1-based rank in the provider's result list.
    pub provider_rank: u32,

    /// Direct image URL.
    pub image_url: Option<String>,

    /// Source page URL.
    pub source_page_url: Option<String>,

    /// Thumbnail / preview URL.
    pub thumbnail_url: Option<String>,

    /// Title or alt text.
    pub title: Option<String>,

    /// Snippet / description.
    pub snippet: Option<String>,

    /// Image width in pixels.
    pub width: Option<u32>,

    /// Image height in pixels.
    pub height: Option<u32>,

    /// MIME type (e.g. "image/jpeg").
    pub mime_type: Option<String>,

    /// License hint from the provider.
    pub license_hint: Option<String>,

    /// Attribution string.
    pub attribution: Option<String>,

    /// Safe extras: explicitly allow-listed metadata only.
    pub provider_extra_safe: std::collections::BTreeMap<String, String>,
}

// ---------------------------------------------------------------------------
// Diagnostic severity
// ---------------------------------------------------------------------------

/// Severity of a search diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiagnosticSeverity {
    /// Informational — normal operation.
    #[serde(rename = "info")]
    Info,

    /// Warning — notable but not blocking.
    #[serde(rename = "warning")]
    Warning,

    /// Error — something went wrong.
    #[serde(rename = "error")]
    Error,

    /// Blocker — search cannot proceed.
    #[serde(rename = "blocker")]
    Blocker,
}

// ---------------------------------------------------------------------------
// Search diagnostic codes
// ---------------------------------------------------------------------------

/// Machine-readable search diagnostic codes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SearchDiagnosticCode {
    #[serde(rename = "DEFAULT_PROVIDER_UNAVAILABLE")]
    DefaultProviderUnavailable,
    #[serde(rename = "PROVIDER_DISABLED")]
    ProviderDisabled,
    #[serde(rename = "PROVIDER_ADAPTER_MISSING")]
    ProviderAdapterMissing,
    #[serde(rename = "PROVIDER_CREDENTIAL_MISSING")]
    ProviderCredentialMissing,
    #[serde(rename = "PROVIDER_WEIGHT_INVALID")]
    ProviderWeightInvalid,
    #[serde(rename = "PROVIDER_HEALTH_FAILED")]
    ProviderHealthFailed,
    #[serde(rename = "PROVIDER_QUOTA_EXHAUSTED")]
    ProviderQuotaExhausted,
    #[serde(rename = "PROVIDER_CONSTRAINT_UNSUPPORTED")]
    ProviderConstraintUnsupported,
    #[serde(rename = "PROVIDER_FIXTURE_NOT_PRODUCTION")]
    ProviderFixtureNotProduction,
    #[serde(rename = "SEARCH_PROVIDER_TIMEOUT")]
    SearchProviderTimeout,
    #[serde(rename = "SEARCH_PROVIDER_RATE_LIMITED")]
    SearchProviderRateLimited,
    #[serde(rename = "SEARCH_RESPONSE_UNNORMALIZABLE")]
    SearchResponseUnnormalizable,
    #[serde(rename = "PROVIDER_UNAVAILABLE")]
    ProviderUnavailable,
    #[serde(rename = "CANDIDATE_IMAGE_URL_MISSING")]
    CandidateImageUrlMissing,
    #[serde(rename = "CANDIDATE_SOURCE_URL_MISSING")]
    CandidateSourceUrlMissing,
    #[serde(rename = "CANDIDATE_DIMENSIONS_MISSING")]
    CandidateDimensionsMissing,
    #[serde(rename = "CANDIDATE_LICENSE_UNKNOWN")]
    CandidateLicenseUnknown,
    #[serde(rename = "CANDIDATE_DUPLICATE_MERGED")]
    CandidateDuplicateMerged,
    #[serde(rename = "SEARCH_TARGET_SHORTAGE")]
    SearchTargetShortage,
    #[serde(rename = "NO_AVAILABLE_SEARCH_PROVIDER")]
    NoAvailableSearchProvider,
    #[serde(rename = "SEARCH_INVOCATION_LIMIT_REACHED")]
    SearchInvocationLimitReached,
}

// ---------------------------------------------------------------------------
// Search diagnostic
// ---------------------------------------------------------------------------

/// A diagnostic produced during search execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchDiagnostic {
    /// Machine-readable code.
    pub code: SearchDiagnosticCode,

    /// Severity.
    pub severity: DiagnosticSeverity,

    /// Which provider this relates to, if any.
    pub provider_id: Option<ProviderId>,

    /// Which candidate this relates to, if any.
    pub candidate_id: Option<crate::domain::candidate::CandidateId>,

    /// Which search request, if any.
    pub search_request_id: Option<String>,

    /// Human-readable message (redacted).
    pub message: String,

    /// Suggested remediation.
    pub remediation: Option<String>,

    /// Whether sensitive content was redacted.
    pub redacted: bool,
}

impl SearchDiagnostic {
    pub fn info(code: SearchDiagnosticCode, message: impl Into<String>) -> Self {
        Self {
            code,
            severity: DiagnosticSeverity::Info,
            provider_id: None,
            candidate_id: None,
            search_request_id: None,
            message: message.into(),
            remediation: None,
            redacted: false,
        }
    }

    pub fn warning(code: SearchDiagnosticCode, message: impl Into<String>) -> Self {
        Self {
            code,
            severity: DiagnosticSeverity::Warning,
            provider_id: None,
            candidate_id: None,
            search_request_id: None,
            message: message.into(),
            remediation: None,
            redacted: false,
        }
    }

    pub fn error(code: SearchDiagnosticCode, message: impl Into<String>) -> Self {
        Self {
            code,
            severity: DiagnosticSeverity::Error,
            provider_id: None,
            candidate_id: None,
            search_request_id: None,
            message: message.into(),
            remediation: None,
            redacted: false,
        }
    }

    pub fn blocker(code: SearchDiagnosticCode, message: impl Into<String>) -> Self {
        Self {
            code,
            severity: DiagnosticSeverity::Blocker,
            provider_id: None,
            candidate_id: None,
            search_request_id: None,
            message: message.into(),
            remediation: None,
            redacted: false,
        }
    }

    pub fn with_provider(mut self, provider_id: ProviderId) -> Self {
        self.provider_id = Some(provider_id);
        self
    }

    pub fn with_candidate(mut self, candidate_id: crate::domain::candidate::CandidateId) -> Self {
        self.candidate_id = Some(candidate_id);
        self
    }

    pub fn with_request(mut self, search_request_id: impl Into<String>) -> Self {
        self.search_request_id = Some(search_request_id.into());
        self
    }
}

// ---------------------------------------------------------------------------
// Weighted provider entry
// ---------------------------------------------------------------------------

/// One entry in the effective weight table for weighted random selection.
#[derive(Debug, Clone)]
pub struct WeightedProviderEntry {
    pub provider_id: ProviderId,
    pub display_name: String,
    pub effective_weight: u32,
    pub capabilities: ProviderConstraintSupport,
}

// ---------------------------------------------------------------------------
// Search usage event
// ---------------------------------------------------------------------------

/// A record of one provider invocation during search scheduling.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchUsageEvent {
    /// Query plan id.
    pub query_plan_id: QueryPlanId,

    /// Which provider was invoked.
    pub provider_id: ProviderId,

    /// The search request id.
    pub search_request_id: String,

    /// Full attempt count from the orchestrator.
    pub full_attempt_count: u8,

    /// Retry count.
    pub retry_count: u8,

    /// Which search round (1-based).
    pub search_round: u32,

    /// The effective weight used for selection.
    pub selected_weight: u32,

    /// Total active weight across all non-exhausted providers.
    pub total_active_weight: u32,

    /// Raw candidate count from the provider.
    pub raw_candidate_count: u32,

    /// Candidates after normalization (before dedupe).
    pub normalized_candidate_count: u32,

    /// Unique candidates after deduplication.
    pub unique_candidate_count_after_dedupe: u32,

    /// How many duplicates were found.
    pub duplicate_count: u32,

    /// Response status.
    pub response_status: SearchResponseStatus,

    /// Failure code if the invocation failed.
    pub failure_code: Option<ProviderFailureCode>,

    /// Whether the provider was exhausted after this call.
    pub exhausted: bool,

    /// Invocation duration in milliseconds.
    pub duration_ms: u64,
}

impl SearchUsageEvent {
    pub fn success_event(
        query_plan_id: QueryPlanId,
        provider_id: ProviderId,
        search_request_id: String,
        full_attempt_count: u8,
        retry_count: u8,
        search_round: u32,
        selected_weight: u32,
        total_active_weight: u32,
        raw: u32,
        normalized: u32,
        unique: u32,
        duplicates: u32,
        exhausted: bool,
        duration_ms: u64,
    ) -> Self {
        Self {
            query_plan_id,
            provider_id,
            search_request_id,
            full_attempt_count,
            retry_count,
            search_round,
            selected_weight,
            total_active_weight,
            raw_candidate_count: raw,
            normalized_candidate_count: normalized,
            unique_candidate_count_after_dedupe: unique,
            duplicate_count: duplicates,
            response_status: SearchResponseStatus::Complete,
            failure_code: None,
            exhausted,
            duration_ms,
        }
    }

    pub fn failure_event(
        query_plan_id: QueryPlanId,
        provider_id: ProviderId,
        search_request_id: String,
        full_attempt_count: u8,
        retry_count: u8,
        search_round: u32,
        selected_weight: u32,
        total_active_weight: u32,
        failure_code: ProviderFailureCode,
        duration_ms: u64,
    ) -> Self {
        Self {
            query_plan_id,
            provider_id,
            search_request_id,
            full_attempt_count,
            retry_count,
            search_round,
            selected_weight,
            total_active_weight,
            raw_candidate_count: 0,
            normalized_candidate_count: 0,
            unique_candidate_count_after_dedupe: 0,
            duplicate_count: 0,
            response_status: SearchResponseStatus::Failed,
            failure_code: Some(failure_code),
            exhausted: false,
            duration_ms,
        }
    }
}

// ---------------------------------------------------------------------------
// Candidate shortage reasons
// ---------------------------------------------------------------------------

/// Why the candidate target was not met.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CandidateShortageReason {
    /// No ready provider was in the effective table.
    #[serde(rename = "NO_AVAILABLE_SEARCH_PROVIDER")]
    NoAvailableSearchProvider,

    /// Every ready provider signaled exhaustion before target.
    #[serde(rename = "ALL_PROVIDERS_EXHAUSTED")]
    AllProvidersExhausted,

    /// Raw recalls existed but dedupe left fewer than target.
    #[serde(rename = "INSUFFICIENT_UNIQUE_CANDIDATES")]
    InsufficientUniqueCandidates {
        total_raw: u32,
        total_deduped: u32,
        duplicates_removed: u32,
    },

    /// Some providers failed during the session.
    #[serde(rename = "PROVIDER_PARTIAL_FAILURE")]
    ProviderPartialFailure {
        failed_providers: Vec<ProviderId>,
        exhausted_providers: Vec<ProviderId>,
    },

    /// Safety cap stopped scheduling before target.
    #[serde(rename = "SEARCH_INVOCATION_LIMIT_REACHED")]
    SearchInvocationLimitReached,
}

impl std::fmt::Display for CandidateShortageReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoAvailableSearchProvider => {
                write!(f, "no available search provider")
            }
            Self::AllProvidersExhausted => {
                write!(
                    f,
                    "all providers exhausted before reaching candidate target"
                )
            }
            Self::InsufficientUniqueCandidates {
                total_raw,
                total_deduped,
                duplicates_removed,
            } => {
                write!(
                    f,
                    "insufficient unique candidates: {} raw, {} after dedup ({} duplicates removed)",
                    total_raw, total_deduped, duplicates_removed
                )
            }
            Self::ProviderPartialFailure {
                failed_providers,
                exhausted_providers,
            } => {
                write!(
                    f,
                    "partial failure: {} provider(s) failed, {} provider(s) exhausted",
                    failed_providers.len(),
                    exhausted_providers.len()
                )
            }
            Self::SearchInvocationLimitReached => {
                write!(f, "search invocation limit reached before target")
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Search session outcome
// ---------------------------------------------------------------------------

/// The aggregated result of a search scheduling session.
///
/// This is the handoff to TASK-003 (candidate quality) and the source of
/// `image-recalls.json` content.
#[derive(Debug, Clone)]
pub struct SearchSessionOutcome {
    /// Query plan that was searched for.
    pub query_plan_id: QueryPlanId,

    /// Full attempt count from the orchestrator.
    pub full_attempt_count: u8,

    /// Retry count.
    pub retry_count: u8,

    /// The candidate target that was requested.
    pub candidate_target: u32,

    /// How many unique candidates were collected.
    pub unique_candidate_count: u32,

    /// Whether the target was met.
    pub target_met: bool,

    /// All unique candidates (normalized, deduplicated).
    pub candidates: Vec<CandidateRecord>,

    /// Readiness reports for all configured providers.
    pub readiness_reports: Vec<ProviderReadinessReport>,

    /// Per-invocation usage events.
    pub usage_events: Vec<SearchUsageEvent>,

    /// Deduplication evidence for merged candidates.
    pub dedupe_events: Vec<CandidateDedupeEvidence>,

    /// Search diagnostics (warnings, errors, blockers).
    pub diagnostics: Vec<SearchDiagnostic>,

    /// Shortage reason if the target was not met.
    pub shortage_reason: Option<CandidateShortageReason>,
}

// ---------------------------------------------------------------------------
// Search session state (in-memory, not serialized to package)
// ---------------------------------------------------------------------------

/// In-memory scheduler state during a search session.
#[derive(Debug, Clone)]
pub struct SearchSessionState {
    /// Query plan id.
    pub query_plan_id: QueryPlanId,

    /// Full attempt count.
    pub full_attempt_count: u8,

    /// Retry count.
    pub retry_count: u8,

    /// Current search round (1-based).
    pub search_round: u32,

    /// Candidate target.
    pub candidate_target: u32,

    /// How many unique candidates collected so far.
    pub unique_candidate_count: u32,

    /// How many provider invocations made.
    pub invocation_count: u32,

    /// Providers that have been exhausted.
    pub exhausted_providers: BTreeSet<ProviderId>,

    /// Providers that have terminally failed.
    pub terminal_failed_providers: BTreeSet<ProviderId>,

    /// Dedupe index: dedupe_key → canonical candidate_id.
    pub dedupe_index: std::collections::BTreeMap<String, crate::domain::candidate::CandidateId>,
}

// ---------------------------------------------------------------------------
// Provider registration (legacy compat)
// ---------------------------------------------------------------------------

/// Registration metadata for a search provider.
///
/// **Deprecated for v1.1**: prefer [`ProviderReadinessReport`] for readiness
/// and [`crate::domain::config::SearchProviderConfig`] for config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderRegistration {
    pub provider_id: ProviderId,
    pub display_name: String,

    #[serde(default = "default_enabled")]
    pub enabled: bool,

    #[serde(default = "default_weight")]
    pub weight: i32,

    #[serde(default)]
    pub capabilities: ProviderCapabilities,
}

fn default_enabled() -> bool {
    true
}
fn default_weight() -> i32 {
    1
}

impl ProviderRegistration {
    pub fn new(provider_id: ProviderId, display_name: impl Into<String>) -> Self {
        Self {
            provider_id,
            display_name: display_name.into(),
            enabled: true,
            weight: 1,
            capabilities: ProviderCapabilities::default(),
        }
    }

    pub fn with_weight(mut self, weight: i32) -> Self {
        self.weight = weight;
        self
    }

    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

/// Legacy provider capabilities declaration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProviderCapabilities {
    pub max_results_per_request: Option<u32>,
    #[serde(default)]
    pub supported_content_types: Vec<String>,
}

// ---------------------------------------------------------------------------
// Legacy readiness enum
// ---------------------------------------------------------------------------

/// **Deprecated for v1.1**: prefer [`ProviderReadinessStatus`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderReadiness {
    Ready,
    Disabled,
    MissingCredentials,
    Misconfigured,
    RateLimited,
    Unavailable,
}

impl ProviderReadiness {
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready)
    }
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Disabled | Self::MissingCredentials | Self::Misconfigured
        )
    }
}

impl std::fmt::Display for ProviderReadiness {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Self::Ready => "ready",
            Self::Disabled => "disabled",
            Self::MissingCredentials => "missing_credentials",
            Self::Misconfigured => "misconfigured",
            Self::RateLimited => "rate_limited",
            Self::Unavailable => "unavailable",
        };
        write!(f, "{}", label)
    }
}

// ---------------------------------------------------------------------------
// Legacy WeightEntry
// ---------------------------------------------------------------------------

/// **Deprecated for v1.1**: prefer [`WeightedProviderEntry`].
#[derive(Debug, Clone)]
pub struct WeightEntry {
    pub provider_id: ProviderId,
    pub display_name: String,
    pub weight: u32,
}

// ---------------------------------------------------------------------------
// Legacy SearchOutcome
// ---------------------------------------------------------------------------

/// **Deprecated for v1.1**: prefer [`SearchSessionOutcome`].
#[derive(Debug, Clone)]
pub struct SearchOutcome {
    pub candidates: Vec<CandidateRecord>,
    pub usage_events: Vec<SearchUsageEvent>,
    pub total_invocations: u32,
    pub candidate_target: u32,
    pub target_met: bool,
    pub shortage_reason: Option<CandidateShortageReason>,
    pub readiness_summary: Vec<ProviderReadinessRecord>,
}

/// Legacy readiness record.
#[derive(Debug, Clone)]
pub struct ProviderReadinessRecord {
    pub provider_id: ProviderId,
    pub display_name: String,
    pub readiness: ProviderReadiness,
    pub configured_weight: i32,
    pub included_in_table: bool,
}

// ---------------------------------------------------------------------------
// Legacy SearchFailureCategory
// ---------------------------------------------------------------------------

/// **Deprecated for v1.1**: prefer [`ProviderFailureCode`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SearchFailureCategory {
    ProviderError,
    Timeout,
    RateLimited,
    UnnormalizableResponse,
    EmptyResult,
    Unavailable,
    Other,
}

impl std::fmt::Display for SearchFailureCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Self::ProviderError => "provider_error",
            Self::Timeout => "timeout",
            Self::RateLimited => "rate_limited",
            Self::UnnormalizableResponse => "unnormalizable_response",
            Self::EmptyResult => "empty_result",
            Self::Unavailable => "unavailable",
            Self::Other => "other",
        };
        write!(f, "{}", label)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readiness_status_is_ready() {
        assert!(ProviderReadinessStatus::Ready.is_ready());
        assert!(!ProviderReadinessStatus::Disabled.is_ready());
        assert!(!ProviderReadinessStatus::MissingCredentials.is_ready());
        assert!(!ProviderReadinessStatus::Unavailable.is_ready());
        assert!(!ProviderReadinessStatus::FixtureOnly.is_ready());
    }

    #[test]
    fn readiness_status_terminal() {
        assert!(ProviderReadinessStatus::Disabled.is_terminal());
        assert!(ProviderReadinessStatus::MissingCredentials.is_terminal());
        assert!(ProviderReadinessStatus::Misconfigured.is_terminal());
        assert!(ProviderReadinessStatus::Retired.is_terminal());
        assert!(ProviderReadinessStatus::FixtureOnly.is_terminal());
        assert!(!ProviderReadinessStatus::Ready.is_terminal());
        assert!(!ProviderReadinessStatus::HealthFailed.is_terminal());
        assert!(!ProviderReadinessStatus::QuotaExhausted.is_terminal());
        assert!(!ProviderReadinessStatus::Unavailable.is_terminal());
    }

    #[test]
    fn readiness_status_display() {
        assert_eq!(ProviderReadinessStatus::Ready.to_string(), "ready");
        assert_eq!(ProviderReadinessStatus::Disabled.to_string(), "disabled");
        assert_eq!(
            ProviderReadinessStatus::MissingCredentials.to_string(),
            "missing_credentials"
        );
    }

    #[test]
    fn search_error_to_failure_code() {
        assert_eq!(
            SearchError::credential_missing("SERPAPI_API_KEY").to_failure_code(),
            ProviderFailureCode::ProviderCredentialMissing
        );
        assert_eq!(
            SearchError::rate_limited("too many").to_failure_code(),
            ProviderFailureCode::ProviderQuotaExhausted
        );
        assert_eq!(
            SearchError::http(Some(429), "rate limited").to_failure_code(),
            ProviderFailureCode::ProviderQuotaExhausted
        );
    }

    #[test]
    fn search_error_display() {
        let err = SearchError::credential_missing("SERPAPI_API_KEY");
        assert!(err.to_string().contains("SERPAPI_API_KEY"));
        assert!(err.to_string().contains("not set"));
    }

    #[test]
    fn search_request_builder() {
        let req = SearchRequest::new(
            QueryPlanId::new("qp-1"),
            ProviderId::new("serpapi"),
            "cats playing",
            20,
            1,
            1,
        );
        assert_eq!(req.query_text, "cats playing");
        assert_eq!(req.max_results, 20);
        assert_eq!(req.search_round, 1);
        assert_eq!(req.full_attempt_count, 1);
        assert_eq!(req.retry_count, 0);
        assert!(!req.search_request_id.is_empty());
    }

    #[test]
    fn search_response_empty() {
        let req = SearchRequest::new(
            QueryPlanId::new("qp-1"),
            ProviderId::new("p1"),
            "test",
            10,
            1,
            1,
        );
        let resp = SearchResponse::empty(
            &req,
            ProviderId::new("p1"),
            SearchProviderKind::SerpapiGoogleImages,
        );
        assert_eq!(resp.status, SearchResponseStatus::Empty);
        assert!(resp.exhausted);
        assert!(resp.candidates.is_empty());
    }

    #[test]
    fn readiness_report_ready() {
        let report = ProviderReadinessReport::ready(
            ProviderId::new("p1"),
            SearchProviderKind::SerpapiGoogleImages,
            "SerpApi",
        );
        assert!(report.available);
        assert!(report.included_in_weight_table);
        assert!(report.failure_code.is_none());
    }

    #[test]
    fn readiness_report_not_ready() {
        let report = ProviderReadinessReport::not_ready(
            ProviderId::new("p1"),
            SearchProviderKind::SerpapiGoogleImages,
            "SerpApi",
            ProviderReadinessStatus::MissingCredentials,
            ProviderFailureCode::ProviderCredentialMissing,
            vec![ProviderEvidence {
                code: "CREDENTIAL_MISSING".into(),
                message: "SERPAPI_API_KEY is not set".into(),
                severity: "blocker".into(),
            }],
        );
        assert!(!report.available);
        assert!(!report.included_in_weight_table);
        assert_eq!(
            report.failure_code,
            Some(ProviderFailureCode::ProviderCredentialMissing)
        );
        assert_eq!(report.evidence.len(), 1);
    }

    #[test]
    fn usage_event_success() {
        let event = SearchUsageEvent::success_event(
            QueryPlanId::new("qp-1"),
            ProviderId::new("p1"),
            "sr-1".into(),
            1,
            0,
            1,
            100,
            100,
            50,
            45,
            40,
            5,
            false,
            1200,
        );
        assert_eq!(event.raw_candidate_count, 50);
        assert_eq!(event.normalized_candidate_count, 45);
        assert_eq!(event.unique_candidate_count_after_dedupe, 40);
        assert_eq!(event.duplicate_count, 5);
        assert!(event.failure_code.is_none());
    }

    #[test]
    fn usage_event_failure() {
        let event = SearchUsageEvent::failure_event(
            QueryPlanId::new("qp-1"),
            ProviderId::new("p1"),
            "sr-1".into(),
            1,
            0,
            1,
            100,
            100,
            ProviderFailureCode::ProviderCredentialMissing,
            0,
        );
        assert_eq!(event.raw_candidate_count, 0);
        assert!(event.failure_code.is_some());
    }

    #[test]
    fn search_diagnostic_builders() {
        let d = SearchDiagnostic::info(
            SearchDiagnosticCode::CandidateDimensionsMissing,
            "dimensions not reported by provider",
        );
        assert_eq!(d.severity, DiagnosticSeverity::Info);

        let d = SearchDiagnostic::blocker(
            SearchDiagnosticCode::NoAvailableSearchProvider,
            "no providers ready",
        );
        assert_eq!(d.severity, DiagnosticSeverity::Blocker);
    }

    #[test]
    fn search_diagnostic_with_context() {
        let d = SearchDiagnostic::warning(
            SearchDiagnosticCode::CandidateImageUrlMissing,
            "image URL missing",
        )
        .with_provider(ProviderId::new("p1"))
        .with_request("sr-1");
        assert_eq!(d.provider_id, Some(ProviderId::new("p1")));
        assert!(d.search_request_id.is_some());
    }

    #[test]
    fn failure_code_display() {
        assert_eq!(
            ProviderFailureCode::ProviderCredentialMissing.to_string(),
            "PROVIDER_CREDENTIAL_MISSING"
        );
        assert_eq!(
            ProviderFailureCode::ProviderDisabled.to_string(),
            "PROVIDER_DISABLED"
        );
        assert_eq!(
            ProviderFailureCode::ProviderFixtureNotProduction.to_string(),
            "PROVIDER_FIXTURE_NOT_PRODUCTION"
        );
    }

    #[test]
    fn shortage_reason_display() {
        let r = CandidateShortageReason::NoAvailableSearchProvider;
        assert!(r.to_string().contains("no available search provider"));

        let r = CandidateShortageReason::AllProvidersExhausted;
        assert!(r.to_string().contains("exhausted"));

        let r = CandidateShortageReason::InsufficientUniqueCandidates {
            total_raw: 50,
            total_deduped: 30,
            duplicates_removed: 20,
        };
        assert!(r.to_string().contains("50"));
        assert!(r.to_string().contains("30"));

        let r = CandidateShortageReason::SearchInvocationLimitReached;
        assert!(r.to_string().contains("invocation limit"));
    }

    #[test]
    fn legacy_provider_registration_defaults() {
        let reg = ProviderRegistration::new(ProviderId::new("p1"), "P1");
        assert!(reg.enabled);
        assert_eq!(reg.weight, 1);
    }

    #[test]
    fn legacy_readiness_is_ready() {
        assert!(ProviderReadiness::Ready.is_ready());
        assert!(!ProviderReadiness::Disabled.is_ready());
    }
}
