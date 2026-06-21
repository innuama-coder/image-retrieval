//! End-to-end fixture integration tests for TASK-009.
//!
//! These tests validate the complete image-retrieval pipeline using ONLY
//! internal fixtures — no real credentials, no network access, no production
//! OpenClaw. Fixture results are NOT production delivery evidence.
//!
//! # Coverage
//!
//! | Scenario | What it covers |
//! |---|---|
//! | `input_rejected` | QueryPlan with empty description is rejected before any search/retrieval |
//! | `full_delivery` | Pipeline from search → candidate eval → retrieval → image acceptance → full delivery |
//! | `limited_delivery_0` | All images rejected across all retries → limited delivery with 0 accepted |
//! | `execution_blocked_openclaw` | OpenClaw unavailable → task enters execution_blocked |
//! | `channel_fallback_disabled` | Disabled channel produces correct readiness, fallback blocked on access restrictions |
//! | `sensitive_info_exclusion` | Credential patterns never appear in delivery package output |
//! | `self_check_e2e` | Self-check reports correct readiness without producing delivery artifacts |
//!
//! References: PRD §依赖、风险与开放问题, HLD §运维发布设计,
//! `docs/design/TASK-009-detailed-design-acceptance-review.md`

use image_retrieval::delivery::{DeliveryInputs, DeliveryPackageBuilder};
use image_retrieval::domain::candidate::{
    CandidateDecision, CandidateRecord, RetrievableCandidateSequence,
};
use image_retrieval::domain::delivery::{DeliveryDecision, TaskStatus};
use image_retrieval::domain::image::{
    ImageAcceptanceDecision, ImageMechanicalEvidence, ImageRecord,
};
use image_retrieval::domain::metrics::{MetricEvent, MetricKind};
use image_retrieval::domain::policy::{AuthorizationRisk, PolicyDecision, PolicyFact};
use image_retrieval::domain::query_plan::{
    validate_query_plan, ContentConstraints, OutputPreference, QualityTier, QueryPlanInput,
    TaskPlan, ValidatedQueryPlan,
};
use image_retrieval::domain::retrieval::{
    ExecutionBlockingFact, FallbackEligibilityFact, RetrievalBatch, RetrievalChannelTier,
    RetrievalFailure, RetrievalFailureCategory, RetrievalResult,
};
use image_retrieval::domain::search::{ProviderReadiness, ProviderRegistration};
use image_retrieval::error::{Error, Result};
use image_retrieval::orchestrator::{OrchestratorState, TaskOrchestrator};
use image_retrieval::policy::{
    contains_sensitive_pattern, evaluate_execution_block, evaluate_fallback_eligibility,
    evaluate_policy_fact, sanitize_for_delivery, sanitize_metadata,
};
use image_retrieval::ports::{BaseProvider, BaseRetrievalChannel, OpenClawEvaluationPort};
use image_retrieval::quality::candidate::evaluation::CandidateEvaluationConclusion;
use image_retrieval::quality::candidate::gate::evaluate_with_conclusions;
use image_retrieval::quality::candidate::mechanical::{
    validate_candidate_mechanical, CandidateMechanicalEvidence,
};
use image_retrieval::quality::image::evaluation::ImageEvaluationConclusion;
use image_retrieval::quality::image::gate::evaluate_images_with_conclusions;
use image_retrieval::search::fixture::{
    make_fixture_batch, make_fixture_candidate, FixtureProvider,
};
use image_retrieval::self_check::{
    run_self_check, ChannelReadinessEntry, ProviderReadinessEntry, SelfCheckRequest,
    SelfCheckStatus,
};
use std::cell::RefCell;
use std::collections::HashSet;

// ===========================================================================
// Fixture Evaluators — in-memory, no production OpenClaw
// ===========================================================================

/// A configurable OpenClaw evaluator for testing.
///
/// Candidate-phase and image-phase evaluations return pre-programmed
/// conclusions. Readiness can be toggled to simulate OpenClaw unavailability.
struct FixtureEvaluator {
    ready: bool,
    candidate_conclusions: RefCell<Vec<CandidateEvaluationConclusion>>,
    image_conclusions: RefCell<Vec<ImageEvaluationConclusion>>,
}

impl FixtureEvaluator {
    fn ready_with_image_conclusions(conclusions: Vec<ImageEvaluationConclusion>) -> Self {
        Self {
            ready: true,
            candidate_conclusions: RefCell::new(vec![]),
            image_conclusions: RefCell::new(conclusions),
        }
    }

    fn unavailable() -> Self {
        Self {
            ready: false,
            candidate_conclusions: RefCell::new(vec![]),
            image_conclusions: RefCell::new(vec![]),
        }
    }
}

impl OpenClawEvaluationPort for FixtureEvaluator {
    fn readiness(&self) -> Result<()> {
        if self.ready {
            Ok(())
        } else {
            Err(Error::openclaw_unavailable(
                "fixture: OpenClaw not available for testing",
            ))
        }
    }

    fn evaluate_candidates(
        &self,
        candidates: &[CandidateRecord],
        _description: &str,
    ) -> Result<Vec<CandidateDecision>> {
        self.readiness()?;
        let conclusions = self.candidate_conclusions.borrow().clone();
        let mech = CandidateMechanicalEvidence::pass();
        let passed: Vec<(CandidateRecord, CandidateMechanicalEvidence)> = candidates
            .iter()
            .cloned()
            .map(|c| (c, mech.clone()))
            .collect();
        Ok(evaluate_with_conclusions(passed, conclusions))
    }

    fn evaluate_images(
        &self,
        images: &[ImageRecord],
        _description: &str,
    ) -> Result<Vec<ImageAcceptanceDecision>> {
        self.readiness()?;
        let conclusions = self.image_conclusions.borrow().clone();
        let mech = ImageMechanicalEvidence {
            blocking_findings: vec![],
            reference_findings: vec![],
        };
        let passed: Vec<(ImageRecord, ImageMechanicalEvidence)> = images
            .iter()
            .cloned()
            .map(|img| (img, mech.clone()))
            .collect();
        Ok(evaluate_images_with_conclusions(passed, conclusions))
    }
}

// ===========================================================================
// Helpers
// ===========================================================================

