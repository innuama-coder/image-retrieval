//! Delivery package builder module placeholder.
//!
//! Will host delivery package construction: status.json, manifest.json,
//! summary.md, and image/evidence staging.
//!
//! For TASK-001 this module declares the delivery type boundaries.
//! Implementation belongs to TASK-007.
//!
//! References: PRD §交付物产品设计, HLD §Delivery Package Builder

/// Re-export delivery-related domain types for convenience.
pub use crate::domain::delivery::{DeliveryDecision, DeliveryManifest, TaskStatus};
