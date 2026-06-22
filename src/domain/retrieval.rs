//! Retrieval domain types — v1.1 artifact-backed model.
//!
//! Covers retrieval channel tiers, fallback execution, batch planning,
//! job-level artifact results, attempt traces, fallback decisions,
//! channel readiness, diagnostics, and evidence DTOs.
//!
//! References: PRD FR-007/FR-008/FR-009, HLD §Retrieval, LLD §Retrieval Contract,
//! `docs/design/v1.1-TASK-004-retrieval-artifact-channel-design.md`

#![allow(clippy::too_many_arguments)]

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Channel identity
// ---------------------------------------------------------------------------

/// Stable identifier for a retrieval channel.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RetrievalChannelId(pub String);

impl RetrievalChannelId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl std::fmt::Display for RetrievalChannelId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ---------------------------------------------------------------------------
// Retrieval job identity
// ---------------------------------------------------------------------------

/// Opaque identifier for a retrieval job.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RetrievalJobId(pub String);

impl RetrievalJobId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl std::fmt::Display for RetrievalJobId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ---------------------------------------------------------------------------
// Channel tiers — v1.1 canonical names
// ---------------------------------------------------------------------------

/// The three confirmed retrieval channel tiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RetrievalChannelTier {
    /// Normal web fetch — default, lowest-cost channel.
    #[serde(rename = "normal_web_fetch", alias = "web_fetch")]
    NormalWebFetch = 1,

    /// Self-hosted open-source service.
    #[serde(rename = "self_hosted_service", alias = "self_hosted")]
    SelfHostedService = 2,

    /// Paid online service. Must be explicitly enabled; never used silently.
    #[serde(rename = "paid_online_service", alias = "paid")]
    PaidOnlineService = 3,
}

impl RetrievalChannelTier {
    /// Returns the next tier to try during fallback, or `None`.
    pub fn next_fallback(&self) -> Option<Self> {
        match self {
            Self::NormalWebFetch => Some(Self::SelfHostedService),
            Self::SelfHostedService => Some(Self::PaidOnlineService),
            Self::PaidOnlineService => None,
        }
    }

    /// Whether this is the paid tier.
    pub fn is_paid(&self) -> bool {
        matches!(self, Self::PaidOnlineService)
    }

    /// Stable string for diagnostics and reports.
    pub fn as_stable_str(&self) -> &'static str {
        match self {
            Self::NormalWebFetch => "normal_web_fetch",
            Self::SelfHostedService => "self_hosted_service",
            Self::PaidOnlineService => "paid_online_service",
        }
    }
}

impl std::fmt::Display for RetrievalChannelTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_stable_str())
    }
}

// ---------------------------------------------------------------------------
// Attempt modes
// ---------------------------------------------------------------------------

/// The specific retrieval attempt mode within a channel tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RetrievalAttemptMode {
    /// Direct HTTP GET of the primary image URL.
    #[serde(rename = "direct_image_fetch")]
    DirectImageFetch,

    /// Source-page resolution: fetch the page, extract image URLs, fetch one.
    #[serde(rename = "source_page_resolve")]
    SourcePageResolve,

    /// Self-hosted retrieval service attempt.
    #[serde(rename = "self_hosted_service")]
    SelfHostedService,

    /// Paid online retrieval service attempt.
    #[serde(rename = "paid_online_service")]
    PaidOnlineService,
}

impl RetrievalAttemptMode {
    pub fn is_normal_web(&self) -> bool {
        matches!(self, Self::DirectImageFetch | Self::SourcePageResolve)
    }
}

impl std::fmt::Display for RetrievalAttemptMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::DirectImageFetch => "direct_image_fetch",
            Self::SourcePageResolve => "source_page_resolve",
            Self::SelfHostedService => "self_hosted_service",
            Self::PaidOnlineService => "paid_online_service",
        };
        write!(f, "{}", s)
    }
}

// ---------------------------------------------------------------------------
// Retrieval status
// ---------------------------------------------------------------------------

/// Terminal status of a retrieval job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RetrievalStatus {
    /// Retrieval completed with full artifact evidence.
    #[serde(rename = "complete")]
    Complete,

    /// At least one artifact exists but required evidence is incomplete.
    #[serde(rename = "partial")]
    Partial,

    /// No usable artifact was produced.
    #[serde(rename = "failed")]
    Failed,

    /// Retrieval was blocked by policy.
    #[serde(rename = "policy_blocked")]
    PolicyBlocked,

    /// Access restriction (401, 403, login, paywall) blocked retrieval.
    #[serde(rename = "access_restricted")]
    AccessRestricted,

    /// Paid channel required but not confirmed.
    #[serde(rename = "paid_unconfirmed")]
    PaidUnconfirmed,

    /// Channel returned only metadata/URL/summary, no image artifact.
    #[serde(rename = "metadata_only_rejected")]
    MetadataOnlyRejected,

    /// Fixture evidence attempted in production mode.
    #[serde(rename = "fixture_rejected_for_production")]
    FixtureRejectedForProduction,
}

impl RetrievalStatus {
    pub fn is_complete(&self) -> bool {
        matches!(self, Self::Complete)
    }

    pub fn is_terminal_failure(&self) -> bool {
        matches!(
            self,
            Self::Failed
                | Self::PolicyBlocked
                | Self::AccessRestricted
                | Self::PaidUnconfirmed
                | Self::MetadataOnlyRejected
                | Self::FixtureRejectedForProduction
        )
    }
}

// ---------------------------------------------------------------------------
// Retrieval target
// ---------------------------------------------------------------------------

/// What kind of retrieval target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RetrievalTargetType {
    #[serde(rename = "image")]
    Image,
}

/// The retrieval target for a single job — derived from a retrievable candidate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalTarget {
    /// What kind of target.
    pub target_type: RetrievalTargetType,

    /// Primary image URL from the candidate.
    pub primary_image_url: String,

    /// Source page URL for source-page-resolve fallback.
    pub alternate_source_page_url: Option<String>,

    /// Thumbnail URL for reference only.
    pub thumbnail_url: Option<String>,

    /// Expected MIME type, if known from the candidate.
    pub expected_mime_type: Option<String>,

    /// License or rights hint from the candidate.
    pub license_hint: Option<String>,

    /// Provider that discovered this candidate.
    pub provider_id: String,

    /// Provenance reference paths/links.
    pub candidate_provenance_refs: Vec<String>,
}

// ---------------------------------------------------------------------------
// Requested outputs
// ---------------------------------------------------------------------------

/// Outputs requested from a retrieval job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RequestedRetrievalOutput {
    #[serde(rename = "local_artifact")]
    LocalArtifact,
    #[serde(rename = "source_artifact")]
    SourceArtifact,
    #[serde(rename = "source_sidecar")]
    SourceSidecar,
    #[serde(rename = "content_summary")]
    ContentSummary,
    #[serde(rename = "task_report")]
    TaskReport,
    #[serde(rename = "visual_description")]
    VisualDescription,
    #[serde(rename = "checksum")]
    Checksum,
    #[serde(rename = "fetch_trace")]
    FetchTrace,
}

// ---------------------------------------------------------------------------
// Retrieval policy context (per-job)
// ---------------------------------------------------------------------------

/// Policy context carried per retrieval job.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RetrievalPolicyContext {
    /// Whether paid retrieval is allowed.
    pub allow_paid: bool,
    /// Whether to respect robots.txt / site rules.
    pub respect_robots: bool,
    /// Whether login-required sources are allowed.
    pub allow_login: bool,
    /// Whether paywalled sources are allowed.
    pub allow_paywalled: bool,
    /// Prohibited source domains.
    pub prohibited_domains: Vec<String>,
    /// Robots unknown behaviour: warn or block.
    pub robots_unknown_behavior: String, // "warn" or "block"
    /// Whether fixture mode is active.
    pub fixture_mode: bool,
}

// ---------------------------------------------------------------------------
// Retrieval job
// ---------------------------------------------------------------------------

/// A single retrieval job for one candidate in a full attempt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalJob {
    /// Unique job id within the run.
    pub retrieval_job_id: RetrievalJobId,

    /// Owning query plan.
    pub query_plan_id: String,

    /// Owning candidate.
    pub candidate_id: String,

    /// Full attempt count (1-based).
    pub full_attempt_count: u8,

    /// Retry count (= full_attempt_count - 1).
    pub retry_count: u8,

    /// Retrieval priority (higher = sooner).
    pub retrieval_priority: u32,

    /// What to retrieve.
    pub target: RetrievalTarget,

    /// Reference to the candidate quality decision.
    pub candidate_quality_decision_ref: String,

    /// Required outputs for a complete result.
    pub requested_outputs: Vec<RequestedRetrievalOutput>,

    /// Policy context for this job.
    pub policy_context: RetrievalPolicyContext,
}

