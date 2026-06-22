//! Metrics domain types.
//!
//! Structured task evidence events supporting MET-001 through MET-006.
//!
//! v1.1 adds shared quality evidence types (MetricFact, QualityDiagnostic,
//! QualitySummary, QualityAuditEvent, QualityExecutionBlock) used by both
//! candidate quality and image acceptance gates.
//!
//! References: PRD §数据、埋点与度量方案, HLD §Task Evidence & Metrics,
//! `docs/design/v1.1-TASK-003-quality-vlm-design.md`

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

// =============================================================================
// Legacy metric types (MET-001 … MET-006)
// =============================================================================

/// The kind of metric event, mapping to PRD MET-001 … MET-006.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MetricKind {
    /// MET-001: Task outcome distribution (input_rejected, full_delivery,
    /// limited_delivery, execution_blocked).
    TaskOutcome,

    /// MET-002: Candidate satisfaction rate (actual vs target).
    CandidateSatisfaction,

    /// MET-003: Qualified image achievement rate (qualified vs required).
    QualifiedImageAchievement,

    /// MET-004: Top rejection reasons for candidates and images.
    RejectionReason,

    /// MET-005: Retrieval channel effectiveness.
    ChannelEffectiveness,

    /// MET-006: OpenClaw evaluation pass / reject / uncertain ratio.
    OpenClawEvaluationRate,
}

/// A single metric event emitted during task execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricEvent {
    /// Which metric this event contributes to.
    pub kind: MetricKind,

    /// Human-readable label for the event.
    pub label: String,

    /// Numeric value (e.g. count, ratio numerator).
    pub value: f64,

    /// Optional denominator for rate computation.
    pub denominator: Option<f64>,

    /// Free-form metadata (e.g. provider name, channel tier, rejection
    /// category). Must not contain credentials or sensitive data.
    pub metadata: Vec<(String, String)>,
}

impl MetricEvent {
    pub fn new(kind: MetricKind, label: impl Into<String>, value: f64) -> Self {
        Self {
            kind,
            label: label.into(),
            value,
            denominator: None,
            metadata: Vec::new(),
        }
    }

    pub fn with_denominator(mut self, denominator: f64) -> Self {
        self.denominator = Some(denominator);
        self
    }

    pub fn with_meta(mut self, key: impl Into<String>, val: impl Into<String>) -> Self {
        self.metadata.push((key.into(), val.into()));
        self
    }
}

// =============================================================================
// v1.1 shared quality evidence model
// =============================================================================

/// Which quality phase produced this evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QualityPhase {
    /// Candidate quality gate (before retrieval).
    #[serde(rename = "candidate")]
    Candidate,
    /// Image acceptance gate (after retrieval).
    #[serde(rename = "image")]
    Image,
}

impl std::fmt::Display for QualityPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Candidate => write!(f, "candidate"),
            Self::Image => write!(f, "image"),
        }
    }
}

/// Whether a metric fact is blocking (causes rejection) or reference
/// (supplementary evidence only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MetricClass {
    /// Blocking — subject is rejected when any blocking fact is present.
    #[serde(rename = "blocking")]
    Blocking,
    /// Reference — supplementary evidence, never rejects by itself.
    #[serde(rename = "reference")]
    Reference,
}

impl std::fmt::Display for MetricClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Blocking => write!(f, "blocking"),
            Self::Reference => write!(f, "reference"),
        }
    }
}

/// Severity of a metric fact or diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum QualitySeverity {
    /// Informational — normal operation.
    #[serde(rename = "info")]
    Info,
    /// Warning — notable but not blocking.
    #[serde(rename = "warning")]
    Warning,
    /// Error — something was blocked or failed.
    #[serde(rename = "error")]
    Error,
    /// Blocker — external dependency or policy prevents execution.
    #[serde(rename = "blocker")]
    Blocker,
}

impl std::fmt::Display for QualitySeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Info => write!(f, "info"),
            Self::Warning => write!(f, "warning"),
            Self::Error => write!(f, "error"),
            Self::Blocker => write!(f, "blocker"),
        }
    }
}

// ---------------------------------------------------------------------------
// Quality metric codes — candidate blocking metrics
// ---------------------------------------------------------------------------

