//! QueryPlan domain types.
//!
//! Covers the input lifecycle:
//! `QueryPlanInput` → `ValidatedQueryPlan` → `TaskPlan`
//!
//! References: PRD §QueryPlan 产品设计, HLD §QueryPlan Planner

use serde::{Deserialize, Serialize};

/// Raw user-facing QueryPlan before validation and default-value application.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryPlanInput {
    /// Semantic description of the desired image(s). Required; task is
    /// rejected when missing or empty.
    #[serde(default)]
    pub description: String,

    /// Number of qualified images the user wants. Defaults to 1.
    #[serde(default = "default_required_count")]
    pub required_count: u32,

    /// Quality tier preference.
    #[serde(default)]
    pub quality_tier: QualityTier,

    /// Content constraints: must-include / must-avoid hints.
    #[serde(default)]
    pub content_constraints: ContentConstraints,

    /// Authorization risk preference.
    #[serde(default)]
    pub authorization_preference: AuthorizationPreference,

    /// Output preference: human-readable vs automation-consumable.
    #[serde(default)]
    pub output_preference: OutputPreference,

    /// Maximum number of retries after the initial attempt.
    /// Constitution allows at most 3 (initial attempt + up to 3 retries).
    #[serde(default = "default_retry_limit")]
    pub retry_limit: u32,
}

impl Default for QueryPlanInput {
    fn default() -> Self {
        Self {
            description: String::new(),
            required_count: default_required_count(),
            quality_tier: QualityTier::default(),
            content_constraints: ContentConstraints::default(),
            authorization_preference: AuthorizationPreference::default(),
            output_preference: OutputPreference::default(),
            retry_limit: default_retry_limit(),
        }
    }
}

fn default_required_count() -> u32 {
    1
}

fn default_retry_limit() -> u32 {
    3
}

/// Quality tier expressing user expectations about image usability.
///
/// Per PRD: 通用质量 / 较高质量 / 严格质量.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum QualityTier {
    /// General quality: suitable for regular use; no obvious corruption,
    /// unrecognizable subject, extreme blur, or clear mismatch.
    #[default]
    #[serde(rename = "general")]
    General,

    /// Higher quality: clear subject, usable composition, low visual noise,
    /// stable relevance to the intended use.
    #[serde(rename = "high")]
    High,

    /// Strict quality: conservative pass; boundary / uncertain images
    /// should be rejected or deprioritised.
    #[serde(rename = "strict")]
    Strict,
}

/// Content constraints the user may optionally supply.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContentConstraints {
    /// Elements that should be present in the image.
    #[serde(default)]
    pub must_include: Vec<String>,

    /// Elements that should be avoided.
    #[serde(default)]
    pub must_avoid: Vec<String>,
}

/// User stance on authorization / licensing risk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AuthorizationPreference {
    /// Unknown authorization is retained with risk warnings;
    /// explicitly prohibited sources are blocked.
    #[default]
    #[serde(rename = "default")]
    Default,
}

/// How the delivery result will be consumed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum OutputPreference {
    /// Human-oriented explanations.
    #[default]
    #[serde(rename = "human")]
    Human,

    /// Automation-oriented: stable status codes, machine-readable fields.
    #[serde(rename = "automation")]
    Automation,
}

// ---------------------------------------------------------------------------
// Validated query plan
// ---------------------------------------------------------------------------

/// A QueryPlan that has passed input validation and has all defaults applied.
#[derive(Debug, Clone)]
pub struct ValidatedQueryPlan {
    pub description: String,
    pub required_count: u32,
    pub quality_tier: QualityTier,
    pub content_constraints: ContentConstraints,
    pub authorization_preference: AuthorizationPreference,
    pub output_preference: OutputPreference,
    pub retry_limit: u32,
}

// ---------------------------------------------------------------------------
// Task plan — derived execution parameters
// ---------------------------------------------------------------------------

/// Execution plan derived from a validated QueryPlan.
///
/// Contains the candidate target and batch size derived per the
/// constitution ratios (≈20 candidates per required image;
/// batch target = required_count × 2).
#[derive(Debug, Clone)]
pub struct TaskPlan {
    /// The validated QueryPlan this plan was derived from.
    pub query_plan: ValidatedQueryPlan,

    /// Target number of candidates to search for
    /// (≈ required_count × 20).
    pub candidate_target: u32,

    /// Target number of candidates per retrieval batch
    /// (= required_count × 2).
    pub retrieval_batch_target: u32,

    /// Maximum number of full attempts (1 initial + retry_limit).
    pub max_attempts: u32,
}

impl TaskPlan {
    /// Derive a `TaskPlan` from a validated QueryPlan.
    pub fn from_validated(plan: ValidatedQueryPlan) -> Self {
        let candidate_target = plan.required_count.saturating_mul(20);
        let retrieval_batch_target = plan.required_count.saturating_mul(2);
        let max_attempts = 1 + plan.retry_limit;
        Self {
            query_plan: plan,
            candidate_target,
            retrieval_batch_target,
            max_attempts,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_required_count_is_1() {
        assert_eq!(default_required_count(), 1);
    }

    #[test]
    fn default_retry_limit_is_3() {
        assert_eq!(default_retry_limit(), 3);
    }

    #[test]
    fn task_plan_derives_candidate_target_20x() {
        let plan = ValidatedQueryPlan {
            description: "test".into(),
            required_count: 3,
            quality_tier: QualityTier::General,
            content_constraints: ContentConstraints::default(),
            authorization_preference: AuthorizationPreference::Default,
            output_preference: OutputPreference::Human,
            retry_limit: 3,
        };
        let task = TaskPlan::from_validated(plan);
        assert_eq!(task.candidate_target, 60); // 3 × 20
        assert_eq!(task.retrieval_batch_target, 6); // 3 × 2
        assert_eq!(task.max_attempts, 4); // 1 initial + 3 retries
    }

    #[test]
    fn task_plan_derives_candidate_target_for_single_image() {
        let plan = ValidatedQueryPlan {
            description: "single".into(),
            required_count: 1,
            quality_tier: QualityTier::General,
            content_constraints: ContentConstraints::default(),
            authorization_preference: AuthorizationPreference::Default,
            output_preference: OutputPreference::Human,
            retry_limit: 3,
        };
        let task = TaskPlan::from_validated(plan);
        assert_eq!(task.candidate_target, 20);
        assert_eq!(task.retrieval_batch_target, 2);
    }

    #[test]
    fn task_plan_zero_count_saturates() {
        let plan = ValidatedQueryPlan {
            description: "zero".into(),
            required_count: 0,
            quality_tier: QualityTier::General,
            content_constraints: ContentConstraints::default(),
            authorization_preference: AuthorizationPreference::Default,
            output_preference: OutputPreference::Human,
            retry_limit: 0,
        };
        let task = TaskPlan::from_validated(plan);
        assert_eq!(task.candidate_target, 0);
        assert_eq!(task.retrieval_batch_target, 0);
        assert_eq!(task.max_attempts, 1);
    }
}
