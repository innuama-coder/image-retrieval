//! Delivery domain types — v1.1 canonical package contract.
//!
//! Covers task result status, delivery decisions, package status, pipeline
//! stages, attempt records, run state, coverage gaps, delivered image records,
//! and workflow diagnostics as defined by the v1.1 LLD and TASK-005 detailed
//! design.
//!
//! References: PRD §交付物产品设计, HLD §Delivery Package Builder,
//! LLD §Package Files, TASK-005 design §Interfaces Types And DTO Contracts

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Package status — v1.1 canonical delivery verdict
// ---------------------------------------------------------------------------

/// The terminal status of a delivery package.
///
/// Distinct from `TaskStatus` (legacy MVP). This enum defines the v1.1
/// package-level verdict per the LLD: passed, partial, or blocked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PackageStatus {
    /// Requested count reached and package validation passes.
    #[serde(rename = "passed")]
    Passed,
    /// At least one accepted image exists, requested count remains unmet,
    /// retry limit is exhausted, and package validation passes for the
    /// limited delivery package.
    #[serde(rename = "partial")]
    Partial,
    /// No accepted artifact-backed image exists, or a hard dependency,
    /// policy, input, package validation, or validation command prevents
    /// honest delivery.
    #[serde(rename = "blocked")]
    Blocked,
}

impl PackageStatus {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Partial => "partial",
            Self::Blocked => "blocked",
        }
    }

    pub fn is_passed(&self) -> bool {
        matches!(self, Self::Passed)
    }
}

// ---------------------------------------------------------------------------
// Execution mode — controls production vs fixture vs dry-run behaviour
// ---------------------------------------------------------------------------

/// Execution mode for a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionMode {
    /// Fixture evidence cannot pass readiness or delivery.
    #[serde(rename = "production")]
    Production,
    /// Deterministic tests may run; package status must show fixture mode.
    #[serde(rename = "fixture")]
    Fixture,
    /// Admission and readiness only; no search or retrieval calls.
    #[serde(rename = "dry_run")]
    DryRun,
}

// ---------------------------------------------------------------------------
// Pipeline stage — labels every workflow step
// ---------------------------------------------------------------------------

/// Every diagnostic, metric, and manifest event must include the stage where
/// it was produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PipelineStage {
    #[serde(rename = "admission")]
    Admission,
    #[serde(rename = "provider_readiness")]
    ProviderReadiness,
    #[serde(rename = "search")]
    Search,
    #[serde(rename = "candidate_quality")]
    CandidateQuality,
    #[serde(rename = "retrieval_planning")]
    RetrievalPlanning,
    #[serde(rename = "retrieval_execution")]
    RetrievalExecution,
    #[serde(rename = "image_acceptance")]
    ImageAcceptance,
    #[serde(rename = "coverage_check")]
    CoverageCheck,
    #[serde(rename = "package_build")]
    PackageBuild,
    #[serde(rename = "package_validation")]
    PackageValidation,
    #[serde(rename = "review")]
    Review,
    #[serde(rename = "handoff")]
    Handoff,
}

impl PipelineStage {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Admission => "admission",
            Self::ProviderReadiness => "provider_readiness",
            Self::Search => "search",
            Self::CandidateQuality => "candidate_quality",
            Self::RetrievalPlanning => "retrieval_planning",
            Self::RetrievalExecution => "retrieval_execution",
            Self::ImageAcceptance => "image_acceptance",
            Self::CoverageCheck => "coverage_check",
            Self::PackageBuild => "package_build",
            Self::PackageValidation => "package_validation",
            Self::Review => "review",
            Self::Handoff => "handoff",
        }
    }
}

// ---------------------------------------------------------------------------
// Workflow failure codes — required taxonomy
// ---------------------------------------------------------------------------

