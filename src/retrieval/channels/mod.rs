//! Retrieval channel implementations — v1.1.
//!
//! Provides concrete [`BaseRetrievalChannel`] implementations:
//!
//! - [`WebFetchChannel`] — normal web fetch (tier 1, default).
//! - [`SelfHostedChannel`] — self-hosted service boundary (tier 2).
//! - [`PaidChannel`] — paid online service boundary (tier 3, disabled by default).
//! - [`FixtureChannel`] — configurable fixture for internal testing.
//!
//! References: HLD §BaseRetrievalChannel, LLD §通道模型

pub mod fixture;
pub mod paid;
pub mod self_hosted;
pub mod web_fetch;

pub use fixture::FixtureChannel;
pub use paid::PaidChannel;
pub use self_hosted::SelfHostedChannel;
pub use web_fetch::WebFetchChannel;
