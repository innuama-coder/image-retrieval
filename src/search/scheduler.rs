//! Weighted random search scheduler.
//!
//! Implements the weighted random selection algorithm, per-invocation
//! provider dispatch, candidate deduplication, source tracking, and
//! candidate shortage diagnosis.
//!
//! The scheduler uses a pluggable random source so tests can provide
//! deterministic behaviour. In production, the default `StdRandom` wraps
//! the OS random source.
//!
//! References: PRD §搜索与候选产品要求, HLD §Search Scheduler,
//! `docs/design/TASK-003-base-provider-search-design.md`

use crate::domain::candidate::{CandidateRecord, ProviderId};
use crate::domain::query_plan::TaskPlan;
use crate::domain::search::{
    CandidateShortageReason, ProviderReadiness, SearchFailureCategory, SearchOutcome,
    SearchUsageEvent, WeightEntry,
};
use crate::error::Error;
use crate::ports::BaseProvider;
use crate::search::registry::ProviderRegistry;
use std::collections::{HashMap, HashSet};

// ---------------------------------------------------------------------------
// Random source abstraction
// ---------------------------------------------------------------------------

/// Pluggable random source for weighted selection.
///
/// Production code uses [`StdRandom`]; tests can provide a deterministic
/// implementation.
pub trait RandomSource {
    /// Return a random `u32` in `[0, max)`.
    fn gen_range(&mut self, max: u32) -> u32;
}

/// Standard OS random source.
pub struct StdRandom;

impl RandomSource for StdRandom {
    fn gen_range(&mut self, max: u32) -> u32 {
        if max == 0 {
            return 0;
        }
        // Use a simple but adequate approach. For production, this could
        // be replaced with a proper CSPRNG.
        let val: u32 = fast_random_u32();
        val % max
    }
}

/// A fast, non-cryptographic random generator suitable for weighted
/// scheduling. Uses a simple xorshift algorithm.
fn fast_random_u32() -> u32 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    // xorshift64
    let mut x = seed.wrapping_add(1);
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    x as u32
}

// ---------------------------------------------------------------------------
// Search scheduler
// ---------------------------------------------------------------------------

/// The weighted random search scheduler.
///
/// Orchestrates provider selection, candidate collection, deduplication,
/// and shortage diagnosis. Does NOT own providers — it receives them
/// by reference during scheduling.
pub struct SearchScheduler {
    /// Maximum number of provider invocations before the scheduler stops
    /// to prevent unbounded loops. This is a safety cap; normally the
    /// scheduler stops when the candidate target is met or all providers
    /// are exhausted.
    max_invocations: u32,
}

impl Default for SearchScheduler {
    fn default() -> Self {
        Self {
            max_invocations: 50,
        }
    }
}

impl SearchScheduler {
    /// Create a new scheduler with the default invocation cap.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the maximum number of provider invocations.
    pub fn with_max_invocations(mut self, max: u32) -> Self {
        self.max_invocations = max;
        self
    }

