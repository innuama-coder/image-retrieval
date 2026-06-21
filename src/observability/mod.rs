//! Task evidence & metrics aggregation.
//!
//! Produces structured summaries for MET-001 through MET-006 by consuming
//! [`MetricEvent`]s emitted across the pipeline (TASK-002 through TASK-006).
//!
//! # Metrics by event source (per LLD)
//!
//! | Metric | Source |
//! |---|---|
//! | MET-001 Task outcome distribution | TASK-002 input rejection boundary, TASK-006 terminal state |
//! | MET-002 Candidate satisfaction rate | TASK-003 candidate target vs deduped count |
//! | MET-003 Qualified image achievement rate | TASK-006 qualified count vs required count |
//! | MET-004 Primary rejection reasons | TASK-004 candidate rejection, TASK-006 image rejection |
//! | MET-005 Retrieval channel effectiveness | TASK-005 channel attempts, fallback, success, contribution |
//! | MET-006 OpenClaw evaluation pass rate | TASK-004 candidate evaluation, TASK-006 image evaluation |
//!
//! References: PRD §数据、埋点与度量方案, HLD §Task Evidence & Metrics,
//! `docs/design/TASK-007-delivery-policy-observability-design.md`

use crate::domain::metrics::{MetricEvent, MetricKind};
use serde::Serialize;

// ---------------------------------------------------------------------------
// Re-export domain types for convenience
// ---------------------------------------------------------------------------

pub use crate::domain::metrics::{
    MetricEvent as MetricEventReexport, MetricKind as MetricKindReexport,
};

// ---------------------------------------------------------------------------
// Aggregated metrics summary
// ---------------------------------------------------------------------------

/// Aggregated summary of all six metric families.
///
/// Produced by [`summarize_metrics`] and embedded in the delivery manifest.
#[derive(Debug, Clone, Serialize)]
pub struct MetricsSummary {
    /// MET-001: Task outcome distribution.
    pub task_outcome: Option<TaskOutcomeMetric>,

    /// MET-002: Candidate satisfaction rate.
    pub candidate_satisfaction: Vec<RateMetric>,

    /// MET-003: Qualified image achievement rate.
    pub qualified_image_achievement: Vec<RateMetric>,

    /// MET-004: Primary rejection reasons.
    pub rejection_reasons: Vec<RejectionReasonMetric>,

    /// MET-005: Retrieval channel effectiveness.
    pub channel_effectiveness: Vec<ChannelEffectivenessMetric>,

    /// MET-006: OpenClaw evaluation pass rate.
    pub openclaw_evaluation_rate: Vec<RateMetric>,
}

/// MET-001: A single task outcome event.
#[derive(Debug, Clone, Serialize)]
pub struct TaskOutcomeMetric {
    /// Outcome label: `full_delivery`, `limited_delivery`, `execution_blocked`,
    /// `input_rejected`.
    pub outcome: String,

    /// Count of this outcome (typically 1 per task).
    pub count: f64,
}

/// A rate metric with a numerator and optional denominator.
#[derive(Debug, Clone, Serialize)]
pub struct RateMetric {
    pub label: String,
    pub value: f64,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub denominator: Option<f64>,

    /// Computed rate when denominator is present and non-zero.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate: Option<f64>,
}

/// MET-004: A rejection reason event.
#[derive(Debug, Clone, Serialize)]
pub struct RejectionReasonMetric {
    /// The category of rejection (e.g. "mechanical", "openclaw_rejected").
    pub reason: String,

    /// Number of items rejected for this reason.
    pub count: f64,
}

/// MET-005: A channel effectiveness event.
#[derive(Debug, Clone, Serialize)]
pub struct ChannelEffectivenessMetric {
    /// The channel tier or identifier.
    pub channel: String,

    /// Number of successes attributed to this channel.
    pub successes: f64,

    /// Number of failures attributed to this channel.
    pub failures: f64,

    /// Whether fallback was used to reach this channel.
    pub via_fallback: bool,
}

// ---------------------------------------------------------------------------
// Aggregation
// ---------------------------------------------------------------------------

