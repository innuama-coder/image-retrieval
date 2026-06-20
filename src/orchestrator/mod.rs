//! Task orchestrator module placeholder.
//!
//! Will host the main task loop: search → candidate gate → retrieve →
//! image accept → repeat-or-deliver.
//!
//! For TASK-001 this module declares the task-state types used by the
//! domain model. Full orchestration logic belongs to TASK-006.
//!
//! References: PRD §用户旅程与核心流程, HLD §Task Orchestrator

/// Re-export orchestrator-related domain types for convenience.
pub use crate::domain::delivery::{DeliveryDecision, TaskStatus};
