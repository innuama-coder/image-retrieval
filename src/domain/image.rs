//! Image acceptance domain types.
//!
//! Covers image records, mechanical acceptance evidence, and the final
//! image acceptance decision after OpenClaw subjective evaluation.
//!
//! References: PRD §校验与评价产品要求, HLD §Image Acceptance Gate

use serde::{Deserialize, Serialize};

/// A locally-fetched image that is ready for acceptance checks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageRecord {
    /// Candidate id this image was fetched from.
    pub candidate_id: String,

    /// Path to the local image file.
    pub local_path: String,

    /// Content type (e.g. "image/jpeg").
    pub content_type: Option<String>,

    /// File size in bytes.
    pub file_size_bytes: u64,

    /// Actual image dimensions determined by reading the file.
    pub dimensions: Option<super::candidate::ImageDimensions>,
}

// ---------------------------------------------------------------------------
// Mechanical evidence
// ---------------------------------------------------------------------------

/// Evidence produced by mechanical image validation.
///
/// Evidence is split into two classes per the constitution:
/// - **Blocking**: grounds for automatic rejection.
/// - **Reference**: supplementary information for subjective evaluation
///   and risk explanation. Reference evidence alone cannot reject an image
///   unless a product policy explicitly makes it blocking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageMechanicalEvidence {
    /// Blocking findings — any non-empty list means the image is rejected.
    pub blocking_findings: Vec<String>,

    /// Reference findings — fed into OpenClaw evaluation and risk/policy
    /// explanations.
    pub reference_findings: Vec<String>,
}

impl ImageMechanicalEvidence {
    /// Returns `true` when there are no blocking findings.
    pub fn passed_mechanical(&self) -> bool {
        self.blocking_findings.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Image acceptance decision
// ---------------------------------------------------------------------------

/// The final verdict for a single retrieved image after both mechanical
/// acceptance and OpenClaw subjective evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ImageAcceptanceDecision {
    /// Both mechanical and subjective checks passed — the image counts
    /// toward the delivery quota.
    Accepted {
        image: ImageRecord,
        /// Quality/relevance notes for the delivery manifest.
        notes: String,
    },

    /// Mechanical check blocked the image.
    MechanicallyRejected {
        image: ImageRecord,
        evidence: ImageMechanicalEvidence,
    },

    /// Mechanical check passed but OpenClaw subjective evaluation rejected
    /// or was uncertain.
    SubjectivelyRejected {
        image: ImageRecord,
        mechanical_evidence: ImageMechanicalEvidence,
        reason: String,
    },

    /// OpenClaw evaluation could not be performed (production dependency
    /// unavailable). This is a task-level execution block, not a per-image
    /// rejection.
    ExecutionBlocked { reason: String },
}

impl ImageAcceptanceDecision {
    /// Returns `true` iff the image is accepted and qualified for delivery.
    pub fn is_accepted(&self) -> bool {
        matches!(self, Self::Accepted { .. })
    }
}

// ---------------------------------------------------------------------------
// v1.1 Retrieval artifact result placeholder (owned by TASK-004)
// ---------------------------------------------------------------------------

/// Artifact evidence from a retrieval job.
///
/// This is the TASK-004 output consumed by image quality. TASK-004 owns the
/// full definition; TASK-003 uses a referenceable placeholder so the
/// image acceptance API can be defined against a stable shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalArtifactResult {
    /// Retrieval job identifier.
    pub retrieval_job_id: String,
    /// Candidate that was retrieved.
    pub candidate_id: String,
    /// Query plan that originated the retrieval.
    pub query_plan_id: String,
    /// Channel that produced this result.
    pub channel_id: String,
    /// Retrieval status.
    pub retrieval_status: RetrievalStatus,
    /// Path to the local image artifact.
    pub local_artifact_path: Option<String>,
    /// Path to the source artifact.
    pub source_artifact_path: Option<String>,
    /// Path to the source sidecar file.
    pub source_sidecar_path: Option<String>,
    /// Path to the content summary.
    pub content_summary_path: Option<String>,
    /// Path to the task report.
    pub task_report_path: Option<String>,
    /// Path to the visual description.
    pub visual_description_path: Option<String>,
    /// SHA-256 checksum of the local artifact.
    pub checksum_sha256: Option<String>,
    /// Content type (e.g. "image/jpeg").
    pub content_type: Option<String>,
    /// File size in bytes.
    pub file_size_bytes: Option<u64>,
    /// Image dimensions, if determinable.
    pub image_dimensions: Option<super::candidate::ImageDimensions>,
    /// Whether the media type matches the expected type.
    pub media_type_match: bool,
    /// Ordered fetch attempts.
    pub fetch_trace: Vec<String>,
    /// Failure reason if retrieval was not complete.
    pub failure_reason: Option<String>,
}

