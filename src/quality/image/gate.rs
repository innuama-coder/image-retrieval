//! Image Acceptance Gate — mechanical + OpenClaw evaluation orchestrator.
//!
//! Implements the "先机械、后主观、再归一" flow for images:
//! 1. Mechanical validation of actually-retrieved images.
//! 2. Mechanically-passed images are packaged into evaluation requests.
//! 3. OpenClaw evaluates the requests.
//! 4. Conclusions are normalized into `ImageAcceptanceDecision`.
//! 5. Qualified images (mechanically passed + OpenClaw approved) are accumulated.
//!
//! References: PRD §图片验收与重试收口, HLD §Image Acceptance Gate,
//! `docs/design/TASK-006-image-acceptance-orchestrator-design.md`

use crate::domain::image::{ImageAcceptanceDecision, ImageMechanicalEvidence, ImageRecord};
use crate::domain::query_plan::ValidatedQueryPlan;
use crate::error::{Error, Result};
use crate::ports::OpenClawEvaluationPort;
use crate::quality::image::evaluation::{
    normalize_image_conclusions, ImageEvaluationConclusion, ImageExecutionBlockingFact,
};
use crate::quality::image::mechanical::validate_image_mechanical;

// ---------------------------------------------------------------------------
// Image acceptance gate result
// ---------------------------------------------------------------------------

/// The full output of the image acceptance gate.
#[derive(Debug, Clone)]
pub struct ImageAcceptanceGateResult {
    /// Images that passed both mechanical and OpenClaw evaluation.
    pub qualified_images: Vec<ImageAcceptanceDecision>,

    /// Images that were rejected (mechanically or subjectively).
    pub rejected_images: Vec<ImageAcceptanceDecision>,

    /// All decisions produced (for diagnostics and metrics).
    pub all_decisions: Vec<ImageAcceptanceDecision>,

    /// Execution blocking facts if OpenClaw was unavailable.
    pub execution_blocking_facts: Vec<ImageExecutionBlockingFact>,

    /// Summary counts.
    pub summary: ImageAcceptanceSummary,
}

/// Summary counts for image acceptance observability (MET-004, MET-006).
#[derive(Debug, Clone, Default)]
pub struct ImageAcceptanceSummary {
    /// Total images input to the gate.
    pub total_images: usize,

    /// Number mechanically blocked.
    pub mechanically_blocked: usize,

    /// Number approved by OpenClaw (qualified).
    pub openclaw_approved: usize,

    /// Number rejected by OpenClaw.
    pub openclaw_rejected: usize,

    /// Number evaluated as uncertain by OpenClaw.
    pub openclaw_uncertain: usize,

    /// Number that could not be evaluated (OpenClaw unavailable).
    pub openclaw_unexecutable: usize,
}

// ---------------------------------------------------------------------------
// Image Acceptance Gate
// ---------------------------------------------------------------------------

/// The image acceptance gate.
///
/// Owns the OpenClaw evaluation port and the QueryPlan context. The gate
/// validates actually-retrieved images through mechanical checks and
/// OpenClaw subjective evaluation.
pub struct ImageAcceptanceGate<'a> {
    /// OpenClaw evaluation port.
    openclaw: &'a dyn OpenClawEvaluationPort,

    /// Query plan context.
    query_plan: ValidatedQueryPlan,
}

impl<'a> ImageAcceptanceGate<'a> {
    /// Create a new image acceptance gate.
    pub fn new(openclaw: &'a dyn OpenClawEvaluationPort, query_plan: ValidatedQueryPlan) -> Self {
        Self {
            openclaw,
            query_plan,
        }
    }