/// Machine-readable quality metric codes.
///
/// Covers both candidate and image quality phases, blocking and reference
/// metrics per the detailed design.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum QualityMetricCode {
    // --- Candidate blocking ---
    /// No primary image_url is present after normalization.
    #[serde(rename = "CANDIDATE_IMAGE_URL_MISSING")]
    CandidateImageUrlMissing,
    /// URL is empty, malformed, unsupported, or non-HTTP(S).
    #[serde(rename = "CANDIDATE_IMAGE_URL_INVALID")]
    CandidateImageUrlInvalid,
    /// Candidate query_plan_id differs from the active QueryPlan.
    #[serde(rename = "CANDIDATE_QUERY_OWNERSHIP_MISMATCH")]
    CandidateQueryOwnershipMismatch,
    /// Candidate is an exact duplicate already represented.
    #[serde(rename = "CANDIDATE_DUPLICATE_BLOCKED")]
    CandidateDuplicateBlocked,
    /// Provider/source domain is prohibited by policy.
    #[serde(rename = "CANDIDATE_PROHIBITED_SOURCE")]
    CandidateProhibitedSource,
    /// Candidate is known login-required/paywalled and policy disallows it.
    #[serde(rename = "CANDIDATE_ACCESS_RESTRICTED")]
    CandidateAccessRestricted,
    /// Title/snippet/source evidence clearly contradicts negative_scope or must_avoid.
    #[serde(rename = "CANDIDATE_NEGATIVE_SCOPE_CONTRADICTION")]
    CandidateNegativeScopeContradiction,
    /// Provider dimensions are present and below absolute image minimums.
    #[serde(rename = "CANDIDATE_BELOW_ABSOLUTE_DIMENSIONS")]
    CandidateBelowAbsoluteDimensions,
    /// Fixture candidate evidence is used in production mode.
    #[serde(rename = "CANDIDATE_FIXTURE_NOT_PRODUCTION")]
    CandidateFixtureNotProduction,

    // --- Candidate reference ---
    /// Provider rank and global rank hint.
    #[serde(rename = "CANDIDATE_PROVIDER_RANK")]
    CandidateProviderRank,
    /// Width/height or missing dimension evidence.
    #[serde(rename = "CANDIDATE_DIMENSIONS_REPORTED")]
    CandidateDimensionsReported,
    /// Whether source page URL exists and is usable by retrieval.
    #[serde(rename = "CANDIDATE_SOURCE_PAGE_PRESENT")]
    CandidateSourcePagePresent,
    /// License or authorization hint, including unknown.
    #[serde(rename = "CANDIDATE_LICENSE_HINT")]
    CandidateLicenseHint,
    /// Source authority, domain, or provider evidence.
    #[serde(rename = "CANDIDATE_SOURCE_AUTHORITY_HINT")]
    CandidateSourceAuthorityHint,
    /// Title/snippet match and missing-context signals.
    #[serde(rename = "CANDIDATE_TEXT_CONTEXT_MATCH")]
    CandidateTextContextMatch,
    /// Source/provider diversity and duplicate similarity.
    #[serde(rename = "CANDIDATE_DIVERSITY_SIGNAL")]
    CandidateDiversitySignal,
    /// Search target shortage evidence from TASK-002.
    #[serde(rename = "CANDIDATE_SEARCH_SHORTAGE_CONTEXT")]
    CandidateSearchShortageContext,

    // --- Image blocking ---
    /// retrieval_status is not complete/fetched.
    #[serde(rename = "IMAGE_RETRIEVAL_NOT_COMPLETE")]
    ImageRetrievalNotComplete,
    /// local_artifact_path absent or file missing.
    #[serde(rename = "IMAGE_LOCAL_ARTIFACT_MISSING")]
    ImageLocalArtifactMissing,
    /// source_artifact_path absent or file missing.
    #[serde(rename = "IMAGE_SOURCE_ARTIFACT_MISSING")]
    ImageSourceArtifactMissing,
    /// source_sidecar_path absent or file missing.
    #[serde(rename = "IMAGE_SIDECAR_MISSING")]
    ImageSidecarMissing,
    /// content_summary_path absent or file missing.
    #[serde(rename = "IMAGE_SUMMARY_MISSING")]
    ImageSummaryMissing,
    /// task_report_path absent or file missing.
    #[serde(rename = "IMAGE_TASK_REPORT_MISSING")]
    ImageTaskReportMissing,
    /// visual_description_path absent or file missing.
    #[serde(rename = "IMAGE_VISUAL_DESCRIPTION_MISSING")]
    ImageVisualDescriptionMissing,
    /// checksum_sha256 absent.
    #[serde(rename = "IMAGE_CHECKSUM_MISSING")]
    ImageChecksumMissing,
    /// Content type absent or not image-compatible.
    #[serde(rename = "IMAGE_CONTENT_TYPE_INVALID")]
    ImageContentTypeInvalid,
    /// Retrieval result media_type_match is false.
    #[serde(rename = "IMAGE_MEDIA_TYPE_MISMATCH")]
    ImageMediaTypeMismatch,
    /// File size is zero or below configured minimum.
    #[serde(rename = "IMAGE_FILE_EMPTY_OR_TOO_SMALL")]
    ImageFileEmptyOrTooSmall,
    /// Dimensions violate quality requirements or absolute minimum.
    #[serde(rename = "IMAGE_DIMENSIONS_TOO_SMALL")]
    ImageDimensionsTooSmall,
    /// File cannot be parsed or read safely.
    #[serde(rename = "IMAGE_CORRUPT_OR_UNREADABLE")]
    ImageCorruptOrUnreadable,
    /// Retrieval job/candidate/query ownership does not match.
    #[serde(rename = "IMAGE_JOB_OWNERSHIP_MISMATCH")]
    ImageJobOwnershipMismatch,
    /// Result contains metadata/page/summary but no image artifact.
    #[serde(rename = "IMAGE_METADATA_ONLY_RESULT")]
    ImageMetadataOnlyResult,
    /// Retrieval evidence indicates prohibited access or source.
    #[serde(rename = "IMAGE_PROHIBITED_SOURCE")]
    ImageProhibitedSource,
    /// Visual description or artifact evidence clearly contradicts negative scope.
    #[serde(rename = "IMAGE_NEGATIVE_SCOPE_CONTRADICTION")]
    ImageNegativeScopeContradiction,
    /// Fixture artifact is used in production mode.
    #[serde(rename = "IMAGE_FIXTURE_NOT_PRODUCTION")]
    ImageFixtureNotProduction,

    // --- Image reference ---
    /// Actual width/height and quality threshold comparison.
    #[serde(rename = "IMAGE_DIMENSIONS")]
    ImageDimensionsRef,
    /// File size and unusually large/small file signal.
    #[serde(rename = "IMAGE_FILE_SIZE")]
    ImageFileSizeRef,
    /// MIME type and extension consistency.
    #[serde(rename = "IMAGE_CONTENT_TYPE")]
    ImageContentTypeRef,
    /// Source/provider authority and authorization hints.
    #[serde(rename = "IMAGE_SOURCE_AUTHORITY")]
    ImageSourceAuthorityRef,
    /// License/rights signal retained from candidate/retrieval evidence.
    #[serde(rename = "IMAGE_LICENSE_HINT")]
    ImageLicenseHintRef,
    /// Confidence and completeness of visual description.
    #[serde(rename = "IMAGE_VISUAL_DESCRIPTION_CONFIDENCE")]
    ImageVisualDescriptionConfidence,
    /// Contribution to requested source diversity.
    #[serde(rename = "IMAGE_SOURCE_DIVERSITY")]
    ImageSourceDiversity,
    /// Number of fallback attempts and final channel tier.
    #[serde(rename = "IMAGE_FETCH_TRACE_QUALITY")]
    ImageFetchTraceQuality,
}

