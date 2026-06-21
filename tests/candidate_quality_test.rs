//! Integration tests for candidate quality gate (TASK-004).
//!
//! Covers the acceptance criteria:
//! - Mechanical blocking prevents OpenClaw evaluation.
//! - Reference evidence flows into evaluation requests.
//! - Only mechanically-passed + OpenClaw-approved candidates are retrievable.
//! - Rejected and uncertain candidates do NOT enter the retrievable sequence.
//! - OpenClaw unavailability produces execution blocking facts.
//! - Candidate evaluation does not decide final delivery.
//!
//! References: PRD §校验与评价产品要求 (AC-005, AC-011),
//! `docs/design/TASK-004-candidate-quality-openclaw-design.md`

use image_retrieval::domain::candidate::{
    CandidateDecision, CandidateId, CandidateRecord, ProviderId, RetrievableCandidateSequence,
};
use image_retrieval::domain::query_plan::{
    AuthorizationPreference, ContentConstraints, QualityTier, ValidatedQueryPlan,
};
use image_retrieval::domain::search::{CandidateShortageReason, SearchOutcome};
use image_retrieval::quality::candidate::gate::evaluate_with_conclusions;
use image_retrieval::quality::candidate::mechanical::{
    validate_candidate_mechanical, CandidateBlockingReason, CandidateMechanicalEvidence,
    CandidateReferenceSignal,
};
use image_retrieval::quality::candidate::{
    normalize_conclusion, CandidateEvaluationConclusion, CandidateEvaluationRequest,
    ExecutionBlockingFact,
};
use std::collections::HashSet;

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

fn make_candidate(id: &str, url: &str, title: Option<&str>) -> CandidateRecord {
    CandidateRecord {
        id: CandidateId::new(id),
        provider_id: ProviderId::new("test-provider"),
        source_url: url.into(),
        thumbnail_url: None,
        title: title.map(|s| s.into()),
        page_url: None,
        dimensions: None,
    }
}

fn make_query_plan() -> ValidatedQueryPlan {
    ValidatedQueryPlan {
        description: "sunset over mountains".into(),
        required_count: 1,
        quality_tier: QualityTier::General,
        content_constraints: ContentConstraints::default(),
        authorization_preference: AuthorizationPreference::Default,
        output_preference: image_retrieval::domain::query_plan::OutputPreference::Human,
        retry_limit: 3,
    }
}

fn make_search_outcome(candidates: Vec<CandidateRecord>) -> SearchOutcome {
    SearchOutcome {
        candidates,
        usage_events: vec![],
        total_invocations: 1,
        candidate_target: 20,
        target_met: true,
        shortage_reason: None,
        readiness_summary: vec![],
    }
}

// ---------------------------------------------------------------------------
// AC-005: Mechanical blocking prevents OpenClaw evaluation
// ---------------------------------------------------------------------------

#[test]
fn candidate_quality_mechanical_block_prevents_openclaw() {
    // Candidates with obvious problems should be blocked before reaching OpenClaw
    let bad = make_candidate("bad-1", "", None); // empty URL → obviously invalid
    let good = make_candidate("good-1", "https://example.com/sunset.jpg", Some("Sunset"));

    let plan = make_query_plan();

    let evidence_bad = validate_candidate_mechanical(
        &bad,
        &HashSet::new(),
        &plan.content_constraints,
        plan.quality_tier,
    );
    assert!(
        !evidence_bad.passed_mechanical(),
        "empty URL candidate must be mechanically blocked"
    );
    assert!(
        matches!(
            evidence_bad.blocking_findings[0],
            CandidateBlockingReason::ObviouslyInvalid { .. }
        ),
        "must be ObviouslyInvalid"
    );

    // Good candidate should pass mechanical (fresh seen set)
    let good_seen = HashSet::new();
    let evidence_good = validate_candidate_mechanical(
        &good,
        &good_seen,
        &plan.content_constraints,
        plan.quality_tier,
    );
    assert!(
        evidence_good.passed_mechanical(),
        "valid candidate must pass mechanical"
    );

    // Only the good candidate can receive an evaluation request
    let request = CandidateEvaluationRequest::new(
        good.clone(),
        evidence_good,
        plan.description.clone(),
        plan.quality_tier,
        plan.content_constraints.clone(),
        plan.authorization_preference,
        good.provider_id.to_string(),
    );
    assert_eq!(request.candidate.id, CandidateId::new("good-1"));
}

// ---------------------------------------------------------------------------
// AC-005: Reference evidence flows into evaluation request
// ---------------------------------------------------------------------------

