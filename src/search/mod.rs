//! Search module — provider registry, weighted random scheduling,
//! SerpApi Google Images adapter, and test fixtures.
//!
//! v1.1: implements the search provider contract per LLD and
//! `docs/design/v1.1-TASK-002-search-provider-candidate-design.md`.
//!
//! References: PRD FR-004/FR-005, HLD §Search Scheduler, LLD §Search Provider Contract

pub mod fixture;
pub mod registry;
pub mod scheduler;
pub mod serpapi;

// Re-export key types for convenience.
pub use registry::ProviderRegistry;
pub use scheduler::SearchScheduler;
pub use serpapi::SerpApiGoogleImagesAdapter;