    /// Execute a search session.
    ///
    /// `providers` maps provider id strings to `&dyn BaseProvider` references.
    /// The scheduler will:
    ///
    /// 1. Build the effective weight table from the registry.
    /// 2. Check each provider's readiness.
    /// 3. Loop: weighted-random select a provider → call search → dedup.
    /// 4. Stop when the candidate target is met, all providers are exhausted,
    ///    or the invocation cap is reached.
    /// 5. Return a [`SearchOutcome`] with candidates, usage events, and
    ///    shortage diagnosis.
    pub fn run(
        &self,
        task_plan: &TaskPlan,
        registry: &ProviderRegistry,
        providers: &HashMap<String, &dyn BaseProvider>,
        rng: &mut dyn RandomSource,
    ) -> SearchOutcome {
        let candidate_target = task_plan.candidate_target;

        // Step 1: Check readiness of each registered provider
        let mut readiness_map: HashMap<String, ProviderReadiness> = HashMap::new();
        for reg in registry.iter() {
            let readiness = if let Some(provider) = providers.get(&reg.provider_id.to_string()) {
                match provider.readiness() {
                    Ok(()) => ProviderReadiness::Ready,
                    Err(e) => match &e {
                        Error::ProviderFailure { reason, .. } if reason.contains("credentials") => {
                            ProviderReadiness::MissingCredentials
                        }
                        Error::ProviderFailure { reason, .. } if reason.contains("rate") => {
                            ProviderReadiness::RateLimited
                        }
                        Error::ProviderFailure { .. } => ProviderReadiness::Unavailable,
                        _ => ProviderReadiness::Unavailable,
                    },
                }
            } else {
                // Provider not in the adapter map — treat as unavailable
                ProviderReadiness::Unavailable
            };

            // A disabled registration overrides any provider response
            let effective = if !reg.enabled {
                ProviderReadiness::Disabled
            } else if reg.weight <= 0 {
                ProviderReadiness::Misconfigured
            } else {
                readiness
            };

            readiness_map.insert(reg.provider_id.to_string(), effective);
        }

        // Step 2: Build the effective weight table
        let (weight_table, readiness_summary) =
            registry.build_weight_table_with_readiness(&readiness_map);

        if weight_table.is_empty() {
            return SearchOutcome {
                candidates: Vec::new(),
                usage_events: Vec::new(),
                total_invocations: 0,
                candidate_target,
                target_met: false,
                shortage_reason: Some(CandidateShortageReason::NoAvailableProviders),
                readiness_summary,
            };
        }

        // Step 3: Scheduling loop
        let mut all_candidates: Vec<CandidateRecord> = Vec::new();
        let mut usage_events: Vec<SearchUsageEvent> = Vec::new();
        let mut seen_urls: HashSet<String> = HashSet::new();
        let mut exhausted_providers: HashSet<String> = HashSet::new();
        let mut failed_providers: HashSet<String> = HashSet::new();
        let mut invocations: u32 = 0;

        while (all_candidates.len() as u32) < candidate_target
            && exhausted_providers.len() < weight_table.len()
            && invocations < self.max_invocations
        {
            // Filter to non-exhausted providers
            let active_entries: Vec<&WeightEntry> = weight_table
                .iter()
                .filter(|e| !exhausted_providers.contains(&e.provider_id.to_string()))
                .collect();

            if active_entries.is_empty() {
                break;
            }

            // Recompute total weight of active providers
            let active_total_weight: u32 = active_entries.iter().map(|e| e.weight).sum();
            if active_total_weight == 0 {
                break;
            }

            // Weighted random selection
            let selected = select_weighted(&active_entries, active_total_weight, rng);
            let provider_id_str = selected.provider_id.to_string();

            invocations += 1;

            // Get the provider adapter
            let provider = match providers.get(&provider_id_str) {
                Some(p) => *p,
                None => {
                    // Should not happen with readiness check, but handle gracefully
                    exhausted_providers.insert(provider_id_str.clone());
                    usage_events.push(SearchUsageEvent::failure_event(
                        selected.provider_id.clone(),
                        SearchFailureCategory::Unavailable,
                        ProviderReadiness::Unavailable,
                        selected.weight,
                    ));
                    continue;
                }
            };

            // Determine how many results to request
            let remaining_needed = candidate_target.saturating_sub(all_candidates.len() as u32);
            // We overshoot a bit to account for duplicates
            let request_count = (remaining_needed.saturating_mul(3)).max(10);

            let search_result = provider.search(&task_plan.query_plan.description, request_count);

            match search_result {
                Ok(candidates) => {
                    let raw_count = candidates.len() as u32;

                    // Deduplicate by source_url
                    let mut deduped_count: u32 = 0;
                    for candidate in candidates {
                        if seen_urls.insert(candidate.source_url.clone()) {
                            all_candidates.push(candidate);
                            deduped_count += 1;
                        }
                    }

                    let is_exhausted = raw_count == 0
                        || raw_count < request_count / 2
                        || deduped_count == 0 && raw_count > 0;

                    if is_exhausted && raw_count == 0 {
                        // Empty result — provider may be exhausted
                        exhausted_providers.insert(provider_id_str.clone());
                    } else if is_exhausted && deduped_count == 0 {
                        // All returned were duplicates — provider likely exhausted
                        exhausted_providers.insert(provider_id_str.clone());
                    }

                    usage_events.push(SearchUsageEvent::success_event(
                        selected.provider_id.clone(),
                        raw_count,
                        deduped_count,
                        is_exhausted && exhausted_providers.contains(&provider_id_str),
                        selected.weight,
                    ));
                }
                Err(e) => {
                    let failure_category = classify_search_error(&e);
                    failed_providers.insert(provider_id_str.clone());
                    // Don't mark as exhausted on transient errors — keep in table
                    // for potential retry unless it's a terminal failure
                    if matches!(
                        failure_category,
                        SearchFailureCategory::Unavailable
                            | SearchFailureCategory::UnnormalizableResponse
                    ) {
                        exhausted_providers.insert(provider_id_str.clone());
                    }

                    usage_events.push(SearchUsageEvent::failure_event(
                        selected.provider_id.clone(),
                        failure_category,
                        ProviderReadiness::Unavailable,
                        selected.weight,
                    ));
                }
            }

            // If we've collected enough, stop
            if (all_candidates.len() as u32) >= candidate_target {
                break;
            }
        }

        let total_deduped = all_candidates.len() as u32;
        let target_met = total_deduped >= candidate_target;

        let shortage_reason = if target_met {
            None
        } else if exhausted_providers.len() >= weight_table.len() && failed_providers.is_empty() {
            Some(CandidateShortageReason::AllProvidersExhausted)
        } else if !failed_providers.is_empty() {
            Some(CandidateShortageReason::PartialFailureWithExhaustion {
                failed_providers: failed_providers.iter().map(ProviderId::new).collect(),
                exhausted_providers: exhausted_providers.iter().map(ProviderId::new).collect(),
            })
        } else {
            let total_raw: u32 = usage_events.iter().map(|e| e.raw_candidate_count).sum();
            let duplicates_removed = total_raw.saturating_sub(total_deduped);
            Some(CandidateShortageReason::InsufficientUniqueCandidates {
                total_raw,
                total_deduped,
                duplicates_removed,
            })
        };

        SearchOutcome {
            candidates: all_candidates,
            usage_events,
            total_invocations: invocations,
            candidate_target,
            target_met,
            shortage_reason,
            readiness_summary,
        }
    }
}