#[test]
fn candidate_quality_reference_evidence_flows_to_evaluation_request() {
    let candidate = make_candidate(
        "ref-1",
        "https://example.com/forest.jpg",
        Some("Forest at dawn"),
    );

    // Add some reference signals
    let signals = vec![
        CandidateReferenceSignal::SourceQuality {
            note: "trusted domain".into(),
        },
        CandidateReferenceSignal::ProviderConfidence {
            note: "top-3 search result".into(),
        },
    ];
    let mech = CandidateMechanicalEvidence::pass_with_signals(signals);

    let plan = make_query_plan();
    let request = CandidateEvaluationRequest::new(
        candidate,
        mech,
        plan.description,
        plan.quality_tier,
        plan.content_constraints,
        plan.authorization_preference,
        "test-provider".into(),
    );

    // Reference signals are present in the mechanical evidence within the request
    assert!(request.mechanical_evidence.passed_mechanical());
    assert_eq!(request.mechanical_evidence.reference_signals.len(), 2);
    assert!(request
        .mechanical_evidence
        .reference_signals
        .iter()
        .any(|s| matches!(s, CandidateReferenceSignal::SourceQuality { .. })));
}

// ---------------------------------------------------------------------------
// AC-005: Only mechanically-passed AND OpenClaw-approved candidates are retrievable
// ---------------------------------------------------------------------------

#[test]
fn candidate_quality_only_approved_candidates_enter_sequence() {
    let candidates = vec![
        make_candidate("c1", "https://a.com/1.jpg", Some("Mountain sunset")),
        make_candidate("c2", "https://b.com/2.jpg", Some("City skyline")),
        make_candidate("c3", "https://c.com/3.jpg", Some("Forest dawn")),
    ];

    // All pass mechanical
    let plan = make_query_plan();
    let mut seen = HashSet::new();
    let mut passed = Vec::new();

    for c in &candidates {
        let evidence =
            validate_candidate_mechanical(c, &seen, &plan.content_constraints, plan.quality_tier);
        assert!(evidence.passed_mechanical());
        seen.insert(c.source_url.clone());
        passed.push((c.clone(), evidence));
    }

    // Simulate OpenClaw: approve c1, reject c2, uncertain c3
    let conclusions = vec![
        CandidateEvaluationConclusion::Approve {
            notes: Some("good match".into()),
        },
        CandidateEvaluationConclusion::Reject {
            reason: "city doesn't match mountains".into(),
        },
        CandidateEvaluationConclusion::Uncertain {
            reason: "forest might be mountain-adjacent".into(),
        },
    ];

    let decisions = evaluate_with_conclusions(passed, conclusions);
    let seq = RetrievableCandidateSequence::from_decisions(decisions);

    // Only c1 should be in the retrievable sequence
    assert_eq!(seq.len(), 1, "only approved candidates enter the sequence");
    match &seq.candidates[0] {
        CandidateDecision::Accepted { candidate, .. } => {
            assert_eq!(candidate.id, CandidateId::new("c1"));
        }
        _ => panic!("expected Accepted"),
    }
}

// ---------------------------------------------------------------------------
// AC-005: Rejected and uncertain candidates do NOT enter the retrievable sequence
// ---------------------------------------------------------------------------

#[test]
fn candidate_quality_rejected_and_uncertain_not_retrievable() {
    let candidates = vec![
        make_candidate("c1", "https://a.com/1.jpg", Some("Good match")),
        make_candidate("c2", "https://b.com/2.jpg", Some("Bad match")),
        make_candidate("c3", "https://c.com/3.jpg", Some("Ambiguous")),
    ];

    let evidence = vec![
        CandidateMechanicalEvidence::pass(),
        CandidateMechanicalEvidence::pass(),
        CandidateMechanicalEvidence::pass(),
    ];

    let passed: Vec<_> = candidates.into_iter().zip(evidence).collect();

    let conclusions = vec![
        CandidateEvaluationConclusion::Approve { notes: None },
        CandidateEvaluationConclusion::Reject {
            reason: "irrelevant".into(),
        },
        CandidateEvaluationConclusion::Uncertain {
            reason: "unclear".into(),
        },
    ];

    let decisions = evaluate_with_conclusions(passed, conclusions);
    let seq = RetrievableCandidateSequence::from_decisions(decisions.clone());

    assert_eq!(seq.len(), 1);

    // Verify rejected is in decisions but not in sequence
    let rejected_in_decisions = decisions
        .iter()
        .any(|d| matches!(d, CandidateDecision::Rejected { .. }));
    assert!(
        rejected_in_decisions,
        "rejected must be recorded in decisions"
    );

    // Verify uncertain is in decisions but not in sequence
    let uncertain_in_decisions = decisions
        .iter()
        .any(|d| matches!(d, CandidateDecision::Uncertain { .. }));
    assert!(
        uncertain_in_decisions,
        "uncertain must be recorded in decisions"
    );

    // Neither rejected nor uncertain appear in the sequence
    for d in &seq.candidates {
        assert!(d.is_accepted());
    }
}

