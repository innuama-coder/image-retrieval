//! Port definitions — external capability boundaries.
//!
//! Defines the external capability contracts required by the HLD:
//!
//! - [`BaseSearchProvider`] — canonical v1.1 search provider port.
//! - [`BaseProvider`] — legacy search provider port (deprecated).
//! - [`BaseRetrievalChannel`] — retrieval channel port.
//! - [`VlmEvaluationPort`] — VLM subjective evaluation port (v1.1).
//! - [`OpenClawEvaluationPort`] — legacy OpenClaw evaluation port (deprecated).
//!
//! References: PRD FR-004/FR-005, HLD §Core Interfaces,
//! `docs/design/v1.1-TASK-002-search-provider-candidate-design.md`,
//! `docs/design/v1.1-TASK-003-quality-vlm-design.md`

use crate::domain::candidate::{
    CandidateRecord, ProviderId, VlmCandidateEvaluationRequest, VlmEvaluationResponse,
};
use crate::domain::config::{RetrievalChannelConfig, SearchProviderConfig, VlmEvaluationConfig};
use crate::domain::image::{ImageAcceptanceDecision, ImageRecord, VlmImageEvaluationRequest};
use crate::domain::retrieval::{
    RetrievalBatch, RetrievalBatchResult, RetrievalChannelCapabilities, RetrievalChannelId,
    RetrievalChannelReadinessReport, RetrievalChannelTier,
};
use crate::domain::search::{
    ProviderConstraintSupport, ProviderReadinessReport, SearchError, SearchRequest, SearchResponse,
};
use crate::error::Result;

// ---------------------------------------------------------------------------
// BaseSearchProvider — canonical v1.1 search provider port
// ---------------------------------------------------------------------------

/// Canonical v1.1 search provider contract.
///
/// Every image search engine adapter must satisfy this trait.
/// Production adapters (e.g. `serpapi_google_images`) implement this directly.
///
/// # Security
///
/// - `readiness()` must NOT perform a full search. It checks config shape,
///   credential env var presence, endpoint parseability, and quota signals.
/// - `search()` receives a package-safe [`SearchRequest`]; no credential
///   values may appear in the request or response DTOs.
pub trait BaseSearchProvider: Send + Sync {
    /// Return a stable, unique identifier for this provider.
    fn provider_id(&self) -> ProviderId;

    /// Human-readable display name.
    fn display_name(&self) -> &str;

    /// The adapter family.
    fn provider_kind(&self) -> crate::domain::config::SearchProviderKind;

    /// Declared constraint support for scheduling decisions.
    fn supported_constraints(&self) -> ProviderConstraintSupport;

    /// Evaluate readiness against the given config.
    ///
    /// Returns a structured report. Must not perform a full search.
    fn readiness(&self, config: &SearchProviderConfig) -> ProviderReadinessReport;

    /// Execute a search and return normalized results.
    fn search(&self, request: &SearchRequest) -> std::result::Result<SearchResponse, SearchError>;
}

// ---------------------------------------------------------------------------
// BaseProvider — legacy search provider port (deprecated)
// ---------------------------------------------------------------------------

/// Legacy search provider contract.
///
/// **Deprecated for v1.1**: new providers should implement
/// [`BaseSearchProvider`]. This trait is retained for backward
/// compatibility with existing code and tests.
pub trait BaseProvider {
    /// Return a stable, unique identifier for this provider.
    fn provider_id(&self) -> ProviderId;

    /// Return the user-visible name (e.g. "Brave Image Search").
    fn display_name(&self) -> &str;

    /// Check whether the provider is ready to serve search requests.
    ///
    /// Returns `Ok(())` when ready, or an `Error` describing why it is not
    /// (missing credentials, network, configuration).
    fn readiness(&self) -> Result<()>;

    /// Return the configured scheduling weight for this provider.
    ///
    /// A weight of 0 means the provider is disabled for scheduling.
    /// Negative weights are invalid and must be diagnosed.
    fn weight(&self) -> i32;

    /// Execute a search and return normalised candidate records.
    ///
    /// The query is derived from the validated QueryPlan description.
    /// The implementation is responsible for calling the external search
    /// API and mapping results into the shared `CandidateRecord` format.
    fn search(&self, query: &str, max_results: u32) -> Result<Vec<CandidateRecord>>;
}

// ---------------------------------------------------------------------------
// BaseRetrievalChannel — retrieval channel port
// ---------------------------------------------------------------------------

/// Retrieval channel contract — v1.1 artifact-backed port.
///
/// Every retrieval channel (web fetch, self-hosted, paid) must satisfy
/// this trait. Channels expose their identity, capabilities, readiness,
/// and execute batch retrieval returning structured [`RetrievalBatchResult`].
///
/// # Security
///
/// - `readiness()` checks config shape, credential env var presence, and
///   dependency availability. It must NOT perform actual retrieval.
/// - `retrieve_batch()` returns artifact evidence. No credential values
///   may appear in the result DTOs.
/// - Fixture channels are test-only and must be marked `fixture_only = true`
///   in capabilities. Production runs must reject fixture evidence.
pub trait BaseRetrievalChannel {
    /// Stable unique identifier for this channel.
    fn channel_id(&self) -> RetrievalChannelId;