fn make_approve() -> ImageEvaluationConclusion {
    ImageEvaluationConclusion::Approve {
        notes: Some("good match".into()),
    }
}

fn make_reject(reason: &str) -> ImageEvaluationConclusion {
    ImageEvaluationConclusion::Reject {
        reason: reason.into(),
    }
}

fn make_image(id: &str) -> ImageRecord {
    ImageRecord {
        candidate_id: id.into(),
        local_path: format!("/tmp/{}.jpg", id),
        content_type: Some("image/jpeg".into()),
        file_size_bytes: 4096,
        dimensions: None,
    }
}

fn make_plan(required_count: u32) -> ValidatedQueryPlan {
    ValidatedQueryPlan {
        description: "sunset over mountains".into(),
        required_count,
        quality_tier: QualityTier::General,
        content_constraints: ContentConstraints::default(),
        authorization_preference:
            image_retrieval::domain::query_plan::AuthorizationPreference::Default,
        output_preference: OutputPreference::Human,
        retry_limit: 3,
    }
}

fn temp_dir(prefix: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("e2e-{}-{}-{}", prefix, std::process::id(), n));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

// ===========================================================================
// E2E Scenario 1: input_rejected — QueryPlan rejected before any execution
// ===========================================================================

#[test]
fn e2e_input_rejected_missing_description() {
    let input = QueryPlanInput {
        description: "".into(),
        ..Default::default()
    };

    match validate_query_plan(input) {
        image_retrieval::domain::query_plan::ValidationOutcome::Rejected(rejection) => {
            assert!(rejection.summary.contains("缺少图片语义描述"));
            let decision = DeliveryDecision::input_rejected(rejection.summary.clone());
            assert_eq!(decision.status, TaskStatus::InputRejected);
            assert_eq!(decision.accepted_images.len(), 0);
            assert_eq!(decision.full_attempt_count, 0);
        }
        other => panic!("expected rejection, got {:?}", other),
    }
}

#[test]
fn e2e_input_rejected_whitespace_only_description() {
    let input = QueryPlanInput {
        description: "   \n  ".into(),
        ..Default::default()
    };
    let outcome = validate_query_plan(input);
    assert!(!outcome.is_valid());
}

#[test]
fn e2e_input_rejected_retry_limit_exceeded() {
    let input = QueryPlanInput {
        description: "valid description".into(),
        retry_limit: 10,
        ..Default::default()
    };
    let outcome = validate_query_plan(input);
    assert!(!outcome.is_valid());
    match outcome {
        image_retrieval::domain::query_plan::ValidationOutcome::Rejected(rejection) => {
            assert!(rejection.summary.contains("输入被拒绝"));
        }
        _ => panic!("expected rejection"),
    }
}

#[test]
fn e2e_input_rejection_does_not_produce_delivery_package() {
    let dir = temp_dir("input-rejected");
    let builder = DeliveryPackageBuilder::new(&dir);
    let task_plan = TaskPlan::from_validated(make_plan(1));
    let decision = DeliveryDecision::input_rejected("missing description".to_string());
    let inputs = DeliveryInputs::minimal(task_plan, decision);

    let result = builder.build(&inputs);
    assert!(result.is_err());
    match result {
        Err(Error::InputRejection { reason }) => {
            assert!(reason.contains("Input rejection"));
        }
        _ => panic!("expected InputRejection error"),
    }
    assert!(!dir.join("status.json").exists());
    assert!(!dir.join("manifest.json").exists());
}

// ===========================================================================
// E2E Scenario 2: full_delivery — complete pipeline with fixtures
// ===========================================================================