/// Whether a retrieval job completed successfully.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RetrievalStatus {
    /// Retrieval completed with full artifact evidence.
    Complete,
    /// Retrieval attempted but failed.
    Failed,
    /// Retrieval was blocked by policy.
    Blocked,
    /// Retrieval is still in progress.
    InProgress,
}

impl RetrievalArtifactResult {
    /// Returns `true` if retrieval completed with full artifacts.
    pub fn is_complete(&self) -> bool {
        self.retrieval_status == RetrievalStatus::Complete
    }

    /// Check that all required artifact fields are present.
    pub fn has_all_artifacts(&self) -> bool {
        self.local_artifact_path.is_some()
            && self.source_artifact_path.is_some()
            && self.source_sidecar_path.is_some()
            && self.content_summary_path.is_some()
            && self.task_report_path.is_some()
            && self.visual_description_path.is_some()
            && self.checksum_sha256.is_some()
    }

    /// Check whether this is a metadata-only result (no actual image artifact).
    pub fn is_metadata_only(&self) -> bool {
        self.local_artifact_path.is_none() && self.source_artifact_path.is_none()
    }
}

// ---------------------------------------------------------------------------
// v1.1 Retrieved image evaluation input
// ---------------------------------------------------------------------------

/// Input to the image acceptance API — bundles candidate, quality decision,
/// and retrieval artifact evidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievedImageEvaluationInput {
    /// Query plan that originated this image.
    pub query_plan_id: String,
    /// The candidate that was retrieved.
    pub candidate: super::candidate::CandidateRecord,
    /// Candidate quality decision (must be Retrievable).
    pub candidate_quality_decision: super::candidate::CandidateQualityDecision,
    /// Retrieval artifact result from TASK-004.
    pub retrieval_result: RetrievalArtifactResult,
}

// ---------------------------------------------------------------------------
// v1.1 Image mechanical assessment
// ---------------------------------------------------------------------------

/// Mechanical assessment for a retrieved image (v1.1).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageMechanicalAssessment {
    /// Candidate that was retrieved.
    pub candidate_id: super::candidate::CandidateId,
    /// Retrieval job that produced the image.
    pub retrieval_job_id: String,
    /// Query plan that originated the evaluation.
    pub query_plan_id: String,
    /// Whether mechanical checks passed.
    pub passed: bool,
    /// Blocking metric facts.
    pub blocking_metrics: Vec<crate::domain::metrics::MetricFact>,
    /// Reference metric facts.
    pub reference_metrics: Vec<crate::domain::metrics::MetricFact>,
    /// When assessment was performed (ISO 8601).
    pub evaluated_at: String,
}

impl ImageMechanicalAssessment {
    /// Create a passing mechanical assessment.
    pub fn pass(
        candidate_id: super::candidate::CandidateId,
        retrieval_job_id: impl Into<String>,
        query_plan_id: impl Into<String>,
    ) -> Self {
        Self {
            candidate_id,
            retrieval_job_id: retrieval_job_id.into(),
            query_plan_id: query_plan_id.into(),
            passed: true,
            blocking_metrics: Vec::new(),
            reference_metrics: Vec::new(),
            evaluated_at: String::new(),
        }
    }