impl std::fmt::Display for QualityMetricCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::CandidateImageUrlMissing => "CANDIDATE_IMAGE_URL_MISSING",
            Self::CandidateImageUrlInvalid => "CANDIDATE_IMAGE_URL_INVALID",
            Self::CandidateQueryOwnershipMismatch => "CANDIDATE_QUERY_OWNERSHIP_MISMATCH",
            Self::CandidateDuplicateBlocked => "CANDIDATE_DUPLICATE_BLOCKED",
            Self::CandidateProhibitedSource => "CANDIDATE_PROHIBITED_SOURCE",
            Self::CandidateAccessRestricted => "CANDIDATE_ACCESS_RESTRICTED",
            Self::CandidateNegativeScopeContradiction => "CANDIDATE_NEGATIVE_SCOPE_CONTRADICTION",
            Self::CandidateBelowAbsoluteDimensions => "CANDIDATE_BELOW_ABSOLUTE_DIMENSIONS",
            Self::CandidateFixtureNotProduction => "CANDIDATE_FIXTURE_NOT_PRODUCTION",
            Self::CandidateProviderRank => "CANDIDATE_PROVIDER_RANK",
            Self::CandidateDimensionsReported => "CANDIDATE_DIMENSIONS_REPORTED",
            Self::CandidateSourcePagePresent => "CANDIDATE_SOURCE_PAGE_PRESENT",
            Self::CandidateLicenseHint => "CANDIDATE_LICENSE_HINT",
            Self::CandidateSourceAuthorityHint => "CANDIDATE_SOURCE_AUTHORITY_HINT",
            Self::CandidateTextContextMatch => "CANDIDATE_TEXT_CONTEXT_MATCH",
            Self::CandidateDiversitySignal => "CANDIDATE_DIVERSITY_SIGNAL",
            Self::CandidateSearchShortageContext => "CANDIDATE_SEARCH_SHORTAGE_CONTEXT",
            Self::ImageRetrievalNotComplete => "IMAGE_RETRIEVAL_NOT_COMPLETE",
            Self::ImageLocalArtifactMissing => "IMAGE_LOCAL_ARTIFACT_MISSING",
            Self::ImageSourceArtifactMissing => "IMAGE_SOURCE_ARTIFACT_MISSING",
            Self::ImageSidecarMissing => "IMAGE_SIDECAR_MISSING",
            Self::ImageSummaryMissing => "IMAGE_SUMMARY_MISSING",
            Self::ImageTaskReportMissing => "IMAGE_TASK_REPORT_MISSING",
            Self::ImageVisualDescriptionMissing => "IMAGE_VISUAL_DESCRIPTION_MISSING",
            Self::ImageChecksumMissing => "IMAGE_CHECKSUM_MISSING",
            Self::ImageContentTypeInvalid => "IMAGE_CONTENT_TYPE_INVALID",
            Self::ImageMediaTypeMismatch => "IMAGE_MEDIA_TYPE_MISMATCH",
            Self::ImageFileEmptyOrTooSmall => "IMAGE_FILE_EMPTY_OR_TOO_SMALL",
            Self::ImageDimensionsTooSmall => "IMAGE_DIMENSIONS_TOO_SMALL",
            Self::ImageCorruptOrUnreadable => "IMAGE_CORRUPT_OR_UNREADABLE",
            Self::ImageJobOwnershipMismatch => "IMAGE_JOB_OWNERSHIP_MISMATCH",
            Self::ImageMetadataOnlyResult => "IMAGE_METADATA_ONLY_RESULT",
            Self::ImageProhibitedSource => "IMAGE_PROHIBITED_SOURCE",
            Self::ImageNegativeScopeContradiction => "IMAGE_NEGATIVE_SCOPE_CONTRADICTION",
            Self::ImageFixtureNotProduction => "IMAGE_FIXTURE_NOT_PRODUCTION",
            Self::ImageDimensionsRef => "IMAGE_DIMENSIONS",
            Self::ImageFileSizeRef => "IMAGE_FILE_SIZE",
            Self::ImageContentTypeRef => "IMAGE_CONTENT_TYPE",
            Self::ImageSourceAuthorityRef => "IMAGE_SOURCE_AUTHORITY",
            Self::ImageLicenseHintRef => "IMAGE_LICENSE_HINT",
            Self::ImageVisualDescriptionConfidence => "IMAGE_VISUAL_DESCRIPTION_CONFIDENCE",
            Self::ImageSourceDiversity => "IMAGE_SOURCE_DIVERSITY",
            Self::ImageFetchTraceQuality => "IMAGE_FETCH_TRACE_QUALITY",
        };
        write!(f, "{}", s)
    }
}