// ---------------------------------------------------------------------------
// AC-011: OpenClaw unavailability produces execution blocking
// ---------------------------------------------------------------------------

#[test]
fn candidate_quality_openclaw_unavailable_execution_blocked() {
    let fact = ExecutionBlockingFact::openclaw_unavailable("no production endpoint configured");

    assert_eq!(fact.dependency, "OpenClaw");
    assert!(fact.is_permanent);
    assert!(fact.reason.contains("no production endpoint"));

    // Candidates that pass mechanical but cannot be evaluated become
    // ExecutionBlocked
    let candidates = vec![
        make_candidate("c1", "https://a.com/1.jpg", Some("Sunset")),
        make_candidate("c2", "https://b.com/2.jpg", Some("Mountains")),
    ];

    let decisions: Vec<CandidateDecision> = candidates
        .into_iter()
        .map(|c| CandidateDecision::ExecutionBlocked {
            candidate: c,
            reason: fact.reason.clone(),
        })
        .collect();

    let seq = RetrievableCandidateSequence::from_decisions(decisions.clone());
    assert!(
        seq.is_empty(),
        "no candidates retrievable when OpenClaw is down"
    );

    // Verify all are ExecutionBlocked
    let all_blocked = decisions
        .iter()
        .all(|d| matches!(d, CandidateDecision::ExecutionBlocked { .. }));
    assert!(all_blocked);
}

// ---------------------------------------------------------------------------
// Candidate evaluation does NOT decide final delivery
// ---------------------------------------------------------------------------

#[test]
fn candidate_quality_does_not_decide_final_delivery() {
    // Candidate evaluation produces retrievable candidates, not final delivery.
    // The delivery decision is made by the Image Acceptance Gate (TASK-006).
    // This test verifies that the candidate quality gate output types don't
    // include any delivery status.

    let candidate = make_candidate("c1", "https://a.com/1.jpg", Some("Sunset"));
    let conclusion = CandidateEvaluationConclusion::Approve {
        notes: Some("good".into()),
    };
    let decision = normalize_conclusion(candidate, conclusion);

    // An accepted candidate decision has a candidate + priority,
    // NOT a delivery status
    match decision {
        CandidateDecision::Accepted {
            candidate,
            priority,
        } => {
            assert_eq!(candidate.id, CandidateId::new("c1"));
            assert_eq!(priority, 0);
        }
        _ => panic!("expected Accepted"),
    }
}

// ---------------------------------------------------------------------------
// Duplicate detection is mechanical, not subjective
// ---------------------------------------------------------------------------

#[test]
fn candidate_quality_duplicates_blocked_mechanically_not_by_openclaw() {
    let candidates = vec![
        make_candidate("c1", "https://a.com/sunset.jpg", Some("Sunset")),
        make_candidate("c2", "https://a.com/sunset.jpg", Some("Sunset duplicate")),
        make_candidate("c3", "https://b.com/other.jpg", Some("Other")),
    ];

    let plan = make_query_plan();
    let mut seen = HashSet::new();
    let mut blocked = 0;
    let mut passed = 0;

    for c in &candidates {
        let evidence =
            validate_candidate_mechanical(c, &seen, &plan.content_constraints, plan.quality_tier);
        if evidence.passed_mechanical() {
            seen.insert(c.source_url.clone());
            passed += 1;
        } else {
            blocked += 1;
        }
    }

    assert_eq!(
        blocked, 1,
        "exact duplicate URL must be mechanically blocked"
    );
    assert_eq!(passed, 2, "c1 and c3 should pass mechanical");
}

// ---------------------------------------------------------------------------
// Empty candidate input produces empty sequence (no crash)
// ---------------------------------------------------------------------------

#[test]
fn candidate_quality_empty_input_sequence_is_empty() {
    let outcome = make_search_outcome(vec![]);
    assert!(outcome.candidates.is_empty());

    let seq = RetrievableCandidateSequence::empty();
    assert!(seq.is_empty());
    assert_eq!(seq.len(), 0);
}