    /// Create a blocked mechanical assessment.
    pub fn blocked(
        candidate_id: super::candidate::CandidateId,
        retrieval_job_id: impl Into<String>,
        query_plan_id: impl Into<String>,
        blocking: Vec<crate::domain::metrics::MetricFact>,
    ) -> Self {
        Self {
            candidate_id,
            retrieval_job_id: retrieval_job_id.into(),
            query_plan_id: query_plan_id.into(),
            passed: false,
            blocking_metrics: blocking,
            reference_metrics: Vec::new(),
            evaluated_at: String::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// v1.1 Image VLM evaluation request
// ---------------------------------------------------------------------------

/// A single image subject within a VLM image evaluation request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageEvaluationSubject {
    /// Candidate identifier.
    pub candidate_id: super::candidate::CandidateId,
    /// Retrieval job identifier.
    pub retrieval_job_id: String,
    /// Path to the local artifact for VLM to evaluate.
    pub local_artifact_path: String,
    /// Path to the source artifact.
    pub source_artifact_path: String,
    /// Path to the source sidecar.
    pub source_sidecar_path: String,
    /// Path to the content summary.
    pub content_summary_path: String,
    /// Path to the task report.
    pub task_report_path: String,
    /// Path to the visual description.
    pub visual_description_path: String,
    /// SHA-256 checksum of the local artifact.
    pub checksum_sha256: String,
    /// Mechanical assessment (must be `passed = true` to reach VLM).
    pub mechanical_assessment: ImageMechanicalAssessment,
    /// Reference metrics for VLM context.
    pub reference_metrics: Vec<crate::domain::metrics::MetricFact>,
}

/// Request to evaluate a batch of retrieved images via VLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VlmImageEvaluationRequest {
    /// Unique request identifier.
    pub request_id: String,
    /// Query plan that originated this evaluation.
    pub query_plan_id: String,
    /// Full attempt count from the orchestrator.
    pub full_attempt_count: u8,
    /// Retry count.
    pub retry_count: u8,
    /// Semantic description of what is sought.
    pub semantic_description: String,
    /// Quality tier.
    pub quality: crate::domain::query_plan::QualityTier,
    /// Structured quality requirements.
    pub quality_requirements: crate::domain::query_plan::QualityRequirements,
    /// Visual requirements from the QueryPlan.
    pub visual_requirements: Vec<String>,
    /// Negative scope from the QueryPlan.
    pub negative_scope: Vec<String>,
    /// Images to evaluate (must all have passed mechanical).
    pub images: Vec<ImageEvaluationSubject>,
    /// Policy context for the evaluator.
    pub policy_context: super::candidate::QualityPolicyContext,
    /// Model identifier.
    pub model: String,
    /// Evaluator provider identifier.
    pub evaluator_provider_id: String,
    /// Whether fixture mode is active.
    pub fixture_mode: bool,
}

// ---------------------------------------------------------------------------
// v1.1 Image acceptance decision and outcome
// ---------------------------------------------------------------------------

/// Final status for a retrieved image after mechanical + VLM evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImageAcceptanceStatus {
    /// Image passed both mechanical and VLM gates — qualified for delivery.
    #[serde(rename = "delivered_qualified")]
    DeliveredQualified,
    /// Image was mechanically blocked.
    #[serde(rename = "mechanically_rejected")]
    MechanicallyRejected,
    /// Image was rejected by VLM subjective evaluation.
    #[serde(rename = "subjectively_rejected")]
    SubjectivelyRejected,
    /// VLM was uncertain about the image.
    #[serde(rename = "subjectively_uncertain")]
    SubjectivelyUncertain,
    /// VLM was unavailable — execution blocked.
    #[serde(rename = "execution_blocked")]
    ExecutionBlocked,
}

/// References to the artifact files for a delivered image.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImageArtifactRefs {
    /// Path to the local artifact.
    pub local_artifact_path: Option<String>,
    /// Path to the source artifact.
    pub source_artifact_path: Option<String>,
    /// Path to the source sidecar.
    pub source_sidecar_path: Option<String>,
    /// Path to the content summary.
    pub content_summary_path: Option<String>,
    /// Path to the task report.
    pub task_report_path: Option<String>,
    /// Path to the visual description.
    pub visual_description_path: Option<String>,
    /// SHA-256 checksum.
    pub checksum_sha256: Option<String>,
}

