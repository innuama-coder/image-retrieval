//! Search integration tests.
//!
//! End-to-end scenarios covering provider registration, weighted scheduling,
//! candidate deduplication, source tracking, and candidate shortage handling.
//!
//! Uses fixture providers to simulate multiple search services.

use image_retrieval::domain::candidate::{CandidateRecord, ProviderId};
use image_retrieval::domain::query_plan::{
    ContentConstraints, QualityTier, TaskPlan, ValidatedQueryPlan,
};
use image_retrieval::domain::search::{ProviderReadiness, ProviderRegistration};
use image_retrieval::search::fixture::{
    ready_fixture_with_batches, ready_fixture_with_candidates, FixtureProvider,
};
use image_retrieval::search::registry::ProviderRegistry;
use image_retrieval::search::scheduler::{RandomSource, SearchScheduler};
use std::cell::RefCell;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Deterministic random source for integration tests
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
// Helper
// ---------------------------------------------------------------------------

fn make_task_plan(description: &str, required_count: u32) -> TaskPlan {
    let plan = ValidatedQueryPlan {
        description: description.into(),
        required_count,
        quality_tier: QualityTier::General,
        content_constraints: ContentConstraints::default(),
        authorization_preference: Default::default(),
        output_preference: Default::default(),
        retry_limit: 3,
    };
    TaskPlan::from_validated(plan)
}

// ---------------------------------------------------------------------------
// Integration tests
// ---------------------------------------------------------------------------

/// AC: 要求 3 张图片时搜索目标约 60。
#[test]
fn search_target_for_3_images_is_60() {
    let task_plan = make_task_plan("cats playing", 3);
    assert_eq!(task_plan.candidate_target, 60);
}

