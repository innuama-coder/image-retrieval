//! Search integration tests.
//!
//! End-to-end scenarios covering provider registration, weighted scheduling,
//! candidate deduplication, source tracking, SerpApi fixture normalization,
//! provider readiness, credential safety, and candidate shortage handling.
//!
//! Uses fixture providers to simulate multiple search services and
//! SerpApi JSON fixtures for normalization tests.

use image_retrieval::domain::candidate::{CandidateId, CandidateRecord, ProviderId};
use image_retrieval::domain::config::SearchProviderKind;
use image_retrieval::domain::query_plan::{NormalizedQueryPlan, QueryPlanId};
use image_retrieval::domain::search::{
    CandidateShortageReason, ProviderFailureCode, ProviderReadinessReport, ProviderReadinessStatus,
    SearchDiagnosticCode, SearchRequest, SearchResponse, SearchResponseStatus,
};
use image_retrieval::ports::BaseSearchProvider;
use image_retrieval::search::fixture::{
    make_fixture_candidate, ready_fixture_with_batches, ready_fixture_with_candidates,
    FixtureSearchProvider,
};
use image_retrieval::search::registry::ProviderRegistry;
use image_retrieval::search::scheduler::{RandomSource, SearchScheduler};
use image_retrieval::search::SerpApiGoogleImagesAdapter;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Deterministic random source
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_query_plan(required_image_count: u32) -> NormalizedQueryPlan {
    NormalizedQueryPlan {
        query_plan_id: QueryPlanId::new("qp-test"),
        description: "test query".into(),
        query_texts: vec!["test query".into()],
        required_image_count,
        quality: image_retrieval::domain::query_plan::QualityTier::General,
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

// ---------------------------------------------------------------------------
// Search target tests
// ---------------------------------------------------------------------------

/// AC: 3 images → candidate target = 60
#[test]
fn search_target_for_3_images_is_60() {
    let qp = make_query_plan(3);
    assert_eq!(qp.candidate_target, 60);
}

/// AC: candidate_target = required_image_count * 20
#[test]
fn candidate_target_is_20n() {
    for n in &[1u32, 2, 3, 5, 10] {
        let qp = make_query_plan(*n);
        assert_eq!(qp.candidate_target, n * 20, "candidate_target for n={}", n);
    }
}

// ---------------------------------------------------------------------------
// Weighted scheduling integration tests
// ---------------------------------------------------------------------------

/// AC: Multiple enabled ready providers participate in weighted scheduling.
#[test]
fn multi_provider_weighted_scheduling() {
    let mut registry = ProviderRegistry::new();

    let alpha = Arc::new(ready_fixture_with_candidates(
        "alpha",
        "Alpha Search",
        5,
        30,
    ));
    let beta = Arc::new(ready_fixture_with_candidates("beta", "Beta Search", 1, 10));

    registry.register_adapter(ProviderId::new("alpha"), alpha);
    registry.register_adapter(ProviderId::new("beta"), beta);

    let query_plan = make_query_plan(1); // target = 20
    let scheduler = SearchScheduler::new();

    // total_weight=6; alpha=[0,5), beta=[5,6)
    let mut rng = TestRandom::new(vec![0, 1, 2, 3, 4, 5]);

    let outcome = scheduler.run(&query_plan, &registry, &mut rng);
    assert!(outcome.target_met);

    let alpha_events: Vec<_> = outcome
        .usage_events
        .iter()
        .filter(|e| e.provider_id.to_string() == "alpha")
        .collect();
    let beta_events: Vec<_> = outcome
        .usage_events
        .iter()
        .filter(|e| e.provider_id.to_string() == "beta")
        .collect();
    assert!(
        alpha_events.len() >= beta_events.len(),
        "high-weight alpha should be called at least as often as low-weight beta"
    );
}

/// AC: Equal default weight — both providers participate.
#[test]
fn equal_default_weight_both_used() {
    let mut registry = ProviderRegistry::new();

    let a = Arc::new(ready_fixture_with_candidates("a", "A", 1, 35));
    let b = Arc::new(ready_fixture_with_candidates("b", "B", 1, 35));

    registry.register_adapter(ProviderId::new("a"), a);
    registry.register_adapter(ProviderId::new("b"), b);

    let query_plan = make_query_plan(3); // target = 60
    let scheduler = SearchScheduler::new();
    let mut rng = TestRandom::new(vec![0, 1]);

    let outcome = scheduler.run(&query_plan, &registry, &mut rng);
    assert!(outcome.target_met);

    let a_used = outcome
        .usage_events
        .iter()
        .any(|e| e.provider_id.to_string() == "a");
    let b_used = outcome
        .usage_events
        .iter()
        .any(|e| e.provider_id.to_string() == "b");
    assert!(
        a_used && b_used,
        "both equal-weight providers should be used"
    );

    let a_weight = outcome
        .usage_events
        .iter()
        .find(|e| e.provider_id.to_string() == "a")
        .unwrap()
        .selected_weight;
    let b_weight = outcome
        .usage_events
        .iter()
        .find(|e| e.provider_id.to_string() == "b")
        .unwrap()
        .selected_weight;
    assert_eq!(a_weight, 1);
    assert_eq!(b_weight, 1);
}

/// AC: Zero or negative weight produces Misconfigured readiness and exclusion.
#[test]
fn abnormal_weight_providers_diagnosed_and_excluded() {
    let mut registry = ProviderRegistry::new();

    let good = Arc::new(
        FixtureSearchProvider::new("good", "Good")
            .with_weight(1)
            .with_responses(vec![(0..30)
                .map(|i| make_fixture_candidate(i, "good", "https://good.example.com"))
                .collect()]),
    );
    let zero = Arc::new(FixtureSearchProvider::new("zero", "Zero").with_weight(0));
    let neg = Arc::new(
        FixtureSearchProvider::new("negative", "Negative")
            .with_weight(0)
            .with_enabled(true),
    );

    registry.register_adapter(ProviderId::new("good"), good);
    registry.register_adapter(ProviderId::new("zero"), zero);
    registry.register_adapter(ProviderId::new("negative"), neg);

    let query_plan = make_query_plan(1);
    let scheduler = SearchScheduler::new();
    let mut rng = TestRandom::new(vec![0]);

    let outcome = scheduler.run(&query_plan, &registry, &mut rng);

    // Zero weight is misconfigured — not available
    let zero_report = outcome
        .readiness_reports
        .iter()
        .find(|r| r.provider_id.to_string() == "zero")
        .unwrap();
    assert!(!zero_report.available);
    assert!(!zero_report.included_in_weight_table);
    assert_eq!(zero_report.status, ProviderReadinessStatus::Misconfigured);

    // Only "good" should have been used
    let zero_used = outcome
        .usage_events
        .iter()
        .any(|e| e.provider_id.to_string() == "zero");
    assert!(!zero_used, "zero-weight provider should not be called");
}

// ---------------------------------------------------------------------------
// Candidate shortage tests
// ---------------------------------------------------------------------------

/// AC: Candidate shortage is not blocking — candidates are returned with explanation.
#[test]
fn candidate_shortage_not_blocking() {
    let mut registry = ProviderRegistry::new();

    let sparse = Arc::new(ready_fixture_with_candidates("sparse", "Sparse", 1, 8));
    registry.register_adapter(ProviderId::new("sparse"), sparse);

    let query_plan = make_query_plan(3); // target = 60
    let scheduler = SearchScheduler::new();
    let mut rng = TestRandom::new(vec![0]);

    let outcome = scheduler.run(&query_plan, &registry, &mut rng);

    assert!(!outcome.target_met);
    assert!(!outcome.candidates.is_empty());
    assert_eq!(outcome.candidates.len(), 8);
    assert!(outcome.shortage_reason.is_some());
}

// ---------------------------------------------------------------------------
// Source traceability tests
// ---------------------------------------------------------------------------

/// AC: Every candidate preserves its provider id, search round, and rank.
#[test]
fn candidate_source_traceability() {
    let mut registry = ProviderRegistry::new();

    let src1 = Arc::new(ready_fixture_with_candidates("src1", "Source 1", 2, 12));
    let src2 = Arc::new(ready_fixture_with_candidates("src2", "Source 2", 1, 12));

    registry.register_adapter(ProviderId::new("src1"), src1);
    registry.register_adapter(ProviderId::new("src2"), src2);

    let query_plan = make_query_plan(1); // target = 20
    let scheduler = SearchScheduler::new();
    let mut rng = TestRandom::new(vec![0, 2]); // pick src1 then src2

    let outcome = scheduler.run(&query_plan, &registry, &mut rng);

    for candidate in &outcome.candidates {
        let pid = candidate.provider_id.to_string();
        assert!(
            pid == "src1" || pid == "src2",
            "candidate must carry its source provider id, got {}",
            pid
        );
        assert!(candidate.provider_rank >= 1, "provider_rank must be >= 1");
        assert!(candidate.search_round >= 1, "search_round must be >= 1");
    }

    // Usage events show per-provider contributions
    let src1_contrib: u32 = outcome
        .usage_events
        .iter()
        .filter(|e| e.provider_id.to_string() == "src1")
        .map(|e| e.unique_candidate_count_after_dedupe)
        .sum();
    let src2_contrib: u32 = outcome
        .usage_events
        .iter()
        .filter(|e| e.provider_id.to_string() == "src2")
        .map(|e| e.unique_candidate_count_after_dedupe)
        .sum();

    assert!(src1_contrib > 0, "src1 should contribute candidates");
    assert!(src2_contrib > 0, "src2 should contribute candidates");
}

// ---------------------------------------------------------------------------
// Credential safety tests
// ---------------------------------------------------------------------------

/// AC: No credentials appear in search evidence (candidates, usage events, diagnostics).
#[test]
fn no_credentials_in_search_evidence() {
    let mut registry = ProviderRegistry::new();

    let safe = Arc::new(ready_fixture_with_candidates("safe", "Safe", 1, 5));
    registry.register_adapter(ProviderId::new("safe"), safe);

    let query_plan = make_query_plan(1);
    let scheduler = SearchScheduler::new();
    let mut rng = TestRandom::new(vec![0]);

    let outcome = scheduler.run(&query_plan, &registry, &mut rng);

    let candidates_json = serde_json::to_string(&outcome.candidates).unwrap_or_default();
    let events_json = serde_json::to_string(&outcome.usage_events).unwrap_or_default();
    let diagnostics_json = serde_json::to_string(&outcome.diagnostics).unwrap_or_default();

    for json in &[candidates_json, events_json, diagnostics_json] {
        let lower = json.to_lowercase();
        assert!(!lower.contains("api_key"), "no api_key in output");
        assert!(!lower.contains("token"), "no token in output");
        assert!(!lower.contains("secret"), "no secret in output");
        assert!(!lower.contains("password"), "no password in output");
    }
}

// ---------------------------------------------------------------------------
// Multi-batch exhaustion tests
// ---------------------------------------------------------------------------

/// AC: Provider with multiple batches returns fewer results per call and exhausts.
#[test]
fn multi_batch_exhaustion() {
    let mut registry = ProviderRegistry::new();

    let mb = Arc::new(ready_fixture_with_batches("mb", "MultiBatch", 1, &[15, 10]));
    registry.register_adapter(ProviderId::new("mb"), mb);

    let query_plan = make_query_plan(1); // target = 20
    let scheduler = SearchScheduler::new();
    let mut rng = TestRandom::new(vec![0, 0, 0]);

    let outcome = scheduler.run(&query_plan, &registry, &mut rng);

    // 15 + 10 = 25 candidates, target is 20
    assert_eq!(outcome.candidates.len(), 25);
    assert_eq!(outcome.usage_events.len(), 2);
    assert!(outcome.target_met);
}

// ---------------------------------------------------------------------------
// Cross-provider deduplication tests
// ---------------------------------------------------------------------------

/// AC: Candidates with the same dedupe key are merged across providers.
#[test]
fn cross_provider_dedup() {
    let mut registry = ProviderRegistry::new();

    // Both providers return a candidate with the same image URL
    let dup_url = "https://duplicate.example.com/shared.jpg";
    let unique_x_url = "https://x.example.com/unique.jpg";
    let unique_y_url = "https://y.example.com/unique.jpg";

    use image_retrieval::domain::candidate::CandidateProvenance;

    let qp_id = "qp-test";
    let dedupe_key = CandidateRecord::build_dedupe_key(dup_url);

    let x_batch = vec![
        CandidateRecord {
            candidate_id: CandidateId::new("x-dup"),
            query_plan_id: qp_id.into(),
            provider_id: ProviderId::new("x"),
            provider_kind: "fixture".into(),
            search_request_id: "sr-x".into(),
            search_round: 1,
            provider_rank: 1,
            global_rank_hint: None,
            image_url: dup_url.into(),
            source_page_url: None,
            thumbnail_url: None,
            title: None,
            snippet: None,
            width: None,
            height: None,
            mime_type: None,
            license_hint: None,
            attribution: None,
            dedupe_key: dedupe_key.clone(),
            origin_candidate_ids: vec![CandidateId::new("x-dup")],
            provenance: CandidateProvenance::new(1, "test", 1, 1),
            normalization_warnings: Vec::new(),
        },
        CandidateRecord {
            candidate_id: CandidateId::new("x-uniq"),
            query_plan_id: qp_id.into(),
            provider_id: ProviderId::new("x"),
            provider_kind: "fixture".into(),
            search_request_id: "sr-x".into(),
            search_round: 1,
            provider_rank: 2,
            global_rank_hint: None,
            image_url: unique_x_url.into(),
            source_page_url: None,
            thumbnail_url: None,
            title: None,
            snippet: None,
            width: None,
            height: None,
            mime_type: None,
            license_hint: None,
            attribution: None,
            dedupe_key: CandidateRecord::build_dedupe_key(unique_x_url),
            origin_candidate_ids: vec![CandidateId::new("x-uniq")],
            provenance: CandidateProvenance::new(2, "test", 1, 1),
            normalization_warnings: Vec::new(),
        },
    ];

    let y_batch = vec![
        CandidateRecord {
            candidate_id: CandidateId::new("y-dup"),
            query_plan_id: qp_id.into(),
            provider_id: ProviderId::new("y"),
            provider_kind: "fixture".into(),
            search_request_id: "sr-y".into(),
            search_round: 2,
            provider_rank: 1,
            global_rank_hint: None,
            image_url: dup_url.into(),
            source_page_url: None,
            thumbnail_url: None,
            title: None,
            snippet: None,
            width: None,
            height: None,
            mime_type: None,
            license_hint: None,
            attribution: None,
            dedupe_key: dedupe_key.clone(),
            origin_candidate_ids: vec![CandidateId::new("y-dup")],
            provenance: CandidateProvenance::new(1, "test", 2, 1),
            normalization_warnings: Vec::new(),
        },
        CandidateRecord {
            candidate_id: CandidateId::new("y-uniq"),
            query_plan_id: qp_id.into(),
            provider_id: ProviderId::new("y"),
            provider_kind: "fixture".into(),
            search_request_id: "sr-y".into(),
            search_round: 2,
            provider_rank: 2,
            global_rank_hint: None,
            image_url: unique_y_url.into(),
            source_page_url: None,
            thumbnail_url: None,
            title: None,
            snippet: None,
            width: None,
            height: None,
            mime_type: None,
            license_hint: None,
            attribution: None,
            dedupe_key: CandidateRecord::build_dedupe_key(unique_y_url),
            origin_candidate_ids: vec![CandidateId::new("y-uniq")],
            provenance: CandidateProvenance::new(2, "test", 2, 1),
            normalization_warnings: Vec::new(),
        },
    ];

    let fx = Arc::new(FixtureSearchProvider::new("x", "X").with_responses(vec![x_batch]));
    let fy = Arc::new(FixtureSearchProvider::new("y", "Y").with_responses(vec![y_batch]));

    registry.register_adapter(ProviderId::new("x"), fx);
    registry.register_adapter(ProviderId::new("y"), fy);

    let query_plan = make_query_plan(1); // target = 20
    let scheduler = SearchScheduler::new();
    let mut rng = TestRandom::new(vec![0, 1]); // pick x first, then y

    let outcome = scheduler.run(&query_plan, &registry, &mut rng);

    // Should have 3 unique candidates (not 4 — one duplicate merged)
    assert_eq!(
        outcome.candidates.len(),
        3,
        "should deduplicate by image URL across providers"
    );

    // Dedupe events should include a duplicate
    let dup_count = outcome
        .dedupe_events
        .iter()
        .filter(|e| e.duplicate_of.is_some())
        .count();
    assert_eq!(dup_count, 1, "one candidate should be marked as duplicate");
}

// ---------------------------------------------------------------------------
// Readiness summary tests
// ---------------------------------------------------------------------------

/// AC: Readiness summary covers all registered providers.
#[test]
fn readiness_summary_covers_all_registered_providers() {
    let mut registry = ProviderRegistry::new();

    let r1 = Arc::new(ready_fixture_with_candidates("r1", "R1", 1, 30));
    let r2 = Arc::new(FixtureSearchProvider::new("r2", "R2").with_enabled(false));
    let r3 = Arc::new(FixtureSearchProvider::new("r3", "R3").with_weight(0));

    registry.register_adapter(ProviderId::new("r1"), r1);
    registry.register_adapter(ProviderId::new("r2"), r2);
    registry.register_adapter(ProviderId::new("r3"), r3);

    let query_plan = make_query_plan(1);
    let scheduler = SearchScheduler::new();
    let mut rng = TestRandom::new(vec![0]);

    let outcome = scheduler.run(&query_plan, &registry, &mut rng);

    assert_eq!(outcome.readiness_reports.len(), 3);

    let r1_report = outcome
        .readiness_reports
        .iter()
        .find(|r| r.provider_id.to_string() == "r1")
        .unwrap();
    assert_eq!(r1_report.status, ProviderReadinessStatus::Ready);
    assert!(r1_report.included_in_weight_table);

    let r2_report = outcome
        .readiness_reports
        .iter()
        .find(|r| r.provider_id.to_string() == "r2")
        .unwrap();
    assert!(!r2_report.available);
    assert!(!r2_report.included_in_weight_table);

    let r3_report = outcome
        .readiness_reports
        .iter()
        .find(|r| r.provider_id.to_string() == "r3")
        .unwrap();
    assert!(!r3_report.available);
}

// ---------------------------------------------------------------------------
// SerpApi fixture normalization tests
// ---------------------------------------------------------------------------

/// Sample SerpApi JSON fixture.
const SERPAPI_FIXTURE: &str = r#"{
    "search_metadata": {"id": "test-123", "status": "Success"},
    "search_parameters": {"engine": "google_images", "q": "cats playing"},
    "image_results": [
        {
            "position": 1,
            "title": "Cats playing with yarn",
            "link": "https://example.com/cats-playing",
            "original": "https://example.com/images/cats1.jpg",
            "thumbnail": "https://example.com/thumbs/cats1_t.jpg",
            "original_width": 1920,
            "original_height": 1080,
            "source": "example.com",
            "license": "creative commons"
        },
        {
            "position": 2,
            "title": "Kittens in a box",
            "link": "https://photos.example.com/kittens-box",
            "original": "https://photos.example.com/images/kittens.jpg",
            "thumbnail": "https://photos.example.com/thumbs/kittens_t.jpg",
            "original_width": 800,
            "original_height": 600,
            "source": "photos.example.com"
        }
    ]
}"#;

/// AC: SerpApi image_results[] normalize into CandidateRecord with provenance and dedupe evidence.
#[test]
fn serpapi_fixture_normalization_produces_valid_candidates() {
    let adapter = SerpApiGoogleImagesAdapter::fixture();
    let raw_results = adapter.parse_image_results(SERPAPI_FIXTURE).unwrap();
    assert_eq!(raw_results.len(), 2);

    let candidates: Vec<CandidateRecord> = raw_results
        .iter()
        .filter_map(|raw| {
            adapter.normalize_image_result(raw, "qp-test", "sr-test", 1, 1, "cats playing")
        })
        .collect();

    assert_eq!(candidates.len(), 2);

    // First candidate
    let c1 = &candidates[0];
    assert_eq!(c1.provider_rank, 1);
    assert_eq!(c1.search_round, 1);
    assert_eq!(c1.provider_id.to_string(), "serpapi_google_images");
    assert_eq!(c1.provider_kind, "serpapi_google_images");
    assert_eq!(c1.image_url, "https://example.com/images/cats1.jpg");
    assert_eq!(
        c1.source_page_url,
        Some("https://example.com/cats-playing".into())
    );
    assert_eq!(
        c1.thumbnail_url,
        Some("https://example.com/thumbs/cats1_t.jpg".into())
    );
    assert_eq!(c1.width, Some(1920));
    assert_eq!(c1.height, Some(1080));
    assert_eq!(c1.license_hint, Some("creative commons".into()));
    assert!(!c1.candidate_id.0.is_empty());
    assert!(!c1.dedupe_key.is_empty());
    assert!(!c1.origin_candidate_ids.is_empty());

    // Provenance
    assert_eq!(c1.provenance.provider_rank, 1);
    assert_eq!(c1.provenance.search_round, 1);
    assert_eq!(c1.provenance.search_query, "cats playing");
    assert_eq!(
        c1.provenance.source_authority_hint,
        Some("example.com".into())
    );

    // Second candidate
    let c2 = &candidates[1];
    assert_eq!(c2.provider_rank, 2);
    assert_eq!(c2.width, Some(800));
    assert_eq!(c2.height, Some(600));

    // Dedupe keys should differ for different URLs
    assert_ne!(c1.dedupe_key, c2.dedupe_key);

    // Candidate IDs should differ
    assert_ne!(c1.candidate_id, c2.candidate_id);
}

/// AC: SerpApi candidates have valid provenance with dedupe evidence.
#[test]
fn serpapi_normalized_candidates_have_provenance() {
    let adapter = SerpApiGoogleImagesAdapter::fixture();
    let raw_results = adapter.parse_image_results(SERPAPI_FIXTURE).unwrap();

    let candidate = adapter
        .normalize_image_result(
            &raw_results[0],
            "qp-sunset",
            "sr-001",
            2,
            3,
            "sunset mountains",
        )
        .unwrap();

    assert_eq!(candidate.query_plan_id, "qp-sunset");
    assert_eq!(candidate.search_request_id, "sr-001");
    assert_eq!(candidate.search_round, 2);
    assert_eq!(candidate.provenance.full_attempt_count, 3);
    assert_eq!(candidate.provenance.search_query, "sunset mountains");
    assert!(!candidate.provenance.retrieved_at.is_empty());
}

/// AC: SerpApi credential missing produces PROVIDER_CREDENTIAL_MISSING readiness.
#[test]
fn serpapi_credential_missing_readiness() {
    let adapter = SerpApiGoogleImagesAdapter::fixture();
    let config = image_retrieval::domain::config::SearchProviderConfig {
        provider_id: "serpapi".into(),
        provider_kind: SearchProviderKind::SerpapiGoogleImages,
        enabled: true,
        weight: 100,
        endpoint: Some("https://serpapi.com/search".into()),
        credential_env: Some("SERPAPI_API_KEY".into()),
        default_query_params: BTreeMap::new(),
    };

    let report = adapter.readiness(&config);
    // Fixture adapter has no real credential
    assert!(!report.available);
    assert!(report.failure_code.is_some());
}

/// AC: SerpApi readiness does not leak credential values.
#[test]
fn serpapi_readiness_no_credential_leak() {
    let adapter = SerpApiGoogleImagesAdapter::fixture();
    let config = image_retrieval::domain::config::SearchProviderConfig {
        provider_id: "serpapi".into(),
        provider_kind: SearchProviderKind::SerpapiGoogleImages,
        enabled: true,
        weight: 100,
        endpoint: Some("https://serpapi.com/search".into()),
        credential_env: Some("SERPAPI_API_KEY".into()),
        default_query_params: BTreeMap::new(),
    };

    let report = adapter.readiness(&config);
    let json = serde_json::to_string(&report).unwrap_or_default();
    let lower = json.to_lowercase();

    // The env var NAME may appear, but no resolved secret values
    assert!(lower.contains("serpapi_api_key"));
    // No resolved credential-like values
    assert!(!lower.contains("sk-"));
    assert!(!lower.contains("eyj"));
}

// ---------------------------------------------------------------------------
// Hash-based dedupe key tests
// ---------------------------------------------------------------------------

/// AC: Dedupe keys for the same URL with different tracking params match.
#[test]
fn dedupe_keys_match_after_normalization() {
    let key1 = CandidateRecord::build_dedupe_key(
        "https://EXAMPLE.com/path/image.jpg?utm_source=test&ref=foo",
    );
    let key2 = CandidateRecord::build_dedupe_key("https://example.com/path/image.jpg");
    assert_eq!(
        key1, key2,
        "dedupe keys should match after URL normalization"
    );
}

/// AC: Dedupe key strips fragment.
#[test]
fn dedupe_key_strips_fragment() {
    let key1 = CandidateRecord::build_dedupe_key("https://example.com/image.jpg#section");
    let key2 = CandidateRecord::build_dedupe_key("https://example.com/image.jpg");
    assert_eq!(key1, key2);
}

// ---------------------------------------------------------------------------
// No available providers test
// ---------------------------------------------------------------------------

/// AC: Empty registry produces NO_AVAILABLE_SEARCH_PROVIDER.
#[test]
fn empty_registry_returns_no_available_provider() {
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

/// AC: Only non-ready providers produces NO_AVAILABLE_SEARCH_PROVIDER.
#[test]
fn all_providers_not_ready_returns_no_available() {
    let mut registry = ProviderRegistry::new();

    let not_ready = Arc::new(FixtureSearchProvider::not_ready("p1", "P1"));
    registry.register_adapter(ProviderId::new("p1"), not_ready);

    let query_plan = make_query_plan(1);
    let scheduler = SearchScheduler::new();
    let mut rng = TestRandom::new(vec![0]);

    let outcome = scheduler.run(&query_plan, &registry, &mut rng);
    assert!(!outcome.target_met);
    assert!(outcome.shortage_reason.is_some());
}

// ---------------------------------------------------------------------------
// Scheduler metrics test
// ---------------------------------------------------------------------------

/// AC: Search outcome provides metrics for observability.
#[test]
fn search_outcome_metrics() {
    let mut registry = ProviderRegistry::new();

    let m1 = Arc::new(ready_fixture_with_candidates(
        "m1",
        "Metrics Provider",
        1,
        45,
    ));
    registry.register_adapter(ProviderId::new("m1"), m1);

    let query_plan = make_query_plan(3); // target = 60
    let scheduler = SearchScheduler::new();
    let mut rng = TestRandom::new(vec![0]);

    let outcome = scheduler.run(&query_plan, &registry, &mut rng);

    // candidate_target = 60, actual = 45
    assert_eq!(outcome.candidate_target, 60);
    assert_eq!(outcome.candidates.len() as u32, 45);
    assert!(!outcome.usage_events.is_empty());

    // Usage events have metadata
    for event in &outcome.usage_events {
        assert!(!event.search_request_id.is_empty());
        assert!(event.search_round >= 1, "search_round must be >= 1");
    }
}