/// Decision for a retrieved image after full quality evaluation (v1.1).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageAcceptanceDecisionV11 {
    /// Candidate identifier.
    pub candidate_id: super::candidate::CandidateId,
    /// Retrieval job identifier.
    pub retrieval_job_id: String,
    /// Query plan that originated the evaluation.
    pub query_plan_id: String,
    /// Whether mechanical checks passed.
    pub mechanical_passed: bool,
    /// Whether VLM approved.
    pub vlm_passed: bool,
    /// Whether the image is qualified for delivery.
    pub delivered_qualified: bool,
    /// Final status.
    pub final_status: ImageAcceptanceStatus,
    /// Blocking metric facts.
    pub blocking_metrics: Vec<crate::domain::metrics::MetricFact>,
    /// Reference metric facts.
    pub reference_metrics: Vec<crate::domain::metrics::MetricFact>,
    /// VLM decision, if VLM was reached.
    pub vlm_decision: Option<super::candidate::VlmSubjectDecision>,
    /// Artifact references for the delivered image.
    pub artifact_refs: ImageArtifactRefs,
    /// Diagnostics from the evaluation.
    pub diagnostics: Vec<crate::domain::metrics::QualityDiagnostic>,
}

impl ImageAcceptanceDecisionV11 {
    /// Returns `true` iff the image is qualified for delivery.
    pub fn is_delivered_qualified(&self) -> bool {
        self.delivered_qualified
    }

    /// Build a decision for a mechanically rejected image.
    pub fn mechanically_rejected(
        candidate_id: super::candidate::CandidateId,
        retrieval_job_id: impl Into<String>,
        query_plan_id: impl Into<String>,
        blocking: Vec<crate::domain::metrics::MetricFact>,
    ) -> Self {
        Self {
            candidate_id,
            retrieval_job_id: retrieval_job_id.into(),
            query_plan_id: query_plan_id.into(),
            mechanical_passed: false,
            vlm_passed: false,
            delivered_qualified: false,
            final_status: ImageAcceptanceStatus::MechanicallyRejected,
            blocking_metrics: blocking,
            reference_metrics: Vec::new(),
            vlm_decision: None,
            artifact_refs: ImageArtifactRefs::default(),
            diagnostics: Vec::new(),
        }
    }