    /// Human-readable name for diagnostics and delivery manifests.
    fn display_name(&self) -> &str;

    /// The tier this channel operates at.
    fn tier(&self) -> RetrievalChannelTier;

    /// Declared capabilities of this channel.
    fn capabilities(&self) -> RetrievalChannelCapabilities;

    /// Evaluate readiness against the given config.
    ///
    /// Returns a structured report. Must not perform actual retrieval.
    /// For paid channels, readiness must return `PaidUnconfirmed` when
    /// the user has not explicitly confirmed the paid tier.
    fn readiness(&self, config: &RetrievalChannelConfig) -> RetrievalChannelReadinessReport;

    /// Attempt to retrieve a batch of candidate images.
    ///
    /// Returns a structured [`RetrievalBatchResult`] with per-job artifact
    /// evidence, attempt traces, fallback decisions, and diagnostics.
    /// Implementations must not silently skip access-control or
    /// authorization restrictions.
    fn retrieve_batch(
        &self,
        batch: &RetrievalBatch,
    ) -> std::result::Result<RetrievalBatchResult, RetrievalError>;
}

// ---------------------------------------------------------------------------
// Retrieval error
// ---------------------------------------------------------------------------

/// Errors that can occur during retrieval execution.
#[derive(Debug, Clone)]
pub enum RetrievalError {
    /// Channel is disabled by config.
    ChannelDisabled { channel_id: String },
    /// Required credential is missing.
    CredentialMissing { env_var: String },
    /// Channel dependency is missing.
    DependencyMissing { detail: String },
    /// Channel is misconfigured.
    Misconfigured { reason: String },
    /// Paid channel not confirmed.
    PaidUnconfirmed,
    /// Fixture channel used in production.
    FixtureNotProduction,
    /// Network / transport failure.
    Network { message: String },
    /// HTTP status prevented fetch.
    HttpStatus { code: u16, message: String },
    /// Access was restricted (401, 403, login, paywall).
    AccessRestricted { message: String },
    /// Source domain or URL is prohibited.
    ProhibitedSource { domain: String },
    /// Channel returned only metadata, no image artifact.
    MetadataOnly { message: String },
    /// Artifact write failed.
    ArtifactWriteFailed { path: String, reason: String },
    /// Retrieval timed out.
    Timeout { message: String },
    /// Unknown / internal error.
    Internal { message: String },
}

impl std::fmt::Display for RetrievalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ChannelDisabled { channel_id } => {
                write!(f, "retrieval channel '{}' is disabled", channel_id)
            }
            Self::CredentialMissing { env_var } => {
                write!(f, "retrieval credential env var '{}' is not set", env_var)
            }
            Self::DependencyMissing { detail } => {
                write!(f, "retrieval dependency missing: {}", detail)
            }
            Self::Misconfigured { reason } => {
                write!(f, "retrieval channel misconfigured: {}", reason)
            }
            Self::PaidUnconfirmed => {
                write!(f, "paid retrieval requires explicit user confirmation")
            }
            Self::FixtureNotProduction => {
                write!(f, "fixture channel cannot be used in production")
            }
            Self::Network { message } => write!(f, "retrieval network error: {}", message),
            Self::HttpStatus { code, message } => {
                write!(f, "retrieval HTTP {}: {}", code, message)
            }
            Self::AccessRestricted { message } => {
                write!(f, "retrieval access restricted: {}", message)
            }
            Self::ProhibitedSource { domain } => {
                write!(f, "retrieval prohibited source: {}", domain)
            }
            Self::MetadataOnly { message } => {
                write!(f, "retrieval metadata-only result: {}", message)
            }
            Self::ArtifactWriteFailed { path, reason } => {
                write!(f, "retrieval artifact write failed '{}': {}", path, reason)
            }
            Self::Timeout { message } => write!(f, "retrieval timeout: {}", message),
            Self::Internal { message } => write!(f, "retrieval internal error: {}", message),
        }
    }
}

impl std::error::Error for RetrievalError {}