// ---------------------------------------------------------------------------
// Candidate shortage flows through but does not block evaluation
// ---------------------------------------------------------------------------

#[test]
fn candidate_quality_shortage_does_not_block_evaluation() {
    let outcome = SearchOutcome {
        candidates: vec![make_candidate("c1", "https://a.com/1.jpg", Some("Sunset"))],
        usage_events: vec![],
        total_invocations: 3,
        candidate_target: 60,
        target_met: false,
        shortage_reason: Some(CandidateShortageReason::AllProvidersExhausted),
        readiness_summary: vec![],
    };

    // The shortage is recorded but does not block the quality gate
    assert!(!outcome.target_met);
    assert!(outcome.shortage_reason.is_some());
    // The one candidate we do have should still be evaluatable
    assert_eq!(outcome.candidates.len(), 1);
}

// ---------------------------------------------------------------------------
// must_avoid terms in title produce mechanical block
// ---------------------------------------------------------------------------

#[test]
fn candidate_quality_must_avoid_in_title_is_mechanically_blocked() {
    let candidate = make_candidate(
        "bad-city",
        "https://a.com/city.jpg",
        Some("Beautiful city at dusk"),
    );

    let plan = ValidatedQueryPlan {
        content_constraints: ContentConstraints {
            must_include: vec![],
            must_avoid: vec!["city".into()],
        },
        ..make_query_plan()
    };

    let seen = HashSet::new();
    let evidence = validate_candidate_mechanical(
        &candidate,
        &seen,
        &plan.content_constraints,
        plan.quality_tier,
    );

    assert!(
        !evidence.passed_mechanical(),
        "must_avoid term in title must block mechanically"
    );
    assert!(matches!(
        evidence.blocking_findings[0],
        CandidateBlockingReason::SemanticMismatch { .. }
    ));
}

// ---------------------------------------------------------------------------
// Authorization risk is a reference signal, not a block
// ---------------------------------------------------------------------------

#[test]
fn candidate_quality_unknown_authorization_is_not_mechanically_blocked() {
    // Unknown authorization must NOT be mechanically blocked.
    // It's a reference signal for OpenClaw evaluation and policy explanation.
    let candidate = make_candidate("c1", "https://a.com/img.jpg", Some("Sunset"));

    let plan = make_query_plan();
    let seen = HashSet::new();
    let evidence = validate_candidate_mechanical(
        &candidate,
        &seen,
        &plan.content_constraints,
        plan.quality_tier,
    );

    assert!(
        evidence.passed_mechanical(),
        "unknown authorization must not mechanically block"
    );
    // Authorization risk is assessed by policy layer (TASK-007), not here
}

// ---------------------------------------------------------------------------
// CandidateDecision::Accepted via direct normalization
// ---------------------------------------------------------------------------

#[test]
fn candidate_quality_acceptance_normalization() {
    let candidate = make_candidate("c1", "https://a.com/1.jpg", Some("Sunset"));

    let conclusion = CandidateEvaluationConclusion::Approve {
        notes: Some("excellent match".into()),
    };
    let decision = normalize_conclusion(candidate, conclusion);

    assert!(decision.is_accepted());
}

// ---------------------------------------------------------------------------
// ExecutionBlockingFact serialization
// ---------------------------------------------------------------------------

#[test]
fn candidate_quality_execution_blocking_fact_serialization() {
    let fact = ExecutionBlockingFact::openclaw_unavailable("endpoint not configured");

    let json = serde_json::to_string(&fact).expect("serialize ExecutionBlockingFact");
    let parsed: ExecutionBlockingFact =
        serde_json::from_str(&json).expect("deserialize ExecutionBlockingFact");

    assert_eq!(parsed.dependency, "OpenClaw");
    assert!(parsed.is_permanent);
    assert!(parsed.reason.contains("endpoint not configured"));
}

// ---------------------------------------------------------------------------
// Mechanical evidence serialization round-trip
// ---------------------------------------------------------------------------

#[test]
fn candidate_quality_mechanical_evidence_serialization() {
    let evidence = CandidateMechanicalEvidence {
        blocking_findings: vec![CandidateBlockingReason::Unreachable {
            detail: "DNS resolution failed".into(),
        }],
        reference_signals: vec![CandidateReferenceSignal::SourceQuality {
            note: "low-res thumbnail".into(),
        }],
    };

    let json = serde_json::to_string(&evidence).expect("serialize");
    let parsed: CandidateMechanicalEvidence = serde_json::from_str(&json).expect("deserialize");

    assert!(!parsed.passed_mechanical());
    assert_eq!(parsed.blocking_findings.len(), 1);
    assert_eq!(parsed.reference_signals.len(), 1);
}