impl RetrievalJob {
    /// Create a retrieval job from a retrievable candidate.
    pub fn from_retrievable(
        candidate: &crate::domain::candidate::RetrievableCandidate,
        _batch_id: &str,
        query_plan_id: &str,
        full_attempt_count: u8,
        retry_count: u8,
        candidate_quality_decision_ref: impl Into<String>,
        policy_context: RetrievalPolicyContext,
    ) -> Self {
        let retrieval_job_id = RetrievalJobId::new(format!(
            "ret-{}-{}-{}",
            query_plan_id, full_attempt_count, candidate.candidate.candidate_id
        ));

        let target = RetrievalTarget {
            target_type: RetrievalTargetType::Image,
            primary_image_url: candidate.primary_image_url.clone(),
            alternate_source_page_url: candidate.source_page_url.clone(),
            thumbnail_url: candidate.thumbnail_url.clone(),
            expected_mime_type: candidate.expected_mime_type.clone(),
            license_hint: candidate.license_hint.clone(),
            provider_id: candidate.candidate.provider_id.to_string(),
            candidate_provenance_refs: candidate.provenance_refs.clone(),
        };

        let requested_outputs = vec![
            RequestedRetrievalOutput::LocalArtifact,
            RequestedRetrievalOutput::SourceArtifact,
            RequestedRetrievalOutput::SourceSidecar,
            RequestedRetrievalOutput::ContentSummary,
            RequestedRetrievalOutput::TaskReport,
            RequestedRetrievalOutput::VisualDescription,
            RequestedRetrievalOutput::Checksum,
            RequestedRetrievalOutput::FetchTrace,
        ];

        Self {
            retrieval_job_id,
            query_plan_id: query_plan_id.to_string(),
            candidate_id: candidate.candidate.candidate_id.to_string(),
            full_attempt_count,
            retry_count,
            retrieval_priority: candidate.retrieval_priority,
            target,
            candidate_quality_decision_ref: candidate_quality_decision_ref.into(),
            requested_outputs,
            policy_context,
        }
    }
}

// ---------------------------------------------------------------------------
// Retrieval batch
// ---------------------------------------------------------------------------

/// A planned batch of retrieval jobs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalBatch {
    /// Unique batch id for this full attempt.
    pub retrieval_batch_id: String,

    /// Owning query plan.
    pub query_plan_id: String,

    /// Full attempt count.
    pub full_attempt_count: u8,

    /// Retry count.
    pub retry_count: u8,

    /// Target batch size (required_image_count * 2).
    pub target_size: u32,

    /// Actual number of jobs.
    pub actual_size: u32,

    /// Whether this is a short batch.
    pub is_short_batch: bool,

    /// Jobs in priority order.
    pub jobs: Vec<RetrievalJob>,

    /// Shortage evidence if actual < target.
    pub shortage: Option<RetrievalBatchShortage>,
}

impl RetrievalBatch {
    /// Create a new batch from a list of jobs.
    pub fn new(
        retrieval_batch_id: impl Into<String>,
        query_plan_id: impl Into<String>,
        full_attempt_count: u8,
        retry_count: u8,
        target_size: u32,
        jobs: Vec<RetrievalJob>,
        shortage: Option<RetrievalBatchShortage>,
    ) -> Self {
        let actual_size = jobs.len() as u32;
        let is_short_batch = actual_size < target_size;
        Self {
            retrieval_batch_id: retrieval_batch_id.into(),
            query_plan_id: query_plan_id.into(),
            full_attempt_count,
            retry_count,
            target_size,
            actual_size,
            is_short_batch,
            jobs,
            shortage,
        }
    }

    /// Iterator over candidate ids (for backward compat).
    pub fn candidate_ids(&self) -> Vec<String> {
        self.jobs.iter().map(|j| j.candidate_id.clone()).collect()
    }

    /// Lookup a job by candidate id.
    pub fn job_for(&self, candidate_id: &str) -> Option<&RetrievalJob> {
        self.jobs.iter().find(|j| j.candidate_id == candidate_id)
    }
}

// ---------------------------------------------------------------------------
// Batch shortage
// ---------------------------------------------------------------------------

/// Shortage codes for batch planning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RetrievalShortageCode {
    #[serde(rename = "NO_RETRIEVABLE_CANDIDATES")]
    NoRetrievableCandidates,
    #[serde(rename = "INSUFFICIENT_RETRIEVABLE_CANDIDATES")]
    InsufficientRetrievableCandidates,
    #[serde(rename = "CANDIDATE_QUALITY_EXECUTION_BLOCKED")]
    CandidateQualityExecutionBlocked,
    #[serde(rename = "SEARCH_RECALL_SHORTAGE_PROPAGATED")]
    SearchRecallShortagePropagated,
}

/// Evidence produced when a retrieval batch is shorter than the target.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalBatchShortage {
    /// Owning query plan.
    pub query_plan_id: String,

    /// Target batch size.
    pub target_size: u32,

    /// Actual number of jobs created.
    pub actual_size: u32,

    /// Shortage category.
    pub shortage_code: RetrievalShortageCode,

    /// Human-readable reason.
    pub reason: String,

    /// Blockers from candidate quality.
    pub candidate_quality_blockers: Vec<String>,

    /// Reference to search shortage, if applicable.
    pub search_shortage_ref: Option<String>,
}

impl RetrievalBatchShortage {
    pub fn new(
        query_plan_id: impl Into<String>,
        target_size: u32,
        actual_size: u32,
        shortage_code: RetrievalShortageCode,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            query_plan_id: query_plan_id.into(),
            target_size,
            actual_size,
            shortage_code,
            reason: reason.into(),
            candidate_quality_blockers: Vec::new(),
            search_shortage_ref: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Failure codes
// ---------------------------------------------------------------------------

/// Machine-readable retrieval failure codes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RetrievalFailureCode {
    #[serde(rename = "RETRIEVAL_NO_RETRIEVABLE_CANDIDATES")]
    RetrievalNoRetrievableCandidates,
    #[serde(rename = "RETRIEVAL_BATCH_SHORTAGE")]
    RetrievalBatchShortage,
    #[serde(rename = "RETRIEVAL_CHANNEL_DISABLED")]
    RetrievalChannelDisabled,
    #[serde(rename = "RETRIEVAL_CHANNEL_ADAPTER_MISSING")]
    RetrievalChannelAdapterMissing,
    #[serde(rename = "RETRIEVAL_CHANNEL_DEPENDENCY_MISSING")]
    RetrievalChannelDependencyMissing,
    #[serde(rename = "RETRIEVAL_CHANNEL_MISCONFIGURED")]
    RetrievalChannelMisconfigured,
    #[serde(rename = "RETRIEVAL_CHANNEL_CREDENTIAL_MISSING")]
    RetrievalChannelCredentialMissing,
    #[serde(rename = "RETRIEVAL_PAID_UNCONFIRMED")]
    RetrievalPaidUnconfirmed,
    #[serde(rename = "RETRIEVAL_ROBOTS_POLICY_UNDECIDED")]
    RetrievalRobotsPolicyUndecided,
    #[serde(rename = "RETRIEVAL_FIXTURE_NOT_PRODUCTION")]
    RetrievalFixtureNotProduction,
    #[serde(rename = "RETRIEVAL_UNAVAILABLE")]
    RetrievalUnavailable,
    #[serde(rename = "RETRIEVAL_DIRECT_FETCH_NETWORK")]
    RetrievalDirectFetchNetwork,
    #[serde(rename = "RETRIEVAL_HTTP_STATUS")]
    RetrievalHttpStatus,
    #[serde(rename = "RETRIEVAL_ACCESS_RESTRICTED")]
    RetrievalAccessRestricted,
    #[serde(rename = "RETRIEVAL_ROBOTS_BLOCKED")]
    RetrievalRobotsBlocked,
    #[serde(rename = "RETRIEVAL_ROBOTS_UNDECIDED")]
    RetrievalRobotsUndecided,
    #[serde(rename = "RETRIEVAL_PROHIBITED_SOURCE")]
    RetrievalProhibitedSource,
    #[serde(rename = "RETRIEVAL_METADATA_ONLY")]
    RetrievalMetadataOnly,
    #[serde(rename = "RETRIEVAL_ARTIFACT_WRITE_FAILED")]
    RetrievalArtifactWriteFailed,
    #[serde(rename = "RETRIEVAL_ARTIFACT_MISSING")]
    RetrievalArtifactMissing,
    #[serde(rename = "RETRIEVAL_SIDECAR_MISSING")]
    RetrievalSidecarMissing,
    #[serde(rename = "RETRIEVAL_SUMMARY_MISSING")]
    RetrievalSummaryMissing,
    #[serde(rename = "RETRIEVAL_SUMMARY_QUALITY_FAILED")]
    RetrievalSummaryQualityFailed,
    #[serde(rename = "RETRIEVAL_TASK_REPORT_MISSING")]
    RetrievalTaskReportMissing,
    #[serde(rename = "RETRIEVAL_VISUAL_DESCRIPTION_MISSING")]
    RetrievalVisualDescriptionMissing,
    #[serde(rename = "RETRIEVAL_CHECKSUM_MISSING")]
    RetrievalChecksumMissing,
    #[serde(rename = "RETRIEVAL_CONTENT_TYPE_MISMATCH")]
    RetrievalContentTypeMismatch,
    #[serde(rename = "RETRIEVAL_IMAGE_DIMENSION_PROBE_FAILED")]
    RetrievalImageDimensionProbeFailed,
    #[serde(rename = "RETRIEVAL_JOB_OWNERSHIP_MISMATCH")]
    RetrievalJobOwnershipMismatch,
    #[serde(rename = "RETRIEVAL_SENSITIVE_DATA_REDACTED")]
    RetrievalSensitiveDataRedacted,
}

impl std::fmt::Display for RetrievalFailureCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::RetrievalNoRetrievableCandidates => "RETRIEVAL_NO_RETRIEVABLE_CANDIDATES",
            Self::RetrievalBatchShortage => "RETRIEVAL_BATCH_SHORTAGE",
            Self::RetrievalChannelDisabled => "RETRIEVAL_CHANNEL_DISABLED",
            Self::RetrievalChannelAdapterMissing => "RETRIEVAL_CHANNEL_ADAPTER_MISSING",
            Self::RetrievalChannelDependencyMissing => "RETRIEVAL_CHANNEL_DEPENDENCY_MISSING",
            Self::RetrievalChannelMisconfigured => "RETRIEVAL_CHANNEL_MISCONFIGURED",
            Self::RetrievalChannelCredentialMissing => "RETRIEVAL_CHANNEL_CREDENTIAL_MISSING",
            Self::RetrievalPaidUnconfirmed => "RETRIEVAL_PAID_UNCONFIRMED",
            Self::RetrievalRobotsPolicyUndecided => "RETRIEVAL_ROBOTS_POLICY_UNDECIDED",
            Self::RetrievalFixtureNotProduction => "RETRIEVAL_FIXTURE_NOT_PRODUCTION",
            Self::RetrievalUnavailable => "RETRIEVAL_UNAVAILABLE",
            Self::RetrievalDirectFetchNetwork => "RETRIEVAL_DIRECT_FETCH_NETWORK",
            Self::RetrievalHttpStatus => "RETRIEVAL_HTTP_STATUS",
            Self::RetrievalAccessRestricted => "RETRIEVAL_ACCESS_RESTRICTED",
            Self::RetrievalRobotsBlocked => "RETRIEVAL_ROBOTS_BLOCKED",
            Self::RetrievalRobotsUndecided => "RETRIEVAL_ROBOTS_UNDECIDED",
            Self::RetrievalProhibitedSource => "RETRIEVAL_PROHIBITED_SOURCE",
            Self::RetrievalMetadataOnly => "RETRIEVAL_METADATA_ONLY",
            Self::RetrievalArtifactWriteFailed => "RETRIEVAL_ARTIFACT_WRITE_FAILED",
            Self::RetrievalArtifactMissing => "RETRIEVAL_ARTIFACT_MISSING",
            Self::RetrievalSidecarMissing => "RETRIEVAL_SIDECAR_MISSING",
            Self::RetrievalSummaryMissing => "RETRIEVAL_SUMMARY_MISSING",
            Self::RetrievalSummaryQualityFailed => "RETRIEVAL_SUMMARY_QUALITY_FAILED",
            Self::RetrievalTaskReportMissing => "RETRIEVAL_TASK_REPORT_MISSING",
            Self::RetrievalVisualDescriptionMissing => "RETRIEVAL_VISUAL_DESCRIPTION_MISSING",
            Self::RetrievalChecksumMissing => "RETRIEVAL_CHECKSUM_MISSING",
            Self::RetrievalContentTypeMismatch => "RETRIEVAL_CONTENT_TYPE_MISMATCH",
            Self::RetrievalImageDimensionProbeFailed => "RETRIEVAL_IMAGE_DIMENSION_PROBE_FAILED",
            Self::RetrievalJobOwnershipMismatch => "RETRIEVAL_JOB_OWNERSHIP_MISMATCH",
            Self::RetrievalSensitiveDataRedacted => "RETRIEVAL_SENSITIVE_DATA_REDACTED",
        };
        write!(f, "{}", s)
    }
}

