//! Task evidence & metrics module placeholder.
//!
//! Will host structured metric-event emission for MET-001 through MET-006.
//!
//! For TASK-001 this module declares the metric type boundaries.
//! Implementation belongs to TASK-007.
//!
//! References: PRD §数据、埋点与度量方案, HLD §Task Evidence & Metrics

/// Re-export metrics-related domain types for convenience.
pub use crate::domain::metrics::{MetricEvent, MetricKind};