/// Summarise a collection of [`MetricEvent`]s into structured metric
/// families suitable for the delivery manifest.
///
/// Events are partitioned by [`MetricKind`] and normalised into the
/// appropriate summary types.
pub fn summarize_metrics(events: &[MetricEvent]) -> MetricsSummary {
    // MET-001: Task outcome
    let task_outcome = events
        .iter()
        .find(|e| e.kind == MetricKind::TaskOutcome)
        .map(|e| TaskOutcomeMetric {
            outcome: e
                .metadata
                .iter()
                .find(|(k, _)| k == "state")
                .map(|(_, v)| v.clone())
                .unwrap_or_else(|| e.label.clone()),
            count: e.value,
        });

    // MET-002: Candidate satisfaction
    let candidate_satisfaction: Vec<RateMetric> = events
        .iter()
        .filter(|e| e.kind == MetricKind::CandidateSatisfaction)
        .map(|e| RateMetric {
            label: e.label.clone(),
            value: e.value,
            denominator: e.denominator,
            rate: e
                .denominator
                .and_then(|d| if d > 0.0 { Some(e.value / d) } else { None }),
        })
        .collect();

    // MET-003: Qualified image achievement
    let qualified_image_achievement: Vec<RateMetric> = events
        .iter()
        .filter(|e| e.kind == MetricKind::QualifiedImageAchievement)
        .map(|e| RateMetric {
            label: e.label.clone(),
            value: e.value,
            denominator: e.denominator,
            rate: e
                .denominator
                .and_then(|d| if d > 0.0 { Some(e.value / d) } else { None }),
        })
        .collect();

    // MET-004: Rejection reasons
    let rejection_reasons: Vec<RejectionReasonMetric> = events
        .iter()
        .filter(|e| e.kind == MetricKind::RejectionReason)
        .map(|e| {
            let reason = e
                .metadata
                .iter()
                .find(|(k, _)| k == "reason")
                .map(|(_, v)| v.clone())
                .unwrap_or_else(|| e.label.clone());
            RejectionReasonMetric {
                reason,
                count: e.value,
            }
        })
        .collect();

    // MET-005: Channel effectiveness
    let channel_effectiveness: Vec<ChannelEffectivenessMetric> = events
        .iter()
        .filter(|e| e.kind == MetricKind::ChannelEffectiveness)
        .map(|e| {
            let channel = e
                .metadata
                .iter()
                .find(|(k, _)| k == "channel")
                .map(|(_, v)| v.clone())
                .unwrap_or_else(|| e.label.clone());
            let via_fallback = e
                .metadata
                .iter()
                .any(|(k, v)| k == "fallback" && v == "true");
            let denom = e.denominator.unwrap_or(e.value);
            let failures = if denom > e.value {
                denom - e.value
            } else {
                0.0
            };
            ChannelEffectivenessMetric {
                channel,
                successes: e.value,
                failures,
                via_fallback,
            }
        })
        .collect();

    // MET-006: OpenClaw evaluation pass rate
    let openclaw_evaluation_rate: Vec<RateMetric> = events
        .iter()
        .filter(|e| e.kind == MetricKind::OpenClawEvaluationRate)
        .map(|e| RateMetric {
            label: e.label.clone(),
            value: e.value,
            denominator: e.denominator,
            rate: e
                .denominator
                .and_then(|d| if d > 0.0 { Some(e.value / d) } else { None }),
        })
        .collect();

    MetricsSummary {
        task_outcome,
        candidate_satisfaction,
        qualified_image_achievement,
        rejection_reasons,
        channel_effectiveness,
        openclaw_evaluation_rate,
    }
}

/// Convenience: verify that all six MET families have at least an empty
/// placeholder in the summary. Returns a list of families that are missing.
pub fn check_coverage(summary: &MetricsSummary) -> Vec<&'static str> {
    let mut missing = Vec::new();

    if summary.task_outcome.is_none() {
        missing.push("MET-001: task_outcome");
    }
    // For array-based metrics, we note them even if empty; the caller
    // decides whether empty arrays count as "covered".
    // We'll flag them as covered if they exist at all.

    missing
}

/// Build an iterator over all metric events in a given family.
pub fn filter_by_kind(
    events: &[MetricEvent],
    kind: MetricKind,
) -> impl Iterator<Item = &MetricEvent> {
    events.iter().filter(move |e| e.kind == kind)
}