// ---------------------------------------------------------------------------
// Retrieval severity
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RetrievalSeverity {
    #[serde(rename = "info")]
    Info,
    #[serde(rename = "warning")]
    Warning,
    #[serde(rename = "error")]
    Error,
    #[serde(rename = "blocker")]
    Blocker,
}

// ---------------------------------------------------------------------------
// Retrieval diagnostic
// ---------------------------------------------------------------------------

/// A diagnostic produced during retrieval execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalDiagnostic {
    /// Machine-readable failure code.
    pub code: RetrievalFailureCode,
    /// Severity of this diagnostic.
    pub severity: RetrievalSeverity,
    /// Owning query plan.
    pub query_plan_id: String,
    /// Batch id, if applicable.
    pub retrieval_batch_id: Option<String>,
    /// Job id, if applicable.
    pub retrieval_job_id: Option<RetrievalJobId>,
    /// Candidate id, if applicable.
    pub candidate_id: Option<String>,
    /// Channel id, if applicable.
    pub channel_id: Option<RetrievalChannelId>,
    /// Channel tier, if applicable.
    pub channel_tier: Option<RetrievalChannelTier>,
    /// Attempt mode, if applicable.
    pub attempt_mode: Option<RetrievalAttemptMode>,
    /// Human-readable message (credential-safe).
    pub message: String,
    /// Suggested remediation, if any.
    pub remediation: Option<String>,
    /// References to evidence.
    pub evidence_refs: Vec<String>,
    /// Whether this failure is retryable.
    pub retryable: bool,
    /// Whether redaction was applied.
    pub redacted: bool,
}

impl RetrievalDiagnostic {
    pub fn new(
        code: RetrievalFailureCode,
        severity: RetrievalSeverity,
        query_plan_id: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code,
            severity,
            query_plan_id: query_plan_id.into(),
            retrieval_batch_id: None,
            retrieval_job_id: None,
            candidate_id: None,
            channel_id: None,
            channel_tier: None,
            attempt_mode: None,
            message: message.into(),
            remediation: None,
            evidence_refs: Vec::new(),
            retryable: false,
            redacted: false,
        }
    }

    pub fn with_job(mut self, job_id: impl Into<String>) -> Self {
        self.retrieval_job_id = Some(RetrievalJobId::new(job_id));
        self
    }

    pub fn with_candidate(mut self, candidate_id: impl Into<String>) -> Self {
        self.candidate_id = Some(candidate_id.into());
        self
    }

    pub fn with_channel(
        mut self,
        channel_id: impl Into<String>,
        tier: RetrievalChannelTier,
        mode: RetrievalAttemptMode,
    ) -> Self {
        self.channel_id = Some(RetrievalChannelId::new(channel_id));
        self.channel_tier = Some(tier);
        self.attempt_mode = Some(mode);
        self
    }
}