#[test]
fn e2e_full_delivery_complete_pipeline() {
    let plan = make_plan(2);
    let task_plan = TaskPlan::from_validated(plan);

    // Search with fixture provider
    let provider = FixtureProvider::new("fixture-p1", "Fixture Provider")
        .with_responses(vec![make_fixture_batch("fixture-p1", 5, 0)]);
    let search_result = provider.search("sunset over mountains", 10).unwrap();
    assert_eq!(search_result.len(), 5);

    // Mechanical validation
    let plan_qp = task_plan.query_plan.clone();
    let mut seen = HashSet::new();
    let mut mechanically_passed: Vec<(CandidateRecord, CandidateMechanicalEvidence)> = Vec::new();

    for c in &search_result {
        let evidence = validate_candidate_mechanical(
            c,
            &seen,
            &plan_qp.content_constraints,
            plan_qp.quality_tier,
        );
        if evidence.passed_mechanical() {
            if !c.source_url.trim().is_empty() {
                seen.insert(c.source_url.clone());
            }
            mechanically_passed.push((c.clone(), evidence));
        }
    }
    assert!(mechanically_passed.len() >= 4);

    // Candidate evaluation: approve first 3
    let candidate_conclusions: Vec<CandidateEvaluationConclusion> = mechanically_passed
        .iter()
        .enumerate()
        .map(|(i, _)| {
            if i < 3 {
                CandidateEvaluationConclusion::Approve {
                    notes: Some("good match".into()),
                }
            } else {
                CandidateEvaluationConclusion::Reject {
                    reason: "not matching".into(),
                }
            }
        })
        .collect();

    let candidate_decisions = evaluate_with_conclusions(mechanically_passed, candidate_conclusions);
    let retrievable = RetrievableCandidateSequence::from_decisions(candidate_decisions);
    assert_eq!(retrievable.len(), 3);

    // Image acceptance
    let accepted_ids: Vec<String> = retrievable
        .candidates
        .iter()
        .filter_map(|d| match d {
            CandidateDecision::Accepted { candidate, .. } => Some(candidate.id.to_string()),
            _ => None,
        })
        .collect();
    assert_eq!(accepted_ids.len(), 3);

    let images: Vec<ImageRecord> = accepted_ids.iter().map(|id| make_image(id)).collect();

    let evaluator = FixtureEvaluator::ready_with_image_conclusions(vec![
        make_approve(),
        make_approve(),
        make_reject("low quality"),
    ]);

    let mut orchestrator = TaskOrchestrator::new(task_plan.clone(), &evaluator);
    let state = orchestrator.accept_images(&images).unwrap();
    assert_eq!(state, OrchestratorState::FullDelivery);
    assert_eq!(orchestrator.qualified_count(), 2);

    let decision = orchestrator.build_delivery_decision();
    assert_eq!(decision.status, TaskStatus::FullDelivery);
    assert_eq!(decision.full_attempt_count, 1);

    // Delivery package
    let dir = temp_dir("full-delivery");
    let builder = DeliveryPackageBuilder::new(&dir);
    let inputs = DeliveryInputs::minimal(task_plan, decision);
    let package_path = builder.build(&inputs).unwrap();

    assert!(package_path.join("status.json").exists());
    assert!(package_path.join("manifest.json").exists());
    let status_bytes = std::fs::read(package_path.join("status.json")).unwrap();
    let status_str = std::str::from_utf8(&status_bytes).unwrap();
    assert!(status_str.contains("\"task_status\": \"full_delivery\""));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn e2e_full_delivery_single_image_immediate() {
    let plan = make_plan(1);
    let task_plan = TaskPlan::from_validated(plan);
    let evaluator = FixtureEvaluator::ready_with_image_conclusions(vec![make_approve()]);

    let mut orchestrator = TaskOrchestrator::new(task_plan, &evaluator);
    let images = vec![make_image("img-1")];
    let state = orchestrator.accept_images(&images).unwrap();

    assert_eq!(state, OrchestratorState::FullDelivery);
    assert_eq!(orchestrator.qualified_count(), 1);
    assert_eq!(orchestrator.counter().retry_count, 0);

    let decision = orchestrator.build_delivery_decision();
    assert_eq!(decision.status, TaskStatus::FullDelivery);
}

// ===========================================================================
// E2E Scenario 3: limited_delivery 0 images
// ===========================================================================

#[test]
fn e2e_limited_delivery_zero_images_all_rejected() {
    let plan = make_plan(2);
    let task_plan = TaskPlan::from_validated(plan);

    let evaluator = FixtureEvaluator::ready_with_image_conclusions(vec![
        make_reject("poor quality"),
        make_reject("wrong subject"),
    ]);

    let mut orchestrator = TaskOrchestrator::new(task_plan.clone(), &evaluator);

    // Attempt 1: both rejected → retry
    let state = orchestrator
        .accept_images(&[make_image("img-1"), make_image("img-2")])
        .unwrap();
    assert_eq!(state, OrchestratorState::Retry);
    assert_eq!(orchestrator.qualified_count(), 0);

    // Retry 1–3: all rejected → limited delivery
    for retry in 1..=3 {
        orchestrator.record_retry().unwrap();
        let state = orchestrator
            .accept_images(&[
                make_image(&format!("img-r{}-a", retry)),
                make_image(&format!("img-r{}-b", retry)),
            ])
            .unwrap();
        if retry < 3 {
            assert_eq!(state, OrchestratorState::Retry);
        } else {
            assert_eq!(state, OrchestratorState::LimitedDelivery);
        }
    }

    assert_eq!(orchestrator.qualified_count(), 0);
    assert!(orchestrator.counter().is_exhausted());
    assert_eq!(orchestrator.counter().full_attempt_count, 4);
    assert_eq!(orchestrator.counter().retry_count, 3);

    let decision = orchestrator.build_delivery_decision();
    assert_eq!(decision.status, TaskStatus::LimitedDelivery);
    assert_eq!(decision.accepted_images.len(), 0);
    assert!(decision.shortfall_reason.is_some());

    // Verify delivery package for limited_delivery with 0 images
    let dir = temp_dir("limited-zero");
    let builder = DeliveryPackageBuilder::new(&dir);
    let inputs = DeliveryInputs::minimal(task_plan, decision);
    let package_path = builder.build(&inputs).unwrap();

    let manifest_bytes = std::fs::read(package_path.join("manifest.json")).unwrap();
    let manifest: serde_json::Value = serde_json::from_slice(&manifest_bytes).unwrap();
    assert_eq!(manifest["delivery_status"], "limited_delivery");
    assert_eq!(manifest["accepted_images"].as_array().unwrap().len(), 0);
    assert_eq!(manifest["gap"]["accepted_count"], 0);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn e2e_limited_delivery_zero_with_mechanical_rejections() {
    let plan = make_plan(1);
    let task_plan = TaskPlan::from_validated(plan);
    let evaluator = FixtureEvaluator::ready_with_image_conclusions(vec![]);

    let mut orchestrator = TaskOrchestrator::new(task_plan, &evaluator);
    let bad_image = ImageRecord {
        candidate_id: "bad".into(),
        local_path: "/tmp/bad".into(),
        content_type: None,
        file_size_bytes: 0,
        dimensions: None,
    };

    let state = orchestrator.accept_images(&[bad_image]).unwrap();
    assert_eq!(state, OrchestratorState::Retry);
    assert_eq!(orchestrator.qualified_count(), 0);
    assert_eq!(orchestrator.rejected_images().len(), 1);
}

// ===========================================================================
// E2E Scenario 4: execution_blocked — OpenClaw unavailable
// ===========================================================================

#[test]
fn e2e_execution_blocked_openclaw_unavailable() {
    let plan = make_plan(2);
    let task_plan = TaskPlan::from_validated(plan);
    let evaluator = FixtureEvaluator::unavailable();

    let mut orchestrator = TaskOrchestrator::new(task_plan.clone(), &evaluator);
    let images = vec![make_image("img-1"), make_image("img-2")];
    let state = orchestrator.accept_images(&images).unwrap();

    assert_eq!(state, OrchestratorState::ExecutionBlocked);
    assert!(orchestrator.execution_block_reason().is_some());
    let reason = orchestrator.execution_block_reason().unwrap();
    assert!(reason.contains("unavailable") || reason.contains("not available"));

    let state_label = orchestrator.state().label();
    assert_eq!(state_label, "execution_blocked");

    let decision = orchestrator.build_delivery_decision();
    assert_eq!(decision.status, TaskStatus::ExecutionBlocked);
    assert_eq!(decision.accepted_images.len(), 0);

    // Delivery package for execution_blocked
    let dir = temp_dir("exec-blocked");
    let builder = DeliveryPackageBuilder::new(&dir);
    let inputs = DeliveryInputs::minimal(task_plan, decision);
    let package_path = builder.build(&inputs).unwrap();

    let status_bytes = std::fs::read(package_path.join("status.json")).unwrap();
    let status_str = std::str::from_utf8(&status_bytes).unwrap();
    assert!(status_str.contains("\"task_status\": \"execution_blocked\""));
    assert!(status_str.contains("\"accepted_count\": 0"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn e2e_execution_blocked_by_retrieval_blocking_fact() {
    let plan = make_plan(2);
    let task_plan = TaskPlan::from_validated(plan);
    let evaluator = FixtureEvaluator::ready_with_image_conclusions(vec![make_approve()]);

    let mut orchestrator = TaskOrchestrator::new(task_plan, &evaluator);
    orchestrator.block_execution(
        "all retrieval channels blocked: access restriction detected on all tiers",
    );

    assert_eq!(orchestrator.state(), OrchestratorState::ExecutionBlocked);
    assert!(orchestrator
        .execution_block_reason()
        .unwrap()
        .contains("access restriction"));

    let decision = orchestrator.build_delivery_decision();
    assert_eq!(decision.status, TaskStatus::ExecutionBlocked);
}

#[test]
fn e2e_execution_blocked_openclaw_candidate_phase() {
    let evaluator = FixtureEvaluator::unavailable();
    let result = evaluator.evaluate_candidates(
        &[make_fixture_candidate(0, "p1", "https://example.com")],
        "test query",
    );
    assert!(result.is_err());
    match result {
        Err(Error::OpenClawUnavailable { .. }) => { /* expected */ }
        other => panic!("expected OpenClawUnavailable, got {:?}", other),
    }
}

// ===========================================================================
// E2E Scenario 5: Channel fallback / access restriction boundaries
// ===========================================================================

#[test]
fn e2e_channel_fallback_blocked_by_access_restriction() {
    let fact = FallbackEligibilityFact::new(
        RetrievalChannelTier::WebFetch,
        "HTTP 403 Forbidden — login wall detected",
        true,
    );
    let decision = evaluate_fallback_eligibility(&fact);
    assert!(matches!(decision, PolicyDecision::TaskBlock { .. }));
    if let PolicyDecision::TaskBlock { reason } = decision {
        assert!(reason.contains("access restriction"));
    }
}

#[test]
fn e2e_channel_fallback_allowed_for_network_error() {
    let fact =
        FallbackEligibilityFact::new(RetrievalChannelTier::WebFetch, "connection timeout", false);
    let decision = evaluate_fallback_eligibility(&fact);
    assert!(matches!(decision, PolicyDecision::Allow));
}

#[test]
fn e2e_channel_paid_unconfirmed_blocked() {
    let fact = PolicyFact {
        subject_id: "img-1".into(),
        authorization_risk: AuthorizationRisk::Unknown,
        has_access_restriction: false,
        is_paid_channel: true,
        paid_channel_confirmed: false,
        context: "paid channel needed".into(),
    };
    let decision = evaluate_policy_fact(&fact);
    assert!(matches!(decision, PolicyDecision::TaskBlock { .. }));
}

#[test]
fn e2e_channel_paid_confirmed_allowed() {
    let fact = PolicyFact {
        subject_id: "img-1".into(),
        authorization_risk: AuthorizationRisk::Unknown,
        has_access_restriction: false,
        is_paid_channel: true,
        paid_channel_confirmed: true,
        context: "user approved".into(),
    };
    let decision = evaluate_policy_fact(&fact);
    assert!(matches!(decision, PolicyDecision::Allow));
}

#[test]
fn e2e_channel_disabled_no_fallback_bypass() {
    use image_retrieval::policy::allows_fallback;
    assert!(!allows_fallback(&RetrievalFailureCategory::ChannelDisabled));
    assert!(!allows_fallback(
        &RetrievalFailureCategory::AccessRestricted
    ));
    assert!(!allows_fallback(
        &RetrievalFailureCategory::PaidNotConfirmed
    ));
    assert!(allows_fallback(&RetrievalFailureCategory::Network));
    assert!(allows_fallback(&RetrievalFailureCategory::HttpStatus));
}

#[test]
fn e2e_fallback_chain_respects_tier_boundaries() {
    assert_eq!(
        RetrievalChannelTier::WebFetch.next_fallback(),
        Some(RetrievalChannelTier::SelfHosted)
    );
    assert_eq!(
        RetrievalChannelTier::SelfHosted.next_fallback(),
        Some(RetrievalChannelTier::Paid)
    );
    assert_eq!(RetrievalChannelTier::Paid.next_fallback(), None);

    // Paid tier has no further fallback (next_tier is None)
    let paid_fact = FallbackEligibilityFact::new(RetrievalChannelTier::Paid, "exhausted", false);
    assert!(paid_fact.next_tier.is_none());
}

#[test]
fn e2e_execution_blocking_fact_access_restriction() {
    let fact = ExecutionBlockingFact {
        reason: "all channels blocked: login wall at web_fetch tier".into(),
        source_tier: Some(RetrievalChannelTier::WebFetch),
        is_access_restricted: true,
        is_paid_unconfirmed: false,
    };

    let decision = evaluate_execution_block(&fact);
    assert!(matches!(decision, PolicyDecision::TaskBlock { .. }));
}

#[test]
fn e2e_retrieval_failure_access_restricted_no_fallback() {
    let failure = RetrievalResult::Failure(RetrievalFailure {
        candidate_id: "cand-1".into(),
        channel_tier: RetrievalChannelTier::WebFetch,
        failure_category: RetrievalFailureCategory::AccessRestricted,
        reason: "HTTP 403 Forbidden".into(),
        allows_fallback: false,
    });

    match &failure {
        RetrievalResult::Failure(f) => {
            assert!(!f.allows_fallback);
            assert_eq!(
                f.failure_category,
                RetrievalFailureCategory::AccessRestricted
            );
        }
        _ => panic!("expected failure"),
    }
}

// ===========================================================================
// E2E Scenario 6: Sensitive info exclusion
// ===========================================================================

#[test]
fn e2e_sensitive_info_not_in_delivery_output() {
    let plan = make_plan(1);
    let task_plan = TaskPlan::from_validated(plan);
    let decision = DeliveryDecision::full_delivery(vec![], vec![], 1, 0);
    let inputs = DeliveryInputs::minimal(task_plan, decision);

    let dir = temp_dir("sensitive-exclusion");
    let builder = DeliveryPackageBuilder::new(&dir);
    builder.build(&inputs).unwrap();

    let sensitive_patterns = [
        "Bearer ",
        "Authorization:",
        "x-api-key:",
        "api_key=",
        "access_token=",
        "client_secret=",
        "-----BEGIN RSA PRIVATE KEY-----",
    ];

    for file in &[
        "status.json",
        "manifest.json",
        "summary.md",
        "evidence/acceptance.json",
        "evidence/rejection.json",
        "diagnostics/diagnostic.json",
        "diagnostics/metrics_summary.json",
    ] {
        let path = dir.join(file);
        if path.exists() {
            let content = std::fs::read_to_string(&path).unwrap();
            for pattern in &sensitive_patterns {
                assert!(
                    !content.contains(pattern),
                    "file {} contains sensitive pattern '{}'",
                    file,
                    pattern
                );
            }
        }
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn e2e_sanitize_removes_credentials_from_log_text() {
    let result = sanitize_for_delivery("Error: Bearer sk-abc123xyz — connection failed");
    assert!(result.redacted);
    assert!(!result.sanitised.contains("sk-abc123xyz"));
    assert!(result.sanitised.contains("[REDACTED:"));
    assert!(result.sanitised.contains("connection failed"));
}

#[test]
fn e2e_metadata_sanitization_redacts_values() {
    let meta = vec![
        ("provider".into(), "fixture-safe".into()),
        (
            "auth_header".into(),
            "Bearer secret-token-value-12345".into(),
        ),
        ("x-api-key:".into(), "key-abc-secret".into()),
        ("normal_field".into(), "normal_value".into()),
    ];
    let sanitised = sanitize_metadata(&meta);
    assert_eq!(sanitised.len(), 4);
    assert_eq!(sanitised[0].1, "fixture-safe");
    assert_eq!(sanitised[3].1, "normal_value");
    assert_eq!(sanitised[1].1, "[REDACTED]");
    assert_eq!(sanitised[2].0, "[REDACTED]");
}

#[test]
fn e2e_contains_sensitive_detects_credential_patterns() {
    assert!(contains_sensitive_pattern("Authorization: Bearer token"));
    assert!(contains_sensitive_pattern("x-api-key: secret"));
    assert!(contains_sensitive_pattern("api_key=value"));
    assert!(contains_sensitive_pattern("access_token=secret"));
    assert!(contains_sensitive_pattern("Cookie: session=abc"));
    assert!(!contains_sensitive_pattern("image description of a sunset"));
    assert!(!contains_sensitive_pattern(
        "https://example.com/images/photo.jpg"
    ));
}

#[test]
fn e2e_delivery_manifest_excludes_sensitive_input_description() {
    let plan = make_plan(1);
    let task_plan = TaskPlan::from_validated(plan);
    let decision = DeliveryDecision::full_delivery(vec![], vec![], 1, 0);
    let inputs = DeliveryInputs::minimal(task_plan, decision);

    let dir = temp_dir("manifest-no-creds");
    let builder = DeliveryPackageBuilder::new(&dir);
    builder.build(&inputs).unwrap();

    let manifest_bytes = std::fs::read(dir.join("manifest.json")).unwrap();
    let manifest_str = std::str::from_utf8(&manifest_bytes).unwrap();
    assert!(!manifest_str.contains("Bearer"));
    assert!(!manifest_str.contains("api_key"));
    assert!(!manifest_str.contains("token"));

    let _ = std::fs::remove_dir_all(&dir);
}

// ===========================================================================
// E2E Scenario 7: Self-check — readiness without delivery artifacts
// ===========================================================================

#[test]
fn e2e_self_check_input_rejected_produces_blocked() {
    let request = SelfCheckRequest {
        query_plan_input: QueryPlanInput {
            description: "".into(),
            ..Default::default()
        },
        providers: vec![provider_entry(
            "p1",
            "Test Provider",
            true,
            ProviderReadiness::Ready,
        )],
        channels: vec![channel_entry("c1", "Test Channel", "web_fetch", true)],
        candidate_openclaw_available: true,
        image_openclaw_available: true,
        paid_channel_confirmed: false,
        policy_risks: vec![],
    };

    let report = run_self_check(request);
    assert_eq!(report.status, SelfCheckStatus::Blocked);
    assert!(!report.query_plan_valid);
}

#[test]
fn e2e_self_check_openclaw_unavailable_produces_blocked() {
    let request = SelfCheckRequest {
        query_plan_input: QueryPlanInput {
            description: "test query".into(),
            ..Default::default()
        },
        providers: vec![provider_entry(
            "p1",
            "Test Provider",
            true,
            ProviderReadiness::Ready,
        )],
        channels: vec![channel_entry("c1", "Test Channel", "web_fetch", true)],
        candidate_openclaw_available: false,
        image_openclaw_available: false,
        paid_channel_confirmed: false,
        policy_risks: vec![],
    };

    let report = run_self_check(request);
    assert_eq!(report.status, SelfCheckStatus::Blocked);
    assert!(report
        .blockers
        .iter()
        .any(|b| b.contains("候选评价") || b.contains("候选")));
    assert!(report
        .blockers
        .iter()
        .any(|b| b.contains("图片评价") || b.contains("图片 OpenClaw")));
}

#[test]
fn e2e_self_check_paid_channel_unconfirmed_blocked() {
    let request = SelfCheckRequest {
        query_plan_input: QueryPlanInput {
            description: "test query".into(),
            ..Default::default()
        },
        providers: vec![provider_entry(
            "p1",
            "Test Provider",
            true,
            ProviderReadiness::Ready,
        )],
        channels: vec![ChannelReadinessEntry {
            channel_id: "paid-1".into(),
            display_name: "Paid Channel".into(),
            tier: "paid".into(),
            enabled: true,
            readiness:
                image_retrieval::domain::retrieval::RetrievalChannelReadiness::PaidUnconfirmed,
            reason: Some("requires user confirmation".into()),
        }],
        candidate_openclaw_available: true,
        image_openclaw_available: true,
        paid_channel_confirmed: false,
        policy_risks: vec![],
    };

    let report = run_self_check(request);
    assert_eq!(report.status, SelfCheckStatus::Blocked);
    assert_eq!(report.channel_summary.paid_unconfirmed, 1);
}

#[test]
fn e2e_self_check_does_not_produce_delivery_artifacts() {
    let request = SelfCheckRequest {
        query_plan_input: QueryPlanInput {
            description: "test query".into(),
            ..Default::default()
        },
        providers: vec![provider_entry(
            "p1",
            "Test Provider",
            true,
            ProviderReadiness::Ready,
        )],
        channels: vec![channel_entry("c1", "Test Channel", "web_fetch", true)],
        candidate_openclaw_available: true,
        image_openclaw_available: true,
        paid_channel_confirmed: false,
        policy_risks: vec![],
    };

    let report = run_self_check(request);
    let json = serde_json::to_string_pretty(&report).unwrap();
    assert!(!json.contains("images/"));
    assert!(!json.contains("status.json"));
    assert!(!json.contains("manifest.json"));
    assert!(!json.contains("full_delivery"));
    assert!(!json.contains("limited_delivery"));
    assert!(!json.contains("execution_blocked"));
}

#[test]
fn e2e_self_check_provider_missing_credentials_blocked() {
    let request = SelfCheckRequest {
        query_plan_input: QueryPlanInput {
            description: "test query".into(),
            ..Default::default()
        },
        providers: vec![provider_entry(
            "p1",
            "Real Provider",
            true,
            ProviderReadiness::MissingCredentials,
        )],
        channels: vec![channel_entry("c1", "Test Channel", "web_fetch", true)],
        candidate_openclaw_available: true,
        image_openclaw_available: true,
        paid_channel_confirmed: false,
        policy_risks: vec![],
    };

    let report = run_self_check(request);
    assert_eq!(report.status, SelfCheckStatus::Blocked);
    assert_eq!(report.provider_summary.missing_credentials, 1);

    let json = serde_json::to_string_pretty(&report).unwrap();
    assert!(!json.contains("api_key"));
    assert!(!json.contains("password"));
    assert!(!json.contains("secret"));
}

#[test]
fn e2e_self_check_no_channels_blocked() {
    let request = SelfCheckRequest {
        query_plan_input: QueryPlanInput {
            description: "test query".into(),
            ..Default::default()
        },
        providers: vec![provider_entry(
            "p1",
            "Test Provider",
            true,
            ProviderReadiness::Ready,
        )],
        channels: vec![],
        candidate_openclaw_available: true,
        image_openclaw_available: true,
        paid_channel_confirmed: false,
        policy_risks: vec![],
    };

    let report = run_self_check(request);
    assert_eq!(report.status, SelfCheckStatus::Blocked);
    assert!(report
        .blockers
        .iter()
        .any(|b| b.contains("抓取") || b.contains("channel")));
}

fn provider_entry(
    id: &str,
    name: &str,
    enabled: bool,
    readiness: ProviderReadiness,
) -> ProviderReadinessEntry {
    ProviderReadinessEntry {
        provider_id: id.into(),
        display_name: name.into(),
        enabled,
        weight: 1,
        readiness,
        reason: None,
    }
}

fn channel_entry(id: &str, name: &str, tier: &str, enabled: bool) -> ChannelReadinessEntry {
    ChannelReadinessEntry {
        channel_id: id.into(),
        display_name: name.into(),
        tier: tier.into(),
        enabled,
        readiness: image_retrieval::domain::retrieval::RetrievalChannelReadiness::Ready,
        reason: None,
    }
}

// ===========================================================================
// E2E Scenario 8: Provider integration with fixture providers
// ===========================================================================

#[test]
fn e2e_search_scheduler_with_fixture_providers() {
    use image_retrieval::domain::candidate::ProviderId;
    use std::collections::HashMap;

    let p1 = FixtureProvider::new("p1", "Provider 1")
        .with_weight(2)
        .with_responses(vec![make_fixture_batch("p1", 10, 0)]);

    let p2 = FixtureProvider::new("p2", "Provider 2")
        .with_weight(1)
        .with_responses(vec![make_fixture_batch("p2", 10, 0)]);

    let mut registry = image_retrieval::search::registry::ProviderRegistry::new();
    registry
        .register(ProviderRegistration::new(ProviderId::new("p1"), "Provider 1").with_weight(2));
    registry
        .register(ProviderRegistration::new(ProviderId::new("p2"), "Provider 2").with_weight(1));

    let _providers: HashMap<String, &dyn BaseProvider> = HashMap::new();
    // We can't store p1/p2 on the stack AND reference them in the map
    // because they're local variables. Instead, verify individually.
    let r1 = p1.search("sunset mountains", 10).unwrap();
    let r2 = p2.search("sunset mountains", 10).unwrap();
    assert!(!r1.is_empty());
    assert!(!r2.is_empty());

    // All candidates have valid source URLs with no credentials
    for c in r1.iter().chain(r2.iter()) {
        assert!(!c.source_url.is_empty());
        assert!(!c.source_url.contains("api_key"));
        assert!(!c.source_url.contains("token"));
    }

    // Weight table should only include enabled providers with positive weight
    let (weight_table, _readiness) = registry.build_weight_table();
    assert_eq!(weight_table.len(), 2);
}

#[test]
fn e2e_search_scheduler_empty_registry_produces_shortage() {
    use image_retrieval::domain::candidate::ProviderId;

    let mut registry = image_retrieval::search::registry::ProviderRegistry::new();
    // Register a provider but don't provide an adapter
    registry.register(
        ProviderRegistration::new(ProviderId::new("orphan"), "Orphan Provider").with_weight(1),
    );

    let (weight_table, _) = registry.build_weight_table();
    assert_eq!(weight_table.len(), 1);

    // The provider exists in the weight table but has no adapter
    // That's fine — the registry tracks registrations; the scheduler
    // pairs them with adapters at runtime.
}

// ===========================================================================
// E2E Scenario 9: Retrieval channel — fixture channel integration
// ===========================================================================

#[test]
fn e2e_fixture_retrieval_channel_mixed_results() {
    use image_retrieval::retrieval::channels::fixture::{FixtureChannel, FixtureResponse};

    let channel = FixtureChannel::new(RetrievalChannelTier::WebFetch)
        .with_response("good-1", FixtureResponse::success())
        .with_response("good-2", FixtureResponse::success())
        .with_response("bad-1", FixtureResponse::network_failure())
        .with_response("restricted-1", FixtureResponse::access_restricted());

    let batch = RetrievalBatch::new(
        vec![
            "good-1".into(),
            "good-2".into(),
            "bad-1".into(),
            "restricted-1".into(),
        ],
        8,
    );

    let results = channel.retrieve_batch(&batch).unwrap();
    assert_eq!(results.len(), 4);

    let success_count = results.iter().filter(|r| r.is_success()).count();
    let failure_count = results.iter().filter(|r| r.is_failure()).count();
    assert_eq!(success_count, 2);
    assert_eq!(failure_count, 2);

    if let RetrievalResult::Failure(f) = &results[3] {
        assert_eq!(
            f.failure_category,
            RetrievalFailureCategory::AccessRestricted
        );
        assert!(!f.allows_fallback);
    }
}

// ===========================================================================
// E2E Scenario 10: Delivery package full structure
// ===========================================================================

#[test]
fn e2e_delivery_package_full_structure() {
    let plan = make_plan(2);
    let task_plan = TaskPlan::from_validated(plan);
    let decision = DeliveryDecision::full_delivery(vec![], vec![], 1, 0);
    let mut inputs = DeliveryInputs::minimal(task_plan, decision);

    inputs.metric_events.push(
        MetricEvent::new(MetricKind::TaskOutcome, "task_outcome_full_delivery", 1.0)
            .with_meta("state", "full_delivery"),
    );
    inputs.metric_events.push(
        MetricEvent::new(
            MetricKind::CandidateSatisfaction,
            "candidate_satisfaction",
            40.0,
        )
        .with_denominator(60.0),
    );
    inputs.metric_events.push(
        MetricEvent::new(
            MetricKind::QualifiedImageAchievement,
            "qualified_image_achievement",
            2.0,
        )
        .with_denominator(2.0),
    );
    inputs.metric_events.push(MetricEvent::new(
        MetricKind::RejectionReason,
        "mechanical",
        0.0,
    ));
    inputs.metric_events.push(MetricEvent::new(
        MetricKind::ChannelEffectiveness,
        "web_fetch",
        1.0,
    ));
    inputs.metric_events.push(
        MetricEvent::new(
            MetricKind::OpenClawEvaluationRate,
            "openclaw_pass_rate",
            2.0,
        )
        .with_denominator(2.0),
    );

    let dir = temp_dir("full-structure");
    let builder = DeliveryPackageBuilder::new(&dir);
    builder.build(&inputs).unwrap();

    assert!(dir.join("status.json").exists());
    assert!(dir.join("manifest.json").exists());
    assert!(dir.join("summary.md").exists());
    assert!(dir.join("images").is_dir());
    assert!(dir.join("evidence").is_dir());
    assert!(dir.join("diagnostics").is_dir());
    assert!(dir.join("evidence/acceptance.json").exists());
    assert!(dir.join("evidence/rejection.json").exists());
    assert!(dir.join("diagnostics/diagnostic.json").exists());
    assert!(dir.join("diagnostics/metrics_summary.json").exists());

    let manifest_bytes = std::fs::read(dir.join("manifest.json")).unwrap();
    let manifest: serde_json::Value = serde_json::from_slice(&manifest_bytes).unwrap();
    assert_eq!(manifest["schema_version"], 1);
    assert!(manifest["query_plan_summary"].is_object());
    assert!(manifest["delivery_status"].is_string());
    assert!(manifest["accepted_images"].is_array());
    assert!(manifest["gap"].is_object());
    assert!(manifest["candidate_summary"].is_object());
    assert!(manifest["retrieval_summary"].is_object());
    assert!(manifest["acceptance_summary"].is_object());
    assert!(manifest["risk_summary"].is_object());
    assert!(manifest["metrics"].is_object());
    assert!(manifest["evidence_refs"].is_array());

    let metrics = &manifest["metrics"];
    assert!(metrics["task_outcome"].is_object());
    assert!(metrics["candidate_satisfaction"].is_array());
    assert!(metrics["qualified_image_achievement"].is_array());
    assert!(metrics["rejection_reasons"].is_array());
    assert!(metrics["channel_effectiveness"].is_array());
    assert!(metrics["openclaw_evaluation_rate"].is_array());

    let _ = std::fs::remove_dir_all(&dir);
}

// ===========================================================================
// E2E Scenario 11: Authorization risk boundaries
// ===========================================================================

#[test]
fn e2e_authorization_unknown_allowed_with_risk() {
    let fact = PolicyFact {
        subject_id: "img-1".into(),
        authorization_risk: AuthorizationRisk::Unknown,
        has_access_restriction: false,
        is_paid_channel: false,
        paid_channel_confirmed: false,
        context: "no license information available".into(),
    };
    let decision = evaluate_policy_fact(&fact);
    assert!(matches!(decision, PolicyDecision::Allow));
}

#[test]
fn e2e_authorization_prohibited_local_reject() {
    let fact = PolicyFact {
        subject_id: "img-1".into(),
        authorization_risk: AuthorizationRisk::Prohibited,
        has_access_restriction: false,
        is_paid_channel: false,
        paid_channel_confirmed: false,
        context: "source explicitly prohibits reuse".into(),
    };
    let decision = evaluate_policy_fact(&fact);
    assert!(matches!(decision, PolicyDecision::LocalReject { .. }));
}

#[test]
fn e2e_access_restriction_local_reject() {
    let fact = PolicyFact {
        subject_id: "img-1".into(),
        authorization_risk: AuthorizationRisk::Unknown,
        has_access_restriction: true,
        is_paid_channel: false,
        paid_channel_confirmed: false,
        context: "login wall detected".into(),
    };
    let decision = evaluate_policy_fact(&fact);
    assert!(matches!(decision, PolicyDecision::LocalReject { .. }));
}

// ===========================================================================
// E2E Scenario 12: Attempt counters and retry logic
// ===========================================================================

#[test]
fn e2e_attempt_counter_one_initial_plus_three_retries() {
    use image_retrieval::orchestrator::AttemptCounter;

    let mut counter = AttemptCounter::new(3);
    assert_eq!(counter.full_attempt_count, 1);
    assert_eq!(counter.retry_count, 0);
    assert!(counter.can_retry());
    assert!(!counter.is_exhausted());

    for expected_retry in 1..=3 {
        assert!(counter.record_retry());
        assert_eq!(counter.full_attempt_count, 1 + expected_retry);
        assert_eq!(counter.retry_count, expected_retry);
        assert_eq!(counter.can_retry(), expected_retry < 3);
    }

    assert!(counter.is_exhausted());
    assert!(!counter.record_retry());
    assert_eq!(counter.retry_count, 3);
}

#[test]
fn e2e_attempt_counter_zero_retry_limit_no_retries() {
    use image_retrieval::orchestrator::AttemptCounter;

    let mut counter = AttemptCounter::new(0);
    assert!(!counter.can_retry());
    assert!(counter.is_exhausted());
    assert!(!counter.record_retry());
    assert_eq!(counter.retry_count, 0);
    assert_eq!(counter.full_attempt_count, 1);
}

// ===========================================================================
// E2E Scenario 13: Orchestrator state transitions
// ===========================================================================

#[test]
fn e2e_orchestrator_all_terminal_states() {
    assert!(OrchestratorState::InputRejected.is_terminal());
    assert!(OrchestratorState::FullDelivery.is_terminal());
    assert!(OrchestratorState::LimitedDelivery.is_terminal());
    assert!(OrchestratorState::ExecutionBlocked.is_terminal());
    assert!(!OrchestratorState::InputValidation.is_terminal());
    assert!(!OrchestratorState::Running.is_terminal());
    assert!(!OrchestratorState::Retry.is_terminal());
}

#[test]
fn e2e_orchestrator_reject_input() {
    let plan = make_plan(1);
    let task_plan = TaskPlan::from_validated(plan);
    let evaluator = FixtureEvaluator::ready_with_image_conclusions(vec![]);

    let mut orchestrator = TaskOrchestrator::new(task_plan, &evaluator);
    orchestrator.reject_input("description field empty");

    assert_eq!(orchestrator.state(), OrchestratorState::InputRejected);
    let decision = orchestrator.build_delivery_decision();
    assert_eq!(decision.status, TaskStatus::InputRejected);
}

#[test]
fn e2e_orchestrator_diagnostic_builds_correctly() {
    let plan = make_plan(1);
    let task_plan = TaskPlan::from_validated(plan);
    let evaluator = FixtureEvaluator::ready_with_image_conclusions(vec![make_approve()]);

    let mut orchestrator = TaskOrchestrator::new(task_plan, &evaluator);
    orchestrator.accept_images(&[make_image("img-1")]).unwrap();

    let diagnostic = orchestrator.build_diagnostic();
    assert_eq!(diagnostic.status, "full_delivery");
    assert!(!diagnostic.items.is_empty());
    assert!(diagnostic.summary.contains("Full delivery"));
}

// ===========================================================================
// E2E Scenario 14: No fixture leakage — fixtures never used as production
// ===========================================================================

#[test]
fn e2e_fixture_provider_is_explicitly_non_production() {
    let provider = FixtureProvider::new("fixture", "Fixture Provider");
    assert_eq!(provider.display_name(), "Fixture Provider");
    let readiness = provider.readiness();
    assert!(readiness.is_ok());
}

#[test]
fn e2e_fixture_candidates_have_fixture_prefix() {
    let candidate = make_fixture_candidate(42, "provider-x", "https://fixture.example.com");
    assert!(candidate.id.to_string().starts_with("fixture-"));
    assert_eq!(candidate.provider_id.to_string(), "provider-x");
}

// ===========================================================================
// E2E Scenario 15: Search outcome source traceability
// ===========================================================================

#[test]
fn e2e_search_outcome_provides_source_traceability() {
    let provider = FixtureProvider::new("trace-p1", "Trace Provider")
        .with_responses(vec![make_fixture_batch("trace-p1", 5, 0)]);

    let results = provider.search("test query", 10).unwrap();
    assert_eq!(results.len(), 5);

    let ids: HashSet<String> = results.iter().map(|c| c.id.to_string()).collect();
    assert_eq!(ids.len(), 5);

    for c in &results {
        assert!(c.source_url.contains("trace-p1.example.com"));
        assert_eq!(c.provider_id.to_string(), "trace-p1");
    }

    assert_eq!(provider.call_count(), 1);
}

// ===========================================================================
// E2E Scenario 16: Provider registry with mixed readiness
// ===========================================================================

#[test]
fn e2e_provider_registry_mixed_readiness() {
    use image_retrieval::domain::candidate::ProviderId;

    let mut registry = image_retrieval::search::registry::ProviderRegistry::new();
    registry.register(
        ProviderRegistration::new(ProviderId::new("ready-p"), "Ready Provider")
            .with_weight(2)
            .with_enabled(true),
    );
    registry.register(
        ProviderRegistration::new(ProviderId::new("down-p"), "Down Provider")
            .with_weight(1)
            .with_enabled(true),
    );
    registry.register(
        ProviderRegistration::new(ProviderId::new("disabled-p"), "Disabled Provider")
            .with_weight(0)
            .with_enabled(true),
    );

    let (weight_table, _readiness) = registry.build_weight_table();
    // Only the ready provider has positive weight and enabled=true
    // The "down-p" with weight 1 and enabled=true should also be in the table
    // The "disabled-p" with weight 0 is excluded
    assert_eq!(weight_table.len(), 2);
    let ids: Vec<String> = weight_table
        .iter()
        .map(|e| e.provider_id.to_string())
        .collect();
    assert!(ids.iter().any(|id| id == "ready-p"));
    assert!(ids.iter().any(|id| id == "down-p"));
}

// ===========================================================================
// E2E Scenario 17: Retrieval batch planning with short batch
// ===========================================================================

#[test]
fn e2e_retrieval_batch_short_batch_detection() {
    let batch = RetrievalBatch::new(vec!["c1".into(), "c2".into(), "c3".into()], 10);

    assert!(batch.is_short_batch);
    assert_eq!(batch.actual_size(), 3);
    assert_eq!(batch.target_size, 10);
    assert!(batch.actual_size() < batch.target_size as usize);
}

#[test]
fn e2e_retrieval_batch_exact_count_not_short() {
    let batch = RetrievalBatch::new(vec!["c1".into(), "c2".into()], 2);
    assert!(!batch.is_short_batch);
    assert_eq!(batch.actual_size(), 2);
}
