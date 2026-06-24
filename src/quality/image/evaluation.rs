//! Image evaluation request, conclusion, and normalization.
//!
//! Defines the structured types for OpenClaw image evaluation and the
//! normalization of evaluation conclusions into `ImageAcceptanceDecision`.
//!
//! This mirrors the candidate evaluation flow but operates on actual
//! retrieved image artifacts rather than candidate metadata.
//!
//! References: PRD §校验与评价产品要求, HLD §主观评价架构边界,
//! `docs/design/TASK-006-image-acceptance-orchestrator-design.md`

use crate::domain::image::{ImageAcceptanceDecision, ImageMechanicalEvidence, ImageRecord};
use crate::domain::query_plan::{AuthorizationPreference, ContentConstraints, QualityTier};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Image evaluation request
// ---------------------------------------------------------------------------

/// Structured input for OpenClaw image evaluation.
///
/// Bundles the actual image record, mechanical evidence, and QueryPlan
/// context so OpenClaw can make an informed subjective judgment about
/// the image's quality, relevance, and suitability.
///
/// Per the security boundary, this request MUST NOT contain provider
/// credentials or local sensitive configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageEvaluationRequest {
    /// The image being evaluated.
    pub image: ImageRecord,

    /// Mechanical evidence (blocking findings are empty at this point —
    /// blocked images never reach subjective evaluation).
    pub mechanical_evidence: ImageMechanicalEvidence,

    /// The semantic description from the validated QueryPlan.
    pub query_description: String,

    /// Quality tier preference.
    pub quality_tier: QualityTier,

    /// Content constraints (must-include / must-avoid).
    pub content_constraints: ContentConstraints,

    /// Authorization risk preference.
    pub authorization_preference: AuthorizationPreference,

    /// Candidate id for source traceability.
    pub candidate_id: String,

    /// Provider identity for source traceability.
    pub provider_id: String,
}

impl ImageEvaluationRequest {
    /// Build an evaluation request from an image and its context.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        image: ImageRecord,
        mechanical_evidence: ImageMechanicalEvidence,
        query_description: String,
        quality_tier: QualityTier,
        content_constraints: ContentConstraints,
        authorization_preference: AuthorizationPreference,
        candidate_id: String,
        provider_id: String,
    ) -> Self {
        Self {
            image,
            mechanical_evidence,
            query_description,
            quality_tier,
            content_constraints,
            authorization_preference,
            candidate_id,
            provider_id,
        }
    }
}

// ---------------------------------------------------------------------------
// Image evaluation conclusion
// ---------------------------------------------------------------------------

/// The conclusion of an OpenClaw image evaluation.
///
/// This is the normalized result that the image acceptance gate maps into
/// `ImageAcceptanceDecision`. Four outcomes are actionable:
///
/// | Conclusion | Action |
/// |---|---|
/// | `Approve` | Image counts as qualified. |
/// | `Reject` | Image does not count; rejection evidence recorded. |
/// | `Uncertain` | Image does not count; does NOT block the task. |
/// | `Unexecutable` | OpenClaw unavailable; task enters execution-blocked. |
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImageEvaluationConclusion {
    /// OpenClaw explicitly approves the image.
    Approve {
        /// Quality/relevance notes from OpenClaw.
        notes: Option<String>,
    },

    /// OpenClaw explicitly rejects the image.
    Reject {
        /// Reason for rejection.
        reason: String,
    },

    /// OpenClaw cannot decide — the image is ambiguous or boundary.
    /// Must NOT count as qualified. Does NOT block the task.
    Uncertain {
        /// Why OpenClaw was uncertain.
        reason: String,
    },

    /// OpenClaw evaluation could not be performed. This is a production
    /// dependency failure and causes the task to enter execution-blocked.
    Unexecutable {
        /// Why OpenClaw could not evaluate.
        reason: String,
    },
}

impl ImageEvaluationConclusion {
    /// Returns `true` iff OpenClaw explicitly approved the image.
    pub fn is_approved(&self) -> bool {
        matches!(self, Self::Approve { .. })
    }

    /// Returns `true` iff this conclusion means the task cannot proceed.
    pub fn is_unexecutable(&self) -> bool {
        matches!(self, Self::Unexecutable { .. })
    }

    /// Human-readable label for metrics / diagnostics.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Approve { .. } => "approve",
            Self::Reject { .. } => "reject",
            Self::Uncertain { .. } => "uncertain",
            Self::Unexecutable { .. } => "unexecutable",
        }
    }
}

// ---------------------------------------------------------------------------
// Image execution blocking fact
// ---------------------------------------------------------------------------

/// A fact that records why image evaluation entered execution-blocked state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageExecutionBlockingFact {
    /// Which dependency caused the block.
    pub dependency: String,

    /// Why it is blocked.
    pub reason: String,

    /// Whether this is a permanent block.
    pub is_permanent: bool,

    /// How many images were pending evaluation when the block occurred.
    pub pending_image_count: usize,
}

