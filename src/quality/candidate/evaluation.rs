//! Candidate evaluation request, conclusion, and normalization.
//!
//! Defines the structured types for OpenClaw candidate evaluation and
//! the normalization of evaluation conclusions into `CandidateDecision`.
//!
//! References: PRD §校验与评价产品要求, HLD §主观评价架构边界,
//! `docs/design/TASK-004-candidate-quality-openclaw-design.md`

use crate::domain::candidate::{CandidateDecision, CandidateRecord};
use crate::domain::query_plan::{AuthorizationPreference, ContentConstraints, QualityTier};
use crate::quality::candidate::mechanical::CandidateMechanicalEvidence;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Candidate evaluation request
// ---------------------------------------------------------------------------

/// Structured input for OpenClaw candidate evaluation.
///
/// This is the contract that the candidate quality gate passes to
/// `OpenClawEvaluationPort::evaluate_candidates`. It bundles the candidate
/// record, mechanical evidence, and QueryPlan context so OpenClaw can
/// make an informed subjective judgment.
///
/// Per the security boundary, this request MUST NOT contain provider
/// credentials or local sensitive configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateEvaluationRequest {
    /// The candidate being evaluated.
    pub candidate: CandidateRecord,

    /// Mechanical evidence (blocking findings are empty at this point —
    /// blocked candidates never reach evaluation).
    pub mechanical_evidence: CandidateMechanicalEvidence,

    /// The semantic description from the validated QueryPlan.
    pub query_description: String,

    /// Quality tier preference.
    pub quality_tier: QualityTier,

    /// Content constraints (must-include / must-avoid).
    pub content_constraints: ContentConstraints,

    /// Authorization risk preference.
    pub authorization_preference: AuthorizationPreference,

    /// Provider identity for source traceability.
    pub provider_id: String,
}

impl CandidateEvaluationRequest {
    /// Build an evaluation request from a candidate and its context.
    pub fn new(
        candidate: CandidateRecord,
        mechanical_evidence: CandidateMechanicalEvidence,
        query_description: String,
        quality_tier: QualityTier,
        content_constraints: ContentConstraints,
        authorization_preference: AuthorizationPreference,
        provider_id: String,
    ) -> Self {
        Self {
            candidate,
            mechanical_evidence,
            query_description,
            quality_tier,
            content_constraints,
            authorization_preference,
            provider_id,
        }
    }
}

// ---------------------------------------------------------------------------
// Candidate evaluation conclusion
// ---------------------------------------------------------------------------

/// The conclusion of an OpenClaw candidate evaluation.
///
/// This is the normalized result that the quality gate maps into
/// `CandidateDecision`. It is intentionally coarse-grained; OpenClaw
/// may produce richer internal output but only these four outcomes
/// are actionable for the retrieval pipeline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CandidateEvaluationConclusion {
    /// OpenClaw explicitly approves the candidate for retrieval.
    Approve {
        /// Optional quality/relevance notes from OpenClaw.
        notes: Option<String>,
    },

    /// OpenClaw explicitly rejects the candidate.
    Reject {
        /// Reason for rejection.
        reason: String,
    },

    /// OpenClaw cannot decide — the candidate is ambiguous, boundary, or
    /// there is insufficient information. Must NOT enter the retrievable
    /// sequence.
    Uncertain {
        /// Why OpenClaw was uncertain.
        reason: String,
    },

    /// OpenClaw evaluation could not be performed. This is a production
    /// dependency failure, not a candidate rejection. The task must
    /// enter execution-blocked state.
    Unexecutable {
        /// Why OpenClaw could not evaluate.
        reason: String,
    },
}

impl CandidateEvaluationConclusion {
    /// Returns `true` iff OpenClaw explicitly approved the candidate.
    pub fn is_approved(&self) -> bool {
        matches!(self, Self::Approve { .. })
    }

    /// Returns `true` iff this conclusion means the task cannot proceed
    /// (production dependency unavailable).
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
// Execution blocking fact
// ---------------------------------------------------------------------------

/// A fact that records why the task entered execution-blocked state.
///
/// Produced when OpenClaw evaluation is unavailable and production
/// policy prohibits falling back to mock/fixture evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionBlockingFact {
    /// Which dependency caused the block.
    pub dependency: String,