// ---------------------------------------------------------------------------
// Metric fact — shared quality evidence DTO
// ---------------------------------------------------------------------------

/// A single metric fact produced during candidate or image quality evaluation.
///
/// Rules per the detailed design:
/// - A non-empty blocking metric list means the subject fails mechanical validation.
/// - Reference metrics never accept or reject by themselves.
/// - Every fact must be package-safe after redaction.
/// - Every fact must carry query_plan_id and subject_id.
/// - Evidence refs point to package-safe paths, never secret-bearing data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricFact {
    /// Unique identifier for this fact within the quality evaluation.
    pub metric_id: String,

    /// Which quality phase produced this fact.
    pub phase: QualityPhase,

    /// Blocking vs reference classification.
    pub class: MetricClass,

    /// Machine-readable metric code.
    pub code: QualityMetricCode,

    /// Severity of this fact.
    pub severity: QualitySeverity,

    /// Subject identifier (candidate_id for candidate phase, retrieval_job_id for image phase).
    pub subject_id: String,

    /// Query plan that originated the evaluation.
    pub query_plan_id: String,

    /// Measured value, if applicable.
    pub value: Option<String>,

    /// Threshold that was compared against, if applicable.
    pub threshold: Option<String>,

    /// References to package-safe evidence paths.
    pub evidence_refs: Vec<String>,

    /// Human-readable message (redacted, no credentials).
    pub message: String,

    /// Whether redaction was applied to this fact.
    pub redacted: bool,
}

impl MetricFact {
    /// Create a blocking metric fact for the candidate phase.
    pub fn candidate_blocking(
        code: QualityMetricCode,
        subject_id: impl Into<String>,
        query_plan_id: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            metric_id: format!("mf-{}", uuid::Uuid::new_v4()),
            phase: QualityPhase::Candidate,
            class: MetricClass::Blocking,
            code,
            severity: QualitySeverity::Error,
            subject_id: subject_id.into(),
            query_plan_id: query_plan_id.into(),
            value: None,
            threshold: None,
            evidence_refs: Vec::new(),
            message: message.into(),
            redacted: false,
        }
    }

    /// Create a reference metric fact for the candidate phase.
    pub fn candidate_reference(
        code: QualityMetricCode,
        subject_id: impl Into<String>,
        query_plan_id: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            metric_id: format!("mf-{}", uuid::Uuid::new_v4()),
            phase: QualityPhase::Candidate,
            class: MetricClass::Reference,
            code,
            severity: QualitySeverity::Info,
            subject_id: subject_id.into(),
            query_plan_id: query_plan_id.into(),
            value: None,
            threshold: None,
            evidence_refs: Vec::new(),
            message: message.into(),
            redacted: false,
        }
    }

    /// Create a blocking metric fact for the image phase.
    pub fn image_blocking(
        code: QualityMetricCode,
        subject_id: impl Into<String>,
        query_plan_id: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            metric_id: format!("mf-{}", uuid::Uuid::new_v4()),
            phase: QualityPhase::Image,
            class: MetricClass::Blocking,
            code,
            severity: QualitySeverity::Error,
            subject_id: subject_id.into(),
            query_plan_id: query_plan_id.into(),
            value: None,
            threshold: None,
            evidence_refs: Vec::new(),
            message: message.into(),
            redacted: false,
        }
    }

    /// Create a reference metric fact for the image phase.
    pub fn image_reference(
        code: QualityMetricCode,
        subject_id: impl Into<String>,
        query_plan_id: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            metric_id: format!("mf-{}", uuid::Uuid::new_v4()),
            phase: QualityPhase::Image,
            class: MetricClass::Reference,
            code,
            severity: QualitySeverity::Info,
            subject_id: subject_id.into(),
            query_plan_id: query_plan_id.into(),
            value: None,
            threshold: None,
            evidence_refs: Vec::new(),
            message: message.into(),
            redacted: false,
        }
    }

    pub fn with_value(mut self, value: impl Into<String>) -> Self {
        self.value = Some(value.into());
        self
    }

    pub fn with_threshold(mut self, threshold: impl Into<String>) -> Self {
        self.threshold = Some(threshold.into());
        self
    }

    pub fn with_evidence(mut self, refs: Vec<String>) -> Self {
        self.evidence_refs = refs;
        self
    }

    pub fn with_redacted(mut self) -> Self {
        self.redacted = true;
        self
    }
}