/// Machine-readable workflow failure codes per the TASK-005 design.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkflowFailureCode {
    #[serde(rename = "WORKFLOW_ADMISSION_REJECTED")]
    AdmissionRejected,
    #[serde(rename = "WORKFLOW_NO_SEARCH_PROVIDER")]
    NoSearchProvider,
    #[serde(rename = "WORKFLOW_SEARCH_SHORTAGE")]
    SearchShortage,
    #[serde(rename = "WORKFLOW_CANDIDATE_QUALITY_BLOCKED")]
    CandidateQualityBlocked,
    #[serde(rename = "WORKFLOW_RETRIEVAL_SHORT_BATCH")]
    RetrievalShortBatch,
    #[serde(rename = "WORKFLOW_RETRIEVAL_ALL_FAILED")]
    RetrievalAllFailed,
    #[serde(rename = "WORKFLOW_IMAGE_ACCEPTANCE_BLOCKED")]
    ImageAcceptanceBlocked,
    #[serde(rename = "WORKFLOW_RETRY_EXHAUSTED")]
    RetryExhausted,
    #[serde(rename = "WORKFLOW_PARTIAL_DELIVERY")]
    PartialDelivery,
    #[serde(rename = "WORKFLOW_BLOCKED_DELIVERY")]
    BlockedDelivery,
    #[serde(rename = "WORKFLOW_PACKAGE_BUILD_FAILED")]
    PackageBuildFailed,
    #[serde(rename = "WORKFLOW_PACKAGE_VALIDATION_FAILED")]
    PackageValidationFailed,
    #[serde(rename = "WORKFLOW_SECRET_REDACTED")]
    SecretRedacted,
    #[serde(rename = "WORKFLOW_RELEASE_GATE_OPEN")]
    ReleaseGateOpen,
}

impl WorkflowFailureCode {
    pub fn code(&self) -> &'static str {
        match self {
            Self::AdmissionRejected => "WORKFLOW_ADMISSION_REJECTED",
            Self::NoSearchProvider => "WORKFLOW_NO_SEARCH_PROVIDER",
            Self::SearchShortage => "WORKFLOW_SEARCH_SHORTAGE",
            Self::CandidateQualityBlocked => "WORKFLOW_CANDIDATE_QUALITY_BLOCKED",
            Self::RetrievalShortBatch => "WORKFLOW_RETRIEVAL_SHORT_BATCH",
            Self::RetrievalAllFailed => "WORKFLOW_RETRIEVAL_ALL_FAILED",
            Self::ImageAcceptanceBlocked => "WORKFLOW_IMAGE_ACCEPTANCE_BLOCKED",
            Self::RetryExhausted => "WORKFLOW_RETRY_EXHAUSTED",
            Self::PartialDelivery => "WORKFLOW_PARTIAL_DELIVERY",
            Self::BlockedDelivery => "WORKFLOW_BLOCKED_DELIVERY",
            Self::PackageBuildFailed => "WORKFLOW_PACKAGE_BUILD_FAILED",
            Self::PackageValidationFailed => "WORKFLOW_PACKAGE_VALIDATION_FAILED",
            Self::SecretRedacted => "WORKFLOW_SECRET_REDACTED",
            Self::ReleaseGateOpen => "WORKFLOW_RELEASE_GATE_OPEN",
        }
    }
}

/// Severity of a workflow diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkflowSeverity {
    #[serde(rename = "info")]
    Info,
    #[serde(rename = "warning")]
    Warning,
    #[serde(rename = "blocker")]
    Blocker,
}

// ---------------------------------------------------------------------------
// Workflow diagnostic
// ---------------------------------------------------------------------------

/// A single workflow diagnostic, emitted at any pipeline stage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowDiagnostic {
    pub code: WorkflowFailureCode,
    pub severity: WorkflowSeverity,
    pub stage: PipelineStage,
    pub query_plan_id: Option<String>,
    pub full_attempt_count: Option<u8>,
    pub retry_count: Option<u8>,
    pub subject_id: Option<String>,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remediation: Option<String>,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
    pub retryable: bool,
    pub redacted: bool,
}

