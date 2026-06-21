//! Retrieval channel implementations.
//!
//! Provides concrete [`BaseRetrievalChannel`] implementations:
//!
//! - [`WebFetchChannel`] — minimal web fetch (tier 1, default).
//! - [`FixtureChannel`] — configurable fixture for internal testing.
//!
//! References: HLD §BaseRetrievalChannel, LLD §通道模型

pub mod fixture;
pub mod web_fetch;

pub use fixture::FixtureChannel;
pub use web_fetch::WebFetchChannel;