// =============================================================================
// Quality diagnostic codes
// =============================================================================

/// Machine-readable diagnostic codes for quality evaluation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum QualityDiagnosticCode {
    /// Candidate failed a deterministic blocking metric.
    #[serde(rename = "QUALITY_CANDIDATE_MECHANICAL_BLOCK")]
    QualityCandidateMechanicalBlock,
    /// Retrieved image failed deterministic artifact or image checks.
    #[serde(rename = "QUALITY_IMAGE_MECHANICAL_BLOCK")]
    QualityImageMechanicalBlock,
    /// Production VLM cannot run.
    #[serde(rename = "QUALITY_VLM_EVALUATION_UNAVAILABLE")]
    QualityVlmEvaluationUnavailable,
    /// Fixture evaluator attempted in production.
    #[serde(rename = "QUALITY_VLM_EVALUATION_FIXTURE_NOT_PRODUCTION")]
    QualityVlmEvaluationFixtureNotProduction,
    /// VLM request timed out.
    #[serde(rename = "QUALITY_VLM_EVALUATION_TIMEOUT")]
    QualityVlmEvaluationTimeout,
    /// VLM response has invalid schema/cardinality/subject IDs.
    #[serde(rename = "QUALITY_VLM_EVALUATION_RESPONSE_INVALID")]
    QualityVlmEvaluationResponseInvalid,
    /// VLM rejected subject.
    #[serde(rename = "QUALITY_SUBJECTIVE_REJECTED")]
    QualitySubjectiveRejected,
    /// VLM could not approve subject.
    #[serde(rename = "QUALITY_SUBJECTIVE_UNCERTAIN")]
    QualitySubjectiveUncertain,
    /// Image phase required artifact evidence is missing.
    #[serde(rename = "QUALITY_ARTIFACT_EVIDENCE_MISSING")]
    QualityArtifactEvidenceMissing,
    /// Candidate/query/retrieval ownership mismatch.
    #[serde(rename = "QUALITY_OWNERSHIP_MISMATCH")]
    QualityOwnershipMismatch,
    /// Negative scope contradiction blocks candidate or image.
    #[serde(rename = "QUALITY_NEGATIVE_SCOPE_BLOCK")]
    QualityNegativeScopeBlock,
    /// Redaction changed request/response/diagnostic text.
    #[serde(rename = "QUALITY_SENSITIVE_DATA_REDACTED")]
    QualitySensitiveDataRedacted,
}

impl std::fmt::Display for QualityDiagnosticCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::QualityCandidateMechanicalBlock => "QUALITY_CANDIDATE_MECHANICAL_BLOCK",
            Self::QualityImageMechanicalBlock => "QUALITY_IMAGE_MECHANICAL_BLOCK",
            Self::QualityVlmEvaluationUnavailable => "QUALITY_VLM_EVALUATION_UNAVAILABLE",
            Self::QualityVlmEvaluationFixtureNotProduction => {
                "QUALITY_VLM_EVALUATION_FIXTURE_NOT_PRODUCTION"
            }
            Self::QualityVlmEvaluationTimeout => "QUALITY_VLM_EVALUATION_TIMEOUT",
            Self::QualityVlmEvaluationResponseInvalid => "QUALITY_VLM_EVALUATION_RESPONSE_INVALID",
            Self::QualitySubjectiveRejected => "QUALITY_SUBJECTIVE_REJECTED",
            Self::QualitySubjectiveUncertain => "QUALITY_SUBJECTIVE_UNCERTAIN",
            Self::QualityArtifactEvidenceMissing => "QUALITY_ARTIFACT_EVIDENCE_MISSING",
            Self::QualityOwnershipMismatch => "QUALITY_OWNERSHIP_MISMATCH",
            Self::QualityNegativeScopeBlock => "QUALITY_NEGATIVE_SCOPE_BLOCK",
            Self::QualitySensitiveDataRedacted => "QUALITY_SENSITIVE_DATA_REDACTED",
        };
        write!(f, "{}", s)
    }
}

// ---------------------------------------------------------------------------
// Quality diagnostic
// ---------------------------------------------------------------------------

/// A diagnostic produced during candidate or image quality evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityDiagnostic {
    /// Machine-readable diagnostic code.
    pub code: QualityDiagnosticCode,

    /// Severity of this diagnostic.
    pub severity: QualitySeverity,

    /// Which quality phase produced this diagnostic.
    pub phase: QualityPhase,

    /// Subject identifier, if applicable.
    pub subject_id: Option<String>,

    /// Query plan that originated the evaluation.
    pub query_plan_id: String,

    /// Human-readable message (redacted, no credentials).
    pub message: String,

    /// Suggested remediation, if applicable.
    pub remediation: Option<String>,

    /// References to package-safe evidence paths.
    pub evidence_refs: Vec<String>,

    /// Whether redaction was applied to this diagnostic.
    pub redacted: bool,
}