impl RetrievalError {
    /// Convert this error to a failure code for diagnostics.
    pub fn to_failure_code(&self) -> crate::domain::retrieval::RetrievalFailureCode {
        use crate::domain::retrieval::RetrievalFailureCode;
        match self {
            Self::ChannelDisabled { .. } => RetrievalFailureCode::RetrievalChannelDisabled,
            Self::CredentialMissing { .. } => {
                RetrievalFailureCode::RetrievalChannelCredentialMissing
            }
            Self::DependencyMissing { .. } => {
                RetrievalFailureCode::RetrievalChannelDependencyMissing
            }
            Self::Misconfigured { .. } => RetrievalFailureCode::RetrievalChannelMisconfigured,
            Self::PaidUnconfirmed => RetrievalFailureCode::RetrievalPaidUnconfirmed,
            Self::FixtureNotProduction => RetrievalFailureCode::RetrievalFixtureNotProduction,
            Self::Network { .. } => RetrievalFailureCode::RetrievalDirectFetchNetwork,
            Self::HttpStatus { code, .. } if *code == 401 || *code == 403 => {
                RetrievalFailureCode::RetrievalAccessRestricted
            }
            Self::HttpStatus { .. } => RetrievalFailureCode::RetrievalHttpStatus,
            Self::AccessRestricted { .. } => RetrievalFailureCode::RetrievalAccessRestricted,
            Self::ProhibitedSource { .. } => RetrievalFailureCode::RetrievalProhibitedSource,
            Self::MetadataOnly { .. } => RetrievalFailureCode::RetrievalMetadataOnly,
            Self::ArtifactWriteFailed { .. } => RetrievalFailureCode::RetrievalArtifactWriteFailed,
            Self::Timeout { .. } => RetrievalFailureCode::RetrievalDirectFetchNetwork,
            Self::Internal { .. } => RetrievalFailureCode::RetrievalUnavailable,
        }
    }

    /// Whether this error allows fallback to a higher tier.
    pub fn allows_fallback(&self) -> bool {
        match self {
            Self::AccessRestricted { .. }
            | Self::ProhibitedSource { .. }
            | Self::PaidUnconfirmed
            | Self::FixtureNotProduction => false,
            Self::ChannelDisabled { .. }
            | Self::CredentialMissing { .. }
            | Self::DependencyMissing { .. }
            | Self::Misconfigured { .. }
            | Self::Network { .. }
            | Self::HttpStatus { .. }
            | Self::MetadataOnly { .. }
            | Self::ArtifactWriteFailed { .. }
            | Self::Timeout { .. }
            | Self::Internal { .. } => true,
        }
    }
}

// ---------------------------------------------------------------------------
// OpenClawEvaluationPort — legacy subjective evaluation port (deprecated)
// ---------------------------------------------------------------------------

/// OpenClaw subjective evaluation contract.
///
/// **Deprecated for v1.1**: prefer [`VlmEvaluationPort`]. This trait is
/// retained for backward compatibility with existing code and tests.
///
/// Covers two distinct evaluation boundaries per HLD/ADR-009:
/// 1. Structured candidate evaluation (before retrieval).
/// 2. Actual image evaluation (after retrieval).
#[deprecated(note = "use `VlmEvaluationPort` instead")]
pub trait OpenClawEvaluationPort {
    /// Check whether OpenClaw is available for production evaluation.
    ///
    /// Returns `Ok(())` when ready. When unavailable, production tasks
    /// must enter execution-blocked state — mock or fixture results
    /// cannot be used as production delivery evidence.
    fn readiness(&self) -> Result<()>;

    /// Evaluate a batch of structured candidate records and return an
    /// acceptance decision for each.
    ///
    /// This is the candidate-phase subjective evaluation (HLD §主观评价架构边界).
    fn evaluate_candidates(
        &self,
        candidates: &[CandidateRecord],
        description: &str,
    ) -> Result<Vec<crate::domain::candidate::CandidateDecision>>;

    /// Evaluate structured candidate requests that include mechanical evidence
    /// and redacted QueryPlan context.
    ///
    /// Default implementation preserves legacy implementors by projecting the
    /// request back to the old candidate-record interface. Production adapters
    /// should override this method so reference metrics reach the VLM prompt.
    fn evaluate_candidate_requests(
        &self,
        requests: &[crate::quality::candidate::CandidateEvaluationRequest],
    ) -> Result<Vec<crate::domain::candidate::CandidateDecision>> {
        let candidates: Vec<CandidateRecord> = requests
            .iter()
            .map(|request| request.candidate.clone())
            .collect();
        let description = requests
            .first()
            .map(|request| request.query_description.as_str())
            .unwrap_or("");
        self.evaluate_candidates(&candidates, description)
    }

    /// Evaluate a batch of actually-retrieved images and return an
    /// acceptance decision for each.
    ///
    /// This is the image-phase subjective evaluation.
    fn evaluate_images(
        &self,
        images: &[ImageRecord],
        description: &str,
    ) -> Result<Vec<ImageAcceptanceDecision>>;
}

// ---------------------------------------------------------------------------
// VLM evaluation readiness report
// ---------------------------------------------------------------------------

/// Status of a VLM evaluator's credential.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CredentialStatus {
    /// Credential env var is present and appears valid.
    Present,
    /// Credential env var is not set.
    Missing { env_var: String },
    /// Credential was not checked (e.g. fixture evaluator).
    NotRequired,
}

impl std::fmt::Display for CredentialStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Present => write!(f, "present"),
            Self::Missing { env_var } => write!(f, "missing ({})", env_var),
            Self::NotRequired => write!(f, "not_required"),
        }
    }
}

