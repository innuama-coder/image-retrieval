//! Policy domain types.
//!
//! Covers authorization risk, access restrictions, paid-channel gating,
//! policy decisions that may produce task-level execution blocks, and
//! config-facing policy structs.
//!
//! v1.1 additions:
//! - [`PolicyNarrowingResult`]: outcome of narrowing QueryPlan policy against
//!   runtime config.
//! - [`EffectiveRetrievalPolicy`]: resolved policy after narrowing.
//!
//! References: PRD NFR-002/NFR-003/NFR-006, HLD §Policy & Guardrails,
//! `docs/design/v1.1-TASK-001-queryplan-config-policy-design.md`

use crate::domain::query_plan::{AdmissionDiagnostic, AdmissionFailureCode, QueryRetrievalPolicy};
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

// ---------------------------------------------------------------------------
// v1.1 Policy narrowing
// ---------------------------------------------------------------------------

/// Result of narrowing QueryPlan policy against runtime config.
///
/// QueryPlan policy may only restrict (never broaden) runtime policy.
/// This struct captures the effective policy and any diagnostics from
/// broadening attempts that were blocked.
#[derive(Debug, Clone)]
pub struct PolicyNarrowingResult {
    /// The effective (narrowed) retrieval policy.
    pub effective: EffectiveRetrievalPolicy,

    /// Diagnostics from blocked broadening attempts.
    pub diagnostics: Vec<AdmissionDiagnostic>,

    /// Whether paid retrieval is allowed by both config and query policy.
    pub paid_allowed: bool,
}

/// Effective retrieval policy after narrowing QueryPlan against runtime config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectiveRetrievalPolicy {
    /// Whether paid channels are allowed.
    pub allow_paid: bool,

    /// Whether to respect robots.txt and site rules.
    pub respect_robots: bool,

    /// Whether login-required sources can be accessed.
    pub allow_login: bool,

    /// Whether paywalled sources can be accessed.
    pub allow_paywalled: bool,
}

impl EffectiveRetrievalPolicy {
    /// Create from runtime config defaults (before QueryPlan narrowing).
    pub fn from_config(
        allow_paid: bool,
        respect_robots: bool,
        allow_login: bool,
        allow_paywalled: bool,
    ) -> Self {
        Self {
            allow_paid,
            respect_robots,
            allow_login,
            allow_paywalled,
        }
    }