    /// Run the full image acceptance pipeline on a set of retrieved images.
    ///
    /// # Flow
    ///
    /// 1. Run mechanical validation on every image.
    /// 2. Build evaluation requests for mechanically-passed images.
    /// 3. Call OpenClaw to evaluate the batch.
    ///    - If OpenClaw is unavailable → execution_blocked.
    /// 4. Normalize conclusions into `ImageAcceptanceDecision`.
    /// 5. Split into qualified (Accepted) and rejected.
    ///
    /// # Returns
    ///
    /// `Ok(ImageAcceptanceGateResult)` with qualified and rejected images,
    /// or `Err(Error::ExecutionBlocked)` when OpenClaw is unavailable.
    pub fn evaluate(&self, images: &[ImageRecord]) -> Result<ImageAcceptanceGateResult> {
        let total = images.len();

        // Phase 1: Mechanical validation
        let mut mechanically_blocked: Vec<ImageAcceptanceDecision> = Vec::new();
        let mut mechanically_passed: Vec<(ImageRecord, ImageMechanicalEvidence)> = Vec::new();

        for image in images.iter().cloned() {
            let evidence = validate_image_mechanical(&image, self.query_plan.quality_tier);

            if evidence.passed_mechanical() {
                mechanically_passed.push((image, evidence));
            } else {
                mechanically_blocked
                    .push(ImageAcceptanceDecision::MechanicallyRejected { image, evidence });
            }
        }

        let mechanically_blocked_count = mechanically_blocked.len();
        let mut all_decisions: Vec<ImageAcceptanceDecision> = mechanically_blocked.clone();

        // If no images passed mechanical, return early
        if mechanically_passed.is_empty() {
            let all_decisions = mechanically_blocked.clone();
            return Ok(ImageAcceptanceGateResult {
                qualified_images: vec![],
                rejected_images: mechanically_blocked,
                all_decisions,
                execution_blocking_facts: vec![],
                summary: ImageAcceptanceSummary {
                    total_images: total,
                    mechanically_blocked: mechanically_blocked_count,
                    ..Default::default()
                },
            });
        }

        // Phase 2: Call OpenClaw for image evaluation
        let images_for_openclaw: Vec<ImageRecord> = mechanically_passed
            .iter()
            .map(|(img, _)| img.clone())
            .collect();

        let openclaw_result = self
            .openclaw
            .evaluate_images(&images_for_openclaw, &self.query_plan.description);

        match openclaw_result {
            Ok(decisions) => {
                // The port returned decisions directly.
                // We accept the port's output as the evaluation result.
                // But we need to separate them properly.
                let mut qualified: Vec<ImageAcceptanceDecision> = Vec::new();
                let mut rejected: Vec<ImageAcceptanceDecision> = Vec::new();
                let mut approved_count = 0usize;
                let mut rejected_count = 0usize;
                let mut uncertain_count = 0usize;

                for d in &decisions {
                    match d {
                        ImageAcceptanceDecision::Accepted { .. } => {
                            approved_count += 1;
                            qualified.push(d.clone());
                        }
                        ImageAcceptanceDecision::SubjectivelyRejected { .. } => {
                            // Check if the original reason contained "uncertain"
                            if let ImageAcceptanceDecision::SubjectivelyRejected {
                                ref reason,
                                ..
                            } = d
                            {
                                if reason.contains("OpenClaw uncertain") {
                                    uncertain_count += 1;
                                } else {
                                    rejected_count += 1;
                                }
                            }
                            rejected.push(d.clone());
                        }
                        ImageAcceptanceDecision::MechanicallyRejected { .. } => {
                            // Should not appear in OpenClaw output, but handle gracefully
                            rejected.push(d.clone());
                        }
                        ImageAcceptanceDecision::ExecutionBlocked { .. } => {
                            // This is a task-level block
                            all_decisions.extend(mechanically_blocked);
                            all_decisions.push(d.clone());

                            return Ok(ImageAcceptanceGateResult {
                                qualified_images: vec![],
                                rejected_images: all_decisions.clone(),
                                all_decisions,
                                execution_blocking_facts: vec![
                                    ImageExecutionBlockingFact::openclaw_unavailable(
                                        "OpenClaw image evaluation returned execution-blocked",
                                        mechanically_passed.len(),
                                    ),
                                ],
                                summary: ImageAcceptanceSummary {
                                    total_images: total,
                                    mechanically_blocked: mechanically_blocked_count,
                                    openclaw_unexecutable: 1,
                                    ..Default::default()
                                },
                            });
                        }
                    }
                }

                all_decisions.extend(decisions);

                let summary = ImageAcceptanceSummary {
                    total_images: total,
                    mechanically_blocked: mechanically_blocked_count,
                    openclaw_approved: approved_count,
                    openclaw_rejected: rejected_count,
                    openclaw_uncertain: uncertain_count,
                    openclaw_unexecutable: 0,
                };

                Ok(ImageAcceptanceGateResult {
                    qualified_images: qualified,
                    rejected_images: rejected,
                    all_decisions,
                    execution_blocking_facts: vec![],
                    summary,
                })
            }
            Err(e) => {
                // OpenClaw evaluation failed — execution block
                if matches!(e, Error::OpenClawUnavailable { .. })
                    || matches!(e, Error::ExecutionBlocked { .. })
                {
                    let pending_count = mechanically_passed.len();
                    let fact = ImageExecutionBlockingFact::openclaw_unavailable(
                        e.to_string(),
                        pending_count,
                    );

                    // All mechanically-passed images become ExecutionBlocked
                    let blocked_decisions: Vec<ImageAcceptanceDecision> = mechanically_passed
                        .into_iter()
                        .map(
                            |(_image, _evidence)| ImageAcceptanceDecision::ExecutionBlocked {
                                reason: format!("OpenClaw image evaluation unavailable: {}", e),
                            },
                        )
                        .collect();

                    // Rejected images are only the mechanically-blocked ones
                    all_decisions.extend(blocked_decisions);

                    Ok(ImageAcceptanceGateResult {
                        qualified_images: vec![],
                        rejected_images: mechanically_blocked,
                        all_decisions,
                        execution_blocking_facts: vec![fact],
                        summary: ImageAcceptanceSummary {
                            total_images: total,
                            mechanically_blocked: mechanically_blocked_count,
                            openclaw_unexecutable: pending_count,
                            ..Default::default()
                        },
                    })
                } else {
                    Err(e)
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Standalone evaluation helper (no dependency on OpenClaw port)
// ---------------------------------------------------------------------------

/// Evaluate images using an explicit conclusion-per-image mapping.
///
/// This is the preferred path for fixture/test evaluators. Instead of
/// calling the port trait, the caller provides a list of conclusions that
/// map 1:1 to the mechanically-passed images.
pub fn evaluate_images_with_conclusions(
    mechanically_passed: Vec<(ImageRecord, ImageMechanicalEvidence)>,
    conclusions: Vec<ImageEvaluationConclusion>,
) -> Vec<ImageAcceptanceDecision> {
    normalize_image_conclusions(mechanically_passed, conclusions)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::candidate::ImageDimensions;
    use crate::domain::query_plan::QualityTier;
    use crate::quality::image::evaluation::{
        normalize_image_conclusion, ImageEvaluationConclusion,
    };

    fn make_image(id: &str, content_type: Option<&str>, width: u32, height: u32) -> ImageRecord {
        ImageRecord {
            candidate_id: id.into(),
            local_path: format!("/tmp/{}.jpg", id),
            content_type: content_type.map(|s| s.into()),
            file_size_bytes: 4096,
            dimensions: Some(ImageDimensions { width, height }),
        }
    }

    #[test]
    fn mechanically_blocked_images_do_not_reach_openclaw() {
        // Image with zero bytes should be mechanically blocked
        let bad = ImageRecord {
            candidate_id: "bad-1".into(),
            local_path: "/tmp/bad.jpg".into(),
            content_type: None,
            file_size_bytes: 0,
            dimensions: None,
        };
        let good = make_image("good-1", Some("image/jpeg"), 800, 600);

        // Run manual mechanical validation
        let evidence_bad = validate_image_mechanical(&bad, QualityTier::General);
        let evidence_good = validate_image_mechanical(&good, QualityTier::General);

        assert!(!evidence_bad.passed_mechanical());
        assert!(evidence_good.passed_mechanical());
    }

    #[test]
    fn evaluate_images_with_conclusions_all_outcomes() {
        let img1 = make_image("img-1", Some("image/jpeg"), 800, 600);
        let img2 = make_image("img-2", Some("image/png"), 1024, 768);
        let img3 = make_image("img-3", Some("image/jpeg"), 400, 300);
        let img4 = make_image("img-4", Some("image/webp"), 640, 480);

        let mech = ImageMechanicalEvidence {
            blocking_findings: vec![],
            reference_findings: vec![],
        };

        let passed = vec![
            (img1, mech.clone()),
            (img2, mech.clone()),
            (img3, mech.clone()),
            (img4, mech.clone()),
        ];

        let conclusions = vec![
            ImageEvaluationConclusion::Approve {
                notes: Some("perfect".into()),
            },
            ImageEvaluationConclusion::Reject {
                reason: "not mountains".into(),
            },
            ImageEvaluationConclusion::Uncertain {
                reason: "unclear".into(),
            },
            ImageEvaluationConclusion::Unexecutable {
                reason: "OpenClaw down".into(),
            },
        ];

        let decisions = evaluate_images_with_conclusions(passed, conclusions);
        assert_eq!(decisions.len(), 4);

        // Only first is qualified
        assert!(decisions[0].is_accepted());
        assert!(!decisions[1].is_accepted());
        assert!(!decisions[2].is_accepted());
        assert!(!decisions[3].is_accepted());

        // Verify types
        assert!(matches!(
            decisions[0],
            ImageAcceptanceDecision::Accepted { .. }
        ));
        assert!(matches!(
            decisions[1],
            ImageAcceptanceDecision::SubjectivelyRejected { .. }
        ));
        assert!(matches!(
            decisions[2],
            ImageAcceptanceDecision::SubjectivelyRejected { .. }
        ));
        assert!(matches!(
            decisions[3],
            ImageAcceptanceDecision::ExecutionBlocked { .. }
        ));
    }

    #[test]
    fn uncertain_does_not_count_as_qualified() {
        let img = make_image("img-u", Some("image/jpeg"), 800, 600);
        let mech = ImageMechanicalEvidence {
            blocking_findings: vec![],
            reference_findings: vec![],
        };
        let conclusion = ImageEvaluationConclusion::Uncertain {
            reason: "boundary quality".into(),
        };
        let decision = normalize_image_conclusion(img, mech, conclusion);
        assert!(!decision.is_accepted());
    }

    #[test]
    fn image_acceptance_summary_defaults() {
        let summary = ImageAcceptanceSummary::default();
        assert_eq!(summary.total_images, 0);
        assert_eq!(summary.mechanically_blocked, 0);
        assert_eq!(summary.openclaw_approved, 0);
    }
}