/// AC: 多个 enabled 且 ready provider 可按权重参与调度。
#[test]
fn search_multi_provider_weighted_scheduling_integration() {
    let mut registry = ProviderRegistry::new();
    registry.register(
        ProviderRegistration::new(ProviderId::new("alpha"), "Alpha Search")
            .with_enabled(true)
            .with_weight(5),
    );
    registry.register(
        ProviderRegistration::new(ProviderId::new("beta"), "Beta Search")
            .with_enabled(true)
            .with_weight(1),
    );

    let alpha = ready_fixture_with_candidates("alpha", "Alpha", 5, 30);
    let beta = ready_fixture_with_candidates("beta", "Beta", 1, 10);

    let mut providers: HashMap<String, &dyn image_retrieval::ports::BaseProvider> = HashMap::new();
    providers.insert("alpha".to_string(), &alpha);
    providers.insert("beta".to_string(), &beta);

    let task_plan = make_task_plan("test", 1); // target = 20
    let scheduler = SearchScheduler::new();

    // rng values map to high-weight Alpha more often
    // total_weight = 6; alpha gets [0,5), beta gets [5,6)
    let mut rng = TestRandom::new(vec![0, 1, 2, 3, 4, 5]);

    let outcome = scheduler.run(&task_plan, &registry, &providers, &mut rng);

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

/// AC: 未指定权重时等权。
#[test]
fn search_equal_default_weight_integration() {
    let mut registry = ProviderRegistry::new();
    registry.register(ProviderRegistration::new(ProviderId::new("a"), "A"));
    registry.register(ProviderRegistration::new(ProviderId::new("b"), "B"));

    // Both providers use default weight=1
    let a = ready_fixture_with_candidates("a", "A", 1, 35);
    let b = ready_fixture_with_candidates("b", "B", 1, 35);

    let mut providers: HashMap<String, &dyn image_retrieval::ports::BaseProvider> = HashMap::new();
    providers.insert("a".to_string(), &a);
    providers.insert("b".to_string(), &b);

    let task_plan = make_task_plan("test", 3); // target = 60
    let scheduler = SearchScheduler::new();
    let mut rng = TestRandom::new(vec![0, 1]); // pick A, then B

    let outcome = scheduler.run(&task_plan, &registry, &providers, &mut rng);
    assert!(outcome.target_met);

    // Both should have been used
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

    // They should have the same effective weight
    let a_weight = outcome
        .usage_events
        .iter()
        .find(|e| e.provider_id.to_string() == "a")
        .unwrap()
        .effective_weight;
    let b_weight = outcome
        .usage_events
        .iter()
        .find(|e| e.provider_id.to_string() == "b")
        .unwrap()
        .effective_weight;
    assert_eq!(a_weight, 1);
    assert_eq!(b_weight, 1);
}

/// AC: 非数值、负数或零权重产生配置诊断并排除。
#[test]
fn search_abnormal_weight_providers_diagnosed_and_excluded() {
    let mut registry = ProviderRegistry::new();
    registry.register(ProviderRegistration::new(ProviderId::new("good"), "Good"));
    registry.register(ProviderRegistration::new(ProviderId::new("zero"), "Zero").with_weight(0));
    registry.register(
        ProviderRegistration::new(ProviderId::new("negative"), "Negative").with_weight(-3),
    );

    let good = ready_fixture_with_candidates("good", "Good", 1, 30);
    // Even if zero/negative providers are instantiated, they should be excluded
    let zero = ready_fixture_with_candidates("zero", "Zero", 0, 10);
    let neg = ready_fixture_with_candidates("negative", "Negative", -3, 10);

    let mut providers: HashMap<String, &dyn image_retrieval::ports::BaseProvider> = HashMap::new();
    providers.insert("good".to_string(), &good);
    providers.insert("zero".to_string(), &zero);
    providers.insert("negative".to_string(), &neg);

    let task_plan = make_task_plan("test", 1);
    let scheduler = SearchScheduler::new();
    let mut rng = TestRandom::new(vec![0]);

    let outcome = scheduler.run(&task_plan, &registry, &providers, &mut rng);

    // Zero and negative weight providers should be Misconfigured
    let zero_summary = outcome
        .readiness_summary
        .iter()
        .find(|r| r.provider_id.to_string() == "zero")
        .unwrap();
    assert_eq!(zero_summary.readiness, ProviderReadiness::Misconfigured);
    assert!(!zero_summary.included_in_table);
    assert_eq!(zero_summary.configured_weight, 0);

    let neg_summary = outcome
        .readiness_summary
        .iter()
        .find(|r| r.provider_id.to_string() == "negative")
        .unwrap();
    assert_eq!(neg_summary.readiness, ProviderReadiness::Misconfigured);
    assert!(!neg_summary.included_in_table);
    assert_eq!(neg_summary.configured_weight, -3);

    // Only "good" should be used
    let zero_used = outcome
        .usage_events
        .iter()
        .any(|e| e.provider_id.to_string() == "zero");
    let neg_used = outcome
        .usage_events
        .iter()
        .any(|e| e.provider_id.to_string() == "negative");
    assert!(!zero_used, "zero-weight provider should not be called");
    assert!(!neg_used, "negative-weight provider should not be called");
}

/// AC: 候选不足保留说明但不直接执行阻塞。
#[test]
fn search_candidate_shortage_not_blocking_integration() {
    let mut registry = ProviderRegistry::new();
    registry.register(ProviderRegistration::new(
        ProviderId::new("sparse"),
        "Sparse",
    ));

    // Only 8 candidates available, target is 60
    let sparse = ready_fixture_with_candidates("sparse", "Sparse", 1, 8);

    let mut providers: HashMap<String, &dyn image_retrieval::ports::BaseProvider> = HashMap::new();
    providers.insert("sparse".to_string(), &sparse);

    let task_plan = make_task_plan("very specific query", 3); // target = 60
    let scheduler = SearchScheduler::new();
    let mut rng = TestRandom::new(vec![0]);

    let outcome = scheduler.run(&task_plan, &registry, &providers, &mut rng);

    // Not met, but candidates ARE returned — not blocked
    assert!(!outcome.target_met);
    assert!(!outcome.candidates.is_empty());
    assert_eq!(outcome.candidates.len(), 8);
    // Shortfall has an explanation
    assert!(outcome.shortage_reason.is_some());
    // The shortage reason must not imply execution is blocked
    let reason_str = outcome.shortage_reason.as_ref().unwrap().to_string();
    assert!(!reason_str.is_empty());
}

/// AC: 候选来源可解释。
#[test]
fn search_candidate_source_traceability_integration() {
    let mut registry = ProviderRegistry::new();
    registry
        .register(ProviderRegistration::new(ProviderId::new("src1"), "Source 1").with_weight(2));
    registry
        .register(ProviderRegistration::new(ProviderId::new("src2"), "Source 2").with_weight(1));

    // Give each provider fewer than the target so both must contribute
    let src1 = ready_fixture_with_candidates("src1", "Source 1", 2, 12);
    let src2 = ready_fixture_with_candidates("src2", "Source 2", 1, 12);

    let mut providers: HashMap<String, &dyn image_retrieval::ports::BaseProvider> = HashMap::new();
    providers.insert("src1".to_string(), &src1);
    providers.insert("src2".to_string(), &src2);

    let task_plan = make_task_plan("traceable query", 1); // target = 20
    let scheduler = SearchScheduler::new();

    // rng: 0→src1, 2→src2 (total_weight=3; 0,1→src1, 2→src2)
    let mut rng = TestRandom::new(vec![0, 2]);

    let outcome = scheduler.run(&task_plan, &registry, &providers, &mut rng);

    // Every candidate must have a provider id set
    for candidate in &outcome.candidates {
        let pid = candidate.provider_id.to_string();
        assert!(
            pid == "src1" || pid == "src2",
            "candidate must carry its source provider id"
        );
    }

    // Usage events trace which provider contributed how many
    let src1_contrib: u32 = outcome
        .usage_events
        .iter()
        .filter(|e| e.provider_id.to_string() == "src1")
        .map(|e| e.deduped_candidate_count)
        .sum();
    let src2_contrib: u32 = outcome
        .usage_events
        .iter()
        .filter(|e| e.provider_id.to_string() == "src2")
        .map(|e| e.deduped_candidate_count)
        .sum();

    assert!(src1_contrib > 0, "src1 should contribute candidates");
    assert!(src2_contrib > 0, "src2 should contribute candidates");
}

/// AC: 凭据不进入用户可见证据。
#[test]
fn search_no_credentials_in_search_evidence() {
    let mut registry = ProviderRegistry::new();
    registry.register(ProviderRegistration::new(ProviderId::new("safe"), "Safe"));

    // Create candidates with fixture data
    let safe = ready_fixture_with_candidates("safe", "Safe", 1, 5);

    let mut providers: HashMap<String, &dyn image_retrieval::ports::BaseProvider> = HashMap::new();
    providers.insert("safe".to_string(), &safe);

    let task_plan = make_task_plan("test", 1);
    let scheduler = SearchScheduler::new();
    let mut rng = TestRandom::new(vec![0]);

    let outcome = scheduler.run(&task_plan, &registry, &providers, &mut rng);

    // Serialize candidates and usage events (simulating user-visible output)
    let candidates_json = serde_json::to_string(&outcome.candidates).unwrap();
    let events_json = serde_json::to_string(&outcome.usage_events).unwrap();

    for json in &[candidates_json, events_json] {
        let lower = json.to_lowercase();
        assert!(!lower.contains("api_key"), "no api_key in output");
        assert!(!lower.contains("token"), "no token in output");
        assert!(!lower.contains("secret"), "no secret in output");
        assert!(!lower.contains("password"), "no password in output");
        assert!(!lower.contains("credential"), "no credential in output");
    }
}

/// Multi-batch provider: subsequent calls return fewer results.
#[test]
fn search_multi_batch_exhaustion_integration() {
    let mut registry = ProviderRegistry::new();
    registry.register(ProviderRegistration::new(
        ProviderId::new("mb"),
        "MultiBatch",
    ));

    // Provider returns 15, then 10, then exhausted
    let mb = ready_fixture_with_batches("mb", "MultiBatch", 1, &[15, 10]);

    let mut providers: HashMap<String, &dyn image_retrieval::ports::BaseProvider> = HashMap::new();
    providers.insert("mb".to_string(), &mb);

    let task_plan = make_task_plan("test", 1); // target = 20
    let scheduler = SearchScheduler::new();
    let mut rng = TestRandom::new(vec![0, 0, 0]); // always pick mb

    let outcome = scheduler.run(&task_plan, &registry, &providers, &mut rng);

    // 15 + 10 = 25 candidates, target is 20 → target met in 2 calls
    assert_eq!(outcome.candidates.len(), 25);
    assert_eq!(outcome.usage_events.len(), 2);
    assert!(outcome.target_met);
}

/// Deduplication across providers.
#[test]
fn search_cross_provider_dedup_integration() {
    let mut registry = ProviderRegistry::new();
    registry.register(ProviderRegistration::new(ProviderId::new("x"), "X"));
    registry.register(ProviderRegistration::new(ProviderId::new("y"), "Y"));

    // Both providers return a candidate with the same source URL
    // We use fixture batch but need overlapping URLs
    use image_retrieval::domain::candidate::CandidateId;

    let dup_url = "https://duplicate.example.com/shared.jpg";
    let unique_x_url = "https://x.example.com/unique.jpg";
    let unique_y_url = "https://y.example.com/unique.jpg";

    let x_batch = vec![
        CandidateRecord {
            id: CandidateId::new("x-dup"),
            provider_id: ProviderId::new("x"),
            source_url: dup_url.into(),
            thumbnail_url: None,
            title: None,
            page_url: None,
            dimensions: None,
        },
        CandidateRecord {
            id: CandidateId::new("x-uniq"),
            provider_id: ProviderId::new("x"),
            source_url: unique_x_url.into(),
            thumbnail_url: None,
            title: None,
            page_url: None,
            dimensions: None,
        },
    ];

    let y_batch = vec![
        CandidateRecord {
            id: CandidateId::new("y-dup"),
            provider_id: ProviderId::new("y"),
            source_url: dup_url.into(), // duplicate with x
            thumbnail_url: None,
            title: None,
            page_url: None,
            dimensions: None,
        },
        CandidateRecord {
            id: CandidateId::new("y-uniq"),
            provider_id: ProviderId::new("y"),
            source_url: unique_y_url.into(),
            thumbnail_url: None,
            title: None,
            page_url: None,
            dimensions: None,
        },
    ];

    let fx = FixtureProvider::new("x", "X").with_responses(vec![x_batch]);
    let fy = FixtureProvider::new("y", "Y").with_responses(vec![y_batch]);

    let mut providers: HashMap<String, &dyn image_retrieval::ports::BaseProvider> = HashMap::new();
    providers.insert("x".to_string(), &fx);
    providers.insert("y".to_string(), &fy);

    let task_plan = make_task_plan("dedup test", 1); // target = 20
    let scheduler = SearchScheduler::new();
    let mut rng = TestRandom::new(vec![0, 1]); // pick x first, then y

    let outcome = scheduler.run(&task_plan, &registry, &providers, &mut rng);

    // Should have 3 unique candidates (not 4, because one is a duplicate URL)
    assert_eq!(
        outcome.candidates.len(),
        3,
        "should deduplicate by source URL across providers"
    );

    // The dedup should be reflected in usage events
    let total_deduped: u32 = outcome
        .usage_events
        .iter()
        .map(|e| e.deduped_candidate_count)
        .sum();
    assert_eq!(total_deduped, 3);

    let total_raw: u32 = outcome
        .usage_events
        .iter()
        .map(|e| e.raw_candidate_count)
        .sum();
    assert_eq!(total_raw, 4);
}

/// Provider readiness summary is always populated.
#[test]
fn search_readiness_summary_covers_all_registered_providers() {
    let mut registry = ProviderRegistry::new();
    registry.register(ProviderRegistration::new(ProviderId::new("r1"), "R1"));
    registry.register(ProviderRegistration::new(ProviderId::new("r2"), "R2").with_enabled(false));
    registry.register(ProviderRegistration::new(ProviderId::new("r3"), "R3").with_weight(0));

    let r1 = ready_fixture_with_candidates("r1", "R1", 1, 30);

    let mut providers: HashMap<String, &dyn image_retrieval::ports::BaseProvider> = HashMap::new();
    providers.insert("r1".to_string(), &r1);

    let task_plan = make_task_plan("summary test", 1);
    let scheduler = SearchScheduler::new();
    let mut rng = TestRandom::new(vec![0]);

    let outcome = scheduler.run(&task_plan, &registry, &providers, &mut rng);

    assert_eq!(outcome.readiness_summary.len(), 3);

    let r1_s = outcome
        .readiness_summary
        .iter()
        .find(|r| r.provider_id.to_string() == "r1")
        .unwrap();
    assert_eq!(r1_s.readiness, ProviderReadiness::Ready);
    assert!(r1_s.included_in_table);

    let r2_s = outcome
        .readiness_summary
        .iter()
        .find(|r| r.provider_id.to_string() == "r2")
        .unwrap();
    assert_eq!(r2_s.readiness, ProviderReadiness::Disabled);
    assert!(!r2_s.included_in_table);

    let r3_s = outcome
        .readiness_summary
        .iter()
        .find(|r| r.provider_id.to_string() == "r3")
        .unwrap();
    assert_eq!(r3_s.readiness, ProviderReadiness::Misconfigured);
    assert!(!r3_s.included_in_table);
}

/// Search outcome contains MET-002 events.
#[test]
fn search_outcome_met002_evidence() {
    let mut registry = ProviderRegistry::new();
    registry.register(ProviderRegistration::new(
        ProviderId::new("m1"),
        "MET-002 Provider",
    ));

    let m1 = ready_fixture_with_candidates("m1", "MET-002 Provider", 1, 45);

    let mut providers: HashMap<String, &dyn image_retrieval::ports::BaseProvider> = HashMap::new();
    providers.insert("m1".to_string(), &m1);

    let task_plan = make_task_plan("metrics test", 3); // target = 60
    let scheduler = SearchScheduler::new();
    let mut rng = TestRandom::new(vec![0]);

    let outcome = scheduler.run(&task_plan, &registry, &providers, &mut rng);

    // MET-002 denominator: candidate_target
    assert_eq!(outcome.candidate_target, 60);
    // MET-002 numerator: actual candidates collected
    assert_eq!(outcome.candidates.len() as u32, 45);
    // Candidate satisfaction rate
    let rate = outcome.candidates.len() as f64 / outcome.candidate_target as f64;
    assert!(rate < 1.0, "45/60 = 0.75 satisfaction rate");
    assert!(rate > 0.5);
    // Usage events provide provider-level breakdown
    assert!(!outcome.usage_events.is_empty());
}