/// Count the number of distinct rejection reasons in MET-004 events.
pub fn distinct_rejection_reasons(events: &[MetricEvent]) -> usize {
    use std::collections::HashSet;
    events
        .iter()
        .filter(|e| e.kind == MetricKind::RejectionReason)
        .filter_map(|e| {
            e.metadata
                .iter()
                .find(|(k, _)| k == "reason")
                .map(|(_, v)| v.clone())
        })
        .collect::<HashSet<_>>()
        .len()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // MET-001: Task outcome
    // -----------------------------------------------------------------------

    #[test]
    fn task_outcome_metric_from_event() {
        let event = MetricEvent::new(MetricKind::TaskOutcome, "task_outcome_full_delivery", 1.0)
            .with_meta("state", "full_delivery");

        let summary = summarize_metrics(&[event]);
        let outcome = summary.task_outcome.unwrap();
        assert_eq!(outcome.outcome, "full_delivery");
        assert_eq!(outcome.count, 1.0);
    }

    #[test]
    fn task_outcome_falls_back_to_label() {
        let event = MetricEvent::new(
            MetricKind::TaskOutcome,
            "task_outcome_limited_delivery",
            1.0,
        );

        let summary = summarize_metrics(&[event]);
        let outcome = summary.task_outcome.unwrap();
        assert!(outcome.outcome.contains("limited"));
    }

    #[test]
    fn missing_task_outcome_is_none() {
        let summary = summarize_metrics(&[]);
        assert!(summary.task_outcome.is_none());
    }

    // -----------------------------------------------------------------------
    // MET-002: Candidate satisfaction
    // -----------------------------------------------------------------------

    #[test]
    fn candidate_satisfaction_rate() {
        let event = MetricEvent::new(
            MetricKind::CandidateSatisfaction,
            "candidate_satisfaction",
            45.0,
        )
        .with_denominator(60.0);

        let summary = summarize_metrics(&[event]);
        assert_eq!(summary.candidate_satisfaction.len(), 1);
        let metric = &summary.candidate_satisfaction[0];
        assert_eq!(metric.value, 45.0);
        assert_eq!(metric.denominator, Some(60.0));
        assert_eq!(metric.rate, Some(0.75));
    }

    #[test]
    fn candidate_satisfaction_no_rate_when_denominator_zero() {
        let event = MetricEvent::new(MetricKind::CandidateSatisfaction, "zero_target", 0.0)
            .with_denominator(0.0);

        let summary = summarize_metrics(&[event]);
        let metric = &summary.candidate_satisfaction[0];
        assert_eq!(metric.rate, None);
    }

    // -----------------------------------------------------------------------
    // MET-003: Qualified image achievement
    // -----------------------------------------------------------------------

    #[test]
    fn qualified_image_achievement_rate() {
        let event = MetricEvent::new(MetricKind::QualifiedImageAchievement, "qualified", 3.0)
            .with_denominator(4.0);

        let summary = summarize_metrics(&[event]);
        assert_eq!(summary.qualified_image_achievement.len(), 1);
        let metric = &summary.qualified_image_achievement[0];
        assert_eq!(metric.value, 3.0);
        assert_eq!(metric.rate, Some(0.75));
    }

    // -----------------------------------------------------------------------
    // MET-004: Rejection reasons
    // -----------------------------------------------------------------------

    #[test]
    fn rejection_reason_metric() {
        let event = MetricEvent::new(MetricKind::RejectionReason, "mechanical_rejection", 2.0)
            .with_meta("reason", "mechanical");

        let summary = summarize_metrics(&[event]);
        assert_eq!(summary.rejection_reasons.len(), 1);
        let metric = &summary.rejection_reasons[0];
        assert_eq!(metric.reason, "mechanical");
        assert_eq!(metric.count, 2.0);
    }

    #[test]
    fn multiple_rejection_reasons() {
        let events = vec![
            MetricEvent::new(MetricKind::RejectionReason, "mech", 3.0)
                .with_meta("reason", "mechanical"),
            MetricEvent::new(MetricKind::RejectionReason, "subj_reject", 2.0)
                .with_meta("reason", "openclaw_rejected"),
            MetricEvent::new(MetricKind::RejectionReason, "subj_uncertain", 1.0)
                .with_meta("reason", "openclaw_uncertain"),
        ];

        let summary = summarize_metrics(&events);
        assert_eq!(summary.rejection_reasons.len(), 3);
        assert_eq!(
            summary
                .rejection_reasons
                .iter()
                .map(|r| r.count as u32)
                .sum::<u32>(),
            6
        );
    }

    #[test]
    fn distinct_rejection_reasons_count() {
        let events = vec![
            MetricEvent::new(MetricKind::RejectionReason, "a", 1.0)
                .with_meta("reason", "mechanical"),
            MetricEvent::new(MetricKind::RejectionReason, "b", 1.0)
                .with_meta("reason", "openclaw_rejected"),
            MetricEvent::new(MetricKind::RejectionReason, "c", 1.0)
                .with_meta("reason", "mechanical"), // same reason
        ];

        assert_eq!(distinct_rejection_reasons(&events), 2);
    }

    // -----------------------------------------------------------------------
    // MET-005: Channel effectiveness
    // -----------------------------------------------------------------------

    #[test]
    fn channel_effectiveness_metric() {
        let event = MetricEvent::new(MetricKind::ChannelEffectiveness, "web_fetch", 8.0)
            .with_denominator(10.0)
            .with_meta("channel", "web_fetch");

        let summary = summarize_metrics(&[event]);
        assert_eq!(summary.channel_effectiveness.len(), 1);
        let metric = &summary.channel_effectiveness[0];
        assert_eq!(metric.channel, "web_fetch");
        assert_eq!(metric.successes, 8.0);
        assert_eq!(metric.failures, 2.0);
        assert!(!metric.via_fallback);
    }

    #[test]
    fn channel_effectiveness_via_fallback() {
        let event = MetricEvent::new(
            MetricKind::ChannelEffectiveness,
            "self_hosted_fallback",
            3.0,
        )
        .with_denominator(3.0)
        .with_meta("channel", "self_hosted")
        .with_meta("fallback", "true");

        let summary = summarize_metrics(&[event]);
        let metric = &summary.channel_effectiveness[0];
        assert!(metric.via_fallback);
        assert_eq!(metric.failures, 0.0);
    }

    // -----------------------------------------------------------------------
    // MET-006: OpenClaw evaluation rate
    // -----------------------------------------------------------------------

    #[test]
    fn openclaw_evaluation_rate() {
        let event = MetricEvent::new(
            MetricKind::OpenClawEvaluationRate,
            "image_openclaw_pass_rate",
            8.0,
        )
        .with_denominator(10.0);

        let summary = summarize_metrics(&[event]);
        assert_eq!(summary.openclaw_evaluation_rate.len(), 1);
        let metric = &summary.openclaw_evaluation_rate[0];
        assert_eq!(metric.value, 8.0);
        assert_eq!(metric.denominator, Some(10.0));
        assert_eq!(metric.rate, Some(0.8));
    }

    // -----------------------------------------------------------------------
    // Coverage
    // -----------------------------------------------------------------------

    #[test]
    fn coverage_notes_missing_task_outcome() {
        let summary = summarize_metrics(&[]);
        let missing = check_coverage(&summary);
        assert!(missing.contains(&"MET-001: task_outcome"));
    }

    #[test]
    fn filter_by_kind_isolates_events() {
        let events = vec![
            MetricEvent::new(MetricKind::TaskOutcome, "t1", 1.0),
            MetricEvent::new(MetricKind::RejectionReason, "r1", 1.0),
            MetricEvent::new(MetricKind::TaskOutcome, "t2", 0.0),
        ];

        let outcomes: Vec<_> = filter_by_kind(&events, MetricKind::TaskOutcome).collect();
        assert_eq!(outcomes.len(), 2);
    }

    // -----------------------------------------------------------------------
    // Full pipeline simulation
    // -----------------------------------------------------------------------

    #[test]
    fn full_metrics_summary_all_families_populated() {
        let events = vec![
            // MET-001
            MetricEvent::new(MetricKind::TaskOutcome, "outcome", 1.0)
                .with_meta("state", "full_delivery"),
            // MET-002
            MetricEvent::new(MetricKind::CandidateSatisfaction, "cand_sat", 55.0)
                .with_denominator(60.0),
            // MET-003
            MetricEvent::new(MetricKind::QualifiedImageAchievement, "qualified", 4.0)
                .with_denominator(4.0),
            // MET-004
            MetricEvent::new(MetricKind::RejectionReason, "mech_rej", 2.0)
                .with_meta("reason", "mechanical"),
            MetricEvent::new(MetricKind::RejectionReason, "subj_rej", 1.0)
                .with_meta("reason", "openclaw_rejected"),
            // MET-005
            MetricEvent::new(MetricKind::ChannelEffectiveness, "web", 7.0)
                .with_denominator(8.0)
                .with_meta("channel", "web_fetch"),
            // MET-006
            MetricEvent::new(MetricKind::OpenClawEvaluationRate, "openclaw_rate", 5.0)
                .with_denominator(6.0),
        ];

        let summary = summarize_metrics(&events);

        assert!(summary.task_outcome.is_some());
        assert_eq!(summary.candidate_satisfaction.len(), 1);
        assert_eq!(summary.qualified_image_achievement.len(), 1);
        assert_eq!(summary.rejection_reasons.len(), 2);
        assert_eq!(summary.channel_effectiveness.len(), 1);
        assert_eq!(summary.openclaw_evaluation_rate.len(), 1);
    }
}