// ---------------------------------------------------------------------------
// Credential / dependency status enums
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CredentialStatus {
    #[serde(rename = "present")]
    Present,
    #[serde(rename = "missing")]
    Missing { env_var: String },
    #[serde(rename = "not_required")]
    NotRequired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DependencyStatus {
    #[serde(rename = "available")]
    Available,
    #[serde(rename = "missing")]
    Missing { detail: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RetrievalPolicyStatus {
    #[serde(rename = "allowed")]
    Allowed,
    #[serde(rename = "blocked")]
    Blocked { reason: String },
}

// ---------------------------------------------------------------------------
// Channel capabilities
// ---------------------------------------------------------------------------

/// What a retrieval channel can produce.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalChannelCapabilities {
    pub supports_direct_image_fetch: bool,
    pub supports_source_page_resolve: bool,
    pub supports_sidecar: bool,
    pub supports_content_summary: bool,
    pub supports_task_report: bool,
    pub supports_visual_description: bool,
    pub supports_checksum: bool,
    pub supports_content_type_sniffing: bool,
    pub supports_dimension_probe: bool,
    pub supports_batching: bool,
    pub max_batch_size: Option<u32>,
    pub fixture_only: bool,
}

impl Default for RetrievalChannelCapabilities {
    fn default() -> Self {
        Self {
            supports_direct_image_fetch: true,
            supports_source_page_resolve: true,
            supports_sidecar: true,
            supports_content_summary: true,
            supports_task_report: true,
            supports_visual_description: true,
            supports_checksum: true,
            supports_content_type_sniffing: true,
            supports_dimension_probe: true,
            supports_batching: true,
            max_batch_size: None,
            fixture_only: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Channel readiness report
// ---------------------------------------------------------------------------

/// Full readiness report for a retrieval channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalChannelReadinessReport {
    pub channel_id: RetrievalChannelId,
    pub display_name: String,
    pub tier: RetrievalChannelTier,
    pub enabled: bool,
    pub available: bool,
    pub included_in_fallback_order: bool,
    pub credential_status: CredentialStatus,
    pub dependency_status: DependencyStatus,
    pub policy_status: RetrievalPolicyStatus,
    pub failure_code: Option<RetrievalFailureCode>,
    pub checked_at: String,
    pub evidence: Vec<RetrievalEvidenceFact>,
    pub redaction_applied: bool,
}

impl RetrievalChannelReadinessReport {
    pub fn ready(
        channel_id: RetrievalChannelId,
        display_name: impl Into<String>,
        tier: RetrievalChannelTier,
    ) -> Self {
        Self {
            channel_id,
            display_name: display_name.into(),
            tier,
            enabled: true,
            available: true,
            included_in_fallback_order: true,
            credential_status: CredentialStatus::NotRequired,
            dependency_status: DependencyStatus::Available,
            policy_status: RetrievalPolicyStatus::Allowed,
            failure_code: None,
            checked_at: String::new(),
            evidence: Vec::new(),
            redaction_applied: false,
        }
    }

    pub fn disabled(
        channel_id: RetrievalChannelId,
        display_name: impl Into<String>,
        tier: RetrievalChannelTier,
        failure_code: RetrievalFailureCode,
    ) -> Self {
        Self {
            channel_id,
            display_name: display_name.into(),
            tier,
            enabled: false,
            available: false,
            included_in_fallback_order: false,
            credential_status: CredentialStatus::NotRequired,
            dependency_status: DependencyStatus::Available,
            policy_status: RetrievalPolicyStatus::Allowed,
            failure_code: Some(failure_code),
            checked_at: String::new(),
            evidence: Vec::new(),
            redaction_applied: false,
        }
    }

    pub fn paid_unconfirmed(
        channel_id: RetrievalChannelId,
        display_name: impl Into<String>,
    ) -> Self {
        Self {
            channel_id,
            display_name: display_name.into(),
            tier: RetrievalChannelTier::PaidOnlineService,
            enabled: false,
            available: false,
            included_in_fallback_order: false,
            credential_status: CredentialStatus::NotRequired,
            dependency_status: DependencyStatus::Available,
            policy_status: RetrievalPolicyStatus::Blocked {
                reason: "Paid channels require explicit runtime config and QueryPlan allowance."
                    .into(),
            },
            failure_code: Some(RetrievalFailureCode::RetrievalPaidUnconfirmed),
            checked_at: String::new(),
            evidence: Vec::new(),
            redaction_applied: false,
        }
    }

    pub fn fixture_blocked(
        channel_id: RetrievalChannelId,
        display_name: impl Into<String>,
        tier: RetrievalChannelTier,
    ) -> Self {
        Self {
            channel_id,
            display_name: display_name.into(),
            tier,
            enabled: true,
            available: false,
            included_in_fallback_order: false,
            credential_status: CredentialStatus::NotRequired,
            dependency_status: DependencyStatus::Available,
            policy_status: RetrievalPolicyStatus::Blocked {
                reason: "Fixture channel cannot satisfy production retrieval evidence.".into(),
            },
            failure_code: Some(RetrievalFailureCode::RetrievalFixtureNotProduction),
            checked_at: String::new(),
            evidence: Vec::new(),
            redaction_applied: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Evidence fact
// ---------------------------------------------------------------------------

/// A single evidence fact in a readiness report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalEvidenceFact {
    pub key: String,
    pub value: String,
}

// ---------------------------------------------------------------------------
// Attempt trace
// ---------------------------------------------------------------------------

/// Status of a single attempt step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RetrievalAttemptStatus {
    #[serde(rename = "started")]
    Started,
    #[serde(rename = "succeeded")]
    Succeeded,
    #[serde(rename = "failed")]
    Failed,
    #[serde(rename = "abandoned")]
    Abandoned,
    #[serde(rename = "policy_blocked")]
    PolicyBlocked,
    #[serde(rename = "access_restricted")]
    AccessRestricted,
    #[serde(rename = "paid_unconfirmed")]
    PaidUnconfirmed,
    #[serde(rename = "metadata_only_rejected")]
    MetadataOnlyRejected,
}

/// Trace of a single retrieval attempt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalAttemptTrace {
    /// Unique attempt id.
    pub attempt_id: String,
    /// Owning job.
    pub retrieval_job_id: RetrievalJobId,
    /// Owning query plan.
    pub query_plan_id: String,
    /// Owning candidate.
    pub candidate_id: String,
    /// Channel used.
    pub channel_id: RetrievalChannelId,
    /// Channel tier.
    pub channel_tier: RetrievalChannelTier,
    /// Attempt mode.
    pub attempt_mode: RetrievalAttemptMode,
    /// When started (ISO 8601).
    pub started_at: String,
    /// When completed (ISO 8601).
    pub completed_at: Option<String>,
    /// Target URL (redacted).
    pub target_url_redacted: Option<String>,
    /// Source page URL (redacted).
    pub source_page_url_redacted: Option<String>,
    /// Final URL after redirects (redacted).
    pub final_url_redacted: Option<String>,
    /// HTTP status code if applicable.
    pub http_status: Option<u16>,
    /// Bytes received.
    pub bytes_received: Option<u64>,
    /// Terminal status of this attempt.
    pub status: RetrievalAttemptStatus,
    /// Failure code if failed.
    pub failure_code: Option<RetrievalFailureCode>,
    /// Whether this failure is retryable.
    pub retryable: bool,
    /// Whether fallback is allowed after this attempt.
    pub fallback_allowed: bool,
    /// Policy explanation if blocked.
    pub policy_reason: Option<String>,
    /// References to written artifact files.
    pub artifact_refs: Vec<String>,
    /// Whether redaction was applied.
    pub redaction_applied: bool,
}

// ---------------------------------------------------------------------------
// Fallback decision
// ---------------------------------------------------------------------------

/// Kind of fallback decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FallbackDecisionKind {
    #[serde(rename = "proceed")]
    Proceed,
    #[serde(rename = "stop_terminal_success")]
    StopTerminalSuccess,
    #[serde(rename = "stop_non_fallbackable_failure")]
    StopNonFallbackableFailure,
    #[serde(rename = "stop_access_restricted")]
    StopAccessRestricted,
    #[serde(rename = "stop_paid_unconfirmed")]
    StopPaidUnconfirmed,
    #[serde(rename = "stop_no_higher_tier")]
    StopNoHigherTier,
    #[serde(rename = "stop_policy_blocked")]
    StopPolicyBlocked,
}

/// A fallback decision made for a single job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalFallbackDecision {
    /// Owning job.
    pub retrieval_job_id: RetrievalJobId,
    /// Tier that was attempted.
    pub from_tier: RetrievalChannelTier,
    /// Attempt mode that was attempted.
    pub from_attempt_mode: RetrievalAttemptMode,
    /// Tier being escalated to, if any.
    pub to_tier: Option<RetrievalChannelTier>,
    /// Attempt mode being escalated to, if any.
    pub to_attempt_mode: Option<RetrievalAttemptMode>,
    /// What was decided.
    pub decision: FallbackDecisionKind,
    /// Reason code.
    pub reason_code: RetrievalFailureCode,
    /// Policy explanation, if applicable.
    pub policy_reason: Option<String>,
}

// ---------------------------------------------------------------------------
// Retrieval artifact result
// ---------------------------------------------------------------------------

/// Complete artifact evidence from a retrieval job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalArtifactResult {
    /// Owning job.
    pub retrieval_job_id: RetrievalJobId,
    /// Batch that produced this result.
    pub retrieval_batch_id: String,
    /// Owning query plan.
    pub query_plan_id: String,
    /// Owning candidate.
    pub candidate_id: String,
    /// Channel that produced this result.
    pub channel_id: RetrievalChannelId,
    /// Channel tier.
    pub channel_tier: RetrievalChannelTier,
    /// Attempt mode that succeeded (or last attempted).
    pub attempt_mode: RetrievalAttemptMode,
    /// Terminal retrieval status.
    pub retrieval_status: RetrievalStatus,
    /// Path to local image artifact.
    pub local_artifact_path: Option<PathBuf>,
    /// Path to source artifact.
    pub source_artifact_path: Option<PathBuf>,
    /// Path to source sidecar.
    pub source_sidecar_path: Option<PathBuf>,
    /// Path to content summary.
    pub content_summary_path: Option<PathBuf>,
    /// Path to task report.
    pub task_report_path: Option<PathBuf>,
    /// Path to visual description.
    pub visual_description_path: Option<PathBuf>,
    /// Path to diagnostics.
    pub diagnostics_path: Option<PathBuf>,
    /// SHA-256 checksum of the local artifact.
    pub checksum_sha256: Option<String>,
    /// Content-Type reported by the server.
    pub content_type_reported: Option<String>,
    /// Content-Type determined by local sniffing.
    pub content_type_sniffed: Option<String>,
    /// Resolved content type.
    pub content_type: Option<String>,
    /// File extension.
    pub file_extension: Option<String>,
    /// File size in bytes.
    pub file_size_bytes: Option<u64>,
    /// Image dimensions, if determinable.
    pub image_dimensions: Option<crate::domain::candidate::ImageDimensions>,
    /// Whether the media type matches expectation.
    pub media_type_match: bool,
    /// Whether the local artifact file exists.
    pub local_artifact_exists: bool,
    /// Whether the source artifact file exists.
    pub source_artifact_exists: bool,
    /// Whether the sidecar is valid.
    pub sidecar_valid: bool,
    /// Whether the summary quality gate passed.
    pub summary_quality_passed: bool,
    /// Whether the task report is valid.
    pub task_report_valid: bool,
    /// Whether the visual description is valid.
    pub visual_description_valid: bool,
    /// Whether job ownership is consistent.
    pub job_ownership_valid: bool,
    /// Whether this is a metadata-only result.
    pub metadata_only: bool,
    /// All attempt traces for this job.
    pub fetch_trace: Vec<RetrievalAttemptTrace>,
    /// Policy decisions that affected this job.
    pub policy_decisions: Vec<RetrievalPolicyDecision>,
    /// Diagnostics produced.
    pub diagnostics: Vec<RetrievalDiagnostic>,
    /// Failure reason if not complete.
    pub failure_reason: Option<RetrievalFailureReason>,
    /// Whether redaction was applied.
    pub redaction_applied: bool,
}

impl RetrievalArtifactResult {
    /// Returns `true` if retrieval completed with full artifacts.
    pub fn is_complete(&self) -> bool {
        self.retrieval_status == RetrievalStatus::Complete
    }

    /// Check that all required artifact paths are present.
    pub fn has_all_required_paths(&self) -> bool {
        self.local_artifact_path.is_some()
            && self.source_artifact_path.is_some()
            && self.source_sidecar_path.is_some()
            && self.content_summary_path.is_some()
            && self.task_report_path.is_some()
            && self.visual_description_path.is_some()
    }

    /// Check that all required integrity fields are present.
    pub fn has_all_integrity_fields(&self) -> bool {
        self.checksum_sha256.is_some()
            && self.content_type.is_some()
            && self.file_size_bytes.unwrap_or(0) > 0
            && self.media_type_match
            && self.job_ownership_valid
            && !self.metadata_only
    }

    /// Full completeness check per the acceptance criteria.
    pub fn is_fully_complete(&self) -> bool {
        self.is_complete()
            && self.has_all_required_paths()
            && self.has_all_integrity_fields()
            && !self.fetch_trace.is_empty()
    }

    /// Check whether this is a metadata-only result.
    pub fn is_metadata_only_result(&self) -> bool {
        self.metadata_only
            || (self.local_artifact_path.is_none() && self.source_artifact_path.is_none())
    }

    /// Build a failed result for a job.
    pub fn failed(
        job: &RetrievalJob,
        batch_id: impl Into<String>,
        channel_id: RetrievalChannelId,
        channel_tier: RetrievalChannelTier,
        attempt_mode: RetrievalAttemptMode,
        reason: impl Into<String>,
        failure_code: RetrievalFailureCode,
        trace: Vec<RetrievalAttemptTrace>,
        diagnostics: Vec<RetrievalDiagnostic>,
    ) -> Self {
        Self {
            retrieval_job_id: job.retrieval_job_id.clone(),
            retrieval_batch_id: batch_id.into(),
            query_plan_id: job.query_plan_id.clone(),
            candidate_id: job.candidate_id.clone(),
            channel_id,
            channel_tier,
            attempt_mode,
            retrieval_status: RetrievalStatus::Failed,
            local_artifact_path: None,
            source_artifact_path: None,
            source_sidecar_path: None,
            content_summary_path: None,
            task_report_path: None,
            visual_description_path: None,
            diagnostics_path: None,
            checksum_sha256: None,
            content_type_reported: None,
            content_type_sniffed: None,
            content_type: None,
            file_extension: None,
            file_size_bytes: None,
            image_dimensions: None,
            media_type_match: false,
            local_artifact_exists: false,
            source_artifact_exists: false,
            sidecar_valid: false,
            summary_quality_passed: false,
            task_report_valid: false,
            visual_description_valid: false,
            job_ownership_valid: false,
            metadata_only: true,
            fetch_trace: trace,
            policy_decisions: Vec::new(),
            diagnostics,
            failure_reason: Some(RetrievalFailureReason {
                code: failure_code,
                message: reason.into(),
            }),
            redaction_applied: false,
        }
    }

    /// Build a policy-blocked result.
    pub fn policy_blocked(
        job: &RetrievalJob,
        batch_id: impl Into<String>,
        channel_id: RetrievalChannelId,
        channel_tier: RetrievalChannelTier,
        attempt_mode: RetrievalAttemptMode,
        reason: impl Into<String>,
        failure_code: RetrievalFailureCode,
        trace: Vec<RetrievalAttemptTrace>,
        diagnostics: Vec<RetrievalDiagnostic>,
        policy_decisions: Vec<RetrievalPolicyDecision>,
    ) -> Self {
        Self {
            retrieval_job_id: job.retrieval_job_id.clone(),
            retrieval_batch_id: batch_id.into(),
            query_plan_id: job.query_plan_id.clone(),
            candidate_id: job.candidate_id.clone(),
            channel_id,
            channel_tier,
            attempt_mode,
            retrieval_status: RetrievalStatus::PolicyBlocked,
            local_artifact_path: None,
            source_artifact_path: None,
            source_sidecar_path: None,
            content_summary_path: None,
            task_report_path: None,
            visual_description_path: None,
            diagnostics_path: None,
            checksum_sha256: None,
            content_type_reported: None,
            content_type_sniffed: None,
            content_type: None,
            file_extension: None,
            file_size_bytes: None,
            image_dimensions: None,
            media_type_match: false,
            local_artifact_exists: false,
            source_artifact_exists: false,
            sidecar_valid: false,
            summary_quality_passed: false,
            task_report_valid: false,
            visual_description_valid: false,
            job_ownership_valid: false,
            metadata_only: true,
            fetch_trace: trace,
            policy_decisions,
            diagnostics,
            failure_reason: Some(RetrievalFailureReason {
                code: failure_code,
                message: reason.into(),
            }),
            redaction_applied: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Failure reason
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalFailureReason {
    pub code: RetrievalFailureCode,
    pub message: String,
}

// ---------------------------------------------------------------------------
// Policy decision
// ---------------------------------------------------------------------------

/// A policy decision that affected a retrieval job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalPolicyDecision {
    /// What was decided.
    pub decision: String,
    /// Which policy rule was applied.
    pub policy_rule: String,
    /// Why this decision was made.
    pub reason: String,
}

// ---------------------------------------------------------------------------
// Execution block
// ---------------------------------------------------------------------------

/// A fact recording why retrieval execution was blocked.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalExecutionBlock {
    /// Owning query plan.
    pub query_plan_id: String,
    /// Batch id, if applicable.
    pub retrieval_batch_id: Option<String>,
    /// The dependency or condition that caused the block.
    pub dependency: String,
    /// Machine-readable failure code.
    pub failure_code: RetrievalFailureCode,
    /// Human-readable reason.
    pub reason: String,
    /// Whether this block is permanent (requires config change).
    pub is_permanent: bool,
    /// How many jobs were pending.
    pub pending_job_count: usize,
}

// ---------------------------------------------------------------------------
// Batch summary
// ---------------------------------------------------------------------------

/// Summary statistics for a retrieval batch.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RetrievalBatchSummary {
    /// Total jobs attempted.
    pub total_jobs: u32,
    /// Jobs that completed with full artifacts.
    pub complete: u32,
    /// Jobs that partially succeeded.
    pub partial: u32,
    /// Jobs that failed.
    pub failed: u32,
    /// Jobs blocked by policy.
    pub policy_blocked: u32,
    /// Jobs blocked by access restrictions.
    pub access_restricted: u32,
    /// Jobs blocked by paid unconfirmed.
    pub paid_unconfirmed: u32,
    /// Jobs rejected as metadata-only.
    pub metadata_only_rejected: u32,
    /// Jobs rejected for fixture-in-production.
    pub fixture_rejected: u32,
}

impl RetrievalBatchSummary {
    pub fn from_results(results: &[RetrievalArtifactResult]) -> Self {
        let mut complete = 0u32;
        let mut partial = 0u32;
        let mut failed = 0u32;
        let mut policy_blocked = 0u32;
        let mut access_restricted = 0u32;
        let mut paid_unconfirmed = 0u32;
        let mut metadata_only_rejected = 0u32;
        let mut fixture_rejected = 0u32;
        for r in results {
            match r.retrieval_status {
                RetrievalStatus::Complete => complete += 1,
                RetrievalStatus::Partial => partial += 1,
                RetrievalStatus::Failed => failed += 1,
                RetrievalStatus::PolicyBlocked => policy_blocked += 1,
                RetrievalStatus::AccessRestricted => access_restricted += 1,
                RetrievalStatus::PaidUnconfirmed => paid_unconfirmed += 1,
                RetrievalStatus::MetadataOnlyRejected => metadata_only_rejected += 1,
                RetrievalStatus::FixtureRejectedForProduction => fixture_rejected += 1,
            }
        }
        Self {
            total_jobs: results.len() as u32,
            complete,
            partial,
            failed,
            policy_blocked,
            access_restricted,
            paid_unconfirmed,
            metadata_only_rejected,
            fixture_rejected,
        }
    }
}

// ---------------------------------------------------------------------------
// Retrieval batch result
// ---------------------------------------------------------------------------

/// The aggregated result of a retrieval batch attempt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalBatchResult {
    /// Batch identifier.
    pub retrieval_batch_id: String,
    /// Owning query plan.
    pub query_plan_id: String,
    /// Full attempt count.
    pub full_attempt_count: u8,
    /// Retry count.
    pub retry_count: u8,
    /// Target batch size.
    pub target_size: u32,
    /// Actual number of jobs.
    pub actual_size: u32,
    /// Channel readiness reports.
    pub channel_readiness: Vec<RetrievalChannelReadinessReport>,
    /// Per-job results.
    pub results: Vec<RetrievalArtifactResult>,
    /// All attempt traces across all jobs.
    pub attempt_trace: Vec<RetrievalAttemptTrace>,
    /// All fallback decisions.
    pub fallback_decisions: Vec<RetrievalFallbackDecision>,
    /// Shortage evidence if any.
    pub shortage: Option<RetrievalBatchShortage>,
    /// Execution blocking facts.
    pub execution_blocking_facts: Vec<RetrievalExecutionBlock>,
    /// Diagnostics.
    pub diagnostics: Vec<RetrievalDiagnostic>,
    /// Aggregate summary.
    pub summary: RetrievalBatchSummary,
}