impl WorkflowDiagnostic {
    pub fn blocker(
        code: WorkflowFailureCode,
        stage: PipelineStage,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code,
            severity: WorkflowSeverity::Blocker,
            stage,
            query_plan_id: None,
            full_attempt_count: None,
            retry_count: None,
            subject_id: None,
            message: message.into(),
            remediation: None,
            evidence_refs: Vec::new(),
            retryable: false,
            redacted: false,
        }
    }

    pub fn warning(
        code: WorkflowFailureCode,
        stage: PipelineStage,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code,
            severity: WorkflowSeverity::Warning,
            stage,
            query_plan_id: None,
            full_attempt_count: None,
            retry_count: None,
            subject_id: None,
            message: message.into(),
            remediation: None,
            evidence_refs: Vec::new(),
            retryable: true,
            redacted: false,
        }
    }

    pub fn info(
        code: WorkflowFailureCode,
        stage: PipelineStage,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code,
            severity: WorkflowSeverity::Info,
            stage,
            query_plan_id: None,
            full_attempt_count: None,
            retry_count: None,
            subject_id: None,
            message: message.into(),
            remediation: None,
            evidence_refs: Vec::new(),
            retryable: false,
            redacted: false,
        }
    }

    pub fn with_subject(mut self, id: impl Into<String>) -> Self {
        self.subject_id = Some(id.into());
        self
    }

    pub fn with_attempt(mut self, full: u8, retry: u8) -> Self {
        self.full_attempt_count = Some(full);
        self.retry_count = Some(retry);
        self
    }

    pub fn with_query_plan(mut self, id: impl Into<String>) -> Self {
        self.query_plan_id = Some(id.into());
        self
    }
}

// ---------------------------------------------------------------------------
// Coverage gap types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CoverageGapType {
    #[serde(rename = "search_recall_shortage")]
    SearchRecallShortage,
    #[serde(rename = "candidate_quality_rejected")]
    CandidateQualityRejected,
    #[serde(rename = "candidate_quality_execution_blocked")]
    CandidateQualityExecutionBlocked,
    #[serde(rename = "retrieval_batch_shortage")]
    RetrievalBatchShortage,
    #[serde(rename = "retrieval_failed")]
    RetrievalFailed,
    #[serde(rename = "retrieval_policy_blocked")]
    RetrievalPolicyBlocked,
    #[serde(rename = "image_acceptance_rejected")]
    ImageAcceptanceRejected,
    #[serde(rename = "image_acceptance_execution_blocked")]
    ImageAcceptanceExecutionBlocked,
    #[serde(rename = "package_validation_failed")]
    PackageValidationFailed,
    #[serde(rename = "external_decision_blocked")]
    ExternalDecisionBlocked,
}

/// A single coverage gap record — emitted per attempt or aggregated at end.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverageGap {
    pub gap_id: String,
    pub query_plan_id: String,
    pub full_attempt_count: u8,
    pub retry_count: u8,
    pub gap_type: CoverageGapType,
    pub missing_count: u32,
    pub primary_code: WorkflowFailureCode,
    pub source_stage: PipelineStage,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
    pub retryable: bool,
    pub message: String,
}

// ---------------------------------------------------------------------------
// Run attempt record
// ---------------------------------------------------------------------------

/// A single full-attempt record within the run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunAttemptRecord {
    pub attempt_id: String,
    pub full_attempt_count: u8,
    pub retry_count: u8,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub search_candidate_count: u32,
    pub retrievable_candidate_count: u32,
    pub retrieval_job_count: u32,
    pub retrieval_complete_count: u32,
    pub accepted_delta_count: u32,
    pub gap_delta_count: u32,
    #[serde(default)]
    pub terminal_reason: Option<WorkflowFailureCode>,
    #[serde(default)]
    pub diagnostics: Vec<WorkflowDiagnostic>,
}

// ---------------------------------------------------------------------------
// RunState — orchestrator's full state across attempts
// ---------------------------------------------------------------------------

/// The orchestrator's runtime state across all attempts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunState {
    pub run_id: String,
    pub query_plan_id: String,
    pub status: PackageStatus,
    pub full_attempt_count: u8,
    pub retry_count: u8,
    pub retry_limit: u8,
    pub full_attempt_limit: u8,
    pub required_image_count: u32,
    pub accepted_images: Vec<DeliveredImageRecord>,
    #[serde(default)]
    pub gaps: Vec<CoverageGap>,
    #[serde(default)]
    pub attempts: Vec<RunAttemptRecord>,
    #[serde(default)]
    pub diagnostics: Vec<WorkflowDiagnostic>,
}