// ---------------------------------------------------------------------------
// VLM evaluation failure codes
// ---------------------------------------------------------------------------

/// Machine-readable failure codes for VLM evaluation readiness and execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VlmEvaluationFailureCode {
    /// VLM is disabled in config.
    #[serde(rename = "VLM_EVALUATION_DISABLED")]
    VlmEvaluationDisabled,
    /// Production endpoint or base URL is absent.
    #[serde(rename = "VLM_EVALUATION_ENDPOINT_MISSING")]
    VlmEvaluationEndpointMissing,
    /// Required credential env var is absent.
    #[serde(rename = "VLM_EVALUATION_CREDENTIAL_MISSING")]
    VlmEvaluationCredentialMissing,
    /// Candidate evaluation prompt/template is absent.
    #[serde(rename = "VLM_EVALUATION_CANDIDATE_PROMPT_MISSING")]
    VlmEvaluationCandidatePromptMissing,
    /// Image evaluation prompt/template is absent.
    #[serde(rename = "VLM_EVALUATION_IMAGE_PROMPT_MISSING")]
    VlmEvaluationImagePromptMissing,
    /// Health/readiness probe failed.
    #[serde(rename = "VLM_EVALUATION_HEALTH_FAILED")]
    VlmEvaluationHealthFailed,
    /// Evaluation request timed out.
    #[serde(rename = "VLM_EVALUATION_TIMEOUT")]
    VlmEvaluationTimeout,
    /// Response could not be validated.
    #[serde(rename = "VLM_EVALUATION_RESPONSE_INVALID")]
    VlmEvaluationResponseInvalid,
    /// Fixture evaluator attempted in production mode.
    #[serde(rename = "VLM_EVALUATION_FIXTURE_NOT_PRODUCTION")]
    VlmEvaluationFixtureNotProduction,
    /// General production unavailability.
    #[serde(rename = "VLM_EVALUATION_UNAVAILABLE")]
    VlmEvaluationUnavailable,
}

impl std::fmt::Display for VlmEvaluationFailureCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::VlmEvaluationDisabled => "VLM_EVALUATION_DISABLED",
            Self::VlmEvaluationEndpointMissing => "VLM_EVALUATION_ENDPOINT_MISSING",
            Self::VlmEvaluationCredentialMissing => "VLM_EVALUATION_CREDENTIAL_MISSING",
            Self::VlmEvaluationCandidatePromptMissing => "VLM_EVALUATION_CANDIDATE_PROMPT_MISSING",
            Self::VlmEvaluationImagePromptMissing => "VLM_EVALUATION_IMAGE_PROMPT_MISSING",
            Self::VlmEvaluationHealthFailed => "VLM_EVALUATION_HEALTH_FAILED",
            Self::VlmEvaluationTimeout => "VLM_EVALUATION_TIMEOUT",
            Self::VlmEvaluationResponseInvalid => "VLM_EVALUATION_RESPONSE_INVALID",
            Self::VlmEvaluationFixtureNotProduction => "VLM_EVALUATION_FIXTURE_NOT_PRODUCTION",
            Self::VlmEvaluationUnavailable => "VLM_EVALUATION_UNAVAILABLE",
        };
        write!(f, "{}", s)
    }
}

// ---------------------------------------------------------------------------
// VLM evaluation readiness report
// ---------------------------------------------------------------------------

/// Full readiness report for the VLM evaluation provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VlmEvaluationReadinessReport {
    /// Whether VLM evaluation is enabled.
    pub enabled: bool,
    /// Whether VLM is available for production evaluation.
    pub available: bool,
    /// Whether fixture mode is active.
    pub fixture_mode: bool,
    /// Whether the endpoint/base URL is configured.
    pub endpoint_configured: bool,
    /// Whether a candidate prompt template is configured.
    pub candidate_prompt_configured: bool,
    /// Whether an image prompt template is configured.
    pub image_prompt_configured: bool,
    /// Credential status.
    pub credential_status: CredentialStatus,
    /// Failure code if not available.
    pub failure_code: Option<VlmEvaluationFailureCode>,
    /// When readiness was checked (ISO 8601).
    pub checked_at: String,
    /// Supporting evidence.
    pub evidence: Vec<String>,
    /// Whether redaction was applied to any evidence.
    pub redaction_applied: bool,
}

impl VlmEvaluationReadinessReport {
    /// Build a readiness report for a disabled or unavailable VLM.
    pub fn not_available(
        failure_code: VlmEvaluationFailureCode,
        fixture_mode: bool,
        evidence: Vec<String>,
    ) -> Self {
        Self {
            enabled: false,
            available: false,
            fixture_mode,
            endpoint_configured: false,
            candidate_prompt_configured: false,
            image_prompt_configured: false,
            credential_status: CredentialStatus::Missing {
                env_var: "QWEN_API_KEY".into(),
            },
            failure_code: Some(failure_code),
            checked_at: String::new(),
            evidence,
            redaction_applied: false,
        }
    }