    /// Why it is blocked.
    pub reason: String,

    /// Whether this is a recoverable block (e.g. transient unavailability
    /// that may resolve on retry) or a permanent block (e.g. missing
    /// configuration).
    pub is_permanent: bool,
}

impl ExecutionBlockingFact {
    /// Create an execution blocking fact for OpenClaw unavailability.
    pub fn openclaw_unavailable(reason: impl Into<String>) -> Self {
        Self {
            dependency: "OpenClaw".into(),
            reason: reason.into(),
            is_permanent: true, // production path cannot proceed without OpenClaw
        }
    }

    /// Create an execution blocking fact for a general dependency.
    pub fn new(
        dependency: impl Into<String>,
        reason: impl Into<String>,
        is_permanent: bool,
    ) -> Self {
        Self {
            dependency: dependency.into(),
            reason: reason.into(),
            is_permanent,
        }
    }
}

// ---------------------------------------------------------------------------
// Conclusion → CandidateDecision normalization
// ---------------------------------------------------------------------------

/// Normalize an OpenClaw evaluation conclusion into a `CandidateDecision`.
///
/// # Mapping
///
/// | Conclusion | Decision |
/// |---|---|
/// | `Approve` | `CandidateDecision::Accepted` with default priority |
/// | `Reject` | `CandidateDecision::Rejected` |
/// | `Uncertain` | `CandidateDecision::Uncertain` |
/// | `Unexecutable` | `CandidateDecision::ExecutionBlocked` |
pub fn normalize_conclusion(
    candidate: CandidateRecord,
    conclusion: CandidateEvaluationConclusion,
) -> CandidateDecision {
    match conclusion {
        CandidateEvaluationConclusion::Approve { notes: _notes } => {
            CandidateDecision::Accepted {
                candidate,
                // Default priority; callers may override based on provider
                // confidence, source quality signals, etc.
                priority: 0,
                vlm_evidence: None,
            }
        }
        CandidateEvaluationConclusion::Reject { reason } => {
            CandidateDecision::Rejected { candidate, reason }
        }
        CandidateEvaluationConclusion::Uncertain { reason } => {
            CandidateDecision::Uncertain { candidate, reason }
        }
        CandidateEvaluationConclusion::Unexecutable { reason } => {
            CandidateDecision::ExecutionBlocked { candidate, reason }
        }
    }
}