impl RunState {
    /// Create a fresh RunState with initial counters.
    pub fn new(
        run_id: String,
        query_plan_id: String,
        required_image_count: u32,
        retry_limit: u8,
    ) -> Self {
        let full_attempt_limit = 1u8.saturating_add(retry_limit);
        Self {
            run_id,
            query_plan_id,
            status: PackageStatus::Blocked,
            full_attempt_count: 1,
            retry_count: 0,
            retry_limit,
            full_attempt_limit,
            required_image_count,
            accepted_images: Vec::new(),
            gaps: Vec::new(),
            attempts: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    pub fn accepted_count(&self) -> u32 {
        self.accepted_images.len() as u32
    }

    pub fn gap_count(&self) -> u32 {
        self.required_image_count
            .saturating_sub(self.accepted_count())
    }

    pub fn can_retry(&self) -> bool {
        self.retry_count < self.retry_limit && self.full_attempt_count < self.full_attempt_limit
    }

    pub fn is_exhausted(&self) -> bool {
        !self.can_retry()
    }

    pub fn record_retry(&mut self) -> bool {
        if !self.can_retry() {
            return false;
        }
        self.retry_count += 1;
        self.full_attempt_count += 1;
        true
    }

    pub fn update_status(&mut self) {
        if self.accepted_count() >= self.required_image_count {
            self.status = PackageStatus::Passed;
        } else if self.is_exhausted() {
            if self.accepted_count() > 0 {
                self.status = PackageStatus::Partial;
            } else {
                self.status = PackageStatus::Blocked;
            }
        }
        // else: still running, keep current status
    }
}

// ---------------------------------------------------------------------------
// Delivered image record — per-image delivery evidence
// ---------------------------------------------------------------------------

/// A single delivered image with full artifact traceability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveredImageRecord {
    pub delivered_image_id: String,
    pub query_plan_id: String,
    pub candidate_id: String,
    pub retrieval_job_id: String,
    pub package_image_path: String,
    pub local_artifact_path: String,
    pub source_artifact_path: String,
    pub source_sidecar_path: String,
    pub content_summary_path: String,
    pub task_report_path: String,
    pub visual_description_path: String,
    pub checksum_sha256: String,
    pub content_type: String,
    pub file_size_bytes: u64,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub candidate_quality_decision_ref: String,
    pub image_acceptance_decision_ref: String,
    pub manifest_entry_ref: String,
    #[serde(default)]
    pub evidence: DeliveredImageEvidence,
}

/// Evidence snapshot preserved for a delivered image.
///
/// This keeps the package builder honest: it serializes evidence captured from
/// the retrieval and quality gates instead of inventing pass/trace fields while
/// writing package JSON.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeliveredImageEvidence {
    pub provider_id: String,
    pub channel_id: String,
    pub channel_tier: String,
    pub retrieval_status: String,
    pub media_type_match: bool,
    #[serde(default)]
    pub fetch_trace: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_reason: Option<serde_json::Value>,
    pub candidate_decision: DeliveredCandidateQualityEvidence,
    pub image_decision: DeliveredImageAcceptanceEvidence,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeliveredCandidateQualityEvidence {
    pub mechanical_passed: bool,
    pub vlm_passed: bool,
    pub final_status: String,
    pub priority: u32,
    #[serde(default)]
    pub blocking_metrics: Vec<serde_json::Value>,
    #[serde(default)]
    pub reference_metrics: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vlm_decision: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeliveredImageAcceptanceEvidence {
    pub mechanical_passed: bool,
    pub vlm_passed: bool,
    pub artifact_complete: bool,
    pub final_status: String,
    #[serde(default)]
    pub blocking_reasons: Vec<String>,
    #[serde(default)]
    pub reference_metrics: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vlm_decision: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Run request and outcome — orchestration entry/exit
// ---------------------------------------------------------------------------

/// Input to a full orchestrator run.
#[derive(Debug, Clone)]
pub struct RunRequest {
    pub query_plan_id: String,
    pub description: String,
    pub required_image_count: u32,
    pub retry_limit: u8,
    pub candidate_target: u32,
    pub retrieval_batch_target: u32,
    pub execution_mode: ExecutionMode,
    pub output_dir: std::path::PathBuf,
    pub run_id: String,
}

/// Output from a full orchestrator run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunOutcome {
    pub run_id: String,
    pub query_plan_id: String,
    pub status: PackageStatus,
    pub full_attempt_count: u8,
    pub retry_count: u8,
    pub required_image_count: u32,
    pub accepted_image_count: u32,
    pub gap_count: u32,
    pub package_dir: Option<String>,
    pub validation_status: Option<String>,
    pub primary_reason: Option<String>,
    #[serde(default)]
    pub diagnostics: Vec<WorkflowDiagnostic>,
}

// ---------------------------------------------------------------------------
// Validation types
// ---------------------------------------------------------------------------

/// Validation status for package and individual checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValidationStatus {
    #[serde(rename = "pass")]
    Pass,
    #[serde(rename = "fail")]
    Fail,
    #[serde(rename = "blocked")]
    Blocked,
}

/// A single package-validation issue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageValidationIssue {
    pub issue_id: String,
    pub code: String,
    pub severity: String,
    pub subject: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual: Option<String>,
}

