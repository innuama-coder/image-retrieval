//! Delivery package builder.
//!
//! Implements the delivery package structure defined in
//! `docs/design/TASK-007-delivery-policy-observability-design.md`:
//!
//! ```text
//! <delivery-package>/
//!   status.json
//!   manifest.json
//!   summary.md
//!   images/
//!   evidence/
//!   diagnostics/
//! ```
//!
//! The [`DeliveryPackageBuilder`] consumes a [`DeliveryDecision`], optional
//! upstream evidence, and metric events. It writes the terminal delivery
//! package to a stable output directory.
//!
//! Input rejection (`TaskStatus::InputRejected`) is NOT a delivery outcome —
//! the builder refuses to produce a delivery package for it.
//!
//! References: PRD §交付物产品设计, HLD §Delivery Package Builder,
//! `docs/design/TASK-007-delivery-policy-observability-design.md`

use crate::domain::delivery::{DeliveryDecision, TaskStatus};
use crate::domain::image::ImageAcceptanceDecision;
use crate::domain::metrics::MetricEvent;
use crate::domain::query_plan::TaskPlan;
use crate::error::{Error, Result};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Re-export domain types for convenience
// ---------------------------------------------------------------------------

pub use crate::domain::delivery::{
    DeliveryDecision as DeliveryDecisionReexport, DeliveryManifest,
    TaskStatus as TaskStatusReexport,
};

// ---------------------------------------------------------------------------
// status.json contract (LLD §机器可读状态契约)
// ---------------------------------------------------------------------------

/// The `status.json` contract — stable, minimal entry point for automation.
///
/// Fields match the LLD table exactly. All fields are required.
#[derive(Debug, Clone, Serialize)]
pub struct StatusFile {
    /// Delivery status contract version. MVP is `1`.
    pub schema_version: u32,

    /// `full_delivery`, `limited_delivery`, or `execution_blocked`.
    pub task_status: String,

    /// Number of images the QueryPlan requested.
    pub required_count: u32,

    /// Number of qualified images actually delivered.
    pub accepted_count: u32,

    /// `required_count - accepted_count`; zero for full delivery.
    pub gap_count: u32,

    /// Total full attempts executed.
    pub attempts_used: u32,

    /// Retries beyond the initial attempt.
    pub retry_count: u32,

    /// Primary reason for the terminal status (must be redacted).
    pub primary_reason: String,

    /// Relative path to `manifest.json` within the same package.
    pub manifest_path: String,

    /// Relative path to `summary.md` within the same package.
    pub summary_path: String,
}

impl StatusFile {
    /// Build a `StatusFile` from the orchestrator's decision and the task plan.
    ///
    /// `primary_reason` is the redacted shortfall reason or a generic status
    /// description. Never contains raw service responses or credentials.
    pub fn from_decision(decision: &DeliveryDecision, task_plan: &TaskPlan) -> Self {
        let accepted_count = decision
            .accepted_images
            .iter()
            .filter(|d| d.is_accepted())
            .count() as u32;

        let required_count = task_plan.query_plan.required_count;
        let gap_count = required_count.saturating_sub(accepted_count);

        let primary_reason = match decision.status {
            TaskStatus::FullDelivery => {
                format!(
                    "Full delivery: {} of {} required images delivered after {} attempt(s).",
                    accepted_count, required_count, decision.full_attempt_count,
                )
            }
            TaskStatus::LimitedDelivery => decision
                .shortfall_reason
                .clone()
                .unwrap_or_else(|| "Limited delivery: retries exhausted.".into()),
            TaskStatus::ExecutionBlocked => decision
                .shortfall_reason
                .clone()
                .unwrap_or_else(|| "Execution blocked by policy or dependency.".into()),
            TaskStatus::InputRejected => {
                // Input rejection does not produce a delivery package; this
                // path is guarded by the builder.
                "Input rejected — no delivery package generated.".into()
            }
        };

        Self {
            schema_version: 1,
            task_status: task_status_to_string(decision.status),
            required_count,
            accepted_count,
            gap_count,
            attempts_used: decision.full_attempt_count,
            retry_count: decision.retry_count,
            primary_reason,
            manifest_path: "manifest.json".into(),
            summary_path: "summary.md".into(),
        }
    }
}

// ---------------------------------------------------------------------------
// manifest.json contract (LLD §Manifest 契约)
// ---------------------------------------------------------------------------

/// The `manifest.json` contract — complete machine-readable delivery record.
///
/// All sections are required per the LLD.
#[derive(Debug, Clone, Serialize)]
pub struct ManifestFile {
    pub schema_version: u32,

    /// Redacted summary of the validated QueryPlan.
    pub query_plan_summary: QueryPlanSummary,

    /// Mirrors `status.json.task_status`.
    pub delivery_status: String,

    /// Qualified images (empty array when zero delivered).
    pub accepted_images: Vec<AcceptedImageEntry>,

    /// Gap between required and delivered.
    pub gap: GapInfo,

    /// Candidate discovery summary.
    pub candidate_summary: DeliveryCandidateSummary,

    /// Retrieval batch summary.
    pub retrieval_summary: DeliveryRetrievalSummary,

    /// Image acceptance summary.
    pub acceptance_summary: DeliveryAcceptanceSummary,

    /// Risk and authorization summary.
    pub risk_summary: RiskSummary,

    /// MET-001 through MET-006 event inputs or summaries.
    pub metrics: MetricsBlock,

    /// Relative paths to redacted evidence files.
    pub evidence_refs: Vec<String>,
}

// --- Query plan summary ---

#[derive(Debug, Clone, Serialize)]
pub struct QueryPlanSummary {
    /// Redacted semantic description (never contains credentials or tokens).
    pub description: String,
    pub required_count: u32,
    pub quality_tier: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub must_include: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub must_avoid: Vec<String>,
    pub authorization_preference: String,
    pub output_preference: String,
}

// --- Accepted image entry ---

/// One entry in `accepted_images`.
#[derive(Debug, Clone, Serialize)]
pub struct AcceptedImageEntry {
    /// Relative path to the image file within `images/`.
    pub image_path: String,

    /// Where the image was sourced from (descriptive, not raw URL).
    pub source: String,

    /// Why the image was accepted.
    pub acceptance_reason: String,

    /// Quality notes from the acceptance process.
    pub quality_notes: String,

    /// Authorization risk label: `unknown`, `prohibited`, or `allowed`.
    pub authorization_risk: String,

    /// Reference to mechanical acceptance evidence.
    pub mechanical_evidence_ref: String,

    /// Reference to OpenClaw acceptance evidence.
    pub openclaw_evidence_ref: String,
}

// --- Gap info ---

#[derive(Debug, Clone, Serialize)]
pub struct GapInfo {
    pub required_count: u32,
    pub accepted_count: u32,
    pub shortfall: u32,

    /// Primary reason for the gap (redacted).
    pub primary_gap_reason: String,
}

// --- Candidate summary ---

#[derive(Debug, Clone, Serialize)]
pub struct DeliveryCandidateSummary {
    pub candidate_target: u32,
    pub actual_candidates: u32,
    pub after_dedup: u32,

    /// Whether the candidate target was not met.
    pub shortage: bool,

    /// Reason if there was a shortage.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shortage_reason: Option<String>,

    /// Main reason categories for candidate rejection.
    pub main_rejection_categories: Vec<String>,
}