impl ImageExecutionBlockingFact {
    /// Create an execution blocking fact for OpenClaw image evaluation unavailability.
    pub fn openclaw_unavailable(reason: impl Into<String>, pending_count: usize) -> Self {
        Self {
            dependency: "OpenClaw".into(),
            reason: reason.into(),
            is_permanent: true,
            pending_image_count: pending_count,
        }
    }
}

// ---------------------------------------------------------------------------
// Conclusion → ImageAcceptanceDecision normalization
// ---------------------------------------------------------------------------

/// Normalize an OpenClaw image evaluation conclusion into an
/// `ImageAcceptanceDecision`.
///
/// # Mapping
///
/// | Conclusion | Decision |
/// |---|---|
/// | `Approve` | `ImageAcceptanceDecision::Accepted` |
/// | `Reject` | `ImageAcceptanceDecision::SubjectivelyRejected` |
/// | `Uncertain` | `ImageAcceptanceDecision::SubjectivelyRejected` |
/// | `Unexecutable` | `ImageAcceptanceDecision::ExecutionBlocked` |
///
/// Note: `Uncertain` maps to `SubjectivelyRejected` because the image does
/// NOT count as qualified, but it is not a task-level execution block.
pub fn normalize_image_conclusion(
    image: ImageRecord,
    mechanical_evidence: ImageMechanicalEvidence,
    conclusion: ImageEvaluationConclusion,
) -> ImageAcceptanceDecision {
    match conclusion {
        ImageEvaluationConclusion::Approve { notes } => ImageAcceptanceDecision::Accepted {
            image,
            notes: notes.unwrap_or_default(),
            vlm_evidence: None,
        },
        ImageEvaluationConclusion::Reject { reason } => {
            ImageAcceptanceDecision::SubjectivelyRejected {
                image,
                mechanical_evidence,
                reason,
            }
        }
        ImageEvaluationConclusion::Uncertain { reason } => {
            ImageAcceptanceDecision::SubjectivelyRejected {
                image,
                mechanical_evidence,
                reason: format!("OpenClaw uncertain: {}", reason),
            }
        }
        ImageEvaluationConclusion::Unexecutable { reason } => {
            ImageAcceptanceDecision::ExecutionBlocked {
                reason: format!("OpenClaw image evaluation unavailable: {}", reason),
            }
        }
    }
}

