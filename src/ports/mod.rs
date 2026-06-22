//! Port definitions — external capability boundaries.
//!
//! Defines the external capability contracts required by the HLD:
//!
//! - [`BaseSearchProvider`] — canonical v1.1 search provider port.
//! - [`BaseProvider`] — legacy search provider port (deprecated).
//! - [`BaseRetrievalChannel`] — retrieval channel port.
//! - [`OpenClawEvaluationPort`] — OpenClaw subjective evaluation port.
//!
//! References: PRD FR-004/FR-005, HLD §Core Interfaces,
//! `docs/design/v1.1-TASK-002-search-provider-candidate-design.md`

use crate::domain::candidate::{CandidateRecord, ProviderId};
use crate::domain::config::SearchProviderConfig;
use crate::domain::image::{ImageAcceptanceDecision, ImageRecord};
use crate::domain::retrieval::{
    FallbackEligibilityFact, RetrievalBatch, RetrievalChannelTier, RetrievalResult,
};
use crate::domain::search::{
    ProviderConstraintSupport, ProviderReadinessReport, SearchError, SearchRequest, SearchResponse,
};
use crate::error::Result;

// ---------------------------------------------------------------------------
// BaseSearchProvider — canonical v1.1 search provider port
// ---------------------------------------------------------------------------

/// Canonical v1.1 search provider contract.
///
/// Every image search engine adapter must satisfy this trait.
/// Production adapters (e.g. `serpapi_google_images`) implement this directly.
///
/// # Security
///
/// - `readiness()` must NOT perform a full search. It checks config shape,
///   credential env var presence, endpoint parseability, and quota signals.
/// - `search()` receives a package-safe [`SearchRequest`]; no credential
///   values may appear in the request or response DTOs.
pub trait BaseSearchProvider: Send + Sync {
    /// Return a stable, unique identifier for this provider.
    fn provider_id(&self) -> ProviderId;

    /// Human-readable display name.
    fn display_name(&self) -> &str;

    /// The adapter family.
    fn provider_kind(&self) -> crate::domain::config::SearchProviderKind;

    /// Declared constraint support for scheduling decisions.
    fn supported_constraints(&self) -> ProviderConstraintSupport;

    /// Evaluate readiness against the given config.
    ///
    /// Returns a structured report. Must not perform a full search.
    fn readiness(&self, config: &SearchProviderConfig) -> ProviderReadinessReport;

    /// Execute a search and return normalized results.
    fn search(&self, request: &SearchRequest) -> std::result::Result<SearchResponse, SearchError>;
}

// ---------------------------------------------------------------------------
// BaseProvider — legacy search provider port (deprecated)
// ---------------------------------------------------------------------------

/// Legacy search provider contract.
///
/// **Deprecated for v1.1**: new providers should implement
/// [`BaseSearchProvider`]. This trait is retained for backward
/// compatibility with existing code and tests.
pub trait BaseProvider {
    /// Return a stable, unique identifier for this provider.
    fn provider_id(&self) -> ProviderId;

    /// Return the user-visible name (e.g. "Brave Image Search").
    fn display_name(&self) -> &str;

    /// Check whether the provider is ready to serve search requests.
    ///
    /// Returns `Ok(())` when ready, or an `Error` describing why it is not
    /// (missing credentials, network, configuration).
    fn readiness(&self) -> Result<()>;

    /// Return the configured scheduling weight for this provider.
    ///
    /// A weight of 0 means the provider is disabled for scheduling.
    /// Negative weights are invalid and must be diagnosed.
    fn weight(&self) -> i32;

    /// Execute a search and return normalised candidate records.
    ///
    /// The query is derived from the validated QueryPlan description.
    /// The implementation is responsible for calling the external search
    /// API and mapping results into the shared `CandidateRecord` format.
    fn search(&self, query: &str, max_results: u32) -> Result<Vec<CandidateRecord>>;
}

// ---------------------------------------------------------------------------
// BaseRetrievalChannel — retrieval channel port
// ---------------------------------------------------------------------------

/// Retrieval channel contract.
///
/// Every retrieval channel (web fetch, self-hosted, paid) must satisfy
/// this trait. Channels expose their capabilities, tier, and failure
/// information; the orchestrator and policy layer decide fallback.
pub trait BaseRetrievalChannel {
    /// Return the tier this channel operates at.
    fn tier(&self) -> RetrievalChannelTier;

    /// Human-readable name for diagnostics and delivery manifests.
    fn display_name(&self) -> &str;

    /// Check whether the channel is enabled and ready.
    ///
    /// For paid channels, readiness must return an error when the user
    /// has not explicitly confirmed the paid tier.
    fn readiness(&self) -> Result<()>;

    /// Attempt to retrieve a batch of candidate images.
    ///
    /// Returns one `RetrievalResult` per candidate in the batch.
    /// Implementations must not silently skip access-control or
    /// authorization restrictions.
    fn retrieve_batch(&self, batch: &RetrievalBatch) -> Result<Vec<RetrievalResult>>;

    /// Produce a fallback eligibility fact after this channel failed for
    /// one or more candidates.
    fn fallback_fact(&self, reason: &str) -> FallbackEligibilityFact;
}

// ---------------------------------------------------------------------------
// OpenClawEvaluationPort — subjective evaluation port
// ---------------------------------------------------------------------------

/// OpenClaw subjective evaluation contract.
///
/// Covers two distinct evaluation boundaries per HLD/ADR-009:
/// 1. Structured candidate evaluation (before retrieval).
/// 2. Actual image evaluation (after retrieval).
pub trait OpenClawEvaluationPort {
    /// Check whether OpenClaw is available for production evaluation.
    ///
    /// Returns `Ok(())` when ready. When unavailable, production tasks
    /// must enter execution-blocked state — mock or fixture results
    /// cannot be used as production delivery evidence.
    fn readiness(&self) -> Result<()>;

    /// Evaluate a batch of structured candidate records and return an
    /// acceptance decision for each.
    ///
    /// This is the candidate-phase subjective evaluation (HLD §主观评价架构边界).
    fn evaluate_candidates(
        &self,
        candidates: &[CandidateRecord],
        description: &str,
    ) -> Result<Vec<crate::domain::candidate::CandidateDecision>>;

    /// Evaluate a batch of actually-retrieved images and return an
    /// acceptance decision for each.
    ///
    /// This is the image-phase subjective evaluation.
    fn evaluate_images(
        &self,
        images: &[ImageRecord],
        description: &str,
    ) -> Result<Vec<ImageAcceptanceDecision>>;
}

#[cfg(test)]
mod tests {
    //! Compile-time verification that the port traits are object-safe
    //! (can be used as `dyn Trait`).

    use super::*;

    #[test]
    fn base_search_provider_is_object_safe() {
        fn _assert(_p: &dyn BaseSearchProvider) {}
    }

    #[test]
    fn base_provider_is_object_safe() {
        fn _assert(_p: &dyn BaseProvider) {}
    }

    #[test]
    fn base_retrieval_channel_is_object_safe() {
        fn _assert(_c: &dyn BaseRetrievalChannel) {}
    }

    #[test]
    fn openclaw_evaluation_port_is_object_safe() {
        fn _assert(_p: &dyn OpenClawEvaluationPort) {}
    }
}