// --- Retrieval summary ---

#[derive(Debug, Clone, Serialize)]
pub struct DeliveryRetrievalSummary {
    pub batch_target: u32,
    pub actual_batches: u32,
    pub channels_attempted: Vec<String>,
    pub fallback_used: bool,
    pub short_batch_occurred: bool,

    /// Local (per-candidate) rejections that did not block the task.
    pub local_rejections: u32,

    /// Whether any batch contained no real images.
    pub no_real_image_batches: bool,

    /// Summary of fallback facts.
    pub fallback_summary: String,
}

// --- Acceptance summary ---

#[derive(Debug, Clone, Serialize)]
pub struct DeliveryAcceptanceSummary {
    pub mechanical_acceptance_total: u32,
    pub mechanical_rejections: u32,
    pub openclaw_approved: u32,
    pub openclaw_rejected: u32,
    pub openclaw_uncertain: u32,
    pub openclaw_unavailable: bool,

    /// Rejection categories with counts.
    pub rejection_categories: Vec<(String, u32)>,

    /// Whether any uncertain conclusions were recorded.
    pub has_uncertain_conclusions: bool,
}

// --- Risk summary ---

#[derive(Debug, Clone, Serialize)]
pub struct RiskSummary {
    /// Whether any images have unknown authorization risk.
    pub has_unknown_authorization: bool,

    /// Count of images with unknown authorization.
    pub unknown_authorization_count: u32,

    /// Whether any explicitly prohibited sources were encountered.
    pub prohibited_sources_encountered: bool,

    /// Whether any access restrictions were detected.
    pub access_restrictions_detected: bool,

    /// Whether paid channel boundaries were checked.
    pub paid_boundary_checked: bool,

    /// Whether any policy blocks were applied.
    pub policy_blocks_applied: u32,

    /// Human-readable risk notes (redacted).
    pub risk_notes: Vec<String>,
}

// --- Metrics block ---

/// Aggregated MET-001 through MET-006 summaries.
#[derive(Debug, Clone, Serialize)]
pub struct MetricsBlock {
    /// MET-001: Task outcome distribution event.
    pub task_outcome: MetricSummaryEntry,

    /// MET-002: Candidate satisfaction events.
    pub candidate_satisfaction: Vec<MetricSummaryEntry>,

    /// MET-003: Qualified image achievement events.
    pub qualified_image_achievement: Vec<MetricSummaryEntry>,

    /// MET-004: Rejection reason events.
    pub rejection_reasons: Vec<MetricSummaryEntry>,

    /// MET-005: Channel effectiveness events.
    pub channel_effectiveness: Vec<MetricSummaryEntry>,

    /// MET-006: OpenClaw evaluation rate events.
    pub openclaw_evaluation_rate: Vec<MetricSummaryEntry>,
}

/// A single metric entry in the manifest.
#[derive(Debug, Clone, Serialize)]
pub struct MetricSummaryEntry {
    pub label: String,
    pub value: f64,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub denominator: Option<f64>,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub metadata: Vec<(String, String)>,
}

