//! Search module — provider registry, weighted random scheduling, and test
//! fixtures.
//!
//! This module implements TASK-003: BaseProvider search port integration,
//! provider registry with readiness checks, weighted random scheduling,
//! candidate normalisation, deduplication, source tracking, and shortage
//! diagnosis.
//!
//! References: PRD §搜索与候选产品要求, HLD §Search Scheduler,
//! `docs/design/TASK-003-base-provider-search-design.md`

pub mod fixture;
pub mod registry;
pub mod scheduler;

// Re-export key types for convenience.
pub use registry::ProviderRegistry;
pub use scheduler::SearchScheduler;