    /// Build a readiness report for an available VLM.
    pub fn available(fixture_mode: bool) -> Self {
        Self {
            enabled: true,
            available: true,
            fixture_mode,
            endpoint_configured: true,
            candidate_prompt_configured: true,
            image_prompt_configured: true,
            credential_status: CredentialStatus::Present,
            failure_code: None,
            checked_at: String::new(),
            evidence: Vec::new(),
            redaction_applied: false,
        }
    }

    /// Build a readiness report for a fixture evaluator that is blocked in production.
    pub fn fixture_blocked_in_production() -> Self {
        Self {
            enabled: true,
            available: false,
            fixture_mode: true,
            endpoint_configured: true,
            candidate_prompt_configured: false,
            image_prompt_configured: false,
            credential_status: CredentialStatus::NotRequired,
            failure_code: Some(VlmEvaluationFailureCode::VlmEvaluationFixtureNotProduction),
            checked_at: String::new(),
            evidence: vec!["Fixture evaluator cannot satisfy production delivery evidence.".into()],
            redaction_applied: false,
        }
    }
}

// ---------------------------------------------------------------------------
// VLM evaluation error
// ---------------------------------------------------------------------------

/// Errors that can occur during VLM evaluation.
#[derive(Debug, Clone)]
pub enum VlmEvaluationError {
    /// VLM is disabled in config.
    Disabled,
    /// Required credential is missing.
    CredentialMissing { env_var: String },
    /// Endpoint is not configured.
    EndpointMissing,
    /// Prompt template is missing.
    PromptTemplateMissing { phase: String },
    /// Health check failed.
    HealthFailed { reason: String },
    /// Request timed out.
    Timeout { message: String },
    /// Response was invalid (cardinality, schema, subject IDs).
    InvalidResponse { message: String },
    /// Fixture evaluator attempted in production.
    FixtureNotProduction,
    /// General unavailability.
    Unavailable { reason: String },
}

impl std::fmt::Display for VlmEvaluationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Disabled => write!(f, "VLM evaluation is disabled"),
            Self::CredentialMissing { env_var } => {
                write!(f, "VLM credential env var '{}' is not set", env_var)
            }
            Self::EndpointMissing => write!(f, "VLM endpoint is not configured"),
            Self::PromptTemplateMissing { phase } => {
                write!(f, "VLM {} prompt template is missing", phase)
            }
            Self::HealthFailed { reason } => write!(f, "VLM health check failed: {}", reason),
            Self::Timeout { message } => write!(f, "VLM request timed out: {}", message),
            Self::InvalidResponse { message } => {
                write!(f, "VLM response invalid: {}", message)
            }
            Self::FixtureNotProduction => {
                write!(f, "Fixture evaluator cannot be used in production")
            }
            Self::Unavailable { reason } => write!(f, "VLM unavailable: {}", reason),
        }
    }
}

impl std::error::Error for VlmEvaluationError {}

impl VlmEvaluationError {
    /// Convert to a failure code.
    pub fn to_failure_code(&self) -> VlmEvaluationFailureCode {
        match self {
            Self::Disabled => VlmEvaluationFailureCode::VlmEvaluationDisabled,
            Self::CredentialMissing { .. } => {
                VlmEvaluationFailureCode::VlmEvaluationCredentialMissing
            }
            Self::EndpointMissing => VlmEvaluationFailureCode::VlmEvaluationEndpointMissing,
            Self::PromptTemplateMissing { .. } => {
                VlmEvaluationFailureCode::VlmEvaluationCandidatePromptMissing
            }
            Self::HealthFailed { .. } => VlmEvaluationFailureCode::VlmEvaluationHealthFailed,
            Self::Timeout { .. } => VlmEvaluationFailureCode::VlmEvaluationTimeout,
            Self::InvalidResponse { .. } => VlmEvaluationFailureCode::VlmEvaluationResponseInvalid,
            Self::FixtureNotProduction => {
                VlmEvaluationFailureCode::VlmEvaluationFixtureNotProduction
            }
            Self::Unavailable { .. } => VlmEvaluationFailureCode::VlmEvaluationUnavailable,
        }
    }
}

// ---------------------------------------------------------------------------
// VlmEvaluationPort — v1.1 VLM subjective evaluation port
// ---------------------------------------------------------------------------

/// VLM subjective evaluation contract (v1.1).
///
/// Covers two distinct evaluation boundaries:
/// 1. Candidate evaluation (before retrieval) via [`evaluate_candidates`].
/// 2. Image evaluation (after retrieval) via [`evaluate_images`].
///
/// # Security
///
/// - Request DTOs must not contain provider credentials, cookies, authorization
///   headers, raw VLM credentials, or full authenticated URLs.
/// - URLs are allowed only after redaction strips secret query parameters.
/// - `fixture_mode = true` is allowed only under test or explicit fixture runs.
/// - The resolved API token must never appear in requests, responses, diagnostics,
///   logs, or reports.
///
/// # Production behavior
///
/// - If readiness is not available and any mechanically passed subject requires
///   subjective evaluation, the quality phase returns `ExecutionBlocked`.
/// - Fixture subjective evaluation is allowed only under test/fixture execution
///   and must be marked `fixture_mode = true` in diagnostics and audit events.
pub trait VlmEvaluationPort: Send + Sync {
    /// Evaluate readiness for the VLM evaluation provider.
    ///
    /// Checks config, credential presence, endpoint availability, and
    /// fixture-mode legality. Returns a structured report without performing
    /// actual evaluation.
    fn readiness(&self, config: &VlmEvaluationConfig) -> VlmEvaluationReadinessReport;