impl RetrievalBatchResult {
    /// Create a result from collected data.
    pub fn new(
        retrieval_batch_id: impl Into<String>,
        query_plan_id: impl Into<String>,
        full_attempt_count: u8,
        retry_count: u8,
        target_size: u32,
        channel_readiness: Vec<RetrievalChannelReadinessReport>,
        results: Vec<RetrievalArtifactResult>,
        attempt_trace: Vec<RetrievalAttemptTrace>,
        fallback_decisions: Vec<RetrievalFallbackDecision>,
        shortage: Option<RetrievalBatchShortage>,
        execution_blocking_facts: Vec<RetrievalExecutionBlock>,
        diagnostics: Vec<RetrievalDiagnostic>,
    ) -> Self {
        let actual_size = results.len() as u32;
        let summary = RetrievalBatchSummary::from_results(&results);
        Self {
            retrieval_batch_id: retrieval_batch_id.into(),
            query_plan_id: query_plan_id.into(),
            full_attempt_count,
            retry_count,
            target_size,
            actual_size,
            channel_readiness,
            results,
            attempt_trace,
            fallback_decisions,
            shortage,
            execution_blocking_facts,
            diagnostics,
            summary,
        }
    }

    /// Number of complete results.
    pub fn complete_count(&self) -> usize {
        self.results.iter().filter(|r| r.is_complete()).count()
    }

    /// Whether any complete results exist.
    pub fn has_any_complete(&self) -> bool {
        self.results.iter().any(|r| r.is_complete())
    }
}

// ---------------------------------------------------------------------------
// Artifact evidence DTOs
// ---------------------------------------------------------------------------

