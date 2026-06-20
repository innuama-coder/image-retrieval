//! Policy domain types.
//!
//! Covers authorization risk, access restrictions, paid-channel gating,
//! and policy decisions that may produce task-level execution blocks.
//!
//! References: PRD NFR-002/NFR-003/NFR-006, HLD §Policy & Guardrails

use serde::{Deserialize, Serialize};

/// Outcome of a policy check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PolicyDecision {
    /// The action is allowed.
    Allow,

    /// The action is blocked for this candidate/image only; other
    /// candidates/images may proceed.
    LocalReject { reason: String },

    /// The action is blocked at the task level — the current QueryPlan
    /// cannot continue.
    TaskBlock { reason: String },
}

impl PolicyDecision {
    pub fn is_task_block(&self) -> bool {
        matches!(self, Self::TaskBlock { .. })
    }
}

// ---------------------------------------------------------------------------
// Authorization risk
// ---------------------------------------------------------------------------

/// Classification of authorization / licensing risk for an image.
///
/// Per PRD: unknown authorization must retain risk warnings; explicitly
/// prohibited sources must be rejected or blocked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthorizationRisk {
    /// Authorization status is unknown — must not be described as
    /// commercially safe or risk-free.
    Unknown,

    /// The source explicitly prohibits reuse; the image must be rejected.
    Prohibited,

    /// The source allows reuse under stated terms.
    Allowed,
}

// ---------------------------------------------------------------------------
// Policy fact
// ---------------------------------------------------------------------------

/// A fact passed to the Policy & Guardrails module for evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyFact {
    /// The candidate or image being evaluated.
    pub subject_id: String,

    /// The authorization risk classification.
    pub authorization_risk: AuthorizationRisk,

    /// Whether an access restriction (login wall, paywall, etc.) was
    /// detected.
    pub has_access_restriction: bool,

    /// Whether the retrieval channel is a paid tier.
    pub is_paid_channel: bool,

    /// Whether the paid channel has been explicitly confirmed by the user.
    pub paid_channel_confirmed: bool,

    /// Additional context for the policy decision.
    pub context: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_allow() {
        let d = PolicyDecision::Allow;
        assert!(!d.is_task_block());
    }

    #[test]
    fn policy_local_reject_not_task_block() {
        let d = PolicyDecision::LocalReject {
            reason: "duplicate".into(),
        };
        assert!(!d.is_task_block());
    }

    #[test]
    fn policy_task_block() {
        let d = PolicyDecision::TaskBlock {
            reason: "OpenClaw unavailable".into(),
        };
        assert!(d.is_task_block());
    }
}