    /// Evaluate a batch of candidates and return structured per-subject decisions.
    ///
    /// Only mechanically-passed candidates are submitted. The response must
    /// contain exactly one decision per submitted subject.
    fn evaluate_candidates(
        &self,
        request: &VlmCandidateEvaluationRequest,
    ) -> std::result::Result<VlmEvaluationResponse, VlmEvaluationError>;

    /// Evaluate a batch of retrieved images and return structured per-subject
    /// decisions.
    ///
    /// Only mechanically-passed images are submitted. The response must
    /// contain exactly one decision per submitted subject.
    fn evaluate_images(
        &self,
        request: &VlmImageEvaluationRequest,
    ) -> std::result::Result<VlmEvaluationResponse, VlmEvaluationError>;
}

// ---------------------------------------------------------------------------
// Fixture VLM evaluator — test-only
// ---------------------------------------------------------------------------

/// A fixture VLM evaluator that returns pre-determined decisions.
///
/// This evaluator is **test-only** and must never satisfy production
/// delivery evidence. Production code must detect `fixture_mode = true`
/// and return `ExecutionBlocked`.
pub struct FixtureVlmEvaluator {
    /// Pre-determined candidate decisions to return.
    pub candidate_decisions: Vec<VlmSubjectDecision>,
    /// Pre-determined image decisions to return.
    pub image_decisions: Vec<VlmSubjectDecision>,
    /// Whether the fixture should simulate VLM unavailability.
    pub simulate_unavailable: bool,
}

use crate::domain::candidate::VlmSubjectDecision;
use serde::{Deserialize, Serialize};

impl FixtureVlmEvaluator {
    /// Create a fixture evaluator that always approves.
    pub fn always_approve() -> Self {
        Self {
            candidate_decisions: Vec::new(),
            image_decisions: Vec::new(),
            simulate_unavailable: false,
        }
    }

    /// Create a fixture evaluator with specific candidate decisions.
    pub fn with_candidate_decisions(decisions: Vec<VlmSubjectDecision>) -> Self {
        Self {
            candidate_decisions: decisions,
            image_decisions: Vec::new(),
            simulate_unavailable: false,
        }
    }

    /// Create a fixture evaluator that simulates unavailability.
    pub fn unavailable() -> Self {
        Self {
            candidate_decisions: Vec::new(),
            image_decisions: Vec::new(),
            simulate_unavailable: true,
        }
    }
}

impl VlmEvaluationPort for FixtureVlmEvaluator {
    fn readiness(&self, config: &VlmEvaluationConfig) -> VlmEvaluationReadinessReport {
        if config.fixture_mode {
            VlmEvaluationReadinessReport::available(true)
        } else {
            // Fixture evaluator in production context → blocked
            VlmEvaluationReadinessReport::fixture_blocked_in_production()
        }
    }

    fn evaluate_candidates(
        &self,
        request: &VlmCandidateEvaluationRequest,
    ) -> std::result::Result<VlmEvaluationResponse, VlmEvaluationError> {
        if !request.fixture_mode {
            return Err(VlmEvaluationError::FixtureNotProduction);
        }

        if self.simulate_unavailable {
            return Err(VlmEvaluationError::Unavailable {
                reason: "fixture simulated unavailability".into(),
            });
        }

        // Use pre-set decisions if available, otherwise auto-approve
        let decisions: Vec<VlmSubjectDecision> = if self.candidate_decisions.is_empty() {
            request
                .candidates
                .iter()
                .map(|s| VlmSubjectDecision {
                    subject_id: s.candidate.candidate_id.to_string(),
                    decision: crate::domain::candidate::VlmSubjectDecisionKind::Approve,
                    confidence: Some(0.95),
                    reason_codes: vec!["fixture_approve".into()],
                    rationale_summary: "Fixture auto-approve.".into(),
                    evidence_refs: vec![],
                })
                .collect()
        } else {
            self.candidate_decisions.clone()
        };

        Ok(VlmEvaluationResponse {
            request_id: request.request_id.clone(),
            evaluator_id: "fixture_vlm".into(),
            evaluator_kind: crate::domain::candidate::VlmEvaluatorKind::Fixture,
            status: crate::domain::candidate::VlmResponseStatus::Complete,
            decisions,
            diagnostics: Vec::new(),
            audit_ref: None,
            redaction_applied: false,
        })
    }