/// A single file-existence or JSON-validity check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileCheck {
    pub file_name: String,
    pub exists: bool,
    pub valid_json: Option<bool>,
    pub message: String,
}

/// A single artifact-existence check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactCheck {
    pub candidate_id: String,
    pub artifact_type: String,
    pub path: String,
    pub exists: bool,
    pub non_empty: bool,
    pub message: String,
}

/// A single redaction / sensitive-data check result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedactionCheck {
    pub file: String,
    pub passed: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub found_patterns: Vec<String>,
    pub message: String,
}

/// A single counter-invariant check result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CounterCheck {
    pub invariant: String,
    pub passed: bool,
    pub expected: String,
    pub actual: String,
    pub message: String,
}

/// A single coverage-invariant check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverageCheck {
    pub check: String,
    pub passed: bool,
    pub message: String,
}

/// Full package validation report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageValidationReport {
    pub schema_version: u32,
    pub validator_version: String,
    pub package_dir: String,
    pub status: ValidationStatus,
    pub validated_at: String,
    #[serde(default)]
    pub issues: Vec<PackageValidationIssue>,
    #[serde(default)]
    pub file_checks: Vec<FileCheck>,
    #[serde(default)]
    pub artifact_checks: Vec<ArtifactCheck>,
    #[serde(default)]
    pub redaction_checks: Vec<RedactionCheck>,
    #[serde(default)]
    pub counter_checks: Vec<CounterCheck>,
    #[serde(default)]
    pub coverage_checks: Vec<CoverageCheck>,
}

// ---------------------------------------------------------------------------
// Review and handoff statuses
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReviewStatus {
    #[serde(rename = "pass")]
    Pass,
    #[serde(rename = "revise")]
    Revise,
    #[serde(rename = "fail")]
    Fail,
    #[serde(rename = "blocked")]
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HandoffStatus {
    #[serde(rename = "ready")]
    Ready,
    #[serde(rename = "limited")]
    Limited,
    #[serde(rename = "blocked")]
    Blocked,
}

// ---------------------------------------------------------------------------
// Legacy types — preserved for backward compatibility with existing modules
// ---------------------------------------------------------------------------

/// The final status of a QueryPlan task (MVP legacy).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    #[serde(rename = "full_delivery")]
    FullDelivery,
    #[serde(rename = "limited_delivery")]
    LimitedDelivery,
    #[serde(rename = "execution_blocked")]
    ExecutionBlocked,
    #[serde(rename = "input_rejected")]
    InputRejected,
}