/// Normalize a batch of conclusions.
pub fn normalize_conclusions(
    candidates: Vec<CandidateRecord>,
    conclusions: Vec<CandidateEvaluationConclusion>,
) -> Vec<CandidateDecision> {
    candidates
        .into_iter()
        .zip(conclusions)
        .map(|(candidate, conclusion)| normalize_conclusion(candidate, conclusion))
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::candidate::{CandidateId, ProviderId};

    fn make_candidate(id: &str) -> CandidateRecord {
        let cid = CandidateId::new(id);
        let url = format!("https://example.com/{}.jpg", id);
        CandidateRecord {
            candidate_id: cid.clone(),
            query_plan_id: "qp-test".into(),
            provider_id: ProviderId::new("test-provider"),
            provider_kind: "fixture".into(),
            search_request_id: "sr-test".into(),
            search_round: 1,
            provider_rank: 1,
            global_rank_hint: None,
            image_url: url.clone(),
            source_page_url: None,
            thumbnail_url: None,
            title: Some(format!("Image {}", id)),
            snippet: None,
            width: None,
            height: None,
            mime_type: None,
            license_hint: None,
            attribution: None,
            dedupe_key: CandidateRecord::build_dedupe_key(&url),
            origin_candidate_ids: vec![cid],
            provenance: crate::domain::candidate::CandidateProvenance::new(1, "test", 1, 1),
            normalization_warnings: Vec::new(),
        }
    }

    // -----------------------------------------------------------------------
    // CandidateEvaluationRequest
    // -----------------------------------------------------------------------

    #[test]
    fn evaluation_request_bundles_context() {
        let c = make_candidate("c1");
        let mech = CandidateMechanicalEvidence::pass();
        let req = CandidateEvaluationRequest::new(
            c,
            mech,
            "sunset over mountains".into(),
            QualityTier::High,
            ContentConstraints {
                must_include: vec!["sunset".into()],
                must_avoid: vec!["city".into()],
            },
            AuthorizationPreference::Default,
            "test-provider".into(),
        );

        assert_eq!(req.query_description, "sunset over mountains");
        assert_eq!(req.quality_tier, QualityTier::High);
        assert_eq!(req.content_constraints.must_include, vec!["sunset"]);
        assert_eq!(req.provider_id, "test-provider");
        // Mechanical evidence must have passed (otherwise request wouldn't exist)
        assert!(req.mechanical_evidence.passed_mechanical());
    }

    // -----------------------------------------------------------------------
    // CandidateEvaluationConclusion
    // -----------------------------------------------------------------------

    #[test]
    fn approve_conclusion_is_approved() {
        let c = CandidateEvaluationConclusion::Approve {
            notes: Some("good match".into()),
        };
        assert!(c.is_approved());
        assert!(!c.is_unexecutable());
        assert_eq!(c.label(), "approve");
    }

    #[test]
    fn reject_conclusion_not_approved() {
        let c = CandidateEvaluationConclusion::Reject {
            reason: "poor match".into(),
        };
        assert!(!c.is_approved());
        assert!(!c.is_unexecutable());
        assert_eq!(c.label(), "reject");
    }

    #[test]
    fn uncertain_conclusion_not_approved() {
        let c = CandidateEvaluationConclusion::Uncertain {
            reason: "ambiguous content".into(),
        };
        assert!(!c.is_approved());
        assert!(!c.is_unexecutable());
        assert_eq!(c.label(), "uncertain");
    }

    #[test]
    fn unexecutable_conclusion_not_approved_but_is_unexecutable() {
        let c = CandidateEvaluationConclusion::Unexecutable {
            reason: "OpenClaw endpoint not configured".into(),
        };
        assert!(!c.is_approved());
        assert!(c.is_unexecutable());
        assert_eq!(c.label(), "unexecutable");
    }

    // -----------------------------------------------------------------------
    // ExecutionBlockingFact
    // -----------------------------------------------------------------------

    #[test]
    fn execution_blocking_fact_openclaw_unavailable() {
        let fact = ExecutionBlockingFact::openclaw_unavailable("no production endpoint");
        assert_eq!(fact.dependency, "OpenClaw");
        assert!(fact.is_permanent);
        assert!(fact.reason.contains("no production endpoint"));
    }

    #[test]
    fn execution_blocking_fact_generic() {
        let fact = ExecutionBlockingFact::new("BaseRetrievalChannel", "no channels enabled", true);
        assert_eq!(fact.dependency, "BaseRetrievalChannel");
        assert!(fact.is_permanent);
    }

    // -----------------------------------------------------------------------
    // Normalization — conclusion → decision
    // -----------------------------------------------------------------------

    #[test]
    fn approve_normalizes_to_accepted() {
        let c = make_candidate("c1");
        let conclusion = CandidateEvaluationConclusion::Approve {
            notes: Some("good".into()),
        };
        let decision = normalize_conclusion(c, conclusion);
        assert!(decision.is_accepted());
        match decision {
            CandidateDecision::Accepted { candidate, .. } => {
                assert_eq!(candidate.candidate_id, CandidateId::new("c1"));
            }
            _ => panic!("expected Accepted"),
        }
    }

    #[test]
    fn reject_normalizes_to_rejected() {
        let c = make_candidate("c2");
        let conclusion = CandidateEvaluationConclusion::Reject {
            reason: "irrelevant".into(),
        };
        let decision = normalize_conclusion(c, conclusion);
        assert!(!decision.is_accepted());
        match decision {
            CandidateDecision::Rejected { candidate, reason } => {
                assert_eq!(candidate.candidate_id, CandidateId::new("c2"));
                assert_eq!(reason, "irrelevant");
            }
            _ => panic!("expected Rejected"),
        }
    }

    #[test]
    fn uncertain_normalizes_to_uncertain() {
        let c = make_candidate("c3");
        let conclusion = CandidateEvaluationConclusion::Uncertain {
            reason: "ambiguous".into(),
        };
        let decision = normalize_conclusion(c, conclusion);
        assert!(!decision.is_accepted());
        match decision {
            CandidateDecision::Uncertain { candidate, reason } => {
                assert_eq!(candidate.candidate_id, CandidateId::new("c3"));
                assert_eq!(reason, "ambiguous");
            }
            _ => panic!("expected Uncertain"),
        }
    }

    #[test]
    fn unexecutable_normalizes_to_execution_blocked() {
        let c = make_candidate("c4");
        let conclusion = CandidateEvaluationConclusion::Unexecutable {
            reason: "OpenClaw down".into(),
        };
        let decision = normalize_conclusion(c, conclusion);
        assert!(!decision.is_accepted());
        match decision {
            CandidateDecision::ExecutionBlocked { candidate, reason } => {
                assert_eq!(candidate.candidate_id, CandidateId::new("c4"));
                assert_eq!(reason, "OpenClaw down");
            }
            _ => panic!("expected ExecutionBlocked"),
        }
    }

    // -----------------------------------------------------------------------
    // Batch normalization
    // -----------------------------------------------------------------------

    #[test]
    fn batch_normalization_maps_all_outcomes() {
        let candidates = vec![
            make_candidate("a"),
            make_candidate("b"),
            make_candidate("c"),
            make_candidate("d"),
        ];
        let conclusions = vec![
            CandidateEvaluationConclusion::Approve { notes: None },
            CandidateEvaluationConclusion::Reject {
                reason: "bad".into(),
            },
            CandidateEvaluationConclusion::Uncertain {
                reason: "maybe".into(),
            },
            CandidateEvaluationConclusion::Unexecutable {
                reason: "down".into(),
            },
        ];

        let decisions = normalize_conclusions(candidates, conclusions);
        assert_eq!(decisions.len(), 4);
        assert!(decisions[0].is_accepted());
        assert!(!decisions[1].is_accepted());
        assert!(!decisions[2].is_accepted());
        assert!(!decisions[3].is_accepted());

        // Verify specific types
        assert!(matches!(decisions[0], CandidateDecision::Accepted { .. }));
        assert!(matches!(decisions[1], CandidateDecision::Rejected { .. }));
        assert!(matches!(decisions[2], CandidateDecision::Uncertain { .. }));
        assert!(matches!(
            decisions[3],
            CandidateDecision::ExecutionBlocked { .. }
        ));
    }

    // -----------------------------------------------------------------------
    // Label coverage
    // -----------------------------------------------------------------------

    #[test]
    fn conclusion_labels_cover_all_variants() {
        assert_eq!(
            CandidateEvaluationConclusion::Approve { notes: None }.label(),
            "approve"
        );
        assert_eq!(
            CandidateEvaluationConclusion::Reject { reason: "r".into() }.label(),
            "reject"
        );
        assert_eq!(
            CandidateEvaluationConclusion::Uncertain { reason: "u".into() }.label(),
            "uncertain"
        );
        assert_eq!(
            CandidateEvaluationConclusion::Unexecutable { reason: "x".into() }.label(),
            "unexecutable"
        );
    }

    // -----------------------------------------------------------------------
    // Serialization round-trip
    // -----------------------------------------------------------------------

    #[test]
    fn conclusion_serialization_round_trip() {
        let conclusions = vec![
            CandidateEvaluationConclusion::Approve {
                notes: Some("good".into()),
            },
            CandidateEvaluationConclusion::Reject {
                reason: "bad".into(),
            },
            CandidateEvaluationConclusion::Uncertain {
                reason: "maybe".into(),
            },
            CandidateEvaluationConclusion::Unexecutable {
                reason: "down".into(),
            },
        ];

        for original in &conclusions {
            let json = serde_json::to_string(original).expect("serialize");
            let parsed: CandidateEvaluationConclusion =
                serde_json::from_str(&json).expect("deserialize");
            assert_eq!(*original, parsed);
        }
    }
}