    fn evaluate_images(
        &self,
        request: &VlmImageEvaluationRequest,
    ) -> std::result::Result<VlmEvaluationResponse, VlmEvaluationError> {
        if !request.fixture_mode {
            return Err(VlmEvaluationError::FixtureNotProduction);
        }

        if self.simulate_unavailable {
            return Err(VlmEvaluationError::Unavailable {
                reason: "fixture simulated unavailability".into(),
            });
        }

        let decisions: Vec<VlmSubjectDecision> = if self.image_decisions.is_empty() {
            request
                .images
                .iter()
                .map(|s| VlmSubjectDecision {
                    subject_id: s.candidate_id.to_string(),
                    decision: crate::domain::candidate::VlmSubjectDecisionKind::Approve,
                    confidence: Some(0.95),
                    reason_codes: vec!["fixture_approve".into()],
                    rationale_summary: "Fixture auto-approve.".into(),
                    evidence_refs: vec![],
                })
                .collect()
        } else {
            self.image_decisions.clone()
        };

        Ok(VlmEvaluationResponse {
            request_id: request.request_id.clone(),
            evaluator_id: "fixture_vlm".into(),
            evaluator_kind: crate::domain::candidate::VlmEvaluatorKind::Fixture,
            status: crate::domain::candidate::VlmResponseStatus::Complete,
            decisions,
            diagnostics: Vec::new(),
            audit_ref: None,
            redaction_applied: false,
        })
    }
}

#[cfg(test)]
mod tests {
    //! Compile-time verification that the port traits are object-safe
    //! (can be used as `dyn Trait`).

    use super::*;

    #[test]
    fn base_search_provider_is_object_safe() {
        fn _assert(_p: &dyn BaseSearchProvider) {}
    }

    #[test]
    fn base_provider_is_object_safe() {
        fn _assert(_p: &dyn BaseProvider) {}
    }

    #[test]
    fn base_retrieval_channel_is_object_safe() {
        fn _assert(_c: &dyn BaseRetrievalChannel) {}
    }

    #[test]
    fn openclaw_evaluation_port_is_object_safe() {
        #[allow(deprecated)]
        fn _assert(_p: &dyn OpenClawEvaluationPort) {}
    }

    #[test]
    fn vlm_evaluation_port_is_object_safe() {
        fn _assert(_p: &dyn VlmEvaluationPort) {}
    }

    // -----------------------------------------------------------------------
    // VlmEvaluationReadinessReport tests
    // -----------------------------------------------------------------------

    #[test]
    fn readiness_report_available() {
        let report = VlmEvaluationReadinessReport::available(false);
        assert!(report.enabled);
        assert!(report.available);
        assert!(!report.fixture_mode);
        assert!(report.failure_code.is_none());
    }

    #[test]
    fn readiness_report_not_available() {
        let report = VlmEvaluationReadinessReport::not_available(
            VlmEvaluationFailureCode::VlmEvaluationCredentialMissing,
            false,
            vec!["QWEN_API_KEY not set".into()],
        );
        assert!(!report.enabled);
        assert!(!report.available);
        assert_eq!(
            report.failure_code,
            Some(VlmEvaluationFailureCode::VlmEvaluationCredentialMissing)
        );
    }

    #[test]
    fn readiness_report_fixture_blocked() {
        let report = VlmEvaluationReadinessReport::fixture_blocked_in_production();
        assert!(!report.available);
        assert_eq!(
            report.failure_code,
            Some(VlmEvaluationFailureCode::VlmEvaluationFixtureNotProduction)
        );
    }

    // -----------------------------------------------------------------------
    // VlmEvaluationError tests
    // -----------------------------------------------------------------------

    #[test]
    fn vlm_evaluation_error_to_failure_code() {
        assert_eq!(
            VlmEvaluationError::Disabled.to_failure_code(),
            VlmEvaluationFailureCode::VlmEvaluationDisabled
        );
        assert_eq!(
            VlmEvaluationError::CredentialMissing {
                env_var: "QWEN_API_KEY".into()
            }
            .to_failure_code(),
            VlmEvaluationFailureCode::VlmEvaluationCredentialMissing
        );
        assert_eq!(
            VlmEvaluationError::FixtureNotProduction.to_failure_code(),
            VlmEvaluationFailureCode::VlmEvaluationFixtureNotProduction
        );
        assert_eq!(
            VlmEvaluationError::Unavailable {
                reason: "down".into()
            }
            .to_failure_code(),
            VlmEvaluationFailureCode::VlmEvaluationUnavailable
        );
    }

    #[test]
    fn vlm_evaluation_error_display() {
        let err = VlmEvaluationError::CredentialMissing {
            env_var: "QWEN_API_KEY".into(),
        };
        assert!(err.to_string().contains("QWEN_API_KEY"));

        let err = VlmEvaluationError::FixtureNotProduction;
        assert!(err.to_string().contains("Fixture"));
    }

    // -----------------------------------------------------------------------
    // Failure code display
    // -----------------------------------------------------------------------

