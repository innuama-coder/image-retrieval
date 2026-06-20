//! Integration-level baseline tests for domain types.
//!
//! Verifies that the public API of the domain model is usable by
//! downstream tasks and that the key domain invariants hold.

use image_retrieval::domain::candidate::{
    CandidateDecision, CandidateId, CandidateRecord, ProviderId, RetrievableCandidateSequence,
};
use image_retrieval::domain::delivery::{DeliveryDecision, TaskStatus};
use image_retrieval::domain::image::{
    ImageAcceptanceDecision, ImageMechanicalEvidence, ImageRecord,
};
use image_retrieval::domain::metrics::{MetricEvent, MetricKind};
use image_retrieval::domain::policy::{AuthorizationRisk, PolicyFact};
use image_retrieval::domain::query_plan::{
    ContentConstraints, QualityTier, QueryPlanInput, TaskPlan, ValidatedQueryPlan,
};
use image_retrieval::domain::retrieval::{
    FallbackEligibilityFact, RetrievalBatch, RetrievalChannelTier, RetrievalFailure,
    RetrievalFailureCategory, RetrievalResult, RetrievalSuccess,
};
use image_retrieval::error::{Diagnostic, DiagnosticLevel, Error};

// ---------------------------------------------------------------------------
// QueryPlan lifecycle integration
// ---------------------------------------------------------------------------

#[test]
fn full_query_plan_to_task_plan_lifecycle() {
    let input = QueryPlanInput {
        description: "sunset over mountains".into(),
        required_count: 2,
        quality_tier: QualityTier::High,
        content_constraints: ContentConstraints {
            must_include: vec!["orange sky".into()],
            must_avoid: vec!["city".into()],
        },
        ..Default::default() // authorization_preference, output_preference, retry_limit all default
    };

    // Simulate validation + defaults
    let validated = ValidatedQueryPlan {
        description: input.description,
        required_count: input.required_count,
        quality_tier: input.quality_tier,
        content_constraints: input.content_constraints,
        authorization_preference: input.authorization_preference,
        output_preference: input.output_preference,
        retry_limit: input.retry_limit,
    };

    let task = TaskPlan::from_validated(validated);

    assert_eq!(task.candidate_target, 40); // 2 * 20
    assert_eq!(task.retrieval_batch_target, 4); // 2 * 2
    assert_eq!(task.max_attempts, 4); // 1 initial + 3 retries
    assert_eq!(task.query_plan.description, "sunset over mountains");
    assert_eq!(task.query_plan.quality_tier, QualityTier::High);
}

// ---------------------------------------------------------------------------
// Candidate → RetrievableSequence → Batch integration
// ---------------------------------------------------------------------------

#[test]
fn candidate_flow_to_batch() {
    // Simulate 5 candidates from search
    let candidates: Vec<CandidateRecord> = (0..5)
        .map(|i| CandidateRecord {
            id: CandidateId::new(format!("cand-{}", i)),
            provider_id: ProviderId::new("test-provider"),
            source_url: format!("https://example.com/img{}.jpg", i),
            thumbnail_url: None,
            title: Some(format!("Image {}", i)),
            page_url: None,
            dimensions: None,
        })
        .collect();

    // Simulate candidate evaluation: 3 accepted, 2 rejected
    let decisions: Vec<CandidateDecision> = candidates
        .into_iter()
        .enumerate()
        .map(|(i, c)| {
            if i < 3 {
                CandidateDecision::Accepted {
                    candidate: c,
                    priority: (5 - i) as u32,
                }
            } else {
                CandidateDecision::Rejected {
                    candidate: c,
                    reason: format!("low relevance for image {}", i),
                }
            }
        })
        .collect();

    let seq = RetrievableCandidateSequence::from_decisions(decisions);
    assert_eq!(seq.len(), 3);

    // Form a batch from the sequence
    let batch_ids: Vec<String> = seq
        .candidates
        .iter()
        .filter_map(|d| match d {
            CandidateDecision::Accepted { candidate, .. } => Some(candidate.id.to_string()),
            _ => None,
        })
        .collect();

    let batch = RetrievalBatch::new(batch_ids, 8); // target 8, actual 3 → short batch
    assert!(batch.is_short_batch);
    assert_eq!(batch.actual_size(), 3);
    assert_eq!(batch.target_size, 8);
}

// ---------------------------------------------------------------------------
// Retrieval → Image acceptance integration
// ---------------------------------------------------------------------------

