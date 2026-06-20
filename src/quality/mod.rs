//! Quality gate module placeholder.
//!
//! Will host candidate mechanical validation, image mechanical acceptance,
//! and subjective evaluation normalisation logic.
//!
//! For TASK-001 this module declares the type-level boundaries used by
//! the domain model and port traits. Implementation belongs to TASK-004
//! (candidate quality gate) and TASK-006 (image acceptance gate).
//!
//! References: PRD §校验与评价产品要求, HLD §Candidate Quality Gate / Image Acceptance Gate

/// Re-export quality-related domain types for convenience.
pub use crate::domain::candidate::CandidateDecision;
pub use crate::domain::image::{ImageAcceptanceDecision, ImageMechanicalEvidence};