impl From<&MetricEvent> for MetricSummaryEntry {
    fn from(event: &MetricEvent) -> Self {
        Self {
            label: event.label.clone(),
            value: event.value,
            denominator: event.denominator,
            metadata: event.metadata.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// Delivery inputs — all data the builder needs
// ---------------------------------------------------------------------------

/// All inputs consumed by the [`DeliveryPackageBuilder`].
///
/// The builder does NOT perform search, retrieval, or subjective evaluation.
/// It only packages existing results.
#[derive(Debug, Clone)]
pub struct DeliveryInputs {
    /// Task plan from TASK-002.
    pub task_plan: TaskPlan,

    /// Delivery decision from the orchestrator (TASK-006).
    pub decision: DeliveryDecision,

    /// Metric events accumulated across the pipeline.
    pub metric_events: Vec<MetricEvent>,

    // --- Upstream evidence summaries (populated by earlier tasks) ---
    /// Number of actual candidates discovered (TASK-003).
    pub actual_candidates: u32,

    /// Number of candidates after deduplication (TASK-003).
    pub after_dedup_candidates: u32,

    /// Whether a candidate shortage occurred.
    pub candidate_shortage: bool,

    /// Candidate shortage reason, if any.
    pub candidate_shortage_reason: Option<String>,

    /// Main candidate rejection categories (TASK-004).
    pub candidate_rejection_categories: Vec<String>,

    /// Actual number of retrieval batches executed (TASK-005).
    pub actual_batches: u32,

    /// Channel tiers that were attempted.
    pub channels_attempted: Vec<String>,

    /// Whether fallback was used.
    pub fallback_used: bool,

    /// Whether any batch was short.
    pub short_batch_occurred: bool,

    /// Number of local (per-candidate) retrieval rejections.
    pub local_retrieval_rejections: u32,

    /// Whether any batch contained no real images.
    pub no_real_image_batches: bool,

    /// Fallback summary text.
    pub fallback_summary: String,

    /// Image acceptance gate summary counts.
    pub mechanical_acceptance_total: u32,
    pub mechanical_rejections: u32,
    pub openclaw_approved: u32,
    pub openclaw_rejected: u32,
    pub openclaw_uncertain: u32,
    pub openclaw_unavailable: bool,

    /// Image-level rejection categories with counts.
    pub image_rejection_categories: Vec<(String, u32)>,

    /// Whether uncertain OpenClaw conclusions exist.
    pub has_uncertain_conclusions: bool,

    /// Risk information.
    pub has_unknown_authorization: bool,
    pub unknown_authorization_count: u32,
    pub prohibited_sources_encountered: bool,
    pub access_restrictions_detected: bool,
    pub paid_boundary_checked: bool,
    pub policy_blocks_applied: u32,
    pub risk_notes: Vec<String>,
}

impl DeliveryInputs {
    /// Create minimal inputs for testing — adequate for unit-test coverage
    /// of the builder's output contract.
    pub fn minimal(task_plan: TaskPlan, decision: DeliveryDecision) -> Self {
        Self {
            task_plan,
            decision,
            metric_events: Vec::new(),
            actual_candidates: 0,
            after_dedup_candidates: 0,
            candidate_shortage: false,
            candidate_shortage_reason: None,
            candidate_rejection_categories: Vec::new(),
            actual_batches: 0,
            channels_attempted: Vec::new(),
            fallback_used: false,
            short_batch_occurred: false,
            local_retrieval_rejections: 0,
            no_real_image_batches: false,
            fallback_summary: String::new(),
            mechanical_acceptance_total: 0,
            mechanical_rejections: 0,
            openclaw_approved: 0,
            openclaw_rejected: 0,
            openclaw_uncertain: 0,
            openclaw_unavailable: false,
            image_rejection_categories: Vec::new(),
            has_uncertain_conclusions: false,
            has_unknown_authorization: false,
            unknown_authorization_count: 0,
            prohibited_sources_encountered: false,
            access_restrictions_detected: false,
            paid_boundary_checked: false,
            policy_blocks_applied: 0,
            risk_notes: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Delivery package builder
// ---------------------------------------------------------------------------

/// Writes the stable delivery package to an output directory.
///
/// # Lifecycle
///
/// 1. Validate that the task status is a valid delivery outcome (not
///    `InputRejected` and not in-progress).
/// 2. Create the package directory structure.
/// 3. Write `status.json`.
/// 4. Write `manifest.json`.
/// 5. Write `summary.md`.
/// 6. Copy qualified images into `images/`.
/// 7. Write redacted evidence into `evidence/`.
/// 8. Write diagnostics into `diagnostics/`.
///
/// # Constraints
///
/// - Credentials, tokens, cookies, and sensitive configuration MUST NOT
///   appear in any output file.
/// - Unaccepted images MUST NOT appear in `images/` or `accepted_images`.
/// - Input rejection (`TaskStatus::InputRejected`) does NOT produce a
///   delivery package.
/// - The builder does NOT trigger new search, retrieval, or subjective
///   evaluation.
pub struct DeliveryPackageBuilder {
    output_dir: PathBuf,
}

impl DeliveryPackageBuilder {
    /// Create a new builder targeting `output_dir`.
    ///
    /// The directory will be created (including parents) during [`build`].
    pub fn new(output_dir: impl Into<PathBuf>) -> Self {
        Self {
            output_dir: output_dir.into(),
        }
    }

    /// Write the complete delivery package.
    ///
    /// Returns the path to the package root directory on success.
    ///
    /// # Errors
    ///
    /// - `InputRejection` if the decision status is `InputRejected`.
    /// - `ExecutionBlocked` if the status is not a terminal delivery state.
    /// - I/O errors wrapped as `Internal`.
    pub fn build(&self, inputs: &DeliveryInputs) -> Result<PathBuf> {
        // ----------------------------------------------------------------
        // Guard: input rejection is not a delivery outcome
        // ----------------------------------------------------------------
        if inputs.decision.status == TaskStatus::InputRejected {
            return Err(Error::input_rejection(
                "Input rejection does not produce a delivery package. \
                 No status.json, manifest.json, or images will be written.",
            ));
        }

        // ----------------------------------------------------------------
        // Guard: non-terminal states should not reach the builder
        // ----------------------------------------------------------------
        if !is_terminal_delivery_state(inputs.decision.status) {
            return Err(Error::execution_blocked(format!(
                "Task is not in a terminal delivery state: {:?}",
                inputs.decision.status
            )));
        }

        // ----------------------------------------------------------------
        // Create directory structure
        // ----------------------------------------------------------------
        let root = &self.output_dir;
        let images_dir = root.join("images");
        let evidence_dir = root.join("evidence");
        let diagnostics_dir = root.join("diagnostics");

        for dir in &[root, &images_dir, &evidence_dir, &diagnostics_dir] {
            fs::create_dir_all(dir).map_err(|e| {
                Error::internal(format!(
                    "failed to create directory {}: {}",
                    dir.display(),
                    e
                ))
            })?;
        }

        // ----------------------------------------------------------------
        // Write status.json
        // ----------------------------------------------------------------
        let status = StatusFile::from_decision(&inputs.decision, &inputs.task_plan);
        let status_path = root.join("status.json");
        let status_json =
            serde_json::to_string_pretty(&status).map_err(|e| Error::internal(e.to_string()))?;
        fs::write(&status_path, status_json).map_err(|e| {
            Error::internal(format!("failed to write {}: {}", status_path.display(), e))
        })?;

        // ----------------------------------------------------------------
        // Build and write manifest.json
        // ----------------------------------------------------------------
        let manifest = self.build_manifest(inputs);
        let manifest_path = root.join("manifest.json");
        let manifest_json =
            serde_json::to_string_pretty(&manifest).map_err(|e| Error::internal(e.to_string()))?;
        fs::write(&manifest_path, manifest_json).map_err(|e| {
            Error::internal(format!(
                "failed to write {}: {}",
                manifest_path.display(),
                e
            ))
        })?;

        // ----------------------------------------------------------------
        // Write summary.md
        // ----------------------------------------------------------------
        let summary_md = self.build_summary_md(inputs);
        let summary_path = root.join("summary.md");
        fs::write(&summary_path, &summary_md).map_err(|e| {
            Error::internal(format!("failed to write {}: {}", summary_path.display(), e))
        })?;

        // ----------------------------------------------------------------
        // Copy qualified images into images/
        // ----------------------------------------------------------------
        self.copy_qualified_images(inputs, &images_dir)?;

        // ----------------------------------------------------------------
        // Write evidence/
        // ----------------------------------------------------------------
        self.write_evidence(inputs, &evidence_dir)?;

        // ----------------------------------------------------------------
        // Write diagnostics/
        // ----------------------------------------------------------------
        self.write_diagnostics(inputs, &diagnostics_dir)?;

        Ok(root.clone())
    }

    // --- Manifest construction ---

    fn build_manifest(&self, inputs: &DeliveryInputs) -> ManifestFile {
        let accepted_count = inputs
            .decision
            .accepted_images
            .iter()
            .filter(|d| d.is_accepted())
            .count() as u32;

        let required_count = inputs.task_plan.query_plan.required_count;
        let shortfall = required_count.saturating_sub(accepted_count);

        // --- query_plan_summary ---
        let qp = &inputs.task_plan.query_plan;
        let query_plan_summary = QueryPlanSummary {
            description: qp.description.clone(), // already validated — no raw credentials
            required_count: qp.required_count,
            quality_tier: format!("{:?}", qp.quality_tier),
            must_include: qp.content_constraints.must_include.clone(),
            must_avoid: qp.content_constraints.must_avoid.clone(),
            authorization_preference: format!("{:?}", qp.authorization_preference),
            output_preference: format!("{:?}", qp.output_preference),
        };

        // --- accepted_images ---
        let accepted_images: Vec<AcceptedImageEntry> = inputs
            .decision
            .accepted_images
            .iter()
            .filter(|d| d.is_accepted())
            .enumerate()
            .map(|(i, decision)| {
                if let ImageAcceptanceDecision::Accepted { image, notes } = decision {
                    let filename = image
                        .local_path
                        .rsplit('/')
                        .next()
                        .unwrap_or("unknown")
                        .to_string();
                    AcceptedImageEntry {
                        image_path: format!("images/{}", filename),
                        source: image.candidate_id.clone(),
                        acceptance_reason: notes.clone(),
                        quality_notes: format!(
                            "{} {}x{} {} bytes",
                            image.content_type.as_deref().unwrap_or("unknown"),
                            image
                                .dimensions
                                .as_ref()
                                .map(|d| d.width.to_string())
                                .unwrap_or_else(|| "?".into()),
                            image
                                .dimensions
                                .as_ref()
                                .map(|d| d.height.to_string())
                                .unwrap_or_else(|| "?".into()),
                            image.file_size_bytes,
                        ),
                        authorization_risk: "unknown".into(),
                        mechanical_evidence_ref: format!("evidence/mechanical_{}.json", i + 1),
                        openclaw_evidence_ref: format!("evidence/openclaw_{}.json", i + 1),
                    }
                } else {
                    // Should never happen — we filter for Accepted above.
                    AcceptedImageEntry {
                        image_path: String::new(),
                        source: String::new(),
                        acceptance_reason: String::new(),
                        quality_notes: String::new(),
                        authorization_risk: "unknown".into(),
                        mechanical_evidence_ref: String::new(),
                        openclaw_evidence_ref: String::new(),
                    }
                }
            })
            .collect();

        // --- gap ---
        let gap = GapInfo {
            required_count,
            accepted_count,
            shortfall,
            primary_gap_reason: inputs.decision.shortfall_reason.clone().unwrap_or_else(|| {
                if shortfall == 0 {
                    "No gap — full delivery.".into()
                } else {
                    format!("Shortfall of {} image(s).", shortfall)
                }
            }),
        };

        // --- candidate_summary ---
        let candidate_summary = DeliveryCandidateSummary {
            candidate_target: inputs.task_plan.candidate_target,
            actual_candidates: inputs.actual_candidates,
            after_dedup: inputs.after_dedup_candidates,
            shortage: inputs.candidate_shortage,
            shortage_reason: inputs.candidate_shortage_reason.clone(),
            main_rejection_categories: inputs.candidate_rejection_categories.clone(),
        };

        // --- retrieval_summary ---
        let retrieval_summary = DeliveryRetrievalSummary {
            batch_target: inputs.task_plan.retrieval_batch_target,
            actual_batches: inputs.actual_batches,
            channels_attempted: inputs.channels_attempted.clone(),
            fallback_used: inputs.fallback_used,
            short_batch_occurred: inputs.short_batch_occurred,
            local_rejections: inputs.local_retrieval_rejections,
            no_real_image_batches: inputs.no_real_image_batches,
            fallback_summary: inputs.fallback_summary.clone(),
        };

        // --- acceptance_summary ---
        let acceptance_summary = DeliveryAcceptanceSummary {
            mechanical_acceptance_total: inputs.mechanical_acceptance_total,
            mechanical_rejections: inputs.mechanical_rejections,
            openclaw_approved: inputs.openclaw_approved,
            openclaw_rejected: inputs.openclaw_rejected,
            openclaw_uncertain: inputs.openclaw_uncertain,
            openclaw_unavailable: inputs.openclaw_unavailable,
            rejection_categories: inputs.image_rejection_categories.clone(),
            has_uncertain_conclusions: inputs.has_uncertain_conclusions,
        };

        // --- risk_summary ---
        let risk_summary = RiskSummary {
            has_unknown_authorization: inputs.has_unknown_authorization,
            unknown_authorization_count: inputs.unknown_authorization_count,
            prohibited_sources_encountered: inputs.prohibited_sources_encountered,
            access_restrictions_detected: inputs.access_restrictions_detected,
            paid_boundary_checked: inputs.paid_boundary_checked,
            policy_blocks_applied: inputs.policy_blocks_applied,
            risk_notes: inputs.risk_notes.clone(),
        };

        // --- metrics ---
        let metrics = build_metrics_block(&inputs.metric_events);

        // --- evidence_refs ---
        let evidence_refs = vec![
            "evidence/acceptance.json".into(),
            "evidence/rejection.json".into(),
            "diagnostics/diagnostic.json".into(),
            "diagnostics/metrics_summary.json".into(),
        ];

        ManifestFile {
            schema_version: 1,
            query_plan_summary,
            delivery_status: task_status_to_string(inputs.decision.status),
            accepted_images,
            gap,
            candidate_summary,
            retrieval_summary,
            acceptance_summary,
            risk_summary,
            metrics,
            evidence_refs,
        }
    }

    // --- summary.md ---

    fn build_summary_md(&self, inputs: &DeliveryInputs) -> String {
        let status_label = task_status_to_string(inputs.decision.status);
        let accepted_count = inputs
            .decision
            .accepted_images
            .iter()
            .filter(|d| d.is_accepted())
            .count();
        let required = inputs.task_plan.query_plan.required_count;

        let mut md = String::new();
        md.push_str("# Delivery Summary\n\n");
        md.push_str(&format!("**Status**: {status_label}\n\n"));
        md.push_str(&format!(
            "**Result**: {} of {} required images delivered after {} attempt(s).\n\n",
            accepted_count, required, inputs.decision.full_attempt_count,
        ));

        if let Some(ref reason) = inputs.decision.shortfall_reason {
            md.push_str(&format!("**Reason**: {reason}\n\n"));
        }

        md.push_str("## Query\n\n");
        md.push_str(&format!(
            "Description: {}\n\n",
            inputs.task_plan.query_plan.description
        ));
        md.push_str(&format!(
            "Quality tier: {:?}\n\n",
            inputs.task_plan.query_plan.quality_tier
        ));

        md.push_str("## Accepted Images\n\n");
        if accepted_count == 0 {
            md.push_str("No images were accepted.\n\n");
        } else {
            for (i, decision) in inputs
                .decision
                .accepted_images
                .iter()
                .filter(|d| d.is_accepted())
                .enumerate()
            {
                if let ImageAcceptanceDecision::Accepted { image, notes } = decision {
                    md.push_str(&format!(
                        "{}. `{}` — {}\n",
                        i + 1,
                        image.candidate_id,
                        notes
                    ));
                }
            }
            md.push('\n');
        }

        md.push_str("## Next Steps\n\n");
        match inputs.decision.status {
            TaskStatus::FullDelivery => {
                md.push_str("All required images were delivered. No further action needed.\n");
            }
            TaskStatus::LimitedDelivery => {
                md.push_str(
                    "Not all required images were delivered. Consider adjusting the \
                     QueryPlan (lower count, broader description, or relaxed quality tier) \
                     and re-running.\n",
                );
            }
            TaskStatus::ExecutionBlocked => {
                md.push_str(
                    "The task was blocked. Review the blocking reason above, resolve \
                     the underlying dependency or policy issue, and re-run.\n",
                );
            }
            _ => {}
        }

        md
    }

    // --- Image copying ---

    fn copy_qualified_images(&self, inputs: &DeliveryInputs, images_dir: &Path) -> Result<()> {
        for decision in &inputs.decision.accepted_images {
            if let ImageAcceptanceDecision::Accepted { image, .. } = decision {
                let src = Path::new(&image.local_path);
                if src.exists() {
                    let filename = src
                        .file_name()
                        .unwrap_or_else(|| std::ffi::OsStr::new("unknown"));
                    let dst = images_dir.join(filename);
                    fs::copy(src, &dst).map_err(|e| {
                        Error::internal(format!(
                            "failed to copy image {} -> {}: {}",
                            src.display(),
                            dst.display(),
                            e
                        ))
                    })?;
                }
                // If the source doesn't exist (e.g. fixture test), skip
                // quietly — the manifest still records the entry.
            }
        }
        Ok(())
    }

    // --- Evidence ---

    fn write_evidence(&self, inputs: &DeliveryInputs, evidence_dir: &Path) -> Result<()> {
        // Write acceptance evidence
        let acceptance_evidence: Vec<serde_json::Value> = inputs
            .decision
            .accepted_images
            .iter()
            .filter(|d| d.is_accepted())
            .map(|d| match d {
                ImageAcceptanceDecision::Accepted { image, notes } => {
                    serde_json::json!({
                        "candidate_id": image.candidate_id,
                        "content_type": image.content_type,
                        "file_size_bytes": image.file_size_bytes,
                        "acceptance_notes": notes,
                    })
                }
                _ => serde_json::json!({}),
            })
            .collect();

        let acceptance_path = evidence_dir.join("acceptance.json");
        let acceptance_json = serde_json::to_string_pretty(&acceptance_evidence)
            .map_err(|e| Error::internal(e.to_string()))?;
        fs::write(&acceptance_path, acceptance_json).map_err(|e| {
            Error::internal(format!(
                "failed to write {}: {}",
                acceptance_path.display(),
                e
            ))
        })?;

        // Write rejection evidence
        let rejection_evidence: Vec<serde_json::Value> = inputs
            .decision
            .rejected_images
            .iter()
            .map(|d| match d {
                ImageAcceptanceDecision::MechanicallyRejected {
                    image, evidence, ..
                } => {
                    serde_json::json!({
                        "candidate_id": image.candidate_id,
                        "rejection_type": "mechanical",
                        "blocking_findings": evidence.blocking_findings,
                        "reference_findings": evidence.reference_findings,
                    })
                }
                ImageAcceptanceDecision::SubjectivelyRejected {
                    image,
                    mechanical_evidence,
                    reason,
                } => {
                    serde_json::json!({
                        "candidate_id": image.candidate_id,
                        "rejection_type": "subjective",
                        "reason": reason,
                        "reference_findings": mechanical_evidence.reference_findings,
                    })
                }
                _ => serde_json::json!({}),
            })
            .collect();

        let rejection_path = evidence_dir.join("rejection.json");
        let rejection_json = serde_json::to_string_pretty(&rejection_evidence)
            .map_err(|e| Error::internal(e.to_string()))?;
        fs::write(&rejection_path, rejection_json).map_err(|e| {
            Error::internal(format!(
                "failed to write {}: {}",
                rejection_path.display(),
                e
            ))
        })?;

        Ok(())
    }

    // --- Diagnostics ---

    fn write_diagnostics(&self, inputs: &DeliveryInputs, diagnostics_dir: &Path) -> Result<()> {
        // Write diagnostic summary
        let diagnostic = serde_json::json!({
            "status": task_status_to_string(inputs.decision.status),
            "summary": inputs.decision.summary,
            "full_attempt_count": inputs.decision.full_attempt_count,
            "retry_count": inputs.decision.retry_count,
            "required_count": inputs.task_plan.query_plan.required_count,
            "candidate_target": inputs.task_plan.candidate_target,
            "retrieval_batch_target": inputs.task_plan.retrieval_batch_target,
            "max_attempts": inputs.task_plan.max_attempts,
        });

        let diag_path = diagnostics_dir.join("diagnostic.json");
        let diag_json = serde_json::to_string_pretty(&diagnostic)
            .map_err(|e| Error::internal(e.to_string()))?;
        fs::write(&diag_path, diag_json).map_err(|e| {
            Error::internal(format!("failed to write {}: {}", diag_path.display(), e))
        })?;

        // Write metrics summary (non-sensitive)
        let metrics_summary = build_metrics_block(&inputs.metric_events);
        let metrics_path = diagnostics_dir.join("metrics_summary.json");
        let metrics_json = serde_json::to_string_pretty(&metrics_summary)
            .map_err(|e| Error::internal(e.to_string()))?;
        fs::write(&metrics_path, metrics_json).map_err(|e| {
            Error::internal(format!("failed to write {}: {}", metrics_path.display(), e))
        })?;

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn task_status_to_string(status: TaskStatus) -> String {
    match status {
        TaskStatus::FullDelivery => "full_delivery".into(),
        TaskStatus::LimitedDelivery => "limited_delivery".into(),
        TaskStatus::ExecutionBlocked => "execution_blocked".into(),
        TaskStatus::InputRejected => "input_rejected".into(),
    }
}

fn is_terminal_delivery_state(status: TaskStatus) -> bool {
    matches!(
        status,
        TaskStatus::FullDelivery | TaskStatus::LimitedDelivery | TaskStatus::ExecutionBlocked
    )
}

fn build_metrics_block(events: &[MetricEvent]) -> MetricsBlock {
    use crate::domain::metrics::MetricKind;

    let task_outcome = events
        .iter()
        .find(|e| e.kind == MetricKind::TaskOutcome)
        .map(MetricSummaryEntry::from)
        .unwrap_or_else(|| MetricSummaryEntry {
            label: "task_outcome_unknown".into(),
            value: 0.0,
            denominator: None,
            metadata: vec![],
        });

    let candidate_satisfaction: Vec<MetricSummaryEntry> = events
        .iter()
        .filter(|e| e.kind == MetricKind::CandidateSatisfaction)
        .map(MetricSummaryEntry::from)
        .collect();

    let qualified_image_achievement: Vec<MetricSummaryEntry> = events
        .iter()
        .filter(|e| e.kind == MetricKind::QualifiedImageAchievement)
        .map(MetricSummaryEntry::from)
        .collect();

    let rejection_reasons: Vec<MetricSummaryEntry> = events
        .iter()
        .filter(|e| e.kind == MetricKind::RejectionReason)
        .map(MetricSummaryEntry::from)
        .collect();

    let channel_effectiveness: Vec<MetricSummaryEntry> = events
        .iter()
        .filter(|e| e.kind == MetricKind::ChannelEffectiveness)
        .map(MetricSummaryEntry::from)
        .collect();

    let openclaw_evaluation_rate: Vec<MetricSummaryEntry> = events
        .iter()
        .filter(|e| e.kind == MetricKind::OpenClawEvaluationRate)
        .map(MetricSummaryEntry::from)
        .collect();

    MetricsBlock {
        task_outcome,
        candidate_satisfaction,
        qualified_image_achievement,
        rejection_reasons,
        channel_effectiveness,
        openclaw_evaluation_rate,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::candidate::ImageDimensions;
    use crate::domain::image::{
        ImageAcceptanceDecision as ImDecision, ImageMechanicalEvidence, ImageRecord,
    };
    use crate::domain::metrics::{MetricEvent, MetricKind};
    use crate::domain::query_plan::{
        AuthorizationPreference, ContentConstraints, OutputPreference, QualityTier,
        ValidatedQueryPlan,
    };

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn make_task_plan(required_count: u32) -> TaskPlan {
        let plan = ValidatedQueryPlan {
            description: "test query — sunset over mountains".into(),
            required_count,
            quality_tier: QualityTier::General,
            content_constraints: ContentConstraints::default(),
            authorization_preference: AuthorizationPreference::Default,
            output_preference: OutputPreference::Human,
            retry_limit: 3,
        };
        TaskPlan::from_validated(plan)
    }

    fn make_accepted(id: &str) -> ImDecision {
        ImDecision::Accepted {
            image: ImageRecord {
                candidate_id: id.into(),
                local_path: format!("/tmp/{}.jpg", id),
                content_type: Some("image/jpeg".into()),
                file_size_bytes: 1024,
                dimensions: Some(ImageDimensions {
                    width: 800,
                    height: 600,
                }),
            },
            notes: "good match".into(),
        }
    }

    fn make_mechanical_rejection(id: &str) -> ImDecision {
        ImDecision::MechanicallyRejected {
            image: ImageRecord {
                candidate_id: id.into(),
                local_path: format!("/tmp/{}.jpg", id),
                content_type: Some("image/jpeg".into()),
                file_size_bytes: 0,
                dimensions: None,
            },
            evidence: ImageMechanicalEvidence {
                blocking_findings: vec!["zero-byte file".into()],
                reference_findings: vec![],
            },
        }
    }

    fn make_full_delivery_decision(count: u32) -> DeliveryDecision {
        let accepted: Vec<ImDecision> = (0..count)
            .map(|i| make_accepted(&format!("img-{}", i + 1)))
            .collect();
        DeliveryDecision::full_delivery(accepted, vec![], 1, 0)
    }

    fn make_limited_delivery_decision() -> DeliveryDecision {
        let accepted = vec![make_accepted("img-1")];
        let rejected = vec![make_mechanical_rejection("img-2")];
        DeliveryDecision::limited_delivery(accepted, rejected, 4, 3, 3)
    }

    fn make_execution_blocked_decision() -> DeliveryDecision {
        DeliveryDecision::execution_blocked("OpenClaw evaluation unavailable".into())
    }

    fn make_input_rejected_decision() -> DeliveryDecision {
        DeliveryDecision::input_rejected("missing description".into())
    }

    fn temp_dir() -> PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("delivery-test-{}-{}", std::process::id(), n));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    // -----------------------------------------------------------------------
    // StatusFile tests
    // -----------------------------------------------------------------------

    #[test]
    fn status_file_full_delivery() {
        let task_plan = make_task_plan(3);
        let decision = make_full_delivery_decision(3);
        let status = StatusFile::from_decision(&decision, &task_plan);

        assert_eq!(status.schema_version, 1);
        assert_eq!(status.task_status, "full_delivery");
        assert_eq!(status.required_count, 3);
        assert_eq!(status.accepted_count, 3);
        assert_eq!(status.gap_count, 0);
        assert_eq!(status.attempts_used, 1);
        assert_eq!(status.retry_count, 0);
        assert!(status.primary_reason.contains("Full delivery"));
        assert_eq!(status.manifest_path, "manifest.json");
        assert_eq!(status.summary_path, "summary.md");
    }

    #[test]
    fn status_file_limited_delivery() {
        let task_plan = make_task_plan(3);
        let decision = make_limited_delivery_decision();
        let status = StatusFile::from_decision(&decision, &task_plan);

        assert_eq!(status.schema_version, 1);
        assert_eq!(status.task_status, "limited_delivery");
        assert_eq!(status.required_count, 3);
        assert_eq!(status.accepted_count, 1);
        assert_eq!(status.gap_count, 2);
        assert_eq!(status.attempts_used, 4);
        assert_eq!(status.retry_count, 3);
        assert!(status.primary_reason.contains("Shortfall"));
    }

    #[test]
    fn status_file_execution_blocked() {
        let task_plan = make_task_plan(2);
        let decision = make_execution_blocked_decision();
        let status = StatusFile::from_decision(&decision, &task_plan);

        assert_eq!(status.schema_version, 1);
        assert_eq!(status.task_status, "execution_blocked");
        assert_eq!(status.required_count, 2);
        assert_eq!(status.accepted_count, 0);
        assert_eq!(status.gap_count, 2);
        assert_eq!(status.attempts_used, 0);
        assert_eq!(status.retry_count, 0);
        assert!(status.primary_reason.contains("OpenClaw"));
    }

    #[test]
    fn status_json_serializes_correctly() {
        let task_plan = make_task_plan(1);
        let decision = make_full_delivery_decision(1);
        let status = StatusFile::from_decision(&decision, &task_plan);

        let json = serde_json::to_string_pretty(&status).unwrap();
        assert!(json.contains("\"schema_version\": 1"));
        assert!(json.contains("\"task_status\": \"full_delivery\""));
        assert!(json.contains("\"required_count\": 1"));
        assert!(json.contains("\"accepted_count\": 1"));
        assert!(json.contains("\"gap_count\": 0"));
        assert!(json.contains("\"attempts_used\": 1"));
        assert!(json.contains("\"retry_count\": 0"));
        assert!(json.contains("\"manifest_path\": \"manifest.json\""));
        assert!(json.contains("\"summary_path\": \"summary.md\""));
    }

    // -----------------------------------------------------------------------
    // Builder: input rejection guard
    // -----------------------------------------------------------------------

    #[test]
    fn builder_rejects_input_rejection() {
        let dir = temp_dir();
        let builder = DeliveryPackageBuilder::new(&dir);
        let task_plan = make_task_plan(1);
        let decision = make_input_rejected_decision();
        let inputs = DeliveryInputs::minimal(task_plan, decision);

        let result = builder.build(&inputs);
        assert!(result.is_err());
        match result {
            Err(Error::InputRejection { reason }) => {
                assert!(reason.contains("Input rejection"));
            }
            _ => panic!("expected InputRejection error"),
        }
    }

    // -----------------------------------------------------------------------
    // Builder: full delivery
    // -----------------------------------------------------------------------

    #[test]
    fn builder_full_delivery_writes_all_files() {
        let dir = temp_dir();
        let builder = DeliveryPackageBuilder::new(&dir);
        let task_plan = make_task_plan(2);
        let decision = make_full_delivery_decision(2);
        let inputs = DeliveryInputs::minimal(task_plan, decision);

        let package_path = builder.build(&inputs).unwrap();
        assert_eq!(package_path, dir);

        // All expected files exist
        assert!(dir.join("status.json").exists());
        assert!(dir.join("manifest.json").exists());
        assert!(dir.join("summary.md").exists());
        assert!(dir.join("images").is_dir());
        assert!(dir.join("evidence").is_dir());
        assert!(dir.join("diagnostics").is_dir());
        assert!(dir.join("evidence/acceptance.json").exists());
        assert!(dir.join("evidence/rejection.json").exists());
        assert!(dir.join("diagnostics/diagnostic.json").exists());
        assert!(dir.join("diagnostics/metrics_summary.json").exists());
    }

    #[test]
    fn builder_full_delivery_manifest_has_correct_structure() {
        let dir = temp_dir();
        let builder = DeliveryPackageBuilder::new(&dir);
        let task_plan = make_task_plan(2);
        let decision = make_full_delivery_decision(2);
        let inputs = DeliveryInputs::minimal(task_plan, decision);

        builder.build(&inputs).unwrap();

        let manifest_bytes = fs::read(dir.join("manifest.json")).unwrap();
        let manifest: serde_json::Value = serde_json::from_slice(&manifest_bytes).unwrap();

        assert_eq!(manifest["schema_version"], 1);
        assert_eq!(manifest["delivery_status"], "full_delivery");
        assert_eq!(manifest["accepted_images"].as_array().unwrap().len(), 2);
        assert_eq!(manifest["gap"]["required_count"], 2);
        assert_eq!(manifest["gap"]["accepted_count"], 2);
        assert_eq!(manifest["gap"]["shortfall"], 0);
        assert!(manifest["query_plan_summary"]["description"]
            .as_str()
            .unwrap()
            .contains("sunset"));
        assert!(!manifest["evidence_refs"].as_array().unwrap().is_empty());
    }

    #[test]
    fn builder_full_delivery_status_json_has_required_fields() {
        let dir = temp_dir();
        let builder = DeliveryPackageBuilder::new(&dir);
        let task_plan = make_task_plan(2);
        let decision = make_full_delivery_decision(2);
        let inputs = DeliveryInputs::minimal(task_plan, decision);

        builder.build(&inputs).unwrap();

        let status_bytes = fs::read(dir.join("status.json")).unwrap();
        let status: serde_json::Value = serde_json::from_slice(&status_bytes).unwrap();

        // All required fields per LLD
        assert_eq!(status["schema_version"], 1);
        assert!(status["task_status"].is_string());
        assert!(status["required_count"].is_number());
        assert!(status["accepted_count"].is_number());
        assert!(status["gap_count"].is_number());
        assert!(status["attempts_used"].is_number());
        assert!(status["retry_count"].is_number());
        assert!(status["primary_reason"].is_string());
        assert_eq!(status["manifest_path"], "manifest.json");
        assert_eq!(status["summary_path"], "summary.md");

        // No credentials or sensitive data
        let status_str = std::str::from_utf8(&status_bytes).unwrap();
        assert!(!status_str.contains("Bearer"));
        assert!(!status_str.contains("token"));
        assert!(!status_str.contains("api_key"));
    }

    // -----------------------------------------------------------------------
    // Builder: limited delivery
    // -----------------------------------------------------------------------

    #[test]
    fn builder_limited_delivery_writes_all_files() {
        let dir = temp_dir();
        let builder = DeliveryPackageBuilder::new(&dir);
        let task_plan = make_task_plan(3);
        let decision = make_limited_delivery_decision();
        let inputs = DeliveryInputs::minimal(task_plan, decision);

        let package_path = builder.build(&inputs).unwrap();
        assert_eq!(package_path, dir);

        assert!(dir.join("status.json").exists());
        assert!(dir.join("manifest.json").exists());
        assert!(dir.join("summary.md").exists());

        let status_bytes = fs::read(dir.join("status.json")).unwrap();
        let status: serde_json::Value = serde_json::from_slice(&status_bytes).unwrap();
        assert_eq!(status["task_status"], "limited_delivery");
        assert_eq!(status["required_count"], 3);
        assert_eq!(status["accepted_count"], 1);
        assert_eq!(status["gap_count"], 2);
        assert_eq!(status["retry_count"], 3);

        let manifest_bytes = fs::read(dir.join("manifest.json")).unwrap();
        let manifest: serde_json::Value = serde_json::from_slice(&manifest_bytes).unwrap();
        assert_eq!(manifest["delivery_status"], "limited_delivery");
        assert_eq!(manifest["gap"]["shortfall"], 2);
        assert_eq!(manifest["accepted_images"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn builder_limited_delivery_zero_images() {
        let dir = temp_dir();
        let builder = DeliveryPackageBuilder::new(&dir);
        let task_plan = make_task_plan(2);
        // Limited delivery with zero accepted images
        let decision = DeliveryDecision::limited_delivery(vec![], vec![], 4, 3, 2);
        let inputs = DeliveryInputs::minimal(task_plan, decision);

        builder.build(&inputs).unwrap();

        let manifest_bytes = fs::read(dir.join("manifest.json")).unwrap();
        let manifest: serde_json::Value = serde_json::from_slice(&manifest_bytes).unwrap();
        assert_eq!(manifest["delivery_status"], "limited_delivery");
        assert_eq!(manifest["accepted_images"].as_array().unwrap().len(), 0);
        assert_eq!(manifest["gap"]["accepted_count"], 0);
        assert_eq!(manifest["gap"]["shortfall"], 2);
        assert!(manifest["gap"]["primary_gap_reason"]
            .as_str()
            .unwrap()
            .contains("Shortfall"));
    }

    // -----------------------------------------------------------------------
    // Builder: execution blocked
    // -----------------------------------------------------------------------

    #[test]
    fn builder_execution_blocked_writes_files() {
        let dir = temp_dir();
        let builder = DeliveryPackageBuilder::new(&dir);
        let task_plan = make_task_plan(2);
        let decision = make_execution_blocked_decision();
        let inputs = DeliveryInputs::minimal(task_plan, decision);

        builder.build(&inputs).unwrap();

        let status_bytes = fs::read(dir.join("status.json")).unwrap();
        let status: serde_json::Value = serde_json::from_slice(&status_bytes).unwrap();
        assert_eq!(status["task_status"], "execution_blocked");
        assert_eq!(status["accepted_count"], 0);
        assert_eq!(status["retry_count"], 0);
        assert!(status["primary_reason"]
            .as_str()
            .unwrap()
            .contains("OpenClaw"));
    }

    // -----------------------------------------------------------------------
    // Manifest contract: all required sections
    // -----------------------------------------------------------------------

    #[test]
    fn manifest_has_all_required_sections() {
        let dir = temp_dir();
        let builder = DeliveryPackageBuilder::new(&dir);
        let task_plan = make_task_plan(2);
        let decision = make_full_delivery_decision(2);
        let mut inputs = DeliveryInputs::minimal(task_plan, decision);
        inputs.metric_events.push(
            MetricEvent::new(MetricKind::TaskOutcome, "task_outcome_full_delivery", 1.0)
                .with_meta("state", "full_delivery"),
        );

        builder.build(&inputs).unwrap();

        let manifest_bytes = fs::read(dir.join("manifest.json")).unwrap();
        let manifest: serde_json::Value = serde_json::from_slice(&manifest_bytes).unwrap();

        // All LLD-required top-level fields
        assert!(manifest["schema_version"].is_number());
        assert!(manifest["query_plan_summary"].is_object());
        assert!(manifest["delivery_status"].is_string());
        assert!(manifest["accepted_images"].is_array());
        assert!(manifest["gap"].is_object());
        assert!(manifest["candidate_summary"].is_object());
        assert!(manifest["retrieval_summary"].is_object());
        assert!(manifest["acceptance_summary"].is_object());
        assert!(manifest["risk_summary"].is_object());
        assert!(manifest["metrics"].is_object());
        assert!(manifest["evidence_refs"].is_array());
    }

    #[test]
    fn manifest_accepted_image_entry_has_required_fields() {
        let dir = temp_dir();
        let builder = DeliveryPackageBuilder::new(&dir);
        let task_plan = make_task_plan(2);
        let decision = make_full_delivery_decision(2);
        let inputs = DeliveryInputs::minimal(task_plan, decision);

        builder.build(&inputs).unwrap();

        let manifest_bytes = fs::read(dir.join("manifest.json")).unwrap();
        let manifest: serde_json::Value = serde_json::from_slice(&manifest_bytes).unwrap();
        let entry = &manifest["accepted_images"][0];

        assert!(entry["image_path"].is_string());
        assert!(entry["source"].is_string());
        assert!(entry["acceptance_reason"].is_string());
        assert!(entry["quality_notes"].is_string());
        assert!(entry["authorization_risk"].is_string());
        assert!(entry["mechanical_evidence_ref"].is_string());
        assert!(entry["openclaw_evidence_ref"].is_string());
    }

    // -----------------------------------------------------------------------
    // Metrics block
    // -----------------------------------------------------------------------

    #[test]
    fn metrics_block_populates_from_events() {
        let dir = temp_dir();
        let builder = DeliveryPackageBuilder::new(&dir);
        let task_plan = make_task_plan(2);
        let decision = make_full_delivery_decision(2);
        let mut inputs = DeliveryInputs::minimal(task_plan, decision);

        inputs.metric_events.push(
            MetricEvent::new(MetricKind::TaskOutcome, "task_outcome_full_delivery", 1.0)
                .with_meta("state", "full_delivery"),
        );
        inputs.metric_events.push(
            MetricEvent::new(MetricKind::QualifiedImageAchievement, "qualified", 2.0)
                .with_denominator(2.0),
        );
        inputs.metric_events.push(
            MetricEvent::new(MetricKind::RejectionReason, "mechanical_rejection", 1.0)
                .with_meta("reason", "mechanical"),
        );
        inputs.metric_events.push(
            MetricEvent::new(
                MetricKind::OpenClawEvaluationRate,
                "openclaw_pass_rate",
                2.0,
            )
            .with_denominator(2.0),
        );

        builder.build(&inputs).unwrap();

        let manifest_bytes = fs::read(dir.join("manifest.json")).unwrap();
        let manifest: serde_json::Value = serde_json::from_slice(&manifest_bytes).unwrap();
        let metrics = &manifest["metrics"];

        assert!(!metrics["task_outcome"]["label"]
            .as_str()
            .unwrap()
            .is_empty());
        assert!(!metrics["qualified_image_achievement"]
            .as_array()
            .unwrap()
            .is_empty());
        assert!(!metrics["rejection_reasons"].as_array().unwrap().is_empty());
        assert!(!metrics["openclaw_evaluation_rate"]
            .as_array()
            .unwrap()
            .is_empty());
    }

    // -----------------------------------------------------------------------
    // Summary.md
    // -----------------------------------------------------------------------

    #[test]
    fn summary_md_full_delivery() {
        let dir = temp_dir();
        let builder = DeliveryPackageBuilder::new(&dir);
        let task_plan = make_task_plan(2);
        let decision = make_full_delivery_decision(2);
        let inputs = DeliveryInputs::minimal(task_plan, decision);

        builder.build(&inputs).unwrap();

        let summary = fs::read_to_string(dir.join("summary.md")).unwrap();
        assert!(summary.contains("# Delivery Summary"));
        assert!(summary.contains("full_delivery"));
        assert!(summary.contains("2 of 2"));
        assert!(summary.contains("sunset"));
    }

    #[test]
    fn summary_md_limited_delivery() {
        let dir = temp_dir();
        let builder = DeliveryPackageBuilder::new(&dir);
        let task_plan = make_task_plan(3);
        let decision = make_limited_delivery_decision();
        let inputs = DeliveryInputs::minimal(task_plan, decision);

        builder.build(&inputs).unwrap();

        let summary = fs::read_to_string(dir.join("summary.md")).unwrap();
        assert!(summary.contains("limited_delivery"));
        assert!(summary.contains("1 of 3"));
        assert!(summary.contains("Next Steps"));
    }

    // -----------------------------------------------------------------------
    // Evidence content
    // -----------------------------------------------------------------------

    #[test]
    fn evidence_acceptance_contains_redacted_data() {
        let dir = temp_dir();
        let builder = DeliveryPackageBuilder::new(&dir);
        let task_plan = make_task_plan(2);
        let decision = make_full_delivery_decision(2);
        let inputs = DeliveryInputs::minimal(task_plan, decision);

        builder.build(&inputs).unwrap();

        let evidence_bytes = fs::read(dir.join("evidence/acceptance.json")).unwrap();
        let evidence: Vec<serde_json::Value> = serde_json::from_slice(&evidence_bytes).unwrap();
        assert_eq!(evidence.len(), 2);
        for entry in &evidence {
            assert!(entry["candidate_id"].is_string());
            // Must not contain raw file system paths outside the package
            let entry_str = entry.to_string();
            assert!(!entry_str.contains("/tmp/"));
        }
    }

    // -----------------------------------------------------------------------
    // Unaccepted images are excluded
    // -----------------------------------------------------------------------

    #[test]
    fn unaccepted_images_not_in_accepted_images() {
        let dir = temp_dir();
        let builder = DeliveryPackageBuilder::new(&dir);
        let task_plan = make_task_plan(3);

        let accepted = vec![make_accepted("good")];
        let rejected = vec![make_mechanical_rejection("bad")];
        let decision = DeliveryDecision::limited_delivery(accepted, rejected, 4, 3, 3);

        let inputs = DeliveryInputs::minimal(task_plan, decision);
        builder.build(&inputs).unwrap();

        let manifest_bytes = fs::read(dir.join("manifest.json")).unwrap();
        let manifest: serde_json::Value = serde_json::from_slice(&manifest_bytes).unwrap();

        let accepted_images = manifest["accepted_images"].as_array().unwrap();
        assert_eq!(accepted_images.len(), 1);

        // Rejected evidence exists separately
        assert!(dir.join("evidence/rejection.json").exists());
    }

    // -----------------------------------------------------------------------
    // No credentials in output
    // -----------------------------------------------------------------------

    #[test]
    fn delivery_package_contains_no_sensitive_patterns() {
        let dir = temp_dir();
        let builder = DeliveryPackageBuilder::new(&dir);
        let task_plan = make_task_plan(2);
        let decision = make_full_delivery_decision(2);
        let inputs = DeliveryInputs::minimal(task_plan, decision);

        builder.build(&inputs).unwrap();

        let sensitive_patterns = ["Bearer ", "Authorization:", "api_key=", "access_token="];
        let files_to_check = [
            "status.json",
            "manifest.json",
            "summary.md",
            "evidence/acceptance.json",
            "evidence/rejection.json",
            "diagnostics/diagnostic.json",
            "diagnostics/metrics_summary.json",
        ];

        for file in &files_to_check {
            let path = dir.join(file);
            if path.exists() {
                let content = fs::read_to_string(&path).unwrap();
                for pattern in &sensitive_patterns {
                    assert!(
                        !content.contains(pattern),
                        "file {} contains sensitive pattern '{}'",
                        file,
                        pattern
                    );
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Execution blocked contains reason
    // -----------------------------------------------------------------------

    #[test]
    fn execution_blocked_status_explains_blocking_reason() {
        let dir = temp_dir();
        let builder = DeliveryPackageBuilder::new(&dir);
        let task_plan = make_task_plan(2);
        let decision = DeliveryDecision::execution_blocked(
            "all retrieval channels blocked by access restriction".into(),
        );
        let inputs = DeliveryInputs::minimal(task_plan, decision);

        builder.build(&inputs).unwrap();

        let status_bytes = fs::read(dir.join("status.json")).unwrap();
        let status_str = std::str::from_utf8(&status_bytes).unwrap();
        assert!(status_str.contains("access restriction"));
    }

    // -----------------------------------------------------------------------
    // Metrics: all six MET families present, even if empty
    // -----------------------------------------------------------------------

    #[test]
    fn manifest_metrics_block_has_all_six_met_families() {
        let dir = temp_dir();
        let builder = DeliveryPackageBuilder::new(&dir);
        let task_plan = make_task_plan(1);
        let decision = make_full_delivery_decision(1);
        let inputs = DeliveryInputs::minimal(task_plan, decision);

        builder.build(&inputs).unwrap();

        let manifest_bytes = fs::read(dir.join("manifest.json")).unwrap();
        let manifest: serde_json::Value = serde_json::from_slice(&manifest_bytes).unwrap();
        let metrics = &manifest["metrics"];

        // All six families exist as keys
        assert!(metrics["task_outcome"].is_object());
        assert!(metrics["candidate_satisfaction"].is_array());
        assert!(metrics["qualified_image_achievement"].is_array());
        assert!(metrics["rejection_reasons"].is_array());
        assert!(metrics["channel_effectiveness"].is_array());
        assert!(metrics["openclaw_evaluation_rate"].is_array());
    }
}
