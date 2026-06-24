#![allow(unused_imports, unused_variables, clippy::useless_vec)]
//! Weighted random search scheduler.
//!
//! Implements the weighted random selection algorithm using the
//! pluggable [`RandomSource`] abstraction. Consumes a
//! [`NormalizedQueryPlan`], [`ProviderRegistry`], and
//! [`BaseSearchProvider`] adapters to produce a
//! [`SearchSessionOutcome`].
//!
//! v1.1: rewritten for `BaseSearchProvider`, structured readiness,
//! search rounds, dedupe evidence, and package-safe output.
//!
//! References: PRD FR-005, LLD §Search Provider Contract,
//! `docs/design/v1.1-TASK-002-search-provider-candidate-design.md`

use crate::domain::candidate::{
    CandidateDedupeEvidence, CandidateId, CandidateRecord, DedupeMergeReason, ProviderId,
};
use crate::domain::query_plan::NormalizedQueryPlan;
use crate::domain::search::{
    CandidateShortageReason, ProviderFailureCode, SearchDiagnostic, SearchDiagnosticCode,
    SearchRequest, SearchResponseStatus, SearchSessionOutcome, SearchUsageEvent,
    WeightedProviderEntry,
};
use crate::ports::BaseSearchProvider;
use crate::search::registry::ProviderRegistry;
use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

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
        let val: u32 = fast_random_u32();
        val % max
    }
}

/// A fast, non-cryptographic random generator using xorshift64.
fn fast_random_u32() -> u32 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let mut x = seed.wrapping_add(1);
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    x as u32
}

// ---------------------------------------------------------------------------
// Search scheduler
// ---------------------------------------------------------------------------

/// Default maximum invocations per search session (safety cap).
const DEFAULT_MAX_INVOCATIONS: u32 = 50;

/// The weighted random search scheduler.
pub struct SearchScheduler {
    max_invocations: u32,
}

