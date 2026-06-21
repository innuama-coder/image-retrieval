//! Candidate quality gate module.
//!
//! Contains:
//! - [`mechanical`] — Candidate mechanical evidence types and validation.
//! - [`evaluation`] — OpenClaw evaluation request/response types and normalization.
//! - [`gate`] — CandidateQualityGate orchestrator.
//!
//! References: PRD §校验与评价产品要求, HLD §Candidate Quality Gate,
//! `docs/design/TASK-004-candidate-quality-openclaw-design.md`

pub mod evaluation;
pub mod gate;
pub mod mechanical;

// Re-export public API
pub use evaluation::{
    normalize_conclusion, normalize_conclusions, CandidateEvaluationConclusion,
    CandidateEvaluationRequest, ExecutionBlockingFact,
};
pub use gate::{
    evaluate_with_conclusions, CandidateQualityGate, CandidateQualityGateResult,
    CandidateQualitySummary,
};
pub use mechanical::{
    validate_candidate_mechanical, CandidateBlockingReason, CandidateMechanicalEvidence,
    CandidateReferenceSignal,
};
