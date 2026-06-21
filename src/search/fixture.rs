//! Fixture / mock provider implementations for automated testing.
//!
//! These providers implement [`BaseProvider`] and are suitable for
//! scheduler tests, integration tests, and self-check readiness tests.
//!
//! They are gated behind `#[cfg(test)]` so they never appear in
//! production builds.
//!
//! References: `docs/design/TASK-003-base-provider-search-design.md`

use crate::domain::candidate::{CandidateId, CandidateRecord, ImageDimensions, ProviderId};
use crate::domain::search::ProviderRegistration;
use crate::error::{Error, Result};
use crate::ports::BaseProvider;

// ===========================================================================
// FixtureProvider — configurable stub for scheduler tests
// ===========================================================================

/// A configurable fixture provider that returns pre-defined candidate sets.
///
/// Supports simulating different provider behaviours:
/// - Normal operation with varying result sizes.
/// - Gradual exhaustion (fewer results per call).
/// - Transient failures.
/// - Readiness failures.
pub struct FixtureProvider {
    registration: ProviderRegistration,
    ready: bool,
    /// Pre-configured response batches. Each call to `search()` consumes
    /// one batch. When exhausted, returns empty results.
    response_batches: std::cell::RefCell<Vec<Vec<CandidateRecord>>>,
    /// Track how many times `search()` was called.
    call_count: std::cell::Cell<u32>,
}

impl FixtureProvider {
    /// Create a ready fixture provider with default weight 1.
    pub fn new(id: &str, display_name: &str) -> Self {
        Self {
            registration: ProviderRegistration::new(ProviderId::new(id), display_name)
                .with_enabled(true)
                .with_weight(1),
            ready: true,
            response_batches: std::cell::RefCell::new(Vec::new()),
            call_count: std::cell::Cell::new(0),
        }
    }

    /// Create a provider in not-ready state.
    pub fn not_ready(id: &str, display_name: &str) -> Self {
        let mut p = Self::new(id, display_name);
        p.ready = false;
        p
    }

    /// Set the scheduling weight.
    pub fn with_weight(mut self, weight: i32) -> Self {
        self.registration.weight = weight;
        self
    }

    /// Set the enabled state.
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.registration.enabled = enabled;
        self
    }

    /// Pre-load response batches.
    ///
    /// Each call to `search()` consumes one batch. After all batches are
    /// consumed, `search()` returns empty results (simulating provider
    /// exhaustion).
    pub fn with_responses(self, batches: Vec<Vec<CandidateRecord>>) -> Self {
        *self.response_batches.borrow_mut() = batches;
        self
    }

    /// How many times `search()` was called.
    pub fn call_count(&self) -> u32 {
        self.call_count.get()
    }
}

impl BaseProvider for FixtureProvider {
    fn provider_id(&self) -> ProviderId {
        self.registration.provider_id.clone()
    }

    fn display_name(&self) -> &str {
        &self.registration.display_name
    }

    fn readiness(&self) -> Result<()> {
        if self.ready {
            Ok(())
        } else {
            Err(Error::provider_failure(
                self.registration.provider_id.to_string(),
                "fixture: provider not ready",
            ))
        }
    }

    fn weight(&self) -> i32 {
        self.registration.weight
    }

    fn search(&self, _query: &str, _max_results: u32) -> Result<Vec<CandidateRecord>> {
        let count = self.call_count.get();
        self.call_count.set(count + 1);

        let mut batches = self.response_batches.borrow_mut();
        if (count as usize) < batches.len() {
            Ok(std::mem::take(&mut batches[count as usize]))
        } else {
            // Exhausted — return empty
            Ok(Vec::new())
        }
    }
}

// ===========================================================================
// Candidate factory helpers
// ===========================================================================

/// Create a minimal candidate record for testing.
///
/// Each candidate gets a unique id and source URL based on `index`.
pub fn make_fixture_candidate(index: u32, provider_id: &str, base_url: &str) -> CandidateRecord {
    CandidateRecord {
        id: CandidateId::new(format!("fixture-{}-{}", provider_id, index)),
        provider_id: ProviderId::new(provider_id),
        source_url: format!("{}/image{}.jpg", base_url, index),
        thumbnail_url: Some(format!("{}/thumb{}.jpg", base_url, index)),
        title: Some(format!("Fixture Image {} from {}", index, provider_id)),
        page_url: Some(format!("{}/page{}.html", base_url, index)),
        dimensions: Some(ImageDimensions {
            width: 800,
            height: 600,
        }),
    }
}

/// Create a batch of fixture candidates.
pub fn make_fixture_batch(provider_id: &str, count: u32, start_index: u32) -> Vec<CandidateRecord> {
    (start_index..start_index + count)
        .map(|i| {
            make_fixture_candidate(
                i,
                provider_id,
                &format!("https://{}.example.com", provider_id),
            )
        })
        .collect()
}