    /// Narrow this policy by applying QueryPlan constraints.
    ///
    /// The query policy can only restrict, never broaden. Returns the
    /// narrowed policy and any diagnostics from blocked broadening attempts.
    pub fn narrow(&self, query: &QueryRetrievalPolicy) -> PolicyNarrowingResult {
        let mut diagnostics = Vec::new();

        // Paid: query cannot enable if config disables
        let paid_allowed = if query.allow_paid && !self.allow_paid {
            diagnostics.push(AdmissionDiagnostic::blocker(
                AdmissionFailureCode::PolicyPaidBlockedByConfig,
                "retrieval_policy.allow_paid",
                "Query plan requests paid retrieval, but paid channels are disabled in runtime config.",
            ));
            false
        } else {
            query.allow_paid && self.allow_paid
        };

        // Robots: config respect is minimum; query cannot override
        let respect_robots_effective = if !query.respect_robots && self.respect_robots {
            diagnostics.push(AdmissionDiagnostic::warning(
                AdmissionFailureCode::PolicyBroadeningBlocked,
                "retrieval_policy.respect_robots",
                "Query plan attempts to disable robots respect, but config requires it.",
            ));
            true
        } else {
            query.respect_robots || self.respect_robots
        };

        // Login: query cannot enable if config disables
        let allow_login_effective = if query.allow_login && !self.allow_login {
            diagnostics.push(AdmissionDiagnostic::warning(
                AdmissionFailureCode::PolicyBroadeningBlocked,
                "retrieval_policy.allow_login",
                "Query plan requests login-required sources, which are disabled by config policy.",
            ));
            false
        } else {
            query.allow_login && self.allow_login
        };

        // Paywalled: query cannot enable if config disables
        let allow_paywalled_effective = if query.allow_paywalled && !self.allow_paywalled {
            diagnostics.push(AdmissionDiagnostic::warning(
                AdmissionFailureCode::PolicyBroadeningBlocked,
                "retrieval_policy.allow_paywalled",
                "Query plan requests paywalled sources, which are disabled by config policy.",
            ));
            false
        } else {
            query.allow_paywalled && self.allow_paywalled
        };

        PolicyNarrowingResult {
            effective: EffectiveRetrievalPolicy {
                allow_paid: paid_allowed,
                respect_robots: respect_robots_effective,
                allow_login: allow_login_effective,
                allow_paywalled: allow_paywalled_effective,
            },
            diagnostics,
            paid_allowed,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

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

    // -----------------------------------------------------------------------
    // v1.1 Policy narrowing tests
    // -----------------------------------------------------------------------

    #[test]
    fn effective_policy_paid_blocked_when_config_disables() {
        let config = EffectiveRetrievalPolicy::from_config(false, true, false, false);
        let query = QueryRetrievalPolicy {
            allow_paid: true,
            ..Default::default()
        };
        let result = config.narrow(&query);
        assert!(!result.paid_allowed);
        assert!(!result.effective.allow_paid);
        assert!(result
            .diagnostics
            .iter()
            .any(|d| d.code == AdmissionFailureCode::PolicyPaidBlockedByConfig));
    }

    #[test]
    fn effective_policy_paid_allowed_when_both_enable() {
        let config = EffectiveRetrievalPolicy::from_config(true, true, false, false);
        let query = QueryRetrievalPolicy {
            allow_paid: true,
            ..Default::default()
        };
        let result = config.narrow(&query);
        assert!(result.paid_allowed);
        assert!(result.effective.allow_paid);
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn effective_policy_robots_enforced_by_config() {
        let config = EffectiveRetrievalPolicy::from_config(false, true, false, false);
        let query = QueryRetrievalPolicy {
            respect_robots: false,
            ..Default::default()
        };
        let result = config.narrow(&query);
        assert!(result.effective.respect_robots);
        assert!(result
            .diagnostics
            .iter()
            .any(|d| d.code == AdmissionFailureCode::PolicyBroadeningBlocked));
    }

    #[test]
    fn effective_policy_login_blocked_by_config() {
        let config = EffectiveRetrievalPolicy::from_config(false, true, false, false);
        let query = QueryRetrievalPolicy {
            allow_login: true,
            ..Default::default()
        };
        let result = config.narrow(&query);
        assert!(!result.effective.allow_login);
    }

    #[test]
    fn effective_policy_paywalled_blocked_by_config() {
        let config = EffectiveRetrievalPolicy::from_config(false, true, false, false);
        let query = QueryRetrievalPolicy {
            allow_paywalled: true,
            ..Default::default()
        };
        let result = config.narrow(&query);
        assert!(!result.effective.allow_paywalled);
    }

    #[test]
    fn effective_policy_query_cannot_broaden_anything() {
        let config = EffectiveRetrievalPolicy::from_config(false, true, false, false);
        let query = QueryRetrievalPolicy {
            allow_paid: true,
            respect_robots: false,
            allow_login: true,
            allow_paywalled: true,
        };
        let result = config.narrow(&query);
        assert!(!result.effective.allow_paid);
        assert!(result.effective.respect_robots);
        assert!(!result.effective.allow_login);
        assert!(!result.effective.allow_paywalled);
        assert!(!result.diagnostics.is_empty());
    }

    #[test]
    fn effective_policy_query_can_narrow() {
        let config = EffectiveRetrievalPolicy::from_config(true, true, true, true);
        let query = QueryRetrievalPolicy {
            allow_paid: false,
            respect_robots: true,
            allow_login: false,
            allow_paywalled: false,
        };
        let result = config.narrow(&query);
        assert!(!result.effective.allow_paid);
        assert!(result.effective.respect_robots);
        assert!(!result.effective.allow_login);
        assert!(!result.effective.allow_paywalled);
        assert!(result.diagnostics.is_empty());
    }
}