    /// Build a decision from merged mechanical + VLM results.
    pub fn merged(
        candidate_id: super::candidate::CandidateId,
        retrieval_job_id: impl Into<String>,
        query_plan_id: impl Into<String>,
        mechanical: &ImageMechanicalAssessment,
        vlm: Option<&super::candidate::VlmSubjectDecision>,
        artifact_refs: ImageArtifactRefs,
    ) -> Self {
        let vlm_passed = vlm
            .map(|d| d.decision == super::candidate::VlmSubjectDecisionKind::Approve)
            .unwrap_or(false);
        let delivered_qualified = mechanical.passed && vlm_passed;
        let final_status = match vlm {
            None => ImageAcceptanceStatus::ExecutionBlocked,
            Some(d) => match d.decision {
                super::candidate::VlmSubjectDecisionKind::Approve => {
                    if mechanical.passed {
                        ImageAcceptanceStatus::DeliveredQualified
                    } else {
                        ImageAcceptanceStatus::MechanicallyRejected
                    }
                }
                super::candidate::VlmSubjectDecisionKind::Reject => {
                    ImageAcceptanceStatus::SubjectivelyRejected
                }
                super::candidate::VlmSubjectDecisionKind::Uncertain => {
                    ImageAcceptanceStatus::SubjectivelyUncertain
                }
                super::candidate::VlmSubjectDecisionKind::Unexecutable => {
                    ImageAcceptanceStatus::ExecutionBlocked
                }
            },
        };
        Self {
            candidate_id,
            retrieval_job_id: retrieval_job_id.into(),
            query_plan_id: query_plan_id.into(),
            mechanical_passed: mechanical.passed,
            vlm_passed,
            delivered_qualified,
            final_status,
            blocking_metrics: mechanical.blocking_metrics.clone(),
            reference_metrics: mechanical.reference_metrics.clone(),
            vlm_decision: vlm.cloned(),
            artifact_refs,
            diagnostics: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Image acceptance outcome — handoff to TASK-005
// ---------------------------------------------------------------------------

/// Full outcome of the image acceptance phase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageAcceptanceOutcome {
    /// Query plan that was evaluated.
    pub query_plan_id: String,
    /// Full attempt count from the orchestrator.
    pub full_attempt_count: u8,
    /// Retry count.
    pub retry_count: u8,
    /// Per-image acceptance decisions.
    pub decisions: Vec<ImageAcceptanceDecisionV11>,
    /// Images that are qualified for delivery.
    pub accepted_images: Vec<ImageAcceptanceDecisionV11>,
    /// Execution blocking facts if any.
    pub execution_blocking_facts: Vec<crate::domain::metrics::QualityExecutionBlock>,
    /// Diagnostics from the evaluation.
    pub diagnostics: Vec<crate::domain::metrics::QualityDiagnostic>,
    /// Aggregate summary.
    pub summary: crate::domain::metrics::QualitySummary,
}

impl ImageAcceptanceOutcome {
    /// Build an outcome from decisions.
    pub fn new(
        query_plan_id: impl Into<String>,
        full_attempt_count: u8,
        retry_count: u8,
        decisions: Vec<ImageAcceptanceDecisionV11>,
        execution_blocking_facts: Vec<crate::domain::metrics::QualityExecutionBlock>,
        diagnostics: Vec<crate::domain::metrics::QualityDiagnostic>,
        summary: crate::domain::metrics::QualitySummary,
    ) -> Self {
        let accepted_images: Vec<ImageAcceptanceDecisionV11> = decisions
            .iter()
            .filter(|d| d.is_delivered_qualified())
            .cloned()
            .collect();
        Self {
            query_plan_id: query_plan_id.into(),
            full_attempt_count,
            retry_count,
            decisions,
            accepted_images,
            execution_blocking_facts,
            diagnostics,
            summary,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::candidate::{CandidateId, ImageDimensions};

    fn make_image() -> ImageRecord {
        ImageRecord {
            candidate_id: CandidateId::new("c1").to_string(),
            local_path: "/tmp/test.jpg".into(),
            content_type: Some("image/jpeg".into()),
            file_size_bytes: 1024,
            dimensions: Some(ImageDimensions {
                width: 800,
                height: 600,
            }),
        }
    }

    #[test]
    fn mechanical_evidence_passes_when_no_blocking() {
        let evidence = ImageMechanicalEvidence {
            blocking_findings: vec![],
            reference_findings: vec!["low resolution".into()],
        };
        assert!(evidence.passed_mechanical());
    }

    #[test]
    fn mechanical_evidence_fails_on_blocking() {
        let evidence = ImageMechanicalEvidence {
            blocking_findings: vec!["file corrupted".into()],
            reference_findings: vec![],
        };
        assert!(!evidence.passed_mechanical());
    }

    #[test]
    fn accepted_decision_is_accepted() {
        let d = ImageAcceptanceDecision::Accepted {
            image: make_image(),
            notes: "good match".into(),
        };
        assert!(d.is_accepted());
    }

    #[test]
    fn rejected_decisions_are_not_accepted() {
        let img = make_image();
        assert!(!ImageAcceptanceDecision::MechanicallyRejected {
            image: img.clone(),
            evidence: ImageMechanicalEvidence {
                blocking_findings: vec!["corrupt".into()],
                reference_findings: vec![],
            },
        }
        .is_accepted());

        assert!(!ImageAcceptanceDecision::SubjectivelyRejected {
            image: img.clone(),
            mechanical_evidence: ImageMechanicalEvidence {
                blocking_findings: vec![],
                reference_findings: vec![],
            },
            reason: "does not match description".into(),
        }
        .is_accepted());

        assert!(!ImageAcceptanceDecision::ExecutionBlocked {
            reason: "OpenClaw unavailable".into(),
        }
        .is_accepted());
    }

    // -----------------------------------------------------------------------
    // v1.1 RetrievalArtifactResult tests
    // -----------------------------------------------------------------------

    #[test]
    fn retrieval_artifact_is_complete() {
        let result = RetrievalArtifactResult {
            retrieval_job_id: "ret-1".into(),
            candidate_id: "cand-1".into(),
            query_plan_id: "qp-1".into(),
            channel_id: "web_fetch".into(),
            retrieval_status: RetrievalStatus::Complete,
            local_artifact_path: Some("/tmp/img.jpg".into()),
            source_artifact_path: Some("/tmp/src.jpg".into()),
            source_sidecar_path: Some("/tmp/sidecar.json".into()),
            content_summary_path: Some("/tmp/summary.txt".into()),
            task_report_path: Some("/tmp/report.json".into()),
            visual_description_path: Some("/tmp/vd.txt".into()),
            checksum_sha256: Some("abc123".into()),
            content_type: Some("image/jpeg".into()),
            file_size_bytes: Some(4096),
            image_dimensions: None,
            media_type_match: true,
            fetch_trace: vec!["direct_fetch".into()],
            failure_reason: None,
        };
        assert!(result.is_complete());
        assert!(result.has_all_artifacts());
        assert!(!result.is_metadata_only());
    }

    #[test]
    fn retrieval_artifact_missing_fields_detected() {
        let result = RetrievalArtifactResult {
            retrieval_job_id: "ret-1".into(),
            candidate_id: "cand-1".into(),
            query_plan_id: "qp-1".into(),
            channel_id: "web_fetch".into(),
            retrieval_status: RetrievalStatus::Failed,
            local_artifact_path: None,
            source_artifact_path: None,
            source_sidecar_path: None,
            content_summary_path: None,
            task_report_path: None,
            visual_description_path: None,
            checksum_sha256: None,
            content_type: None,
            file_size_bytes: None,
            image_dimensions: None,
            media_type_match: false,
            fetch_trace: vec![],
            failure_reason: Some("network error".into()),
        };
        assert!(!result.is_complete());
        assert!(!result.has_all_artifacts());
        assert!(result.is_metadata_only());
    }

    // -----------------------------------------------------------------------
    // v1.1 ImageMechanicalAssessment tests
    // -----------------------------------------------------------------------

    #[test]
    fn image_mechanical_assessment_pass() {
        let assessment = ImageMechanicalAssessment::pass(
            crate::domain::candidate::CandidateId::new("cand-1"),
            "ret-1",
            "qp-1",
        );
        assert!(assessment.passed);
        assert!(assessment.blocking_metrics.is_empty());
    }

    #[test]
    fn image_mechanical_assessment_blocked() {
        use crate::domain::metrics::{MetricFact, QualityMetricCode};
        let fact = MetricFact::image_blocking(
            QualityMetricCode::ImageLocalArtifactMissing,
            "ret-1",
            "qp-1",
            "local artifact missing",
        );
        let assessment = ImageMechanicalAssessment::blocked(
            crate::domain::candidate::CandidateId::new("cand-1"),
            "ret-1",
            "qp-1",
            vec![fact],
        );
        assert!(!assessment.passed);
        assert_eq!(assessment.blocking_metrics.len(), 1);
    }

    // -----------------------------------------------------------------------
    // v1.1 ImageAcceptanceDecisionV11 tests
    // -----------------------------------------------------------------------

    #[test]
    fn image_decision_delivered_qualified() {
        use crate::domain::candidate::{CandidateId, VlmSubjectDecision, VlmSubjectDecisionKind};

        let mechanical =
            ImageMechanicalAssessment::pass(CandidateId::new("cand-1"), "ret-1", "qp-1");
        let vlm = VlmSubjectDecision {
            subject_id: "cand-1".into(),
            decision: VlmSubjectDecisionKind::Approve,
            confidence: Some(0.95),
            reason_codes: vec!["match".into()],
            rationale_summary: "good match".into(),
            evidence_refs: vec![],
        };
        let decision = ImageAcceptanceDecisionV11::merged(
            CandidateId::new("cand-1"),
            "ret-1",
            "qp-1",
            &mechanical,
            Some(&vlm),
            ImageArtifactRefs::default(),
        );
        assert!(decision.is_delivered_qualified());
        assert_eq!(
            decision.final_status,
            ImageAcceptanceStatus::DeliveredQualified
        );
    }

    #[test]
    fn image_decision_mechanically_rejected() {
        use crate::domain::candidate::CandidateId;
        use crate::domain::metrics::{MetricFact, QualityMetricCode};

        let fact = MetricFact::image_blocking(
            QualityMetricCode::ImageChecksumMissing,
            "ret-1",
            "qp-1",
            "checksum missing",
        );
        let decision = ImageAcceptanceDecisionV11::mechanically_rejected(
            CandidateId::new("cand-1"),
            "ret-1",
            "qp-1",
            vec![fact],
        );
        assert!(!decision.is_delivered_qualified());
        assert_eq!(
            decision.final_status,
            ImageAcceptanceStatus::MechanicallyRejected
        );
    }

    #[test]
    fn image_decision_vlm_rejected() {
        use crate::domain::candidate::{CandidateId, VlmSubjectDecision, VlmSubjectDecisionKind};

        let mechanical =
            ImageMechanicalAssessment::pass(CandidateId::new("cand-1"), "ret-1", "qp-1");
        let vlm = VlmSubjectDecision {
            subject_id: "cand-1".into(),
            decision: VlmSubjectDecisionKind::Reject,
            confidence: Some(0.2),
            reason_codes: vec!["mismatch".into()],
            rationale_summary: "not matching query".into(),
            evidence_refs: vec![],
        };
        let decision = ImageAcceptanceDecisionV11::merged(
            CandidateId::new("cand-1"),
            "ret-1",
            "qp-1",
            &mechanical,
            Some(&vlm),
            ImageArtifactRefs::default(),
        );
        assert!(!decision.is_delivered_qualified());
        assert_eq!(
            decision.final_status,
            ImageAcceptanceStatus::SubjectivelyRejected
        );
    }

    #[test]
    fn image_decision_execution_blocked() {
        use crate::domain::candidate::CandidateId;

        let mechanical =
            ImageMechanicalAssessment::pass(CandidateId::new("cand-1"), "ret-1", "qp-1");
        let decision = ImageAcceptanceDecisionV11::merged(
            CandidateId::new("cand-1"),
            "ret-1",
            "qp-1",
            &mechanical,
            None, // VLM unavailable
            ImageArtifactRefs::default(),
        );
        assert!(!decision.is_delivered_qualified());
        assert_eq!(
            decision.final_status,
            ImageAcceptanceStatus::ExecutionBlocked
        );
    }

    // -----------------------------------------------------------------------
    // v1.1 ImageAcceptanceOutcome tests
    // -----------------------------------------------------------------------

    #[test]
    fn image_acceptance_outcome_filters_accepted() {
        use crate::domain::candidate::{CandidateId, VlmSubjectDecision, VlmSubjectDecisionKind};
        use crate::domain::metrics::QualitySummary;

        let mech1 = ImageMechanicalAssessment::pass(CandidateId::new("cand-1"), "ret-1", "qp-1");
        let mech2 = ImageMechanicalAssessment::pass(CandidateId::new("cand-2"), "ret-2", "qp-1");
        let vlm_approve = VlmSubjectDecision {
            subject_id: "cand-1".into(),
            decision: VlmSubjectDecisionKind::Approve,
            confidence: Some(0.9),
            reason_codes: vec![],
            rationale_summary: "good".into(),
            evidence_refs: vec![],
        };
        let vlm_reject = VlmSubjectDecision {
            subject_id: "cand-2".into(),
            decision: VlmSubjectDecisionKind::Reject,
            confidence: Some(0.1),
            reason_codes: vec![],
            rationale_summary: "bad".into(),
            evidence_refs: vec![],
        };

        let d1 = ImageAcceptanceDecisionV11::merged(
            CandidateId::new("cand-1"),
            "ret-1",
            "qp-1",
            &mech1,
            Some(&vlm_approve),
            ImageArtifactRefs::default(),
        );
        let d2 = ImageAcceptanceDecisionV11::merged(
            CandidateId::new("cand-2"),
            "ret-2",
            "qp-1",
            &mech2,
            Some(&vlm_reject),
            ImageArtifactRefs::default(),
        );

        let decisions = vec![d1, d2];
        let outcome = ImageAcceptanceOutcome::new(
            "qp-1",
            1,
            0,
            decisions,
            vec![],
            vec![],
            QualitySummary::default(),
        );
        assert_eq!(outcome.accepted_images.len(), 1);
        assert_eq!(outcome.decisions.len(), 2);
    }

    #[test]
    fn image_artifact_refs_default() {
        let refs = ImageArtifactRefs::default();
        assert!(refs.local_artifact_path.is_none());
        assert!(refs.checksum_sha256.is_none());
    }
}