    #[test]
    fn failure_code_display() {
        assert_eq!(
            VlmEvaluationFailureCode::VlmEvaluationDisabled.to_string(),
            "VLM_EVALUATION_DISABLED"
        );
        assert_eq!(
            VlmEvaluationFailureCode::VlmEvaluationCredentialMissing.to_string(),
            "VLM_EVALUATION_CREDENTIAL_MISSING"
        );
        assert_eq!(
            VlmEvaluationFailureCode::VlmEvaluationFixtureNotProduction.to_string(),
            "VLM_EVALUATION_FIXTURE_NOT_PRODUCTION"
        );
    }

    // -----------------------------------------------------------------------
    // FixtureVlmEvaluator tests
    // -----------------------------------------------------------------------

    #[test]
    fn fixture_evaluator_readiness_in_fixture_mode() {
        let evaluator = FixtureVlmEvaluator::always_approve();
        let config = VlmEvaluationConfig {
            fixture_mode: true,
            ..Default::default()
        };
        let report = evaluator.readiness(&config);
        assert!(report.available);
        assert!(report.fixture_mode);
    }

    #[test]
    fn fixture_evaluator_readiness_in_production() {
        let evaluator = FixtureVlmEvaluator::always_approve();
        let config = VlmEvaluationConfig {
            fixture_mode: false,
            ..Default::default()
        };
        let report = evaluator.readiness(&config);
        assert!(!report.available);
        assert_eq!(
            report.failure_code,
            Some(VlmEvaluationFailureCode::VlmEvaluationFixtureNotProduction)
        );
    }

    #[test]
    fn fixture_evaluator_candidates_in_fixture_mode() {
        use crate::domain::candidate::{QualityPolicyContext, VlmCandidateEvaluationRequest};

        let evaluator = FixtureVlmEvaluator::always_approve();
        let request = VlmCandidateEvaluationRequest {
            request_id: "req-1".into(),
            query_plan_id: "qp-1".into(),
            full_attempt_count: 1,
            retry_count: 0,
            semantic_description: "test".into(),
            quality: crate::domain::query_plan::QualityTier::General,
            quality_requirements: Default::default(),
            visual_requirements: vec![],
            negative_scope: vec![],
            candidates: vec![],
            policy_context: QualityPolicyContext {
                fixture_mode: true,
                ..Default::default()
            },
            model: "qwen3-vl-plus".into(),
            evaluator_provider_id: "fixture_vlm".into(),
            fixture_mode: true,
        };

        let response = evaluator.evaluate_candidates(&request).unwrap();
        assert_eq!(
            response.evaluator_kind,
            crate::domain::candidate::VlmEvaluatorKind::Fixture
        );
        assert_eq!(
            response.status,
            crate::domain::candidate::VlmResponseStatus::Complete
        );
    }

    #[test]
    fn fixture_evaluator_candidates_in_production_blocked() {
        use crate::domain::candidate::{QualityPolicyContext, VlmCandidateEvaluationRequest};

        let evaluator = FixtureVlmEvaluator::always_approve();
        let request = VlmCandidateEvaluationRequest {
            request_id: "req-1".into(),
            query_plan_id: "qp-1".into(),
            full_attempt_count: 1,
            retry_count: 0,
            semantic_description: "test".into(),
            quality: crate::domain::query_plan::QualityTier::General,
            quality_requirements: Default::default(),
            visual_requirements: vec![],
            negative_scope: vec![],
            candidates: vec![],
            policy_context: QualityPolicyContext {
                fixture_mode: false,
                ..Default::default()
            },
            model: "qwen3-vl-plus".into(),
            evaluator_provider_id: "fixture_vlm".into(),
            fixture_mode: false,
        };

        let result = evaluator.evaluate_candidates(&request);
        assert!(result.is_err());
        match result {
            Err(e) => {
                assert!(matches!(e, VlmEvaluationError::FixtureNotProduction));
            }
            _ => panic!("expected FixtureNotProduction error"),
        }
    }

    #[test]
    fn fixture_evaluator_simulate_unavailable() {
        use crate::domain::candidate::{QualityPolicyContext, VlmCandidateEvaluationRequest};

        let evaluator = FixtureVlmEvaluator::unavailable();
        let request = VlmCandidateEvaluationRequest {
            request_id: "req-1".into(),
            query_plan_id: "qp-1".into(),
            full_attempt_count: 1,
            retry_count: 0,
            semantic_description: "test".into(),
            quality: crate::domain::query_plan::QualityTier::General,
            quality_requirements: Default::default(),
            visual_requirements: vec![],
            negative_scope: vec![],
            candidates: vec![],
            policy_context: QualityPolicyContext {
                fixture_mode: true,
                ..Default::default()
            },
            model: "qwen3-vl-plus".into(),
            evaluator_provider_id: "fixture_vlm".into(),
            fixture_mode: true,
        };

        let result = evaluator.evaluate_candidates(&request);
        assert!(result.is_err());
    }
}