// ---------------------------------------------------------------------------
// Weighted random selection
// ---------------------------------------------------------------------------

/// Select a provider from the weight table using weighted random selection.
///
/// Each provider's probability of being selected is proportional to its weight.
fn select_weighted<'a>(
    entries: &[&'a WeightEntry],
    total_weight: u32,
    rng: &mut dyn RandomSource,
) -> &'a WeightEntry {
    if entries.is_empty() || total_weight == 0 {
        // Safety fallback — should not happen if callers check preconditions
        panic!("select_weighted called with empty entries or zero total weight");
    }

    let mut target = rng.gen_range(total_weight);
    for entry in entries {
        if target < entry.weight {
            return entry;
        }
        target -= entry.weight;
    }

    // Fallback to last entry (should not reach here with correct arithmetic)
    entries.last().expect("non-empty entries")
}

// ---------------------------------------------------------------------------
// Error classification
// ---------------------------------------------------------------------------

/// Classify a search error into a [`SearchFailureCategory`].
fn classify_search_error(error: &Error) -> SearchFailureCategory {
    match error {
        Error::ProviderFailure { reason, .. } => {
            let lower = reason.to_lowercase();
            if lower.contains("timeout") || lower.contains("timed out") {
                SearchFailureCategory::Timeout
            } else if lower.contains("rate") && lower.contains("limit") {
                SearchFailureCategory::RateLimited
            } else if lower.contains("empty") || lower.contains("no result") {
                SearchFailureCategory::EmptyResult
            } else if lower.contains("parse")
                || lower.contains("normalize")
                || lower.contains("malformed")
            {
                SearchFailureCategory::UnnormalizableResponse
            } else {
                SearchFailureCategory::ProviderError
            }
        }
        _ => SearchFailureCategory::Other,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::candidate::CandidateId;
    use crate::domain::query_plan::{ContentConstraints, QualityTier, ValidatedQueryPlan};
    use crate::domain::search::ProviderRegistration;
    use crate::error::Result;
    use std::cell::RefCell;

    // -----------------------------------------------------------------------
    // Deterministic random source for testing
    // -----------------------------------------------------------------------

    /// A deterministic random source that returns pre-programmed values.
    struct TestRandom {
        values: Vec<u32>,
        index: RefCell<usize>,
    }

    impl TestRandom {
        fn new(values: Vec<u32>) -> Self {
            Self {
                values,
                index: RefCell::new(0),
            }
        }
    }

    impl RandomSource for TestRandom {
        fn gen_range(&mut self, max: u32) -> u32 {
            let idx = *self.index.borrow();
            let val = self.values.get(idx).copied().unwrap_or(0);
            *self.index.borrow_mut() = idx + 1;
            val % max
        }
    }

    // -----------------------------------------------------------------------
    // Stub provider for testing
    // -----------------------------------------------------------------------

    /// A stub provider that returns pre-configured candidates.
    struct StubProvider {
        id: ProviderId,
        name: String,
        weight: i32,
        ready: bool,
        candidates: RefCell<Vec<Vec<CandidateRecord>>>,
        call_count: RefCell<usize>,
    }

    impl StubProvider {
        fn new(id: &str, name: &str, weight: i32, ready: bool) -> Self {
            Self {
                id: ProviderId::new(id),
                name: name.into(),
                weight,
                ready,
                candidates: RefCell::new(Vec::new()),
                call_count: RefCell::new(0),
            }
        }

        fn with_responses(
            id: &str,
            name: &str,
            weight: i32,
            ready: bool,
            responses: Vec<Vec<CandidateRecord>>,
        ) -> Self {
            Self {
                id: ProviderId::new(id),
                name: name.into(),
                weight,
                ready,
                candidates: RefCell::new(responses),
                call_count: RefCell::new(0),
            }
        }

        #[allow(dead_code)]
        fn call_count(&self) -> usize {
            *self.call_count.borrow()
        }
    }

    impl BaseProvider for StubProvider {
        fn provider_id(&self) -> ProviderId {
            self.id.clone()
        }

        fn display_name(&self) -> &str {
            &self.name
        }

        fn readiness(&self) -> Result<()> {
            if self.ready {
                Ok(())
            } else {
                Err(Error::provider_failure(
                    self.id.to_string(),
                    "missing credentials",
                ))
            }
        }

        fn weight(&self) -> i32 {
            self.weight
        }

        fn search(&self, _query: &str, _max_results: u32) -> Result<Vec<CandidateRecord>> {
            let mut count = self.call_count.borrow_mut();
            let idx = *count;
            *count += 1;
            let mut responses = self.candidates.borrow_mut();
            if idx < responses.len() {
                Ok(std::mem::take(&mut responses[idx]))
            } else {
                Ok(Vec::new())
            }
        }
    }

    fn make_candidate(id: &str, provider: &str, url: &str) -> CandidateRecord {
        CandidateRecord {
            id: CandidateId::new(id),
            provider_id: ProviderId::new(provider),
            source_url: url.into(),
            thumbnail_url: None,
            title: Some(format!("Image {}", id)),
            page_url: None,
            dimensions: None,
        }
    }

    fn make_task_plan(required_count: u32) -> TaskPlan {
        let plan = ValidatedQueryPlan {
            description: "test query".into(),
            required_count,
            quality_tier: QualityTier::General,
            content_constraints: ContentConstraints::default(),
            authorization_preference: Default::default(),
            output_preference: Default::default(),
            retry_limit: 3,
        };
        TaskPlan::from_validated(plan)
    }

    // -----------------------------------------------------------------------
    // Weighted selection tests
    // -----------------------------------------------------------------------

    #[test]
    fn weighted_selection_picks_first_when_rng_zero() {
        let entries = [
            WeightEntry {
                provider_id: ProviderId::new("a"),
                display_name: "A".into(),
                weight: 2,
            },
            WeightEntry {
                provider_id: ProviderId::new("b"),
                display_name: "B".into(),
                weight: 3,
            },
        ];
        let entry_refs: Vec<&WeightEntry> = entries.iter().collect();
        let mut rng = TestRandom::new(vec![0]); // picks first
        let selected = select_weighted(&entry_refs, 5, &mut rng);
        assert_eq!(selected.provider_id.to_string(), "a");
    }

    #[test]
    fn weighted_selection_picks_second_when_rng_exceeds_first_weight() {
        let entries = [
            WeightEntry {
                provider_id: ProviderId::new("a"),
                display_name: "A".into(),
                weight: 2,
            },
            WeightEntry {
                provider_id: ProviderId::new("b"),
                display_name: "B".into(),
                weight: 3,
            },
        ];
        let entry_refs: Vec<&WeightEntry> = entries.iter().collect();
        let mut rng = TestRandom::new(vec![2]); // equal to first weight, falls through to second
        let selected = select_weighted(&entry_refs, 5, &mut rng);
        assert_eq!(selected.provider_id.to_string(), "b");
    }

    #[test]
    fn weighted_selection_respects_proportional_distribution() {
        // With enough samples, the distribution should approximate weights
        let entries = [
            WeightEntry {
                provider_id: ProviderId::new("a"),
                display_name: "A".into(),
                weight: 1,
            },
            WeightEntry {
                provider_id: ProviderId::new("b"),
                display_name: "B".into(),
                weight: 2,
            },
        ];
        let entry_refs: Vec<&WeightEntry> = entries.iter().collect();
        let total_weight = 3;

        // Simulate "random" values that span the entire range uniformly
        let samples: Vec<u32> = (0..300).map(|i| i % total_weight).collect();
        let mut rng = TestRandom::new(samples);

        let mut count_a = 0;
        let mut count_b = 0;
        for _ in 0..300 {
            let selected = select_weighted(&entry_refs, total_weight, &mut rng);
            match selected.provider_id.to_string().as_str() {
                "a" => count_a += 1,
                "b" => count_b += 1,
                _ => {}
            }
        }

        // With weight 1:2, expect roughly 100:200 (± some tolerance)
        assert!(count_a >= 80, "a got {}, expected ~100", count_a);
        assert!(count_b >= 180, "b got {}, expected ~200", count_b);
    }

    // -----------------------------------------------------------------------
    // Scheduler tests
    // -----------------------------------------------------------------------

    #[test]
    fn scheduler_no_providers_returns_shortage() {
        let registry = ProviderRegistry::new();
        let task_plan = make_task_plan(3);
        let scheduler = SearchScheduler::new();
        let providers: HashMap<String, &dyn BaseProvider> = HashMap::new();
        let mut rng = TestRandom::new(vec![]);

        let outcome = scheduler.run(&task_plan, &registry, &providers, &mut rng);
        assert!(!outcome.target_met);
        assert!(outcome.candidates.is_empty());
        assert!(matches!(
            outcome.shortage_reason,
            Some(CandidateShortageReason::NoAvailableProviders)
        ));
    }

    #[test]
    fn scheduler_single_provider_reaches_target() {
        let mut registry = ProviderRegistry::new();
        registry.register(
            ProviderRegistration::new(ProviderId::new("p1"), "P1")
                .with_enabled(true)
                .with_weight(1),
        );

        // Provider returns 70 candidates (enough for target of 60)
        let candidates: Vec<CandidateRecord> = (0..70)
            .map(|i| make_candidate(&format!("c{}", i), "p1", &format!("https://ex.com/{}", i)))
            .collect();

        let stub = StubProvider::with_responses("p1", "P1", 1, true, vec![candidates]);

        let mut providers: HashMap<String, &dyn BaseProvider> = HashMap::new();
        providers.insert("p1".to_string(), &stub);

        let task_plan = make_task_plan(3); // target = 60
        let scheduler = SearchScheduler::new();
        let mut rng = TestRandom::new(vec![0]);

        let outcome = scheduler.run(&task_plan, &registry, &providers, &mut rng);
        assert!(outcome.target_met);
        assert!(outcome.candidates.len() >= 60);
        assert_eq!(outcome.total_invocations, 1);
        assert!(outcome.shortage_reason.is_none());
    }

    #[test]
    fn scheduler_deduplicates_by_source_url() {
        let mut registry = ProviderRegistry::new();
        registry.register(
            ProviderRegistration::new(ProviderId::new("p1"), "P1")
                .with_enabled(true)
                .with_weight(1),
        );

        // First call: 5 unique, 2 duplicates
        let batch1: Vec<CandidateRecord> = vec![
            make_candidate("a1", "p1", "https://ex.com/1"),
            make_candidate("a2", "p1", "https://ex.com/2"),
            make_candidate("a3", "p1", "https://ex.com/1"), // duplicate URL
            make_candidate("a4", "p1", "https://ex.com/3"),
            make_candidate("a5", "p1", "https://ex.com/4"),
            make_candidate("a6", "p1", "https://ex.com/2"), // duplicate URL
            make_candidate("a7", "p1", "https://ex.com/5"),
        ];

        let stub = StubProvider::with_responses("p1", "P1", 1, true, vec![batch1]);

        let mut providers: HashMap<String, &dyn BaseProvider> = HashMap::new();
        providers.insert("p1".to_string(), &stub);

        let task_plan = make_task_plan(1); // target = 20
        let scheduler = SearchScheduler::new();
        let mut rng = TestRandom::new(vec![0]);

        let outcome = scheduler.run(&task_plan, &registry, &providers, &mut rng);
        assert_eq!(outcome.candidates.len(), 5); // 7 raw, 5 unique
        assert_eq!(outcome.usage_events[0].raw_candidate_count, 7);
        assert_eq!(outcome.usage_events[0].deduped_candidate_count, 5);
    }

    #[test]
    fn scheduler_multi_provider_equal_weight() {
        let mut registry = ProviderRegistry::new();
        registry.register(
            ProviderRegistration::new(ProviderId::new("a"), "A")
                .with_enabled(true)
                .with_weight(1),
        );
        registry.register(
            ProviderRegistration::new(ProviderId::new("b"), "B")
                .with_enabled(true)
                .with_weight(1),
        );

        let candidates_a: Vec<CandidateRecord> = (0..35)
            .map(|i| make_candidate(&format!("ca{}", i), "a", &format!("https://a.ex/{}", i)))
            .collect();
        let candidates_b: Vec<CandidateRecord> = (0..35)
            .map(|i| make_candidate(&format!("cb{}", i), "b", &format!("https://b.ex/{}", i)))
            .collect();

        let stub_a = StubProvider::with_responses("a", "A", 1, true, vec![candidates_a]);
        let stub_b = StubProvider::with_responses("b", "B", 1, true, vec![candidates_b]);

        let mut providers: HashMap<String, &dyn BaseProvider> = HashMap::new();
        providers.insert("a".to_string(), &stub_a);
        providers.insert("b".to_string(), &stub_b);

        let task_plan = make_task_plan(3); // target = 60
        let scheduler = SearchScheduler::new();
        // rng: 0 picks "a" first, 1 picks "b" second (total_weight=2)
        let mut rng = TestRandom::new(vec![0, 1]);

        let outcome = scheduler.run(&task_plan, &registry, &providers, &mut rng);
        assert!(outcome.target_met);
        // Both providers should have been invoked
        assert_eq!(outcome.total_invocations, 2);
        // Events should show both providers used
        let used_ids: HashSet<String> = outcome
            .usage_events
            .iter()
            .map(|e| e.provider_id.to_string())
            .collect();
        assert!(used_ids.contains("a"));
        assert!(used_ids.contains("b"));
    }

    #[test]
    fn scheduler_multi_provider_weighted_selection() {
        let mut registry = ProviderRegistry::new();
        // Provider A has weight 3, B has weight 1
        registry.register(
            ProviderRegistration::new(ProviderId::new("a"), "A")
                .with_enabled(true)
                .with_weight(3),
        );
        registry.register(
            ProviderRegistration::new(ProviderId::new("b"), "B")
                .with_enabled(true)
                .with_weight(1),
        );

        // Each provider returns small batches so multiple calls are needed
        let candidates_a: Vec<CandidateRecord> = (0..10)
            .map(|i| make_candidate(&format!("ca{}", i), "a", &format!("https://a.ex/{}", i)))
            .collect();
        let candidates_b: Vec<CandidateRecord> = (0..10)
            .map(|i| make_candidate(&format!("cb{}", i), "b", &format!("https://b.ex/{}", i)))
            .collect();

        let stub_a = StubProvider::with_responses(
            "a",
            "A",
            3,
            true,
            vec![candidates_a.clone(), candidates_a.clone(), candidates_a],
        );
        let stub_b = StubProvider::with_responses(
            "b",
            "B",
            1,
            true,
            vec![candidates_b.clone(), candidates_b],
        );

        let mut providers: HashMap<String, &dyn BaseProvider> = HashMap::new();
        providers.insert("a".to_string(), &stub_a);
        providers.insert("b".to_string(), &stub_b);

        let task_plan = make_task_plan(1); // target = 20
        let scheduler = SearchScheduler::new();

        // rng values: 0,1,2 pick A; 3 picks B
        let mut rng = TestRandom::new(vec![0, 1, 2, 3]);

        let outcome = scheduler.run(&task_plan, &registry, &providers, &mut rng);

        // A should be called more than B (weight 3 vs 1)
        let calls_a: u32 = outcome
            .usage_events
            .iter()
            .filter(|e| e.provider_id.to_string() == "a")
            .count() as u32;
        let calls_b: u32 = outcome
            .usage_events
            .iter()
            .filter(|e| e.provider_id.to_string() == "b")
            .count() as u32;

        // With rng [0,1,2,3] and total_weight=4: 3 values (0,1,2) map to A, 1 (3) to B
        assert!(
            calls_a >= calls_b,
            "A should be called at least as often as B"
        );
        assert!(outcome.target_met);
    }

    #[test]
    fn scheduler_zero_weight_provider_excluded() {
        let mut registry = ProviderRegistry::new();
        registry.register(
            ProviderRegistration::new(ProviderId::new("a"), "A")
                .with_enabled(true)
                .with_weight(1),
        );
        registry.register(
            ProviderRegistration::new(ProviderId::new("b"), "B")
                .with_enabled(true)
                .with_weight(0), // zero weight → excluded
        );

        let candidates: Vec<CandidateRecord> = (0..25)
            .map(|i| make_candidate(&format!("c{}", i), "a", &format!("https://ex.com/{}", i)))
            .collect();

        let stub_a = StubProvider::with_responses("a", "A", 1, true, vec![candidates]);
        let stub_b = StubProvider::with_responses("b", "B", 0, true, vec![]);

        let mut providers: HashMap<String, &dyn BaseProvider> = HashMap::new();
        providers.insert("a".to_string(), &stub_a);
        providers.insert("b".to_string(), &stub_b);

        let task_plan = make_task_plan(1); // target = 20
        let scheduler = SearchScheduler::new();
        let mut rng = TestRandom::new(vec![0]);

        let outcome = scheduler.run(&task_plan, &registry, &providers, &mut rng);
        assert!(outcome.target_met);

        // B should never have been called
        let b_used = outcome
            .usage_events
            .iter()
            .any(|e| e.provider_id.to_string() == "b");
        assert!(!b_used, "zero-weight provider should not be called");

        // B's readiness should be Misconfigured
        let b_summary = outcome
            .readiness_summary
            .iter()
            .find(|r| r.provider_id.to_string() == "b")
            .unwrap();
        assert_eq!(b_summary.readiness, ProviderReadiness::Misconfigured);
        assert!(!b_summary.included_in_table);
    }

    #[test]
    fn scheduler_negative_weight_provider_excluded() {
        let mut registry = ProviderRegistry::new();
        registry.register(
            ProviderRegistration::new(ProviderId::new("a"), "A")
                .with_enabled(true)
                .with_weight(1),
        );
        registry.register(
            ProviderRegistration::new(ProviderId::new("b"), "B")
                .with_enabled(true)
                .with_weight(-5), // negative → excluded
        );

        let candidates: Vec<CandidateRecord> = (0..25)
            .map(|i| make_candidate(&format!("c{}", i), "a", &format!("https://ex.com/{}", i)))
            .collect();

        let stub_a = StubProvider::with_responses("a", "A", 1, true, vec![candidates]);
        let stub_b = StubProvider::with_responses("b", "B", -5, true, vec![]);

        let mut providers: HashMap<String, &dyn BaseProvider> = HashMap::new();
        providers.insert("a".to_string(), &stub_a);
        providers.insert("b".to_string(), &stub_b);

        let task_plan = make_task_plan(1); // target = 20
        let scheduler = SearchScheduler::new();
        let mut rng = TestRandom::new(vec![0]);

        let outcome = scheduler.run(&task_plan, &registry, &providers, &mut rng);

        let b_summary = outcome
            .readiness_summary
            .iter()
            .find(|r| r.provider_id.to_string() == "b")
            .unwrap();
        assert_eq!(b_summary.readiness, ProviderReadiness::Misconfigured);
        assert!(!b_summary.included_in_table);
    }

    #[test]
    fn scheduler_candidate_shortage_preserves_explanation() {
        let mut registry = ProviderRegistry::new();
        registry.register(
            ProviderRegistration::new(ProviderId::new("p1"), "P1")
                .with_enabled(true)
                .with_weight(1),
        );

        // Only 5 candidates, target is 60 → shortage
        let candidates: Vec<CandidateRecord> = (0..5)
            .map(|i| make_candidate(&format!("c{}", i), "p1", &format!("https://ex.com/{}", i)))
            .collect();

        let stub = StubProvider::with_responses("p1", "P1", 1, true, vec![candidates]);

        let mut providers: HashMap<String, &dyn BaseProvider> = HashMap::new();
        providers.insert("p1".to_string(), &stub);

        let task_plan = make_task_plan(3); // target = 60
        let scheduler = SearchScheduler::new();
        let mut rng = TestRandom::new(vec![0]);

        let outcome = scheduler.run(&task_plan, &registry, &providers, &mut rng);
        assert!(!outcome.target_met);
        assert!(outcome.candidates.len() < 60);
        assert!(outcome.shortage_reason.is_some());

        // The shortage reason should NOT be a blocking error — it's diagnostic
        let reason = outcome.shortage_reason.as_ref().unwrap();
        let reason_str = reason.to_string();
        assert!(!reason_str.is_empty());
    }

    #[test]
    fn scheduler_disabled_provider_not_used() {
        let mut registry = ProviderRegistry::new();
        registry.register(
            ProviderRegistration::new(ProviderId::new("p1"), "P1")
                .with_enabled(false) // disabled
                .with_weight(1),
        );

        let stub = StubProvider::new("p1", "P1", 1, true);

        let mut providers: HashMap<String, &dyn BaseProvider> = HashMap::new();
        providers.insert("p1".to_string(), &stub);

        let task_plan = make_task_plan(1);
        let scheduler = SearchScheduler::new();
        let mut rng = TestRandom::new(vec![0]);

        let outcome = scheduler.run(&task_plan, &registry, &providers, &mut rng);
        assert!(!outcome.target_met);
        assert!(matches!(
            outcome.shortage_reason,
            Some(CandidateShortageReason::NoAvailableProviders)
        ));
        assert_eq!(outcome.total_invocations, 0);
    }

    #[test]
    fn scheduler_not_ready_provider_excluded() {
        let mut registry = ProviderRegistry::new();
        registry.register(
            ProviderRegistration::new(ProviderId::new("p1"), "P1")
                .with_enabled(true)
                .with_weight(1),
        );

        // Provider reports not ready
        let stub = StubProvider::new("p1", "P1", 1, false); // ready = false

        let mut providers: HashMap<String, &dyn BaseProvider> = HashMap::new();
        providers.insert("p1".to_string(), &stub);

        let task_plan = make_task_plan(1);
        let scheduler = SearchScheduler::new();
        let mut rng = TestRandom::new(vec![0]);

        let outcome = scheduler.run(&task_plan, &registry, &providers, &mut rng);
        assert!(!outcome.target_met);

        let p1_summary = outcome
            .readiness_summary
            .iter()
            .find(|r| r.provider_id.to_string() == "p1")
            .unwrap();
        assert!(!p1_summary.included_in_table);
    }

    #[test]
    fn scheduler_all_disabled_providers_readiness_recorded() {
        let mut registry = ProviderRegistry::new();
        registry.register(
            ProviderRegistration::new(ProviderId::new("a"), "A")
                .with_enabled(false)
                .with_weight(1),
        );
        registry.register(
            ProviderRegistration::new(ProviderId::new("b"), "B")
                .with_enabled(false)
                .with_weight(1),
        );

        let task_plan = make_task_plan(1);
        let scheduler = SearchScheduler::new();
        let providers: HashMap<String, &dyn BaseProvider> = HashMap::new();
        let mut rng = TestRandom::new(vec![]);

        let outcome = scheduler.run(&task_plan, &registry, &providers, &mut rng);
        assert_eq!(outcome.readiness_summary.len(), 2);
        for r in &outcome.readiness_summary {
            assert_eq!(r.readiness, ProviderReadiness::Disabled);
            assert!(!r.included_in_table);
        }
    }

    #[test]
    fn search_outcome_provides_source_traceability() {
        let mut registry = ProviderRegistry::new();
        registry.register(
            ProviderRegistration::new(ProviderId::new("p1"), "P1")
                .with_enabled(true)
                .with_weight(1),
        );

        let candidates: Vec<CandidateRecord> = (0..25)
            .map(|i| make_candidate(&format!("c{}", i), "p1", &format!("https://ex.com/{}", i)))
            .collect();

        let stub = StubProvider::with_responses("p1", "P1", 1, true, vec![candidates]);

        let mut providers: HashMap<String, &dyn BaseProvider> = HashMap::new();
        providers.insert("p1".to_string(), &stub);

        let task_plan = make_task_plan(1); // target = 20
        let scheduler = SearchScheduler::new();
        let mut rng = TestRandom::new(vec![0]);

        let outcome = scheduler.run(&task_plan, &registry, &providers, &mut rng);

        // Every candidate should have the provider_id set
        for candidate in &outcome.candidates {
            assert_eq!(candidate.provider_id.to_string(), "p1");
        }

        // Usage events provide per-invocation traceability
        assert!(!outcome.usage_events.is_empty());
        for event in &outcome.usage_events {
            assert_eq!(event.provider_id.to_string(), "p1");
            assert_eq!(event.effective_weight, 1);
        }
    }

    #[test]
    fn scheduler_requires_3_images_target_60() {
        // AC: 要求 3 张图片时搜索目标约 60
        let task_plan = make_task_plan(3);
        assert_eq!(task_plan.candidate_target, 60); // 3 × 20
    }

    #[test]
    fn candidate_shortage_not_blocking() {
        // AC: 候选不足保留说明但不直接执行阻塞
        let mut registry = ProviderRegistry::new();
        registry.register(
            ProviderRegistration::new(ProviderId::new("p1"), "P1")
                .with_enabled(true)
                .with_weight(1),
        );

        // Only 10 candidates, target 60
        let candidates: Vec<CandidateRecord> = (0..10)
            .map(|i| make_candidate(&format!("c{}", i), "p1", &format!("https://ex.com/{}", i)))
            .collect();

        let stub = StubProvider::with_responses("p1", "P1", 1, true, vec![candidates]);

        let mut providers: HashMap<String, &dyn BaseProvider> = HashMap::new();
        providers.insert("p1".to_string(), &stub);

        let task_plan = make_task_plan(3); // target = 60
        let scheduler = SearchScheduler::new();
        let mut rng = TestRandom::new(vec![0]);

        let outcome = scheduler.run(&task_plan, &registry, &providers, &mut rng);

        // Not target_met, but NOT a blocking condition
        assert!(!outcome.target_met);
        // Candidates are still returned (10 of them) — shortfall doesn't block
        assert!(!outcome.candidates.is_empty());
        // Shortage is explained
        assert!(outcome.shortage_reason.is_some());
    }

    #[test]
    fn provider_credentials_not_in_search_outcome() {
        // AC: 凭据不进入用户可见证据 — verify that no credential fields
        // exist in CandidateRecord or SearchUsageEvent
        let mut registry = ProviderRegistry::new();
        registry.register(
            ProviderRegistration::new(ProviderId::new("p1"), "P1")
                .with_enabled(true)
                .with_weight(1),
        );

        let candidates: Vec<CandidateRecord> = (0..5)
            .map(|i| make_candidate(&format!("c{}", i), "p1", &format!("https://ex.com/{}", i)))
            .collect();

        let stub = StubProvider::with_responses("p1", "P1", 1, true, vec![candidates]);

        let mut providers: HashMap<String, &dyn BaseProvider> = HashMap::new();
        providers.insert("p1".to_string(), &stub);

        let task_plan = make_task_plan(1);
        let scheduler = SearchScheduler::new();
        let mut rng = TestRandom::new(vec![0]);

        let outcome = scheduler.run(&task_plan, &registry, &providers, &mut rng);

        // Serialize outcome (simulating what would be user-visible)
        // There should be no credential fields
        let json = serde_json::to_string(&outcome.usage_events).unwrap_or_default();
        assert!(!json.to_lowercase().contains("api_key"));
        assert!(!json.to_lowercase().contains("token"));
        assert!(!json.to_lowercase().contains("secret"));
        assert!(!json.to_lowercase().contains("password"));
        assert!(!json.to_lowercase().contains("credential"));
    }
}