impl QualityDiagnostic {
    pub fn new(
        code: QualityDiagnosticCode,
        severity: QualitySeverity,
        phase: QualityPhase,
        query_plan_id: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code,
            severity,
            phase,
            subject_id: None,
            query_plan_id: query_plan_id.into(),
            message: message.into(),
            remediation: None,
            evidence_refs: Vec::new(),
            redacted: false,
        }
    }

    pub fn with_subject(mut self, subject_id: impl Into<String>) -> Self {
        self.subject_id = Some(subject_id.into());
        self
    }

    pub fn with_remediation(mut self, remediation: impl Into<String>) -> Self {
        self.remediation = Some(remediation.into());
        self
    }

    pub fn with_evidence(mut self, refs: Vec<String>) -> Self {
        self.evidence_refs = refs;
        self
    }

    pub fn with_redacted(mut self) -> Self {
        self.redacted = true;
        self
    }
}

// =============================================================================
// Quality summary
// =============================================================================

/// Aggregated quality summary across a phase.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct QualitySummary {
    /// Total subjects evaluated.
    pub total_evaluated: u32,
    /// Number of subjects mechanically blocked.
    pub mechanically_blocked: u32,
    /// Number of subjects submitted to VLM.
    pub vlm_submitted: u32,
    /// Number approved by VLM.
    pub vlm_approved: u32,
    /// Number rejected by VLM.
    pub vlm_rejected: u32,
    /// Number evaluated as uncertain by VLM.
    pub vlm_uncertain: u32,
    /// Number execution-blocked (VLM unavailable).
    pub execution_blocked: u32,
    /// Number that are final-qualified (mechanically passed + VLM approved).
    pub final_qualified: u32,
}

// =============================================================================
// Quality execution block
// =============================================================================

/// A fact recording why execution was blocked during a quality phase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityExecutionBlock {
    /// Which quality phase was affected.
    pub phase: QualityPhase,
    /// The dependency that caused the block.
    pub dependency: String,
    /// Machine-readable failure code.
    pub failure_code: String,
    /// Human-readable reason (redacted).
    pub reason: String,
    /// Whether this block is permanent (requires config change).
    pub is_permanent: bool,
    /// How many subjects were pending when the block occurred.
    pub pending_subject_count: usize,
}

impl QualityExecutionBlock {
    pub fn vlm_unavailable(
        phase: QualityPhase,
        reason: impl Into<String>,
        pending_count: usize,
    ) -> Self {
        Self {
            phase,
            dependency: "Qwen 3.5 VLM".into(),
            failure_code: "VLM_EVALUATION_UNAVAILABLE".into(),
            reason: reason.into(),
            is_permanent: true,
            pending_subject_count: pending_count,
        }
    }

    pub fn vlm_fixture_in_production(phase: QualityPhase, pending_count: usize) -> Self {
        Self {
            phase,
            dependency: "Qwen 3.5 VLM".into(),
            failure_code: "VLM_EVALUATION_FIXTURE_NOT_PRODUCTION".into(),
            reason: "Fixture evaluator cannot satisfy production delivery evidence.".into(),
            is_permanent: true,
            pending_subject_count: pending_count,
        }
    }
}

// =============================================================================
// Quality audit event
// =============================================================================

/// Kinds of quality audit events.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum QualityAuditEventKind {
    /// A subject was mechanically evaluated.
    #[serde(rename = "mechanical_evaluated")]
    MechanicalEvaluated,
    /// A subject was submitted to VLM.
    #[serde(rename = "vlm_submitted")]
    VlmSubmitted,
    /// A VLM decision was received.
    #[serde(rename = "vlm_decision")]
    VlmDecision,
    /// VLM evaluation was unavailable.
    #[serde(rename = "vlm_unavailable")]
    VlmUnavailable,
    /// Fixture mode detected in production context.
    #[serde(rename = "fixture_blocked")]
    FixtureBlocked,
    /// Redaction was applied to output.
    #[serde(rename = "redaction_applied")]
    RedactionApplied,
}