/// The orchestrator's delivery decision (MVP legacy).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryDecision {
    pub status: TaskStatus,
    pub accepted_images: Vec<super::image::ImageAcceptanceDecision>,
    pub rejected_images: Vec<super::image::ImageAcceptanceDecision>,
    pub full_attempt_count: u32,
    pub retry_count: u32,
    pub summary: String,
    pub shortfall_reason: Option<String>,
}

impl DeliveryDecision {
    pub fn full_delivery(
        accepted: Vec<super::image::ImageAcceptanceDecision>,
        rejected: Vec<super::image::ImageAcceptanceDecision>,
        full_attempt_count: u32,
        retry_count: u32,
    ) -> Self {
        let accepted_count = accepted.iter().filter(|d| d.is_accepted()).count();
        Self {
            status: TaskStatus::FullDelivery,
            accepted_images: accepted,
            rejected_images: rejected,
            full_attempt_count,
            retry_count,
            summary: format!(
                "Full delivery: {} images accepted after {} attempt(s).",
                accepted_count, full_attempt_count
            ),
            shortfall_reason: None,
        }
    }

    pub fn limited_delivery(
        accepted: Vec<super::image::ImageAcceptanceDecision>,
        rejected: Vec<super::image::ImageAcceptanceDecision>,
        full_attempt_count: u32,
        retry_count: u32,
        required_count: u32,
    ) -> Self {
        let accepted_count = accepted.iter().filter(|d| d.is_accepted()).count() as u32;
        let shortfall = required_count.saturating_sub(accepted_count);
        Self {
            status: TaskStatus::LimitedDelivery,
            accepted_images: accepted,
            rejected_images: rejected,
            full_attempt_count,
            retry_count,
            summary: format!(
                "Limited delivery: {} of {} required images delivered after {} attempt(s).",
                accepted_count, required_count, full_attempt_count,
            ),
            shortfall_reason: Some(format!(
                "Shortfall of {} image(s). Retry limit ({}) reached.",
                shortfall, retry_count
            )),
        }
    }

    pub fn execution_blocked(reason: String) -> Self {
        Self {
            status: TaskStatus::ExecutionBlocked,
            accepted_images: vec![],
            rejected_images: vec![],
            full_attempt_count: 0,
            retry_count: 0,
            summary: format!("Execution blocked: {}", reason),
            shortfall_reason: Some(reason),
        }
    }

    pub fn input_rejected(reason: String) -> Self {
        Self {
            status: TaskStatus::InputRejected,
            accepted_images: vec![],
            rejected_images: vec![],
            full_attempt_count: 0,
            retry_count: 0,
            summary: format!("Input rejected: {}", reason),
            shortfall_reason: Some(reason),
        }
    }
}

/// Top-level delivery manifest (MVP legacy).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryManifest {
    pub status: TaskStatus,
    pub required_count: u32,
    pub delivered_count: u32,
    pub full_attempt_count: u32,
    pub retry_count: u32,
    pub summary: String,
    pub shortfall_reason: Option<String>,
}

// ---------------------------------------------------------------------------
// Migration helpers — TaskStatus ↔ PackageStatus
// ---------------------------------------------------------------------------