/// Normalize a batch of image evaluation conclusions.
///
/// Each mechanically-passed image is paired with its evaluation conclusion
/// to produce an `ImageAcceptanceDecision`.
pub fn normalize_image_conclusions(
    images: Vec<(ImageRecord, ImageMechanicalEvidence)>,
    conclusions: Vec<ImageEvaluationConclusion>,
) -> Vec<ImageAcceptanceDecision> {
    images
        .into_iter()
        .zip(conclusions)
        .map(|((image, evidence), conclusion)| {
            normalize_image_conclusion(image, evidence, conclusion)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::candidate::ImageDimensions;

    fn make_image(id: &str) -> ImageRecord {
        ImageRecord {
            candidate_id: id.into(),
            local_path: format!("/tmp/{}.jpg", id),
            content_type: Some("image/jpeg".into()),
            file_size_bytes: 4096,
            dimensions: Some(ImageDimensions {
                width: 800,
                height: 600,
            }),
            reference_metrics: vec![],
        }
    }

    fn make_mechanical_evidence() -> ImageMechanicalEvidence {
        ImageMechanicalEvidence {
            blocking_findings: vec![],
            reference_findings: vec![],
        }
    }

    #[test]
    fn approve_normalizes_to_accepted() {
        let img = make_image("img-1");
        let mech = make_mechanical_evidence();
        let conclusion = ImageEvaluationConclusion::Approve {
            notes: Some("great match".into()),
        };
        let decision = normalize_image_conclusion(img, mech, conclusion);
        assert!(decision.is_accepted());
        match decision {
            ImageAcceptanceDecision::Accepted { notes, .. } => {
                assert_eq!(notes, "great match");
            }
            _ => panic!("expected Accepted"),
        }
    }

    #[test]
    fn reject_normalizes_to_subjectively_rejected() {
        let img = make_image("img-2");
        let mech = make_mechanical_evidence();
        let conclusion = ImageEvaluationConclusion::Reject {
            reason: "does not match description".into(),
        };
        let decision = normalize_image_conclusion(img, mech, conclusion);
        assert!(!decision.is_accepted());
        match decision {
            ImageAcceptanceDecision::SubjectivelyRejected { reason, .. } => {
                assert!(reason.contains("does not match"));
            }
            _ => panic!("expected SubjectivelyRejected"),
        }
    }

    #[test]
    fn uncertain_normalizes_to_subjectively_rejected() {
        let img = make_image("img-3");
        let mech = make_mechanical_evidence();
        let conclusion = ImageEvaluationConclusion::Uncertain {
            reason: "ambiguous content".into(),
        };
        let decision = normalize_image_conclusion(img, mech, conclusion);
        assert!(!decision.is_accepted());
        match decision {
            ImageAcceptanceDecision::SubjectivelyRejected { reason, .. } => {
                assert!(reason.contains("OpenClaw uncertain"));
            }
            _ => panic!("expected SubjectivelyRejected"),
        }
    }

    #[test]
    fn unexecutable_normalizes_to_execution_blocked() {
        let img = make_image("img-4");
        let mech = make_mechanical_evidence();
        let conclusion = ImageEvaluationConclusion::Unexecutable {
            reason: "no endpoint configured".into(),
        };
        let decision = normalize_image_conclusion(img, mech, conclusion);
        assert!(!decision.is_accepted());
        match decision {
            ImageAcceptanceDecision::ExecutionBlocked { reason } => {
                assert!(reason.contains("OpenClaw image evaluation unavailable"));
            }
            _ => panic!("expected ExecutionBlocked"),
        }
    }

    #[test]
    fn batch_normalization_covers_all_outcomes() {
        let images = vec![
            (make_image("a"), make_mechanical_evidence()),
            (make_image("b"), make_mechanical_evidence()),
            (make_image("c"), make_mechanical_evidence()),
            (make_image("d"), make_mechanical_evidence()),
        ];
        let conclusions = vec![
            ImageEvaluationConclusion::Approve { notes: None },
            ImageEvaluationConclusion::Reject {
                reason: "bad".into(),
            },
            ImageEvaluationConclusion::Uncertain {
                reason: "maybe".into(),
            },
            ImageEvaluationConclusion::Unexecutable {
                reason: "down".into(),
            },
        ];

        let decisions = normalize_image_conclusions(images, conclusions);
        assert_eq!(decisions.len(), 4);

        assert!(decisions[0].is_accepted());
        assert!(!decisions[1].is_accepted());
        assert!(!decisions[2].is_accepted());
        assert!(!decisions[3].is_accepted());

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
    fn approve_without_notes_uses_default() {
        let img = make_image("img-5");
        let mech = make_mechanical_evidence();
        let conclusion = ImageEvaluationConclusion::Approve { notes: None };
        let decision = normalize_image_conclusion(img, mech, conclusion);
        match decision {
            ImageAcceptanceDecision::Accepted { notes, .. } => {
                assert_eq!(notes, "");
            }
            _ => panic!("expected Accepted"),
        }
    }

    #[test]
    fn conclusion_labels() {
        assert_eq!(
            ImageEvaluationConclusion::Approve { notes: None }.label(),
            "approve"
        );
        assert_eq!(
            ImageEvaluationConclusion::Reject { reason: "r".into() }.label(),
            "reject"
        );
        assert_eq!(
            ImageEvaluationConclusion::Uncertain { reason: "u".into() }.label(),
            "uncertain"
        );
        assert_eq!(
            ImageEvaluationConclusion::Unexecutable { reason: "x".into() }.label(),
            "unexecutable"
        );
    }

    #[test]
    fn evaluation_request_bundles_context() {
        let img = make_image("img-1");
        let mech = make_mechanical_evidence();
        let req = ImageEvaluationRequest::new(
            img,
            mech,
            "sunset over mountains".into(),
            QualityTier::General,
            ContentConstraints::default(),
            AuthorizationPreference::Default,
            "cand-1".into(),
            "test-provider".into(),
        );

        assert_eq!(req.query_description, "sunset over mountains");
        assert_eq!(req.quality_tier, QualityTier::General);
        assert_eq!(req.candidate_id, "cand-1");
        assert_eq!(req.provider_id, "test-provider");
    }

    #[test]
    fn execution_blocking_fact() {
        let fact = ImageExecutionBlockingFact::openclaw_unavailable("no production endpoint", 5);
        assert_eq!(fact.dependency, "OpenClaw");
        assert!(fact.is_permanent);
        assert_eq!(fact.pending_image_count, 5);
    }

    #[test]
    fn conclusion_serialization_round_trip() {
        let conclusions = vec![
            ImageEvaluationConclusion::Approve {
                notes: Some("good".into()),
            },
            ImageEvaluationConclusion::Reject {
                reason: "bad".into(),
            },
            ImageEvaluationConclusion::Uncertain {
                reason: "maybe".into(),
            },
            ImageEvaluationConclusion::Unexecutable {
                reason: "down".into(),
            },
        ];

        for original in &conclusions {
            let json = serde_json::to_string(original).expect("serialize");
            let parsed: ImageEvaluationConclusion =
                serde_json::from_str(&json).expect("deserialize");
            assert_eq!(*original, parsed);
        }
    }
}