/// An audit-safe event emitted during quality evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityAuditEvent {
    /// Unique event identifier.
    pub event_id: String,
    /// Query plan that originated the evaluation.
    pub query_plan_id: String,
    /// Full attempt count at event time.
    pub full_attempt_count: u8,
    /// Retry count at event time.
    pub retry_count: u8,
    /// Which quality phase produced this event.
    pub phase: QualityPhase,
    /// Subject identifier, if applicable.
    pub subject_id: Option<String>,
    /// Kind of event.
    pub event_kind: QualityAuditEventKind,
    /// Associated diagnostic code, if applicable.
    pub diagnostic_code: Option<QualityDiagnosticCode>,
    /// When the event occurred.
    pub timestamp: OffsetDateTime,
    /// Whether redaction was applied.
    pub redacted: bool,
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metric_event_builder() {
        let event = MetricEvent::new(
            MetricKind::CandidateSatisfaction,
            "candidate satisfaction",
            45.0,
        )
        .with_denominator(60.0)
        .with_meta("provider", "fixture-provider");

        assert_eq!(event.kind, MetricKind::CandidateSatisfaction);
        assert_eq!(event.value, 45.0);
        assert_eq!(event.denominator, Some(60.0));
        assert_eq!(event.metadata.len(), 1);
    }

    // -----------------------------------------------------------------------
    // MetricFact
    // -----------------------------------------------------------------------

    #[test]
    fn metric_fact_candidate_blocking() {
        let fact = MetricFact::candidate_blocking(
            QualityMetricCode::CandidateImageUrlMissing,
            "cand-1",
            "qp-1",
            "image URL is missing",
        );
        assert_eq!(fact.phase, QualityPhase::Candidate);
        assert_eq!(fact.class, MetricClass::Blocking);
        assert_eq!(fact.severity, QualitySeverity::Error);
        assert_eq!(fact.subject_id, "cand-1");
        assert_eq!(fact.query_plan_id, "qp-1");
        assert!(!fact.metric_id.is_empty());
    }

    #[test]
    fn metric_fact_candidate_reference() {
        let fact = MetricFact::candidate_reference(
            QualityMetricCode::CandidateProviderRank,
            "cand-1",
            "qp-1",
            "provider rank: 3",
        );
        assert_eq!(fact.class, MetricClass::Reference);
        assert_eq!(fact.severity, QualitySeverity::Info);
    }

    #[test]
    fn metric_fact_image_blocking() {
        let fact = MetricFact::image_blocking(
            QualityMetricCode::ImageLocalArtifactMissing,
            "ret-1",
            "qp-1",
            "local artifact missing",
        );
        assert_eq!(fact.phase, QualityPhase::Image);
        assert_eq!(fact.class, MetricClass::Blocking);
    }

    #[test]
    fn metric_fact_image_reference() {
        let fact = MetricFact::image_reference(
            QualityMetricCode::ImageDimensionsRef,
            "ret-1",
            "qp-1",
            "dimensions: 800x600",
        );
        assert_eq!(fact.class, MetricClass::Reference);
    }

    #[test]
    fn metric_fact_with_value_and_threshold() {
        let fact = MetricFact::candidate_blocking(
            QualityMetricCode::CandidateBelowAbsoluteDimensions,
            "cand-1",
            "qp-1",
            "dimensions below minimum",
        )
        .with_value("1x1")
        .with_threshold("2x2");
        assert_eq!(fact.value, Some("1x1".into()));
        assert_eq!(fact.threshold, Some("2x2".into()));
    }

    #[test]
    fn metric_fact_with_redacted() {
        let fact = MetricFact::candidate_reference(
            QualityMetricCode::CandidateLicenseHint,
            "cand-1",
            "qp-1",
            "license: CC BY 2.0",
        )
        .with_redacted();
        assert!(fact.redacted);
    }

    // -----------------------------------------------------------------------
    // QualityDiagnostic
    // -----------------------------------------------------------------------

    #[test]
    fn quality_diagnostic_builder() {
        let diag = QualityDiagnostic::new(
            QualityDiagnosticCode::QualityVlmEvaluationUnavailable,
            QualitySeverity::Blocker,
            QualityPhase::Candidate,
            "qp-1",
            "Qwen VLM is not available in production",
        )
        .with_subject("cand-1")
        .with_remediation("Configure QWEN_API_TOKEN and enable VLM evaluation.");

        assert_eq!(
            diag.code,
            QualityDiagnosticCode::QualityVlmEvaluationUnavailable
        );
        assert_eq!(diag.severity, QualitySeverity::Blocker);
        assert_eq!(diag.subject_id, Some("cand-1".into()));
        assert!(diag.remediation.is_some());
    }

    #[test]
    fn quality_diagnostic_with_redacted() {
        let diag = QualityDiagnostic::new(
            QualityDiagnosticCode::QualitySensitiveDataRedacted,
            QualitySeverity::Warning,
            QualityPhase::Candidate,
            "qp-1",
            "sensitive content detected and redacted",
        )
        .with_redacted();
        assert!(diag.redacted);
    }

    // -----------------------------------------------------------------------
    // QualitySummary
    // -----------------------------------------------------------------------

    #[test]
    fn quality_summary_defaults() {
        let summary = QualitySummary::default();
        assert_eq!(summary.total_evaluated, 0);
        assert_eq!(summary.mechanically_blocked, 0);
        assert_eq!(summary.vlm_submitted, 0);
        assert_eq!(summary.final_qualified, 0);
    }

    // -----------------------------------------------------------------------
    // QualityExecutionBlock
    // -----------------------------------------------------------------------

    #[test]
    fn execution_block_vlm_unavailable() {
        let block = QualityExecutionBlock::vlm_unavailable(
            QualityPhase::Candidate,
            "QWEN_API_TOKEN not set",
            5,
        );
        assert_eq!(block.phase, QualityPhase::Candidate);
        assert_eq!(block.failure_code, "VLM_EVALUATION_UNAVAILABLE");
        assert!(block.is_permanent);
        assert_eq!(block.pending_subject_count, 5);
    }

    #[test]
    fn execution_block_fixture_in_production() {
        let block = QualityExecutionBlock::vlm_fixture_in_production(QualityPhase::Image, 3);
        assert_eq!(block.failure_code, "VLM_EVALUATION_FIXTURE_NOT_PRODUCTION");
        assert_eq!(block.pending_subject_count, 3);
    }

    // -----------------------------------------------------------------------
    // QualityAuditEvent
    // -----------------------------------------------------------------------

    #[test]
    fn audit_event_construction() {
        let event = QualityAuditEvent {
            event_id: "ev-1".into(),
            query_plan_id: "qp-1".into(),
            full_attempt_count: 1,
            retry_count: 0,
            phase: QualityPhase::Candidate,
            subject_id: Some("cand-1".into()),
            event_kind: QualityAuditEventKind::MechanicalEvaluated,
            diagnostic_code: None,
            timestamp: OffsetDateTime::now_utc(),
            redacted: false,
        };
        assert_eq!(event.phase, QualityPhase::Candidate);
        assert_eq!(event.event_kind, QualityAuditEventKind::MechanicalEvaluated);
    }

    // -----------------------------------------------------------------------
    // Display impls
    // -----------------------------------------------------------------------

    #[test]
    fn quality_phase_display() {
        assert_eq!(QualityPhase::Candidate.to_string(), "candidate");
        assert_eq!(QualityPhase::Image.to_string(), "image");
    }

    #[test]
    fn metric_class_display() {
        assert_eq!(MetricClass::Blocking.to_string(), "blocking");
        assert_eq!(MetricClass::Reference.to_string(), "reference");
    }

    #[test]
    fn quality_severity_display() {
        assert_eq!(QualitySeverity::Info.to_string(), "info");
        assert_eq!(QualitySeverity::Warning.to_string(), "warning");
        assert_eq!(QualitySeverity::Error.to_string(), "error");
        assert_eq!(QualitySeverity::Blocker.to_string(), "blocker");
    }

    #[test]
    fn quality_severity_ordering() {
        assert!(QualitySeverity::Info < QualitySeverity::Warning);
        assert!(QualitySeverity::Warning < QualitySeverity::Error);
        assert!(QualitySeverity::Error < QualitySeverity::Blocker);
    }

    #[test]
    fn metric_code_display() {
        assert_eq!(
            QualityMetricCode::CandidateImageUrlMissing.to_string(),
            "CANDIDATE_IMAGE_URL_MISSING"
        );
        assert_eq!(
            QualityMetricCode::ImageLocalArtifactMissing.to_string(),
            "IMAGE_LOCAL_ARTIFACT_MISSING"
        );
        assert_eq!(
            QualityMetricCode::ImageDimensionsRef.to_string(),
            "IMAGE_DIMENSIONS"
        );
    }

    #[test]
    fn diagnostic_code_display() {
        assert_eq!(
            QualityDiagnosticCode::QualityVlmEvaluationUnavailable.to_string(),
            "QUALITY_VLM_EVALUATION_UNAVAILABLE"
        );
        assert_eq!(
            QualityDiagnosticCode::QualityArtifactEvidenceMissing.to_string(),
            "QUALITY_ARTIFACT_EVIDENCE_MISSING"
        );
    }

    // -----------------------------------------------------------------------
    // Serialization round-trip
    // -----------------------------------------------------------------------

    #[test]
    fn metric_fact_serde_roundtrip() {
        let fact = MetricFact::candidate_blocking(
            QualityMetricCode::CandidateImageUrlMissing,
            "cand-1",
            "qp-1",
            "image URL missing",
        );
        let json = serde_json::to_string(&fact).expect("serialize");
        let round: MetricFact = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(round.code, QualityMetricCode::CandidateImageUrlMissing);
        assert_eq!(round.subject_id, "cand-1");
    }

    #[test]
    fn quality_diagnostic_serde_roundtrip() {
        let diag = QualityDiagnostic::new(
            QualityDiagnosticCode::QualityVlmEvaluationUnavailable,
            QualitySeverity::Blocker,
            QualityPhase::Candidate,
            "qp-1",
            "VLM unavailable",
        );
        let json = serde_json::to_string(&diag).expect("serialize");
        let round: QualityDiagnostic = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(
            round.code,
            QualityDiagnosticCode::QualityVlmEvaluationUnavailable
        );
    }

    #[test]
    fn quality_phase_serde() {
        let json = serde_json::to_string(&QualityPhase::Candidate).unwrap();
        assert_eq!(json, "\"candidate\"");
        let round: QualityPhase = serde_json::from_str(&json).unwrap();
        assert_eq!(round, QualityPhase::Candidate);
    }

    #[test]
    fn metric_class_serde() {
        let json = serde_json::to_string(&MetricClass::Blocking).unwrap();
        assert_eq!(json, "\"blocking\"");
    }

    #[test]
    fn quality_severity_serde() {
        let json = serde_json::to_string(&QualitySeverity::Blocker).unwrap();
        assert_eq!(json, "\"blocker\"");
    }
}
