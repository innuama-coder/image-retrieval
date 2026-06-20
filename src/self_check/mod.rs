//! Readiness self-check module placeholder.
//!
//! Will host the pre-flight readiness reporter: QueryPlan validation,
//! provider readiness, channel readiness, OpenClaw availability, and
//! policy-blocking checks — all without performing search, retrieval,
//! or delivery.
//!
//! For TASK-001 this module is a placeholder. Implementation belongs to
//! TASK-008.
//!
//! References: PRD FR-012/AC-012, HLD §自助检查视图

/// Re-export types that self-check will consume.
pub use crate::domain::query_plan::QueryPlanInput;
pub use crate::error::{Diagnostic, DiagnosticItem, DiagnosticLevel};
