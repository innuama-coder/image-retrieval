//! Policy & guardrails module placeholder.
//!
//! Will host authorization-risk checks, access-restriction detection,
//! paid-channel gating, credential sanitisation, and product-policy
//! blocking decisions.
//!
//! For TASK-001 this module declares the policy type boundaries.
//! Implementation belongs to TASK-007.
//!
//! References: PRD NFR-002/NFR-003/NFR-006, HLD §Policy & Guardrails

/// Re-export policy-related domain types for convenience.
pub use crate::domain::policy::{AuthorizationRisk, PolicyDecision, PolicyFact};