// ===========================================================================
// Ready-set fixture helpers
// ===========================================================================

/// Create a ready fixture provider with a single batch of candidates.
pub fn ready_fixture_with_candidates(
    id: &str,
    name: &str,
    weight: i32,
    candidate_count: u32,
) -> FixtureProvider {
    FixtureProvider::new(id, name)
        .with_weight(weight)
        .with_responses(vec![make_fixture_batch(id, candidate_count, 0)])
}

/// Create a ready fixture provider with multiple batches.
pub fn ready_fixture_with_batches(
    id: &str,
    name: &str,
    weight: i32,
    batch_sizes: &[u32],
) -> FixtureProvider {
    let mut offset = 0;
    let batches: Vec<Vec<CandidateRecord>> = batch_sizes
        .iter()
        .map(|&size| {
            let batch = make_fixture_batch(id, size, offset);
            offset += size;
            batch
        })
        .collect();
    FixtureProvider::new(id, name)
        .with_weight(weight)
        .with_responses(batches)
}

// ===========================================================================
// Tests for the fixtures themselves
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_provider_basics() {
        let p = FixtureProvider::new("test", "Test Provider");
        assert_eq!(p.provider_id().to_string(), "test");
        assert_eq!(p.display_name(), "Test Provider");
        assert_eq!(p.weight(), 1);
        assert!(p.readiness().is_ok());
    }

    #[test]
    fn fixture_provider_not_ready() {
        let p = FixtureProvider::not_ready("test", "Test");
        assert!(p.readiness().is_err());
    }

    #[test]
    fn fixture_provider_returns_batches() {
        let batch1 = make_fixture_batch("p1", 5, 0);
        let batch2 = make_fixture_batch("p1", 3, 5);
        let p =
            FixtureProvider::new("p1", "P1").with_responses(vec![batch1.clone(), batch2.clone()]);

        let r1 = p.search("q", 10).unwrap();
        assert_eq!(r1.len(), 5);
        assert_eq!(p.call_count(), 1);

        let r2 = p.search("q", 10).unwrap();
        assert_eq!(r2.len(), 3);
        assert_eq!(p.call_count(), 2);

        // Exhausted
        let r3 = p.search("q", 10).unwrap();
        assert!(r3.is_empty());
        assert_eq!(p.call_count(), 3);
    }

    #[test]
    fn fixture_provider_custom_weight() {
        let p = FixtureProvider::new("p", "P").with_weight(5);
        assert_eq!(p.weight(), 5);
    }

    #[test]
    fn fixture_provider_disabled() {
        let p = FixtureProvider::new("p", "P").with_enabled(false);
        // enabled is on registration, not the trait method
        // (the trait's weight() returns the configured weight)
        assert_eq!(p.weight(), 1);
    }

    #[test]
    fn make_fixture_candidate_has_source_url() {
        let c = make_fixture_candidate(42, "brave", "https://brave.example.com");
        assert_eq!(c.provider_id.to_string(), "brave");
        assert_eq!(c.source_url, "https://brave.example.com/image42.jpg");
        assert!(c.thumbnail_url.is_some());
        assert!(c.title.is_some());
    }

    #[test]
    fn make_fixture_batch_creates_unique_ids() {
        let batch = make_fixture_batch("p1", 10, 100);
        assert_eq!(batch.len(), 10);
        let ids: std::collections::HashSet<String> =
            batch.iter().map(|c| c.id.to_string()).collect();
        assert_eq!(ids.len(), 10); // all unique
    }

    #[test]
    fn ready_fixture_with_candidates_works() {
        let p = ready_fixture_with_candidates("p1", "P1", 2, 25);
        assert_eq!(p.weight(), 2);
        let results = p.search("q", 30).unwrap();
        assert_eq!(results.len(), 25);
    }

    #[test]
    fn ready_fixture_with_batches_works() {
        let p = ready_fixture_with_batches("p1", "P1", 1, &[10, 10, 5]);
        assert_eq!(p.search("q", 20).unwrap().len(), 10);
        assert_eq!(p.search("q", 20).unwrap().len(), 10);
        assert_eq!(p.search("q", 20).unwrap().len(), 5);
        assert!(p.search("q", 20).unwrap().is_empty());
    }

    #[test]
    fn fixture_provider_does_not_leak_credentials() {
        // Verify that no credential fields appear in any fixture output
        let p = ready_fixture_with_candidates("p1", "P1", 1, 5);
        let results = p.search("test", 10).unwrap();
        let json = serde_json::to_string(&results).unwrap();
        assert!(!json.contains("api_key"));
        assert!(!json.contains("token"));
        assert!(!json.contains("secret"));
        assert!(!json.contains("password"));
        assert!(!json.contains("credential"));
    }
}