impl Default for SearchScheduler {
    fn default() -> Self {
        Self {
            max_invocations: DEFAULT_MAX_INVOCATIONS,
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
    /// Consumes the normalized query plan (which contains the candidate target)
    /// and the provider registry (which contains readiness and adapters).
    /// Produces a [`SearchSessionOutcome`] suitable for handoff to TASK-003.
    pub fn run(
        &self,
        query_plan: &NormalizedQueryPlan,
        registry: &ProviderRegistry,
        rng: &mut dyn RandomSource,
    ) -> SearchSessionOutcome {
        let candidate_target = query_plan.candidate_target;
        let query_plan_id = query_plan.query_plan_id.clone();
        let full_attempt_count = 1u8; // default for first attempt
        let retry_count = 0u8;

        // Step 1: Evaluate provider readiness
        let readiness_reports = registry.evaluate_readiness();

        // Step 2: Build effective weight table
        let weight_table = ProviderRegistry::build_weight_table(&readiness_reports);

        // Check for no providers
        if weight_table.is_empty() {
            return SearchSessionOutcome {
                query_plan_id,
                full_attempt_count,
                retry_count,
                candidate_target,
                unique_candidate_count: 0,
                target_met: false,
                candidates: Vec::new(),
                readiness_reports,
                usage_events: Vec::new(),
                dedupe_events: Vec::new(),
                diagnostics: vec![SearchDiagnostic::blocker(
                    SearchDiagnosticCode::NoAvailableSearchProvider,
                    "No ready provider is available for scheduling.",
                )],
                shortage_reason: Some(CandidateShortageReason::NoAvailableSearchProvider),
            };
        }

        // Step 3: Scheduling loop
        let all_candidates: Vec<CandidateRecord> = Vec::new();
        let mut usage_events: Vec<SearchUsageEvent> = Vec::new();
        let mut dedupe_events: Vec<CandidateDedupeEvidence> = Vec::new();
        let mut diagnostics: Vec<SearchDiagnostic> = Vec::new();

        // Dedupe index: dedupe_key → canonical candidate_id
        let mut dedupe_index: BTreeMap<String, CandidateId> = BTreeMap::new();
        // Canonical candidate store: candidate_id → (record, all origin IDs)
        let mut dedupe_store: BTreeMap<CandidateId, (CandidateRecord, Vec<CandidateId>)> =
            BTreeMap::new();

        let mut exhausted_providers: BTreeSet<ProviderId> = BTreeSet::new();
        let mut terminal_failed_providers: BTreeSet<ProviderId> = BTreeSet::new();
        let mut search_round: u32 = 0;
        let mut invocations: u32 = 0;
        let mut total_unique: u32 = 0;

        // Pick a query text (first one, or rotate)
        let query_text = query_plan
            .query_texts
            .first()
            .cloned()
            .unwrap_or_else(|| query_plan.description.clone());

        while total_unique < candidate_target
            && exhausted_providers.len() + terminal_failed_providers.len() < weight_table.len()
            && invocations < self.max_invocations
        {
            search_round += 1;

            // Filter to non-exhausted, non-failed providers
            let active_entries: Vec<&WeightedProviderEntry> = weight_table
                .iter()
                .filter(|e| {
                    !exhausted_providers.contains(&e.provider_id)
                        && !terminal_failed_providers.contains(&e.provider_id)
                })
                .collect();

            if active_entries.is_empty() {
                break;
            }

            let active_total_weight: u32 = active_entries.iter().map(|e| e.effective_weight).sum();
            if active_total_weight == 0 {
                break;
            }

            // Weighted random selection
            let selected = select_weighted(&active_entries, active_total_weight, rng);
            let provider_id = selected.provider_id.clone();

            invocations += 1;

            // Look up adapter
            let adapter = match registry.get_adapter(&provider_id) {
                Some(a) => a,
                None => {
                    terminal_failed_providers.insert(provider_id.clone());
                    usage_events.push(SearchUsageEvent::failure_event(
                        query_plan_id.clone(),
                        provider_id.clone(),
                        format!("sr-{}-{}", query_plan_id, provider_id),
                        full_attempt_count,
                        retry_count,
                        search_round,
                        selected.effective_weight,
                        active_total_weight,
                        ProviderFailureCode::ProviderAdapterMissing,
                        0,
                    ));
                    diagnostics.push(
                        SearchDiagnostic::error(
                            SearchDiagnosticCode::ProviderAdapterMissing,
                            format!(
                                "Adapter for provider '{}' not found during scheduling",
                                provider_id
                            ),
                        )
                        .with_provider(provider_id.clone()),
                    );
                    continue;
                }
            };

            // Determine request count
            let remaining_needed = candidate_target.saturating_sub(total_unique);
            let request_count = clamp_max_results(
                remaining_needed,
                selected.capabilities.max_results_per_request,
            );

            // Build search request
            let search_req = SearchRequest::new(
                query_plan_id.clone(),
                provider_id.clone(),
                &query_text,
                request_count,
                search_round,
                full_attempt_count,
            );

            // Call provider
            let t0 = Instant::now();
            let search_result = adapter.search(&search_req);
            let duration_ms = t0.elapsed().as_millis() as u64;

            match search_result {
                Ok(response) => {
                    let raw_count = response.raw_result_count;
                    let normalized_count = response.candidates.len() as u32;

                    // Process candidates through dedupe
                    let mut unique_this_round: u32 = 0;
                    let mut duplicate_this_round: u32 = 0;

                    for candidate in &response.candidates {
                        let dedupe_key = candidate.dedupe_key.clone();
                        let image_url_key = Some(candidate.image_url.clone());

                        if let Some(existing_id) = dedupe_index.get(&dedupe_key) {
                            // Duplicate found — merge
                            duplicate_this_round += 1;
                            if let Some((canonical, origin_ids)) = dedupe_store.get_mut(existing_id)
                            {
                                // Merge origin IDs
                                for oid in &candidate.origin_candidate_ids {
                                    if !origin_ids.contains(oid) {
                                        origin_ids.push(oid.clone());
                                    }
                                }
                                canonical.origin_candidate_ids = origin_ids.clone();
                            }

                            dedupe_events.push(CandidateDedupeEvidence::duplicate(
                                dedupe_key,
                                existing_id.clone(),
                                DedupeMergeReason::ExactImageUrl,
                            ));

                            diagnostics.push(
                                SearchDiagnostic::info(
                                    SearchDiagnosticCode::CandidateDuplicateMerged,
                                    format!(
                                        "Candidate '{}' merged into '{}' (duplicate image URL)",
                                        candidate.candidate_id, existing_id
                                    ),
                                )
                                .with_provider(provider_id.clone())
                                .with_candidate(candidate.candidate_id.clone()),
                            );
                        } else {
                            // New candidate
                            unique_this_round += 1;
                            dedupe_index.insert(dedupe_key.clone(), candidate.candidate_id.clone());

                            let record = candidate.clone();
                            let origin_ids = record.origin_candidate_ids.clone();

                            dedupe_store.insert(record.candidate_id.clone(), (record, origin_ids));

                            dedupe_events
                                .push(CandidateDedupeEvidence::unique(dedupe_key, image_url_key));
                        }
                    }

                    total_unique += unique_this_round;

                    let is_exhausted = response.exhausted
                        || (raw_count == 0)
                        || response.status == SearchResponseStatus::Empty;

                    if is_exhausted {
                        exhausted_providers.insert(provider_id.clone());
                    }

                    usage_events.push(SearchUsageEvent::success_event(
                        query_plan_id.clone(),
                        provider_id.clone(),
                        search_req.search_request_id,
                        full_attempt_count,
                        retry_count,
                        search_round,
                        selected.effective_weight,
                        active_total_weight,
                        raw_count,
                        normalized_count,
                        unique_this_round,
                        duplicate_this_round,
                        is_exhausted,
                        duration_ms,
                    ));

                    // Collect provider diagnostics
                    diagnostics.extend(response.diagnostics);
                }
                Err(search_error) => {
                    let failure_code = search_error.to_failure_code();

                    // Transient errors: don't permanently remove
                    let is_transient = matches!(
                        search_error,
                        crate::domain::search::SearchError::Timeout { .. }
                            | crate::domain::search::SearchError::RateLimited { .. }
                    );

                    if !is_transient {
                        terminal_failed_providers.insert(provider_id.clone());
                    } else {
                        // For transient errors, mark exhausted for this round
                        // but don't terminal-fail
                    }

                    let diagnostic_code = match &search_error {
                        crate::domain::search::SearchError::Timeout { .. } => {
                            SearchDiagnosticCode::SearchProviderTimeout
                        }
                        crate::domain::search::SearchError::RateLimited { .. } => {
                            SearchDiagnosticCode::SearchProviderRateLimited
                        }
                        _ => SearchDiagnosticCode::ProviderUnavailable,
                    };

                    diagnostics.push(
                        SearchDiagnostic::error(diagnostic_code, search_error.to_string())
                            .with_provider(provider_id.clone())
                            .with_request(&search_req.search_request_id),
                    );

                    usage_events.push(SearchUsageEvent::failure_event(
                        query_plan_id.clone(),
                        provider_id.clone(),
                        search_req.search_request_id,
                        full_attempt_count,
                        retry_count,
                        search_round,
                        selected.effective_weight,
                        active_total_weight,
                        failure_code,
                        duration_ms,
                    ));
                }
            }

            // Collect all unique candidates into the output list
            if total_unique >= candidate_target {
                break;
            }
        }

        // Build final candidate list from dedupe_store (preserving canonical order)
        let all_candidates: Vec<CandidateRecord> = dedupe_store
            .into_values()
            .map(|(record, _)| record)
            .collect();

        let unique_candidate_count = all_candidates.len() as u32;
        let target_met = unique_candidate_count >= candidate_target;

        let shortage_reason = if target_met {
            None
        } else if invocations >= self.max_invocations {
            diagnostics.push(SearchDiagnostic::warning(
                SearchDiagnosticCode::SearchInvocationLimitReached,
                format!(
                    "Reached invocation limit of {} before meeting candidate target.",
                    self.max_invocations
                ),
            ));
            Some(CandidateShortageReason::SearchInvocationLimitReached)
        } else if exhausted_providers.len() + terminal_failed_providers.len() >= weight_table.len()
        {
            if !terminal_failed_providers.is_empty() {
                diagnostics.push(SearchDiagnostic::warning(
                    SearchDiagnosticCode::SearchTargetShortage,
                    "Not all providers were fully available; target not met.",
                ));
                Some(CandidateShortageReason::ProviderPartialFailure {
                    failed_providers: terminal_failed_providers.iter().cloned().collect(),
                    exhausted_providers: exhausted_providers.iter().cloned().collect(),
                })
            } else {
                diagnostics.push(SearchDiagnostic::warning(
                    SearchDiagnosticCode::SearchTargetShortage,
                    "All providers exhausted before reaching candidate target.",
                ));
                Some(CandidateShortageReason::AllProvidersExhausted)
            }
        } else {
            let total_raw: u32 = usage_events.iter().map(|e| e.raw_candidate_count).sum();
            let duplicates_removed = total_raw.saturating_sub(unique_candidate_count);
            diagnostics.push(SearchDiagnostic::warning(
                SearchDiagnosticCode::SearchTargetShortage,
                format!(
                    "Insufficient unique candidates: {} raw, {} after dedup ({} duplicates).",
                    total_raw, unique_candidate_count, duplicates_removed
                ),
            ));
            Some(CandidateShortageReason::InsufficientUniqueCandidates {
                total_raw,
                total_deduped: unique_candidate_count,
                duplicates_removed,
            })
        };

        SearchSessionOutcome {
            query_plan_id,
            full_attempt_count,
            retry_count,
            candidate_target,
            unique_candidate_count,
            target_met,
            candidates: all_candidates,
            readiness_reports,
            usage_events,
            dedupe_events,
            diagnostics,
            shortage_reason,
        }
    }
}

// ---------------------------------------------------------------------------
// Weighted random selection
// ---------------------------------------------------------------------------

/// Select a provider from the weight table using weighted random selection.
fn select_weighted<'a>(
    entries: &[&'a WeightedProviderEntry],
    total_weight: u32,
    rng: &mut dyn RandomSource,
) -> &'a WeightedProviderEntry {
    if entries.is_empty() || total_weight == 0 {
        panic!("select_weighted called with empty entries or zero total weight");
    }

    let mut target = rng.gen_range(total_weight);
    for entry in entries {
        if target < entry.effective_weight {
            return entry;
        }
        target -= entry.effective_weight;
    }

    // Fallback to last entry
    entries.last().expect("non-empty entries")
}

/// Clamp max_results to provider's declared limit.
fn clamp_max_results(requested: u32, provider_max: Option<u32>) -> u32 {
    match provider_max {
        Some(max) => requested.min(max),
        None => requested,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::candidate::CandidateProvenance;
    use crate::domain::config::SearchProviderKind;
    use crate::domain::query_plan::QueryPlanId;
    use crate::domain::search::{
        ProviderConstraintSupport, ProviderReadinessReport, ProviderReadinessStatus,
        SearchResponse, SearchResponseStatus,
    };
    use crate::ports::BaseSearchProvider;
    use std::cell::RefCell;
    use std::collections::HashSet;
    use std::sync::{Arc, Mutex};

    // -----------------------------------------------------------------------
    // Deterministic random source for testing
    // -----------------------------------------------------------------------

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
    // Stub provider implementing BaseSearchProvider for tests
    // -----------------------------------------------------------------------

    struct StubSearchProvider {
        id: ProviderId,
        name: String,
        kind: SearchProviderKind,
        ready: bool,
        credential_present: bool,
        /// Pre-configured response batches.
        responses: Mutex<Vec<Vec<CandidateRecord>>>,
        call_count: Mutex<usize>,
        request_max_results: Mutex<Vec<u32>>,
    }

    impl StubSearchProvider {
        fn new(id: &str, name: &str, ready: bool, credential_present: bool) -> Self {
            Self {
                id: ProviderId::new(id),
                name: name.into(),
                kind: SearchProviderKind::Custom("stub".into()),
                ready,
                credential_present,
                responses: Mutex::new(Vec::new()),
                call_count: Mutex::new(0),
                request_max_results: Mutex::new(Vec::new()),
            }
        }

        fn with_responses(mut self, responses: Vec<Vec<CandidateRecord>>) -> Self {
            self.responses = Mutex::new(responses);
            self
        }
    }

    impl BaseSearchProvider for StubSearchProvider {
        fn provider_id(&self) -> ProviderId {
            self.id.clone()
        }

        fn display_name(&self) -> &str {
            &self.name
        }

        fn provider_kind(&self) -> SearchProviderKind {
            self.kind.clone()
        }

        fn supported_constraints(&self) -> ProviderConstraintSupport {
            ProviderConstraintSupport::default()
        }

        fn readiness(
            &self,
            _config: &crate::domain::config::SearchProviderConfig,
        ) -> ProviderReadinessReport {
            if self.ready && self.credential_present {
                ProviderReadinessReport::ready(self.id.clone(), self.kind.clone(), &self.name)
            } else if !self.credential_present {
                ProviderReadinessReport::not_ready(
                    self.id.clone(),
                    self.kind.clone(),
                    &self.name,
                    ProviderReadinessStatus::MissingCredentials,
                    ProviderFailureCode::ProviderCredentialMissing,
                    vec![],
                )
            } else {
                ProviderReadinessReport::not_ready(
                    self.id.clone(),
                    self.kind.clone(),
                    &self.name,
                    ProviderReadinessStatus::Unavailable,
                    ProviderFailureCode::ProviderUnavailable,
                    vec![],
                )
            }
        }

        fn search(
            &self,
            request: &SearchRequest,
        ) -> std::result::Result<SearchResponse, crate::domain::search::SearchError> {
            self.request_max_results
                .lock()
                .unwrap()
                .push(request.max_results);
            let mut count = self.call_count.lock().unwrap();
            let idx = *count;
            *count += 1;
            drop(count);

            let mut responses = self.responses.lock().unwrap();
            let candidates = if idx < responses.len() {
                std::mem::take(&mut responses[idx])
            } else {
                Vec::new()
            };

            let raw_count = candidates.len() as u32;
            let exhausted = candidates.is_empty();
            let status = if candidates.is_empty() {
                SearchResponseStatus::Empty
            } else {
                SearchResponseStatus::Complete
            };

            Ok(SearchResponse {
                search_request_id: request.search_request_id.clone(),
                provider_id: self.id.clone(),
                provider_kind: self.kind.clone(),
                query_plan_id: request.query_plan_id.clone(),
                search_round: request.search_round,
                status,
                candidates,
                raw_result_count: raw_count,
                normalized_count: raw_count,
                provider_next_page_token_present: false,
                exhausted,
                diagnostics: Vec::new(),
                redaction_applied: false,
            })
        }
    }

    // -----------------------------------------------------------------------
    // Helper functions
    // -----------------------------------------------------------------------

    fn make_candidate(
        id: &str,
        provider: &str,
        image_url: &str,
        query_plan_id: &str,
        search_round: u32,
        rank: u32,
    ) -> CandidateRecord {
        let candidate_id = CandidateId::new(id);
        CandidateRecord {
            candidate_id: candidate_id.clone(),
            query_plan_id: query_plan_id.into(),
            provider_id: ProviderId::new(provider),
            provider_kind: "stub".into(),
            search_request_id: format!("sr-{}-{}", query_plan_id, provider),
            search_round,
            provider_rank: rank,
            global_rank_hint: None,
            image_url: image_url.into(),
            source_page_url: None,
            thumbnail_url: None,
            title: Some(format!("Image {}", id)),
            snippet: None,
            width: None,
            height: None,
            mime_type: None,
            license_hint: None,
            attribution: None,
            dedupe_key: CandidateRecord::build_dedupe_key(image_url),
            origin_candidate_ids: vec![candidate_id],
            provenance: CandidateProvenance::new(rank, "test query", search_round, 1),
            normalization_warnings: Vec::new(),
        }
    }

    fn make_query_plan(required_image_count: u32) -> NormalizedQueryPlan {
        NormalizedQueryPlan {
            query_plan_id: QueryPlanId::new("qp-test"),
            description: "test query".into(),
            query_texts: vec!["test query".into()],
            required_image_count,
            quality: crate::domain::query_plan::QualityTier::General,
            quality_requirements: Default::default(),
            material_types: Vec::new(),
            visual_requirements: Vec::new(),
            negative_scope: Vec::new(),
            source_diversity_requirement: None,
            candidate_target: required_image_count * 20,
            retrieval_batch_target: required_image_count * 2,
            retry_limit: 3,
            full_attempt_limit: 4,
            provider_policy: Default::default(),
            retrieval_policy: Default::default(),
            admission_diagnostics: Vec::new(),
        }
    }

    // -----------------------------------------------------------------------
    // Weighted selection tests
    // -----------------------------------------------------------------------

    #[test]
    fn weighted_selection_picks_first_when_rng_zero() {
        let entries = vec![
            WeightedProviderEntry {
                provider_id: ProviderId::new("a"),
                display_name: "A".into(),
                effective_weight: 2,
                capabilities: ProviderConstraintSupport::default(),
            },
            WeightedProviderEntry {
                provider_id: ProviderId::new("b"),
                display_name: "B".into(),
                effective_weight: 3,
                capabilities: ProviderConstraintSupport::default(),
            },
        ];
        let entry_refs: Vec<&WeightedProviderEntry> = entries.iter().collect();
        let mut rng = TestRandom::new(vec![0]);
        let selected = select_weighted(&entry_refs, 5, &mut rng);
        assert_eq!(selected.provider_id.to_string(), "a");
    }

    #[test]
    fn weighted_selection_picks_second_when_rng_exceeds_first() {
        let entries = vec![
            WeightedProviderEntry {
                provider_id: ProviderId::new("a"),
                display_name: "A".into(),
                effective_weight: 2,
                capabilities: ProviderConstraintSupport::default(),
            },
            WeightedProviderEntry {
                provider_id: ProviderId::new("b"),
                display_name: "B".into(),
                effective_weight: 3,
                capabilities: ProviderConstraintSupport::default(),
            },
        ];
        let entry_refs: Vec<&WeightedProviderEntry> = entries.iter().collect();
        let mut rng = TestRandom::new(vec![2]); // falls through to b
        let selected = select_weighted(&entry_refs, 5, &mut rng);
        assert_eq!(selected.provider_id.to_string(), "b");
    }

    // -----------------------------------------------------------------------
    // Scheduler tests
    // -----------------------------------------------------------------------

    #[test]
    fn scheduler_no_providers_returns_shortage() {
        let registry = ProviderRegistry::new();
        let query_plan = make_query_plan(3);
        let scheduler = SearchScheduler::new();
        let mut rng = TestRandom::new(vec![]);

        let outcome = scheduler.run(&query_plan, &registry, &mut rng);
        assert!(!outcome.target_met);
        assert!(outcome.candidates.is_empty());
        assert!(matches!(
            outcome.shortage_reason,
            Some(CandidateShortageReason::NoAvailableSearchProvider)
        ));
    }

    #[test]
    fn scheduler_single_provider_reaches_target() {
        let mut registry = ProviderRegistry::new();
        let provider = Arc::new(
            StubSearchProvider::new("p1", "P1", true, true).with_responses(vec![(0..70)
                .map(|i| {
                    make_candidate(
                        &format!("c{}", i),
                        "p1",
                        &format!("https://ex.com/{}", i),
                        "qp-test",
                        1,
                        i + 1,
                    )
                })
                .collect()]),
        );
        registry.register_adapter(ProviderId::new("p1"), provider);

        let query_plan = make_query_plan(3); // target = 60
        let scheduler = SearchScheduler::new();
        let mut rng = TestRandom::new(vec![0]);

        let outcome = scheduler.run(&query_plan, &registry, &mut rng);
        assert!(outcome.target_met);
        assert!(outcome.candidates.len() as u32 >= 60);
        assert_eq!(outcome.usage_events.len(), 1);
        assert!(outcome.shortage_reason.is_none());
    }

    #[test]
    fn scheduler_requests_remaining_candidate_target_without_overfetch() {
        let mut registry = ProviderRegistry::new();
        let provider = Arc::new(
            StubSearchProvider::new("p1", "P1", true, true).with_responses(vec![(0..20)
                .map(|i| {
                    make_candidate(
                        &format!("c{}", i),
                        "p1",
                        &format!("https://ex.com/{}", i),
                        "qp-test",
                        1,
                        i + 1,
                    )
                })
                .collect()]),
        );
        registry.register_adapter(ProviderId::new("p1"), provider.clone());

        let query_plan = make_query_plan(1); // target = 20
        let scheduler = SearchScheduler::new();
        let mut rng = TestRandom::new(vec![0]);

        let outcome = scheduler.run(&query_plan, &registry, &mut rng);
        assert!(outcome.target_met);
        assert_eq!(
            provider.request_max_results.lock().unwrap().as_slice(),
            &[20]
        );
    }

    #[test]
    fn scheduler_deduplicates_by_image_url() {
        let mut registry = ProviderRegistry::new();
        let batch: Vec<CandidateRecord> = vec![
            make_candidate("a1", "p1", "https://ex.com/1", "qp-test", 1, 1),
            make_candidate("a2", "p1", "https://ex.com/2", "qp-test", 1, 2),
            make_candidate("a3", "p1", "https://ex.com/1", "qp-test", 1, 3), // duplicate
            make_candidate("a4", "p1", "https://ex.com/3", "qp-test", 1, 4),
            make_candidate("a5", "p1", "https://ex.com/2", "qp-test", 1, 5), // duplicate
            make_candidate("a6", "p1", "https://ex.com/4", "qp-test", 1, 6),
        ];

        let provider =
            Arc::new(StubSearchProvider::new("p1", "P1", true, true).with_responses(vec![batch]));
        registry.register_adapter(ProviderId::new("p1"), provider);

        let query_plan = make_query_plan(1); // target = 20
        let scheduler = SearchScheduler::new();
        let mut rng = TestRandom::new(vec![0]);

        let outcome = scheduler.run(&query_plan, &registry, &mut rng);
        // 6 raw, 4 unique (1→1,2→2,3→dup1,4→3,5→dup2,6→4)
        assert_eq!(outcome.candidates.len(), 4);
        assert!(!outcome.dedupe_events.is_empty());

        // Count duplicates
        let dup_events: Vec<_> = outcome
            .dedupe_events
            .iter()
            .filter(|e| e.duplicate_of.is_some())
            .collect();
        assert_eq!(dup_events.len(), 2);
    }

    #[test]
    fn scheduler_multi_provider() {
        let mut registry = ProviderRegistry::new();

        let provider_a = Arc::new(
            StubSearchProvider::new("a", "A", true, true).with_responses(vec![(0..35)
                .map(|i| {
                    make_candidate(
                        &format!("ca{}", i),
                        "a",
                        &format!("https://a.ex/{}", i),
                        "qp-test",
                        1,
                        i + 1,
                    )
                })
                .collect()]),
        );
        let provider_b = Arc::new(
            StubSearchProvider::new("b", "B", true, true).with_responses(vec![(0..35)
                .map(|i| {
                    make_candidate(
                        &format!("cb{}", i),
                        "b",
                        &format!("https://b.ex/{}", i),
                        "qp-test",
                        2,
                        i + 1,
                    )
                })
                .collect()]),
        );

        registry.register_adapter(ProviderId::new("a"), provider_a);
        registry.register_adapter(ProviderId::new("b"), provider_b);

        let query_plan = make_query_plan(3); // target = 60
        let scheduler = SearchScheduler::new();
        // rng: 0 picks a, then 1 picks b (total_weight=2)
        let mut rng = TestRandom::new(vec![0, 1]);

        let outcome = scheduler.run(&query_plan, &registry, &mut rng);
        assert!(outcome.target_met);
        assert_eq!(outcome.usage_events.len(), 2);

        let used_ids: HashSet<String> = outcome
            .usage_events
            .iter()
            .map(|e| e.provider_id.to_string())
            .collect();
        assert!(used_ids.contains("a"));
        assert!(used_ids.contains("b"));
    }

    #[test]
    fn scheduler_not_ready_provider_excluded() {
        let mut registry = ProviderRegistry::new();

        // Provider is ready but credential is missing
        let provider = Arc::new(
            StubSearchProvider::new("p1", "P1", true, false) // credential_present = false
                .with_responses(vec![vec![]]),
        );
        registry.register_adapter(ProviderId::new("p1"), provider);

        let query_plan = make_query_plan(1);
        let scheduler = SearchScheduler::new();
        let mut rng = TestRandom::new(vec![0]);

        let outcome = scheduler.run(&query_plan, &registry, &mut rng);
        assert!(!outcome.target_met);

        // Check readiness report shows credential missing
        let p1_report = outcome
            .readiness_reports
            .iter()
            .find(|r| r.provider_id.to_string() == "p1")
            .unwrap();
        assert_eq!(
            p1_report.status,
            ProviderReadinessStatus::MissingCredentials
        );
    }

    #[test]
    fn scheduler_candidate_shortage_not_blocking() {
        let mut registry = ProviderRegistry::new();
        let provider = Arc::new(
            StubSearchProvider::new("p1", "P1", true, true).with_responses(vec![(0..10)
                .map(|i| {
                    make_candidate(
                        &format!("c{}", i),
                        "p1",
                        &format!("https://ex.com/{}", i),
                        "qp-test",
                        1,
                        i + 1,
                    )
                })
                .collect()]),
        );
        registry.register_adapter(ProviderId::new("p1"), provider);

        let query_plan = make_query_plan(3); // target = 60, only 10 results
        let scheduler = SearchScheduler::new();
        let mut rng = TestRandom::new(vec![0]);

        let outcome = scheduler.run(&query_plan, &registry, &mut rng);
        assert!(!outcome.target_met);
        assert!(!outcome.candidates.is_empty());
        assert_eq!(outcome.candidates.len(), 10);
        assert!(outcome.shortage_reason.is_some());
    }

    #[test]
    fn scheduler_preserves_search_round_and_rank() {
        let mut registry = ProviderRegistry::new();
        let provider = Arc::new(
            StubSearchProvider::new("p1", "P1", true, true).with_responses(vec![(0..5)
                .map(|i| {
                    make_candidate(
                        &format!("c{}", i),
                        "p1",
                        &format!("https://ex.com/{}", i),
                        "qp-test",
                        1,
                        (i + 1) as u32,
                    )
                })
                .collect()]),
        );
        registry.register_adapter(ProviderId::new("p1"), provider);

        let query_plan = make_query_plan(1); // target = 20, 5 results
        let scheduler = SearchScheduler::new();
        let mut rng = TestRandom::new(vec![0]);

        let outcome = scheduler.run(&query_plan, &registry, &mut rng);

        for candidate in &outcome.candidates {
            assert_eq!(candidate.search_round, 1);
            assert!(candidate.provider_rank >= 1);
            assert_eq!(candidate.provider_id.to_string(), "p1");
        }
    }

    #[test]
    fn scheduler_no_credentials_leak() {
        let mut registry = ProviderRegistry::new();
        let provider = Arc::new(
            StubSearchProvider::new("p1", "P1", true, true).with_responses(vec![(0..5)
                .map(|i| {
                    make_candidate(
                        &format!("c{}", i),
                        "p1",
                        &format!("https://ex.com/{}", i),
                        "qp-test",
                        1,
                        (i + 1) as u32,
                    )
                })
                .collect()]),
        );
        registry.register_adapter(ProviderId::new("p1"), provider);

        let query_plan = make_query_plan(1);
        let scheduler = SearchScheduler::new();
        let mut rng = TestRandom::new(vec![0]);

        let outcome = scheduler.run(&query_plan, &registry, &mut rng);

        // Serialize candidates and events
        let candidates_json = serde_json::to_string(&outcome.candidates).unwrap_or_default();
        let events_json = serde_json::to_string(&outcome.usage_events).unwrap_or_default();

        for json in &[candidates_json, events_json] {
            let lower = json.to_lowercase();
            assert!(!lower.contains("api_key"));
            assert!(!lower.contains("token"));
            assert!(!lower.contains("secret"));
            assert!(!lower.contains("password"));
            assert!(!lower.contains("credential"));
        }
    }

    #[test]
    fn scheduler_requires_3_images_target_60() {
        let qp = make_query_plan(3);
        assert_eq!(qp.candidate_target, 60);
    }

    #[test]
    fn scheduler_weighted_selection_respects_proportions() {
        // With deterministic RNG, higher weight gets selected more often
        let entries = vec![
            WeightedProviderEntry {
                provider_id: ProviderId::new("a"),
                display_name: "A".into(),
                effective_weight: 1,
                capabilities: ProviderConstraintSupport::default(),
            },
            WeightedProviderEntry {
                provider_id: ProviderId::new("b"),
                display_name: "B".into(),
                effective_weight: 3,
                capabilities: ProviderConstraintSupport::default(),
            },
        ];
        let entry_refs: Vec<&WeightedProviderEntry> = entries.iter().collect();
        let total_weight = 4;

        let samples: Vec<u32> = (0..400).map(|i| i % total_weight).collect();
        let mut rng = TestRandom::new(samples);

        let mut count_a = 0;
        let mut count_b = 0;
        for _ in 0..400 {
            let selected = select_weighted(&entry_refs, total_weight, &mut rng);
            match selected.provider_id.to_string().as_str() {
                "a" => count_a += 1,
                "b" => count_b += 1,
                _ => {}
            }
        }

        // Weight 1:3 → roughly 100:300
        assert!(count_a >= 70, "a got {}, expected ~100", count_a);
        assert!(count_b >= 270, "b got {}, expected ~300", count_b);
    }
}