/// Robots policy outcome for a fetch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RobotsPolicyOutcome {
    #[serde(rename = "allowed")]
    Allowed,
    #[serde(rename = "disallowed")]
    Disallowed,
    #[serde(rename = "unknown_warned")]
    UnknownWarned,
    #[serde(rename = "unknown_blocked")]
    UnknownBlocked,
    #[serde(rename = "not_checked")]
    NotChecked,
}

/// Authorization risk assessment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthorizationRisk {
    #[serde(rename = "none_detected")]
    NoneDetected,
    #[serde(rename = "unknown")]
    Unknown,
    #[serde(rename = "login_required")]
    LoginRequired,
    #[serde(rename = "paywalled")]
    Paywalled,
    #[serde(rename = "access_restricted")]
    AccessRestricted,
}

/// Source sidecar — captures origin and fetch metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceSidecar {
    pub schema_version: String,
    pub retrieval_job_id: RetrievalJobId,
    pub query_plan_id: String,
    pub candidate_id: String,
    pub channel_id: RetrievalChannelId,
    pub channel_tier: RetrievalChannelTier,
    pub attempt_mode: RetrievalAttemptMode,
    pub primary_url_redacted: String,
    pub source_page_url_redacted: Option<String>,
    pub final_url_redacted: Option<String>,
    pub http_status: Option<u16>,
    pub response_headers_safe: BTreeMap<String, String>,
    pub provider_id: String,
    pub license_hint: Option<String>,
    pub robots_policy: RobotsPolicyOutcome,
    pub authorization_risk: AuthorizationRisk,
    pub fetched_at: String,
    pub redaction_applied: bool,
}

/// Kind of retrieved content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RetrievedContentKind {
    #[serde(rename = "image_artifact")]
    ImageArtifact,
    #[serde(rename = "metadata_only")]
    MetadataOnly,
    #[serde(rename = "source_page_only")]
    SourcePageOnly,
}

/// Summary quality level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SummaryQuality {
    #[serde(rename = "pass")]
    Pass,
    #[serde(rename = "fail")]
    Fail,
}

/// What generated the content summary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SummaryGeneratorKind {
    #[serde(rename = "local_retrieval_adapter")]
    LocalRetrievalAdapter,
    #[serde(rename = "self_hosted_service")]
    SelfHostedService,
    #[serde(rename = "paid_service")]
    PaidService,
    #[serde(rename = "fixture")]
    Fixture,
}

/// Content summary for a retrieved artifact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentSummary {
    pub retrieval_job_id: RetrievalJobId,
    pub candidate_id: String,
    pub content_kind: RetrievedContentKind,
    pub summary_text: String,
    pub summary_quality: SummaryQuality,
    pub summary_quality_gate_passed: bool,
    pub evidence_refs: Vec<String>,
    pub generated_by: SummaryGeneratorKind,
}

/// Record of an artifact file that was written.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactWriteRecord {
    pub artifact_type: String,
    pub path: String,
    pub file_size_bytes: u64,
    pub checksum_sha256: Option<String>,
}

/// Task report for a retrieval job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalTaskReport {
    pub retrieval_job_id: RetrievalJobId,
    pub query_plan_id: String,
    pub candidate_id: String,
    pub started_at: String,
    pub completed_at: String,
    pub status: RetrievalStatus,
    pub attempts: Vec<RetrievalAttemptTrace>,
    pub artifacts_written: Vec<ArtifactWriteRecord>,
    pub failure_code: Option<RetrievalFailureCode>,
    pub policy_blocks: Vec<RetrievalPolicyDecision>,
    pub redaction_applied: bool,
}

/// Method used for visual description.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VisualDescriptionMethod {
    #[serde(rename = "metadata_and_filename")]
    MetadataAndFilename,
    #[serde(rename = "lightweight_local_probe")]
    LightweightLocalProbe,
    #[serde(rename = "self_hosted_service")]
    SelfHostedService,
    #[serde(rename = "paid_service")]
    PaidService,
    #[serde(rename = "fixture")]
    Fixture,
}

/// Visual description of a retrieved image — retrieval-side evidence only.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisualDescription {
    pub retrieval_job_id: RetrievalJobId,
    pub candidate_id: String,
    pub description_text: String,
    pub method: VisualDescriptionMethod,
    pub confidence: Option<f32>,
    pub image_dimensions: Option<crate::domain::candidate::ImageDimensions>,
    pub content_type: Option<String>,
    pub evidence_refs: Vec<String>,
}

// =============================================================================
// Legacy / backward-compat types
// =============================================================================

/// Legacy retrieval result enum (backward compat).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RetrievalResult {
    Success(RetrievalSuccess),
    Failure(RetrievalFailure),
}

impl RetrievalResult {
    pub fn is_success(&self) -> bool {
        matches!(self, Self::Success(_))
    }
    pub fn is_failure(&self) -> bool {
        matches!(self, Self::Failure(_))
    }
    pub fn candidate_id(&self) -> &str {
        match self {
            Self::Success(s) => &s.candidate_id,
            Self::Failure(f) => &f.candidate_id,
        }
    }
}

/// Legacy retrieval success (backward compat).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalSuccess {
    pub candidate_id: String,
    pub local_path: String,
    pub channel_tier: RetrievalChannelTier,
    pub content_type: Option<String>,
    pub file_size_bytes: u64,
}

impl RetrievalSuccess {
    pub fn new(
        candidate_id: impl Into<String>,
        local_path: impl Into<String>,
        channel_tier: RetrievalChannelTier,
        content_type: Option<String>,
        file_size_bytes: u64,
    ) -> Self {
        Self {
            candidate_id: candidate_id.into(),
            local_path: local_path.into(),
            channel_tier,
            content_type,
            file_size_bytes,
        }
    }
}

/// Legacy retrieval failure (backward compat).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalFailure {
    pub candidate_id: String,
    pub channel_tier: RetrievalChannelTier,
    pub failure_category: RetrievalFailureCategory,
    pub reason: String,
    pub allows_fallback: bool,
}

impl RetrievalFailure {
    pub fn new(
        candidate_id: impl Into<String>,
        channel_tier: RetrievalChannelTier,
        failure_category: RetrievalFailureCategory,
        reason: impl Into<String>,
        allows_fallback: bool,
    ) -> Self {
        Self {
            candidate_id: candidate_id.into(),
            channel_tier,
            failure_category,
            reason: reason.into(),
            allows_fallback,
        }
    }
}

/// Legacy failure category (backward compat).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RetrievalFailureCategory {
    Network,
    HttpStatus,
    InvalidContent,
    AccessRestricted,
    ChannelDisabled,
    PaidNotConfirmed,
    UnsupportedUrl,
    Other,
}

/// Legacy fallback eligibility fact (backward compat).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FallbackEligibilityFact {
    pub failed_tier: RetrievalChannelTier,
    pub next_tier: Option<RetrievalChannelTier>,
    pub reason: String,
    pub is_access_restricted: bool,
    pub requires_paid_confirmation: bool,
}

impl FallbackEligibilityFact {
    pub fn new(
        failed_tier: RetrievalChannelTier,
        reason: impl Into<String>,
        is_access_restricted: bool,
    ) -> Self {
        let next_tier = failed_tier.next_fallback();
        let requires_paid_confirmation = next_tier.as_ref().map(|t| t.is_paid()).unwrap_or(false);
        Self {
            failed_tier,
            next_tier,
            reason: reason.into(),
            is_access_restricted,
            requires_paid_confirmation,
        }
    }
}

/// Legacy channel attempt result (backward compat).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelAttemptResult {
    pub channel_tier: RetrievalChannelTier,
    pub results: Vec<RetrievalResult>,
    pub success_count: u32,
    pub failure_count: u32,
    pub abandoned: bool,
    pub abandon_reason: Option<String>,
}

/// Legacy retrieval outcome (backward compat).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalOutcome {
    pub batch: RetrievalBatch,
    pub results: Vec<RetrievalResult>,
    pub channel_tier: RetrievalChannelTier,
    pub shortage: Option<RetrievalBatchShortage>,
    pub channels_attempted: u32,
    pub channel_attempts: Vec<ChannelAttemptResult>,
    pub fallback_facts: Vec<FallbackEligibilityFact>,
    pub execution_blocked: Option<ExecutionBlockingFact>,
}

/// Legacy execution blocking fact (backward compat).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionBlockingFact {
    pub reason: String,
    pub source_tier: Option<RetrievalChannelTier>,
    pub is_access_restricted: bool,
    pub is_paid_unconfirmed: bool,
}

/// Legacy channel readiness enum (backward compat).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RetrievalChannelReadiness {
    Ready,
    Disabled,
    MissingDependency,
    Misconfigured,
    PaidUnconfirmed,
}

impl RetrievalChannelReadiness {
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready)
    }
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Disabled | Self::MissingDependency | Self::Misconfigured
        )
    }
}

