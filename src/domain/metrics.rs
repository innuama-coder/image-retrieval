//! Metrics domain types.
//!
//! Structured task evidence events supporting MET-001 through MET-006.
//!
//! References: PRD §数据、埋点与度量方案, HLD §Task Evidence & Metrics

use serde::{Deserialize, Serialize};

/// The kind of metric event, mapping to PRD MET-001 … MET-006.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MetricKind {
    /// MET-001: Task outcome distribution (input_rejected, full_delivery,
    /// limited_delivery, execution_blocked).
    TaskOutcome,

    /// MET-002: Candidate satisfaction rate (actual vs target).
    CandidateSatisfaction,

    /// MET-003: Qualified image achievement rate (qualified vs required).
    QualifiedImageAchievement,

    /// MET-004: Top rejection reasons for candidates and images.
    RejectionReason,

    /// MET-005: Retrieval channel effectiveness.
    ChannelEffectiveness,

    /// MET-006: OpenClaw evaluation pass / reject / uncertain ratio.
    OpenClawEvaluationRate,
}

/// A single metric event emitted during task execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricEvent {
    /// Which metric this event contributes to.
    pub kind: MetricKind,

    /// Human-readable label for the event.
    pub label: String,

    /// Numeric value (e.g. count, ratio numerator).
    pub value: f64,

    /// Optional denominator for rate computation.
    pub denominator: Option<f64>,

    /// Free-form metadata (e.g. provider name, channel tier, rejection
    /// category). Must not contain credentials or sensitive data.
    pub metadata: Vec<(String, String)>,
}

impl MetricEvent {
    pub fn new(kind: MetricKind, label: impl Into<String>, value: f64) -> Self {
        Self {
            kind,
            label: label.into(),
            value,
            denominator: None,
            metadata: Vec::new(),
        }
    }

    pub fn with_denominator(mut self, denominator: f64) -> Self {
        self.denominator = Some(denominator);
        self
    }

    pub fn with_meta(mut self, key: impl Into<String>, val: impl Into<String>) -> Self {
        self.metadata.push((key.into(), val.into()));
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metric_event_builder() {
        let event = MetricEvent::new(
            MetricKind::CandidateSatisfaction,
            "candidate satisfaction",
            45.0,
        )
        .with_denominator(60.0)
        .with_meta("provider", "fixture-provider");

        assert_eq!(event.kind, MetricKind::CandidateSatisfaction);
        assert_eq!(event.value, 45.0);
        assert_eq!(event.denominator, Some(60.0));
        assert_eq!(event.metadata.len(), 1);
    }
}