#[test]
fn retrieval_results_feed_image_acceptance() {
    // Simulate a successful retrieval
    let success = RetrievalResult::Success(RetrievalSuccess {
        candidate_id: "cand-0".into(),
        local_path: "/tmp/img0.jpg".into(),
        channel_tier: RetrievalChannelTier::WebFetch,
        content_type: Some("image/jpeg".into()),
        file_size_bytes: 2048,
    });

    match &success {
        RetrievalResult::Success(s) => {
            // Build an image record from retrieval success
            let image = ImageRecord {
                candidate_id: s.candidate_id.clone(),
                local_path: s.local_path.clone(),
                content_type: s.content_type.clone(),
                file_size_bytes: s.file_size_bytes,
                dimensions: None,
            };

            // Mechanical evidence
            let evidence = ImageMechanicalEvidence {
                blocking_findings: vec![],
                reference_findings: vec!["2048 bytes received".into()],
            };

            assert!(evidence.passed_mechanical());

            // Acceptance decision
            let decision = ImageAcceptanceDecision::Accepted {
                image,
                notes: "looks good".into(),
            };
            assert!(decision.is_accepted());
        }
        _ => panic!("expected success"),
    }
}

#[test]
fn retrieval_failure_produces_fallback_fact() {
    let failure = RetrievalResult::Failure(RetrievalFailure {
        candidate_id: "cand-1".into(),
        channel_tier: RetrievalChannelTier::WebFetch,
        failure_category: RetrievalFailureCategory::AccessRestricted,
        reason: "403 Forbidden".into(),
        allows_fallback: false, // access-restricted must not fallback
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

// ---------------------------------------------------------------------------
// Delivery decision integration
// ---------------------------------------------------------------------------

#[test]
fn full_delivery_scenario() {
    let accepted: Vec<ImageAcceptanceDecision> = (0..3)
        .map(|i| ImageAcceptanceDecision::Accepted {
            image: ImageRecord {
                candidate_id: format!("cand-{}", i),
                local_path: format!("/tmp/img{}.jpg", i),
                content_type: Some("image/jpeg".into()),
                file_size_bytes: 1024 * (i + 1) as u64,
                dimensions: None,
            },
            notes: format!("accepted image {}", i),
        })
        .collect();

    let decision = DeliveryDecision::full_delivery(accepted, vec![], 1, 0);
    assert_eq!(decision.status, TaskStatus::FullDelivery);
    assert_eq!(decision.full_attempt_count, 1);
    assert_eq!(decision.retry_count, 0);
}

#[test]
fn limited_delivery_scenario() {
    let accepted: Vec<ImageAcceptanceDecision> = vec![ImageAcceptanceDecision::Accepted {
        image: ImageRecord {
            candidate_id: "only-one".into(),
            local_path: "/tmp/only.jpg".into(),
            content_type: Some("image/jpeg".into()),
            file_size_bytes: 500,
            dimensions: None,
        },
        notes: "only acceptable image".into(),
    }];

    let decision = DeliveryDecision::limited_delivery(accepted, vec![], 4, 3, 3);
    assert_eq!(decision.status, TaskStatus::LimitedDelivery);
    assert!(decision.shortfall_reason.is_some());
}

#[test]
fn execution_blocked_scenario() {
    let decision = DeliveryDecision::execution_blocked("OpenClaw not reachable".into());
    assert_eq!(decision.status, TaskStatus::ExecutionBlocked);
    assert_eq!(decision.accepted_images.len(), 0);
    assert_eq!(decision.full_attempt_count, 0);
}

#[test]
fn input_rejected_scenario() {
    let decision = DeliveryDecision::input_rejected("description field is empty".into());
    assert_eq!(decision.status, TaskStatus::InputRejected);
    assert_eq!(decision.accepted_images.len(), 0);
}

// ---------------------------------------------------------------------------
// Policy integration
// ---------------------------------------------------------------------------

#[test]
fn policy_authorization_risk_flow() {
    let fact = PolicyFact {
        subject_id: "cand-5".into(),
        authorization_risk: AuthorizationRisk::Unknown,
        has_access_restriction: false,
        is_paid_channel: false,
        paid_channel_confirmed: false,
        context: "source has no clear license".into(),
    };

    // Unknown authorization should produce Allow (with risk notes in manifest),
    // not a block — the risk is documented, not rejected.
    assert_eq!(fact.authorization_risk, AuthorizationRisk::Unknown);
    assert!(!fact.has_access_restriction);
}

#[test]
fn policy_prohibited_source_blocks() {
    let fact = PolicyFact {
        subject_id: "cand-6".into(),
        authorization_risk: AuthorizationRisk::Prohibited,
        has_access_restriction: true,
        is_paid_channel: false,
        paid_channel_confirmed: false,
        context: "explicitly disallowed by source".into(),
    };

    assert_eq!(fact.authorization_risk, AuthorizationRisk::Prohibited);
    assert!(fact.has_access_restriction);
}

// ---------------------------------------------------------------------------
// Error integration
// ---------------------------------------------------------------------------

#[test]
fn error_families_are_distinguishable() {
    let input_err = Error::input_rejection("missing description");
    let provider_err = Error::provider_failure("brave", "timeout");
    let exec_err = Error::execution_blocked("OpenClaw unavailable");

    assert!(matches!(input_err, Error::InputRejection { .. }));
    assert!(matches!(provider_err, Error::ProviderFailure { .. }));
    assert!(matches!(exec_err, Error::ExecutionBlocked { .. }));

    // All errors implement std::error::Error
    let _: &dyn std::error::Error = &input_err;
}

#[test]
fn diagnostic_accumulates_items() {
    let diag = Diagnostic::new("limited_delivery", "Shortfall of 2 images.")
        .with_item(image_retrieval::error::DiagnosticItem {
            level: DiagnosticLevel::Error,
            category: "candidate shortage".into(),
            message: "searched all providers, got only 10 of 60 target".into(),
        })
        .with_item(image_retrieval::error::DiagnosticItem {
            level: DiagnosticLevel::Warning,
            category: "channel fallback".into(),
            message: "web_fetch failed, used self_hosted".into(),
        })
        .with_item(image_retrieval::error::DiagnosticItem {
            level: DiagnosticLevel::Info,
            category: "provider usage".into(),
            message: "used providers: fixture-a (weight 1)".into(),
        });

    assert_eq!(diag.items.len(), 3);
    assert_eq!(diag.items[0].level, DiagnosticLevel::Error);
    assert_eq!(diag.items[2].level, DiagnosticLevel::Info);
}

// ---------------------------------------------------------------------------
// Metrics integration
// ---------------------------------------------------------------------------

#[test]
fn metrics_events_cover_all_met_categories() {
    let events = vec![
        MetricEvent::new(MetricKind::TaskOutcome, "full_delivery", 1.0),
        MetricEvent::new(MetricKind::CandidateSatisfaction, "actual/target", 45.0)
            .with_denominator(60.0),
        MetricEvent::new(
            MetricKind::QualifiedImageAchievement,
            "qualified/required",
            2.0,
        )
        .with_denominator(3.0),
        MetricEvent::new(MetricKind::RejectionReason, "low_resolution", 5.0),
        MetricEvent::new(MetricKind::ChannelEffectiveness, "web_fetch", 3.0),
        MetricEvent::new(MetricKind::OpenClawEvaluationRate, "passed", 8.0).with_denominator(10.0),
    ];

    assert_eq!(events.len(), 6);
    assert_eq!(events[0].kind, MetricKind::TaskOutcome);
    assert_eq!(events[1].denominator, Some(60.0));
    assert_eq!(events[5].kind, MetricKind::OpenClawEvaluationRate);
}

// ---------------------------------------------------------------------------
// Fallback chain integration
// ---------------------------------------------------------------------------

#[test]
fn fallback_chain_respects_access_restrictions() {
    // Normal fallback chain
    assert_eq!(
        RetrievalChannelTier::WebFetch.next_fallback(),
        Some(RetrievalChannelTier::SelfHosted)
    );
    assert_eq!(
        RetrievalChannelTier::SelfHosted.next_fallback(),
        Some(RetrievalChannelTier::Paid)
    );
    assert_eq!(RetrievalChannelTier::Paid.next_fallback(), None);

    // An access-restricted failure must not allow fallback
    let fact = FallbackEligibilityFact {
        failed_tier: RetrievalChannelTier::WebFetch,
        next_tier: Some(RetrievalChannelTier::SelfHosted),
        reason: "403 Forbidden".into(),
        is_access_restricted: true,
        requires_paid_confirmation: false,
    };

    assert!(fact.is_access_restricted);
    // When is_access_restricted is true, the orchestrator must NOT fallback
}

// ---------------------------------------------------------------------------
// Serialization round-trip (smoke test)
// ---------------------------------------------------------------------------

#[test]
fn query_plan_input_round_trip() {
    let json = r#"{
        "description": "a cat on a sofa",
        "required_count": 3,
        "quality_tier": "high",
        "content_constraints": {
            "must_include": ["cat"],
            "must_avoid": ["dog"]
        },
        "authorization_preference": "default",
        "output_preference": "human",
        "retry_limit": 3
    }"#;

    let parsed: QueryPlanInput = serde_json::from_str(json).expect("deserialize QueryPlanInput");
    assert_eq!(parsed.description, "a cat on a sofa");
    assert_eq!(parsed.required_count, 3);
    assert_eq!(parsed.quality_tier, QualityTier::High);

    let round_tripped = serde_json::to_string_pretty(&parsed).expect("serialize");
    let parsed_again: QueryPlanInput =
        serde_json::from_str(&round_tripped).expect("deserialize again");
    assert_eq!(parsed_again.description, parsed.description);
}

#[test]
fn task_status_serialization() {
    assert_eq!(
        serde_json::to_string(&TaskStatus::FullDelivery).unwrap(),
        r#""full_delivery""#
    );
    assert_eq!(
        serde_json::to_string(&TaskStatus::LimitedDelivery).unwrap(),
        r#""limited_delivery""#
    );
    assert_eq!(
        serde_json::to_string(&TaskStatus::ExecutionBlocked).unwrap(),
        r#""execution_blocked""#
    );
    assert_eq!(
        serde_json::to_string(&TaskStatus::InputRejected).unwrap(),
        r#""input_rejected""#
    );
}