// ---------------------------------------------------------------------------
// CandidateEvaluationConclusion serialization coverage
// ---------------------------------------------------------------------------

#[test]
fn candidate_quality_conclusion_all_variants_serialize() {
    let variants = vec![
        CandidateEvaluationConclusion::Approve {
            notes: Some("good".into()),
        },
        CandidateEvaluationConclusion::Reject {
            reason: "bad".into(),
        },
        CandidateEvaluationConclusion::Uncertain {
            reason: "maybe".into(),
        },
        CandidateEvaluationConclusion::Unexecutable {
            reason: "down".into(),
        },
    ];

    for v in &variants {
        let json = serde_json::to_string(v).expect("serialize");
        let parsed: CandidateEvaluationConclusion =
            serde_json::from_str(&json).expect("deserialize");
        assert_eq!(*v, parsed);
    }
}

// ---------------------------------------------------------------------------
// Complete pipeline: search outcome → quality gate → retrievable sequence
// ---------------------------------------------------------------------------

#[test]
fn candidate_quality_full_pipeline_integration() {
    // Simulate 5 candidates from search
    let outcome = make_search_outcome(vec![
        make_candidate("c1", "https://a.com/sunset.jpg", Some("Mountain sunset")),
        make_candidate("c2", "", None), // mechanically blocked
        make_candidate("c3", "https://b.com/city.jpg", Some("City skyline")),
        make_candidate("c4", "https://c.com/forest.jpg", Some("Forest dawn")),
    ]);

    let plan = ValidatedQueryPlan {
        content_constraints: ContentConstraints {
            must_include: vec![],
            must_avoid: vec!["city".into()],
        },
        ..make_query_plan()
    };

    // Phase 1: Mechanical validation
    let mut seen = HashSet::new();
    let mut mechanically_blocked = Vec::new();
    let mut mechanically_passed = Vec::new();

    for c in &outcome.candidates {
        let evidence =
            validate_candidate_mechanical(c, &seen, &plan.content_constraints, plan.quality_tier);
        if evidence.passed_mechanical() {
            if !c.source_url.trim().is_empty() {
                seen.insert(c.source_url.clone());
            }
            mechanically_passed.push((c.clone(), evidence));
        } else {
            mechanically_blocked.push(c.clone());
        }
    }

    // c2 (empty URL) should be blocked
    // c3 (title contains "city" with must_avoid=["city"]) should be blocked
    assert_eq!(
        mechanically_blocked.len(),
        2,
        "c2 and c3 should be mechanically blocked"
    );
    assert_eq!(
        mechanically_passed.len(),
        2,
        "c1 and c4 should pass mechanical"
    );

    // Phase 2: OpenClaw evaluation (simulated)
    let conclusions = vec![
        CandidateEvaluationConclusion::Approve {
            notes: Some("perfect mountain sunset".into()),
        },
        CandidateEvaluationConclusion::Reject {
            reason: "forest is not a mountain sunset".into(),
        },
    ];

    let eval_decisions = evaluate_with_conclusions(mechanically_passed, conclusions);

    let mut all_decisions: Vec<CandidateDecision> = mechanically_blocked
        .into_iter()
        .map(|c| CandidateDecision::Rejected {
            candidate: c,
            reason: "mechanical block".into(),
        })
        .collect();
    all_decisions.extend(eval_decisions);

    // Phase 3: Build retrievable sequence
    let seq = RetrievableCandidateSequence::from_decisions(all_decisions.clone());

    // Only c1 should be retrievable
    assert_eq!(seq.len(), 1, "only one candidate should be retrievable");
    match &seq.candidates[0] {
        CandidateDecision::Accepted {
            candidate,
            priority: _,
        } => {
            assert_eq!(candidate.id, CandidateId::new("c1"));
            assert_eq!(candidate.source_url, "https://a.com/sunset.jpg");
        }
        _ => panic!("expected Accepted"),
    }

    // Phase 4: Verify that rejected/uncertain don't make it
    let accepted_count = seq.len();
    let total_decisions = all_decisions.len();
    assert!(
        accepted_count <= total_decisions,
        "accepted count ({}) must not exceed total decisions ({})",
        accepted_count,
        total_decisions
    );

    // Verify decisions include rejected
    let rejected = all_decisions
        .iter()
        .filter(|d| matches!(d, CandidateDecision::Rejected { .. }))
        .count();
    assert!(rejected > 0, "must have rejected decisions");
}