impl From<&DeliveryDecision> for PackageStatus {
    fn from(d: &DeliveryDecision) -> Self {
        match d.status {
            TaskStatus::FullDelivery => PackageStatus::Passed,
            TaskStatus::LimitedDelivery => {
                let accepted = d.accepted_images.iter().filter(|i| i.is_accepted()).count();
                if accepted > 0 {
                    PackageStatus::Partial
                } else {
                    PackageStatus::Blocked
                }
            }
            TaskStatus::ExecutionBlocked | TaskStatus::InputRejected => PackageStatus::Blocked,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::candidate::ImageDimensions;
    use crate::domain::image::{ImageAcceptanceDecision, ImageRecord};

    fn make_accepted(id: &str) -> ImageAcceptanceDecision {
        ImageAcceptanceDecision::Accepted {
            image: ImageRecord {
                candidate_id: id.into(),
                local_path: format!("/tmp/{}.jpg", id),
                content_type: Some("image/jpeg".into()),
                file_size_bytes: 1024,
                dimensions: Some(ImageDimensions {
                    width: 800,
                    height: 600,
                }),
                reference_metrics: vec![],
            },
            notes: "good".into(),
            vlm_evidence: None,
        }
    }

    // --- Legacy type tests ---

    #[test]
    fn full_delivery_status() {
        let accepted = vec![make_accepted("a"), make_accepted("b")];
        let decision = DeliveryDecision::full_delivery(accepted, vec![], 1, 0);
        assert_eq!(decision.status, TaskStatus::FullDelivery);
        assert!(decision.shortfall_reason.is_none());
    }

    #[test]
    fn limited_delivery_status_with_shortfall() {
        let accepted = vec![make_accepted("a")];
        let decision = DeliveryDecision::limited_delivery(accepted, vec![], 4, 3, 3);
        assert_eq!(decision.status, TaskStatus::LimitedDelivery);
        assert!(decision.shortfall_reason.is_some());
    }

    #[test]
    fn execution_blocked_status() {
        let decision = DeliveryDecision::execution_blocked("OpenClaw unavailable".into());
        assert_eq!(decision.status, TaskStatus::ExecutionBlocked);
        assert_eq!(decision.full_attempt_count, 0);
    }

    #[test]
    fn input_rejected_status() {
        let decision = DeliveryDecision::input_rejected("missing description".into());
        assert_eq!(decision.status, TaskStatus::InputRejected);
        assert_eq!(decision.accepted_images.len(), 0);
    }

    // --- v1.1 RunState tests ---

    #[test]
    fn run_state_starts_with_correct_counters() {
        let state = RunState::new("run-1".into(), "qp-1".into(), 2, 3);
        assert_eq!(state.full_attempt_count, 1);
        assert_eq!(state.retry_count, 0);
        assert_eq!(state.retry_limit, 3);
        assert_eq!(state.full_attempt_limit, 4);
        assert_eq!(state.status, PackageStatus::Blocked);
        assert!(state.can_retry());
        assert!(!state.is_exhausted());
    }

    #[test]
    fn run_state_retry_exhaustion() {
        let mut state = RunState::new("run-1".into(), "qp-1".into(), 2, 3);
        for _ in 0..3 {
            assert!(state.record_retry());
        }
        assert_eq!(state.retry_count, 3);
        assert_eq!(state.full_attempt_count, 4);
        assert!(!state.can_retry());
        assert!(state.is_exhausted());
        assert!(!state.record_retry());
    }

    #[test]
    fn run_state_zero_retry_limit() {
        let mut state = RunState::new("run-1".into(), "qp-1".into(), 1, 0);
        assert_eq!(state.full_attempt_limit, 1);
        assert!(!state.can_retry());
        assert!(state.is_exhausted());
        assert!(!state.record_retry());
    }

    #[test]
    fn run_state_passed_when_count_reached() {
        let mut state = RunState::new("run-1".into(), "qp-1".into(), 2, 3);
        let img = DeliveredImageRecord {
            delivered_image_id: "d-1".into(),
            query_plan_id: "qp-1".into(),
            candidate_id: "c-1".into(),
            retrieval_job_id: "r-1".into(),
            package_image_path: "images/img.jpg".into(),
            local_artifact_path: "/tmp/img.jpg".into(),
            source_artifact_path: "source".into(),
            source_sidecar_path: "sidecar".into(),
            content_summary_path: "summary".into(),
            task_report_path: "report".into(),
            visual_description_path: "visual".into(),
            checksum_sha256: "abc123".into(),
            content_type: "image/jpeg".into(),
            file_size_bytes: 1024,
            width: Some(800),
            height: Some(600),
            candidate_quality_decision_ref: "qd-1".into(),
            image_acceptance_decision_ref: "ia-1".into(),
            manifest_entry_ref: "m-1".into(),
            evidence: Default::default(),
        };
        state.accepted_images.push(img.clone());
        state.accepted_images.push(img);
        state.update_status();
        assert_eq!(state.status, PackageStatus::Passed);
        assert_eq!(state.accepted_count(), 2);
        assert_eq!(state.gap_count(), 0);
    }

    #[test]
    fn run_state_partial_when_exhausted_with_images() {
        let mut state = RunState::new("run-1".into(), "qp-1".into(), 3, 0);
        let img = DeliveredImageRecord {
            delivered_image_id: "d-1".into(),
            query_plan_id: "qp-1".into(),
            candidate_id: "c-1".into(),
            retrieval_job_id: "r-1".into(),
            package_image_path: "images/img.jpg".into(),
            local_artifact_path: "/tmp/img.jpg".into(),
            source_artifact_path: "source".into(),
            source_sidecar_path: "sidecar".into(),
            content_summary_path: "summary".into(),
            task_report_path: "report".into(),
            visual_description_path: "visual".into(),
            checksum_sha256: "abc123".into(),
            content_type: "image/jpeg".into(),
            file_size_bytes: 1024,
            width: Some(800),
            height: Some(600),
            candidate_quality_decision_ref: "qd-1".into(),
            image_acceptance_decision_ref: "ia-1".into(),
            manifest_entry_ref: "m-1".into(),
            evidence: Default::default(),
        };
        state.accepted_images.push(img);
        state.update_status();
        assert_eq!(state.status, PackageStatus::Partial);
        assert_eq!(state.gap_count(), 2);
    }

    #[test]
    fn run_state_blocked_when_exhausted_with_zero_images() {
        let mut state = RunState::new("run-1".into(), "qp-1".into(), 2, 0);
        state.update_status();
        assert_eq!(state.status, PackageStatus::Blocked);
        assert_eq!(state.gap_count(), 2);
    }

    #[test]
    fn package_status_labels() {
        assert_eq!(PackageStatus::Passed.label(), "passed");
        assert_eq!(PackageStatus::Partial.label(), "partial");
        assert_eq!(PackageStatus::Blocked.label(), "blocked");
    }

    #[test]
    fn pipeline_stage_labels() {
        assert_eq!(PipelineStage::Admission.label(), "admission");
        assert_eq!(PipelineStage::Search.label(), "search");
        assert_eq!(PipelineStage::PackageBuild.label(), "package_build");
        assert_eq!(
            PipelineStage::PackageValidation.label(),
            "package_validation"
        );
    }

    #[test]
    fn workflow_diagnostic_constructors() {
        let b = WorkflowDiagnostic::blocker(
            WorkflowFailureCode::NoSearchProvider,
            PipelineStage::ProviderReadiness,
            "no provider",
        );
        assert_eq!(b.severity, WorkflowSeverity::Blocker);
        assert!(!b.retryable);

        let w = WorkflowDiagnostic::warning(
            WorkflowFailureCode::SearchShortage,
            PipelineStage::Search,
            "shortage",
        );
        assert_eq!(w.severity, WorkflowSeverity::Warning);
        assert!(w.retryable);
    }

    #[test]
    fn failure_code_mappings() {
        assert_eq!(
            WorkflowFailureCode::AdmissionRejected.code(),
            "WORKFLOW_ADMISSION_REJECTED"
        );
        assert_eq!(
            WorkflowFailureCode::NoSearchProvider.code(),
            "WORKFLOW_NO_SEARCH_PROVIDER"
        );
    }

    #[test]
    fn delivery_decision_to_package_status() {
        let accepted = vec![make_accepted("a"), make_accepted("b")];
        let fd = DeliveryDecision::full_delivery(accepted.clone(), vec![], 1, 0);
        assert_eq!(PackageStatus::from(&fd), PackageStatus::Passed);

        let ld = DeliveryDecision::limited_delivery(accepted, vec![], 4, 3, 3);
        assert_eq!(PackageStatus::from(&ld), PackageStatus::Partial);

        let ld0 = DeliveryDecision::limited_delivery(vec![], vec![], 4, 3, 3);
        assert_eq!(PackageStatus::from(&ld0), PackageStatus::Blocked);

        let eb = DeliveryDecision::execution_blocked("reason".into());
        assert_eq!(PackageStatus::from(&eb), PackageStatus::Blocked);
    }
}
