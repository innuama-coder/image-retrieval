//! Quality gate module.
//!
//! Hosts candidate mechanical validation, OpenClaw evaluation normalization,
//! and (in TASK-006) image mechanical acceptance.
//!
//! References: PRD §校验与评价产品要求, HLD §Candidate Quality Gate / Image Acceptance Gate

pub mod candidate;

/// Re-export quality-related domain types for convenience.
pub use crate::domain::candidate::CandidateDecision;
pub use crate::domain::image::{ImageAcceptanceDecision, ImageMechanicalEvidence};

// Re-export candidate quality gate public API
pub use candidate::{
    evaluate_with_conclusions, normalize_conclusion, normalize_conclusions,
    validate_candidate_mechanical, CandidateBlockingReason, CandidateEvaluationConclusion,
    CandidateEvaluationRequest, CandidateMechanicalEvidence, CandidateQualityGate,
    CandidateQualityGateResult, CandidateQualitySummary, CandidateReferenceSignal,
    ExecutionBlockingFact,
};