impl std::fmt::Display for RetrievalChannelReadiness {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Self::Ready => "ready",
            Self::Disabled => "disabled",
            Self::MissingDependency => "missing_dependency",
            Self::Misconfigured => "misconfigured",
            Self::PaidUnconfirmed => "paid_unconfirmed",
        };
        write!(f, "{}", label)
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Tier tests
    // -----------------------------------------------------------------------

    #[test]
    fn tier_ordering() {
        assert!(RetrievalChannelTier::NormalWebFetch < RetrievalChannelTier::SelfHostedService);
        assert!(RetrievalChannelTier::SelfHostedService < RetrievalChannelTier::PaidOnlineService);
    }

    #[test]
    fn fallback_chain() {
        assert_eq!(
            RetrievalChannelTier::NormalWebFetch.next_fallback(),
            Some(RetrievalChannelTier::SelfHostedService)
        );
        assert_eq!(
            RetrievalChannelTier::SelfHostedService.next_fallback(),
            Some(RetrievalChannelTier::PaidOnlineService)
        );
        assert_eq!(
            RetrievalChannelTier::PaidOnlineService.next_fallback(),
            None
        );
    }

    #[test]
    fn tier_is_paid() {
        assert!(!RetrievalChannelTier::NormalWebFetch.is_paid());
        assert!(!RetrievalChannelTier::SelfHostedService.is_paid());
        assert!(RetrievalChannelTier::PaidOnlineService.is_paid());
    }

    #[test]
    fn tier_display() {
        assert_eq!(
            RetrievalChannelTier::NormalWebFetch.to_string(),
            "normal_web_fetch"
        );
        assert_eq!(
            RetrievalChannelTier::SelfHostedService.to_string(),
            "self_hosted_service"
        );
        assert_eq!(
            RetrievalChannelTier::PaidOnlineService.to_string(),
            "paid_online_service"
        );
    }

    #[test]
    fn tier_serde_aliases() {
        // web_fetch alias
        let json = r#""web_fetch""#;
        let tier: RetrievalChannelTier = serde_json::from_str(json).expect("deserialize web_fetch");
        assert_eq!(tier, RetrievalChannelTier::NormalWebFetch);

        // self_hosted alias
        let json = r#""self_hosted""#;
        let tier: RetrievalChannelTier =
            serde_json::from_str(json).expect("deserialize self_hosted");
        assert_eq!(tier, RetrievalChannelTier::SelfHostedService);

        // paid alias
        let json = r#""paid""#;
        let tier: RetrievalChannelTier = serde_json::from_str(json).expect("deserialize paid");
        assert_eq!(tier, RetrievalChannelTier::PaidOnlineService);
    }

    // -----------------------------------------------------------------------
    // Channel ID tests
    // -----------------------------------------------------------------------

    #[test]
    fn channel_id_construction() {
        let id = RetrievalChannelId::new("web-fetch-default");
        assert_eq!(id.to_string(), "web-fetch-default");
    }

    #[test]
    fn channel_id_equality() {
        let a = RetrievalChannelId::new("ch1");
        let b = RetrievalChannelId::new("ch1");
        let c = RetrievalChannelId::new("ch2");
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    // -----------------------------------------------------------------------
    // RetrievalJobId tests
    // -----------------------------------------------------------------------

    #[test]
    fn job_id_construction() {
        let id = RetrievalJobId::new("ret-qp-1-1-cand-1");
        assert_eq!(id.to_string(), "ret-qp-1-1-cand-1");
    }

    // -----------------------------------------------------------------------
    // RetrievalStatus tests
    // -----------------------------------------------------------------------

    #[test]
    fn status_complete_detection() {
        assert!(RetrievalStatus::Complete.is_complete());
        assert!(!RetrievalStatus::Failed.is_complete());
        assert!(!RetrievalStatus::Partial.is_complete());
    }

    #[test]
    fn status_terminal_failure_detection() {
        assert!(RetrievalStatus::Failed.is_terminal_failure());
        assert!(RetrievalStatus::PolicyBlocked.is_terminal_failure());
        assert!(RetrievalStatus::MetadataOnlyRejected.is_terminal_failure());
        assert!(!RetrievalStatus::Complete.is_terminal_failure());
        assert!(!RetrievalStatus::Partial.is_terminal_failure());
    }

    // -----------------------------------------------------------------------
    // RetrievalBatchShortage tests
    // -----------------------------------------------------------------------

    #[test]
    fn batch_shortage_records_gap() {
        let shortage = RetrievalBatchShortage::new(
            "qp-1",
            8,
            3,
            RetrievalShortageCode::InsufficientRetrievableCandidates,
            "only 3 retrievable candidates available",
        );
        assert_eq!(shortage.target_size, 8);
        assert_eq!(shortage.actual_size, 3);
        assert!(shortage.reason.contains("retrievable"));
    }

    // -----------------------------------------------------------------------
    // RetrievalBatch tests
    // -----------------------------------------------------------------------

    #[test]
    fn retrieval_batch_normal() {
        let batch = RetrievalBatch::new("b-1", "qp-1", 1, 0, 4, vec![], None);
        assert_eq!(batch.actual_size, 0);
        assert!(batch.is_short_batch);
    }

    #[test]
    fn retrieval_batch_candidate_ids() {
        use crate::domain::candidate::{
            CandidateId, CandidateQualityDecision, CandidateRecord, RetrievableCandidate,
        };

        let rec = CandidateRecord::minimal(
            CandidateId::new("cand-a"),
            crate::domain::candidate::ProviderId::new("p1"),
            "https://example.com/a.jpg",
        );
        let rc = RetrievableCandidate {
            candidate: rec,
            candidate_quality_decision: CandidateQualityDecision {
                candidate_id: CandidateId::new("cand-a"),
                query_plan_id: "qp-1".into(),
                mechanical_passed: true,
                vlm_passed: true,
                final_status: crate::domain::candidate::CandidateQualityStatus::Retrievable,
                priority: 5,
                blocking_metrics: vec![],
                reference_metrics: vec![],
                vlm_decision: None,
                diagnostics: vec![],
            },
            retrieval_priority: 5,
            primary_image_url: "https://example.com/a.jpg".into(),
            source_page_url: None,
            thumbnail_url: None,
            expected_mime_type: Some("image/jpeg".into()),
            license_hint: None,
            provenance_refs: vec![],
        };
        let job = RetrievalJob::from_retrievable(
            &rc,
            "b-1",
            "qp-1",
            1,
            0,
            "qd-ref-1",
            RetrievalPolicyContext::default(),
        );
        let batch = RetrievalBatch::new("b-1", "qp-1", 1, 0, 2, vec![job], None);
        assert_eq!(batch.candidate_ids(), vec!["cand-a"]);
    }

    // -----------------------------------------------------------------------
    // RetrievalArtifactResult tests
    // -----------------------------------------------------------------------

    #[test]
    fn artifact_result_complete_detection() {
        let result = RetrievalArtifactResult {
            retrieval_job_id: RetrievalJobId::new("ret-1"),
            retrieval_batch_id: "b-1".into(),
            query_plan_id: "qp-1".into(),
            candidate_id: "cand-1".into(),
            channel_id: RetrievalChannelId::new("web_fetch"),
            channel_tier: RetrievalChannelTier::NormalWebFetch,
            attempt_mode: RetrievalAttemptMode::DirectImageFetch,
            retrieval_status: RetrievalStatus::Complete,
            local_artifact_path: Some(PathBuf::from("/tmp/img.jpg")),
            source_artifact_path: Some(PathBuf::from("/tmp/src.jpg")),
            source_sidecar_path: Some(PathBuf::from("/tmp/sidecar.json")),
            content_summary_path: Some(PathBuf::from("/tmp/summary.json")),
            task_report_path: Some(PathBuf::from("/tmp/report.json")),
            visual_description_path: Some(PathBuf::from("/tmp/vd.json")),
            diagnostics_path: None,
            checksum_sha256: Some("abc123".into()),
            content_type_reported: Some("image/jpeg".into()),
            content_type_sniffed: Some("image/jpeg".into()),
            content_type: Some("image/jpeg".into()),
            file_extension: Some("jpg".into()),
            file_size_bytes: Some(4096),
            image_dimensions: None,
            media_type_match: true,
            local_artifact_exists: true,
            source_artifact_exists: true,
            sidecar_valid: true,
            summary_quality_passed: true,
            task_report_valid: true,
            visual_description_valid: true,
            job_ownership_valid: true,
            metadata_only: false,
            fetch_trace: vec![],
            policy_decisions: vec![],
            diagnostics: vec![],
            failure_reason: None,
            redaction_applied: false,
        };
        assert!(result.is_complete());
        assert!(result.has_all_required_paths());
        assert!(result.has_all_integrity_fields());
    }

    #[test]
    fn artifact_result_metadata_only_detected() {
        let result = RetrievalArtifactResult {
            retrieval_job_id: RetrievalJobId::new("ret-1"),
            retrieval_batch_id: "b-1".into(),
            query_plan_id: "qp-1".into(),
            candidate_id: "cand-1".into(),
            channel_id: RetrievalChannelId::new("web_fetch"),
            channel_tier: RetrievalChannelTier::NormalWebFetch,
            attempt_mode: RetrievalAttemptMode::DirectImageFetch,
            retrieval_status: RetrievalStatus::MetadataOnlyRejected,
            local_artifact_path: None,
            source_artifact_path: None,
            source_sidecar_path: None,
            content_summary_path: None,
            task_report_path: None,
            visual_description_path: None,
            diagnostics_path: None,
            checksum_sha256: None,
            content_type_reported: None,
            content_type_sniffed: None,
            content_type: None,
            file_extension: None,
            file_size_bytes: None,
            image_dimensions: None,
            media_type_match: false,
            local_artifact_exists: false,
            source_artifact_exists: false,
            sidecar_valid: false,
            summary_quality_passed: false,
            task_report_valid: false,
            visual_description_valid: false,
            job_ownership_valid: false,
            metadata_only: true,
            fetch_trace: vec![],
            policy_decisions: vec![],
            diagnostics: vec![],
            failure_reason: None,
            redaction_applied: false,
        };
        assert!(!result.is_complete());
        assert!(result.is_metadata_only_result());
    }

    // -----------------------------------------------------------------------
    // RetrievalBatchResult tests
    // -----------------------------------------------------------------------

    #[test]
    fn batch_result_summary_counts() {
        let summary = RetrievalBatchSummary {
            total_jobs: 4,
            complete: 1,
            partial: 1,
            failed: 1,
            policy_blocked: 1,
            ..Default::default()
        };
        assert_eq!(summary.total_jobs, 4);
        assert_eq!(summary.complete, 1);
    }

    // -----------------------------------------------------------------------
    // Channel readiness report tests
    // -----------------------------------------------------------------------

    #[test]
    fn readiness_report_ready() {
        let report = RetrievalChannelReadinessReport::ready(
            RetrievalChannelId::new("wf-1"),
            "Web Fetch",
            RetrievalChannelTier::NormalWebFetch,
        );
        assert!(report.available);
        assert!(report.enabled);
        assert!(report.failure_code.is_none());
    }

    #[test]
    fn readiness_report_paid_unconfirmed() {
        let report = RetrievalChannelReadinessReport::paid_unconfirmed(
            RetrievalChannelId::new("paid-1"),
            "Paid Service",
        );
        assert!(!report.available);
        assert_eq!(
            report.failure_code,
            Some(RetrievalFailureCode::RetrievalPaidUnconfirmed)
        );
    }

    #[test]
    fn readiness_report_fixture_blocked() {
        let report = RetrievalChannelReadinessReport::fixture_blocked(
            RetrievalChannelId::new("fix-1"),
            "Fixture",
            RetrievalChannelTier::NormalWebFetch,
        );
        assert!(!report.available);
        assert_eq!(
            report.failure_code,
            Some(RetrievalFailureCode::RetrievalFixtureNotProduction)
        );
    }

    // -----------------------------------------------------------------------
    // Legacy backward-compat tests
    // -----------------------------------------------------------------------

    #[test]
    fn legacy_readiness_is_ready() {
        assert!(RetrievalChannelReadiness::Ready.is_ready());
        assert!(!RetrievalChannelReadiness::Disabled.is_ready());
    }

    #[test]
    fn legacy_readiness_terminal() {
        assert!(RetrievalChannelReadiness::Disabled.is_terminal());
        assert!(RetrievalChannelReadiness::MissingDependency.is_terminal());
        assert!(RetrievalChannelReadiness::Misconfigured.is_terminal());
        assert!(!RetrievalChannelReadiness::Ready.is_terminal());
        assert!(!RetrievalChannelReadiness::PaidUnconfirmed.is_terminal());
    }

    #[test]
    fn legacy_fallback_fact_paid_detection() {
        let fact = FallbackEligibilityFact::new(
            RetrievalChannelTier::SelfHostedService,
            "service unavailable",
            false,
        );
        assert_eq!(fact.failed_tier, RetrievalChannelTier::SelfHostedService);
        assert!(fact.requires_paid_confirmation);
    }

    #[test]
    fn legacy_retrieval_success_fields() {
        let s = RetrievalSuccess::new(
            "c1",
            "/tmp/test.png",
            RetrievalChannelTier::NormalWebFetch,
            Some("image/png".into()),
            8192,
        );
        assert_eq!(s.candidate_id, "c1");
        assert_eq!(s.local_path, "/tmp/test.png");
        assert_eq!(s.file_size_bytes, 8192);
    }

    // -----------------------------------------------------------------------
    // Attempt mode tests
    // -----------------------------------------------------------------------

    #[test]
    fn attempt_mode_is_normal_web() {
        assert!(RetrievalAttemptMode::DirectImageFetch.is_normal_web());
        assert!(RetrievalAttemptMode::SourcePageResolve.is_normal_web());
        assert!(!RetrievalAttemptMode::SelfHostedService.is_normal_web());
        assert!(!RetrievalAttemptMode::PaidOnlineService.is_normal_web());
    }

    #[test]
    fn attempt_mode_display() {
        assert_eq!(
            RetrievalAttemptMode::DirectImageFetch.to_string(),
            "direct_image_fetch"
        );
        assert_eq!(
            RetrievalAttemptMode::SourcePageResolve.to_string(),
            "source_page_resolve"
        );
    }

    // -----------------------------------------------------------------------
    // Failure code display tests
    // -----------------------------------------------------------------------

    #[test]
    fn failure_code_display() {
        assert_eq!(
            RetrievalFailureCode::RetrievalAccessRestricted.to_string(),
            "RETRIEVAL_ACCESS_RESTRICTED"
        );
        assert_eq!(
            RetrievalFailureCode::RetrievalPaidUnconfirmed.to_string(),
            "RETRIEVAL_PAID_UNCONFIRMED"
        );
        assert_eq!(
            RetrievalFailureCode::RetrievalMetadataOnly.to_string(),
            "RETRIEVAL_METADATA_ONLY"
        );
    }

    // -----------------------------------------------------------------------
    // RetrievalDiagnostic tests
    // -----------------------------------------------------------------------

    #[test]
    fn diagnostic_builder() {
        let diag = RetrievalDiagnostic::new(
            RetrievalFailureCode::RetrievalDirectFetchNetwork,
            RetrievalSeverity::Error,
            "qp-1",
            "network timeout",
        )
        .with_job("ret-1")
        .with_candidate("cand-1")
        .with_channel(
            "wf-1",
            RetrievalChannelTier::NormalWebFetch,
            RetrievalAttemptMode::DirectImageFetch,
        );

        assert_eq!(diag.code, RetrievalFailureCode::RetrievalDirectFetchNetwork);
        assert_eq!(diag.severity, RetrievalSeverity::Error);
        assert_eq!(diag.retrieval_job_id, Some(RetrievalJobId::new("ret-1")));
        assert_eq!(diag.candidate_id, Some("cand-1".to_string()));
        assert!(diag.channel_tier.is_some());
    }

    // -----------------------------------------------------------------------
    // ChannelCapabilities default tests
    // -----------------------------------------------------------------------

    #[test]
    fn capabilities_default() {
        let caps = RetrievalChannelCapabilities::default();
        assert!(caps.supports_direct_image_fetch);
        assert!(caps.supports_source_page_resolve);
        assert!(caps.supports_checksum);
        assert!(!caps.fixture_only);
    }

    // -----------------------------------------------------------------------
    // RetrievalBatchSummary::from_results
    // -----------------------------------------------------------------------

    #[test]
    fn batch_summary_from_results() {
        use std::path::PathBuf;

        let results = vec![
            {
                RetrievalArtifactResult {
                    retrieval_job_id: RetrievalJobId::new("ret-1"),
                    retrieval_batch_id: "b-1".into(),
                    query_plan_id: "qp-1".into(),
                    candidate_id: "c-1".into(),
                    channel_id: RetrievalChannelId::new("wf"),
                    channel_tier: RetrievalChannelTier::NormalWebFetch,
                    attempt_mode: RetrievalAttemptMode::DirectImageFetch,
                    retrieval_status: RetrievalStatus::Complete,
                    local_artifact_path: Some(PathBuf::from("/tmp/a.jpg")),
                    source_artifact_path: Some(PathBuf::from("/tmp/a_src.jpg")),
                    source_sidecar_path: Some(PathBuf::from("/tmp/a_sidecar.json")),
                    content_summary_path: Some(PathBuf::from("/tmp/a_summary.json")),
                    task_report_path: Some(PathBuf::from("/tmp/a_report.json")),
                    visual_description_path: Some(PathBuf::from("/tmp/a_vd.json")),
                    diagnostics_path: None,
                    checksum_sha256: Some("abc".into()),
                    content_type_reported: Some("image/jpeg".into()),
                    content_type_sniffed: Some("image/jpeg".into()),
                    content_type: Some("image/jpeg".into()),
                    file_extension: Some("jpg".into()),
                    file_size_bytes: Some(1024),
                    image_dimensions: None,
                    media_type_match: true,
                    local_artifact_exists: true,
                    source_artifact_exists: true,
                    sidecar_valid: true,
                    summary_quality_passed: true,
                    task_report_valid: true,
                    visual_description_valid: true,
                    job_ownership_valid: true,
                    metadata_only: false,
                    fetch_trace: vec![],
                    policy_decisions: vec![],
                    diagnostics: vec![],
                    failure_reason: None,
                    redaction_applied: false,
                }
            },
            {
                let mut r = RetrievalArtifactResult {
                    retrieval_job_id: RetrievalJobId::new("ret-2"),
                    ..RetrievalArtifactResult::failed(
                        &RetrievalJob {
                            retrieval_job_id: RetrievalJobId::new("ret-2"),
                            query_plan_id: "qp-1".into(),
                            candidate_id: "c-2".into(),
                            full_attempt_count: 1,
                            retry_count: 0,
                            retrieval_priority: 1,
                            target: RetrievalTarget {
                                target_type: RetrievalTargetType::Image,
                                primary_image_url: "https://example.com/b.jpg".into(),
                                alternate_source_page_url: None,
                                thumbnail_url: None,
                                expected_mime_type: None,
                                license_hint: None,
                                provider_id: "p1".into(),
                                candidate_provenance_refs: vec![],
                            },
                            candidate_quality_decision_ref: "qd-2".into(),
                            requested_outputs: vec![],
                            policy_context: RetrievalPolicyContext::default(),
                        },
                        "b-1",
                        RetrievalChannelId::new("wf"),
                        RetrievalChannelTier::NormalWebFetch,
                        RetrievalAttemptMode::DirectImageFetch,
                        "network error",
                        RetrievalFailureCode::RetrievalDirectFetchNetwork,
                        vec![],
                        vec![],
                    )
                };
                r.retrieval_status = RetrievalStatus::Failed;
                r
            },
        ];

        let summary = RetrievalBatchSummary::from_results(&results);
        assert_eq!(summary.total_jobs, 2);
        assert_eq!(summary.complete, 1);
        assert_eq!(summary.failed, 1);
    }
}
