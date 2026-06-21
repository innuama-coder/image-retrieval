//! Integration tests for retrieval: batch planning, channel execution,
//! fallback, short batch, and acceptance criteria.
//!
//! # Test categories
//!
//! 1. **Batch planning** — normal, short, empty, URL mapping.
//! 2. **Channel execution** — single channel, fallback, access-restricted stop.
//! 3. **Web fetch channel** — readiness, missing URL, content type checks.
//! 4. **Fixture channel** — success, failure, mixed, readiness states.
//! 5. **Paid channel boundaries** — not silently used, requires confirmation.
//! 6. **Acceptance criteria** — AC-006 (batch), AC-007 (fallback/access).

use image_retrieval::domain::candidate::{
    CandidateDecision, CandidateId, CandidateRecord, ProviderId, RetrievableCandidateSequence,
};
use image_retrieval::domain::retrieval::{
    FallbackEligibilityFact, RetrievalBatch, RetrievalChannelReadiness, RetrievalChannelTier,
    RetrievalFailureCategory, RetrievalResult,
};
use image_retrieval::ports::BaseRetrievalChannel;
use image_retrieval::retrieval::channels::fixture::{
    FixtureChannel, FixtureReadiness, FixtureResponse,
};
use image_retrieval::retrieval::{
    execute_batch, summarise_channel_readiness, RetrievalBatchPlanner, WebFetchChannel,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_accepted(id: &str, url: &str, priority: u32) -> CandidateDecision {
    CandidateDecision::Accepted {
        candidate: CandidateRecord {
            id: CandidateId::new(id),
            provider_id: ProviderId::new("test"),
            source_url: url.to_string(),
            thumbnail_url: None,
            title: None,
            page_url: None,
            dimensions: None,
        },
        priority,
    }
}

fn make_sequence(decisions: Vec<CandidateDecision>) -> RetrievableCandidateSequence {
    RetrievableCandidateSequence::from_decisions(decisions)
}

// ---------------------------------------------------------------------------
// AC-006: Batch size = required_count × 2
// ---------------------------------------------------------------------------

#[test]
fn ac006_batch_target_for_4_images_is_8() {
    let target = RetrievalBatchPlanner::batch_target_for(4);
    assert_eq!(target, 8);
}

#[test]
fn ac006_batch_target_for_1_image_is_2() {
    let target = RetrievalBatchPlanner::batch_target_for(1);
    assert_eq!(target, 2);
}

#[test]
fn ac006_batch_target_for_3_images_is_6() {
    let target = RetrievalBatchPlanner::batch_target_for(3);
    assert_eq!(target, 6);
}

#[test]
fn ac006_batch_exact_target_formed() {
    let decisions: Vec<CandidateDecision> = (0..8)
        .map(|i| {
            make_accepted(
                &format!("c{}", i),
                &format!("https://x.com/{}.jpg", i),
                8 - i,
            )
        })
        .collect();
    let seq = make_sequence(decisions);
    let (batch, shortage) = RetrievalBatchPlanner::plan(&seq, 8);

    assert_eq!(batch.actual_size(), 8);
    assert!(!batch.is_short_batch);
    assert!(shortage.is_none());
}

#[test]
fn ac006_batch_takes_no_more_than_target() {
    let decisions: Vec<CandidateDecision> = (0..20)
        .map(|i| {
            make_accepted(
                &format!("c{}", i),
                &format!("https://x.com/{}.jpg", i),
                20 - i,
            )
        })
        .collect();
    let seq = make_sequence(decisions);
    let (batch, _) = RetrievalBatchPlanner::plan(&seq, 8);

    // Even with 20 candidates, only 8 enter the batch
    assert_eq!(batch.actual_size(), 8);
}

// ---------------------------------------------------------------------------
// Short batch: fewer candidates than target
// ---------------------------------------------------------------------------

#[test]
fn short_batch_formed_when_fewer_candidates() {
    let decisions = vec![
        make_accepted("a", "https://example.com/a.jpg", 3),
        make_accepted("b", "https://example.com/b.jpg", 2),
        make_accepted("c", "https://example.com/c.jpg", 1),
    ];
    let seq = make_sequence(decisions);
    let (batch, shortage) = RetrievalBatchPlanner::plan(&seq, 8);

    assert_eq!(batch.actual_size(), 3);
    assert!(batch.is_short_batch);

    let s = shortage.expect("must have shortage evidence");
    assert_eq!(s.target_size, 8);
    assert_eq!(s.actual_size, 3);
    assert!(s.reason.contains("only 3"));
}

#[test]
fn short_batch_does_not_infinite_backfill() {
    // Only 2 candidates available, target 8 — planner stops at 2.
    // It does NOT loop back, re-search, or fabricate candidates.
    let decisions = vec![
        make_accepted("x", "https://example.com/x.jpg", 1),
        make_accepted("y", "https://example.com/y.jpg", 1),
    ];
    let seq = make_sequence(decisions);

    // Plan twice — both times same result, no side effects
    let (batch1, _) = RetrievalBatchPlanner::plan(&seq, 8);
    let (batch2, _) = RetrievalBatchPlanner::plan(&seq, 8);

    assert_eq!(batch1.actual_size(), 2);
    assert_eq!(batch2.actual_size(), 2);
}

#[test]
fn empty_sequence_produces_empty_short_batch() {
    let seq = RetrievableCandidateSequence::empty();
    let (batch, shortage) = RetrievalBatchPlanner::plan(&seq, 8);

    assert_eq!(batch.actual_size(), 0);
    assert!(batch.is_short_batch);
    assert!(shortage.is_some());
}

// ---------------------------------------------------------------------------
// Rejected / uncertain candidates NOT in batch
// ---------------------------------------------------------------------------

#[test]
fn rejected_candidates_never_enter_batch() {
    let decisions = vec![
        make_accepted("good", "https://example.com/good.jpg", 5),
        CandidateDecision::Rejected {
            candidate: CandidateRecord {
                id: CandidateId::new("bad"),
                provider_id: ProviderId::new("test"),
                source_url: "https://example.com/bad.jpg".into(),
                thumbnail_url: None,
                title: None,
                page_url: None,
                dimensions: None,
            },
            reason: "low quality".into(),
        },
        make_accepted("also-good", "https://example.com/also.jpg", 3),
    ];
    // from_decisions filters to only Accepted
    let seq = make_sequence(decisions);
    let (batch, _) = RetrievalBatchPlanner::plan(&seq, 4);

    assert_eq!(batch.actual_size(), 2);
    assert!(!batch.candidate_ids.contains(&"bad".to_string()));
    assert!(batch.candidate_ids.contains(&"good".to_string()));
    assert!(batch.candidate_ids.contains(&"also-good".to_string()));
}

#[test]
fn uncertain_candidates_never_enter_batch() {
    let decisions = vec![
        make_accepted("yes", "https://example.com/yes.jpg", 3),
        CandidateDecision::Uncertain {
            candidate: CandidateRecord {
                id: CandidateId::new("maybe"),
                provider_id: ProviderId::new("test"),
                source_url: "https://example.com/maybe.jpg".into(),
                thumbnail_url: None,
                title: None,
                page_url: None,
                dimensions: None,
            },
            reason: "ambiguous".into(),
        },
    ];
    let seq = make_sequence(decisions);
    let (batch, _) = RetrievalBatchPlanner::plan(&seq, 2);

    assert_eq!(batch.actual_size(), 1);
    assert_eq!(batch.candidate_ids, vec!["yes"]);
}

#[test]
fn execution_blocked_candidates_never_enter_batch() {
    let decisions = vec![
        make_accepted("ok", "https://example.com/ok.jpg", 5),
        CandidateDecision::ExecutionBlocked {
            candidate: CandidateRecord {
                id: CandidateId::new("blocked"),
                provider_id: ProviderId::new("test"),
                source_url: "https://example.com/blocked.jpg".into(),
                thumbnail_url: None,
                title: None,
                page_url: None,
                dimensions: None,
            },
            reason: "OpenClaw unavailable".into(),
        },
    ];
    let seq = make_sequence(decisions);
    let (batch, _) = RetrievalBatchPlanner::plan(&seq, 2);

    assert_eq!(batch.actual_size(), 1);
    assert_eq!(batch.candidate_ids, vec!["ok"]);
}

// ---------------------------------------------------------------------------
// Fallback: normal failure allows fallback, access-restricted does not
// ---------------------------------------------------------------------------

#[test]
fn ac007_normal_failure_allows_fallback() {
    let decisions = vec![make_accepted("a", "https://example.com/a.jpg", 5)];
    let seq = make_sequence(decisions);

    let fail_channel = FixtureChannel::new(RetrievalChannelTier::WebFetch)
        .with_response("a", FixtureResponse::network_failure());
    let success_channel =
        FixtureChannel::new(RetrievalChannelTier::SelfHosted).with_all_success(&["a"]);
    let channels: Vec<&dyn BaseRetrievalChannel> = vec![&fail_channel, &success_channel];

    let outcome = execute_batch(&seq, 2, &channels);

    // Fallback occurred: started at WebFetch, fell back to SelfHosted
    assert_eq!(outcome.channels_attempted, 2);
    assert_eq!(outcome.channel_tier, RetrievalChannelTier::SelfHosted);
    assert!(!outcome.fallback_facts.is_empty());
    assert!(outcome.results.iter().all(|r| r.is_success()));
}

#[test]
fn ac007_access_restricted_blocks_fallback() {
    let decisions = vec![make_accepted(
        "a",
        "https://restricted.example.com/a.jpg",
        5,
    )];
    let seq = make_sequence(decisions);

    let restricted = FixtureChannel::new(RetrievalChannelTier::WebFetch)
        .with_response("a", FixtureResponse::access_restricted());
    let higher = FixtureChannel::new(RetrievalChannelTier::SelfHosted).with_all_success(&["a"]);
    let channels: Vec<&dyn BaseRetrievalChannel> = vec![&restricted, &higher];

    let outcome = execute_batch(&seq, 2, &channels);

    // MUST NOT fall back past access restriction
    assert_eq!(outcome.channel_tier, RetrievalChannelTier::WebFetch);
    assert_eq!(outcome.channels_attempted, 1);
    assert!(outcome.execution_blocked.is_some());
}

#[test]
fn ac007_access_restriction_not_bypassed_by_upgrading_channel() {
    // Even with SelfHosted available and willing, access restriction on
    // WebFetch must block fallback.
    let decisions = vec![make_accepted(
        "locked",
        "https://paywall.example.com/locked.jpg",
        5,
    )];
    let seq = make_sequence(decisions);

    let web = FixtureChannel::new(RetrievalChannelTier::WebFetch).with_response(
        "locked",
        FixtureResponse::Failure {
            failure_category: RetrievalFailureCategory::AccessRestricted,
            reason: "HTTP 403 Forbidden".into(),
            allows_fallback: false,
        },
    );
    let self_hosted =
        FixtureChannel::new(RetrievalChannelTier::SelfHosted).with_all_success(&["locked"]);
    let paid = FixtureChannel::new(RetrievalChannelTier::Paid).with_all_success(&["locked"]);
    let channels: Vec<&dyn BaseRetrievalChannel> = vec![&web, &self_hosted, &paid];

    let outcome = execute_batch(&seq, 2, &channels);

    // Access restriction must block fallback entirely
    assert_eq!(outcome.channel_tier, RetrievalChannelTier::WebFetch);
    assert!(outcome.execution_blocked.is_some());
}

// ---------------------------------------------------------------------------
// Paid channel boundaries
// ---------------------------------------------------------------------------

#[test]
fn paid_channel_not_silently_used() {
    let decisions = vec![make_accepted("a", "https://example.com/a.jpg", 5)];
    let seq = make_sequence(decisions);

    let web = FixtureChannel::new(RetrievalChannelTier::WebFetch)
        .with_response("a", FixtureResponse::network_failure());
    let paid = FixtureChannel::new(RetrievalChannelTier::Paid)
        .with_readiness(FixtureReadiness::PaidUnconfirmed)
        .with_all_success(&["a"]);
    let channels: Vec<&dyn BaseRetrievalChannel> = vec![&web, &paid];

    let outcome = execute_batch(&seq, 2, &channels);

    // Paid channel should be abandoned (not ready)
    let paid_attempt = outcome
        .channel_attempts
        .iter()
        .find(|a| a.channel_tier == RetrievalChannelTier::Paid);
    assert!(paid_attempt.is_some());
    assert!(paid_attempt.unwrap().abandoned);

    // Results should be empty (web fetch failed, paid not used)
    assert!(outcome.results.is_empty() || outcome.results.iter().all(|r| r.is_failure()));
}

#[test]
fn paid_channel_readiness_reports_paid_unconfirmed() {
    let channel = FixtureChannel::new(RetrievalChannelTier::Paid)
        .with_readiness(FixtureReadiness::PaidUnconfirmed);

    let result = channel.readiness();
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.to_lowercase().contains("paid") || err.to_lowercase().contains("confirmation"),
        "error should mention paid/confirmation, got: {}",
        err
    );
}

#[test]
fn paid_channel_when_ready_is_usable() {
    let decisions = vec![make_accepted("a", "https://example.com/a.jpg", 5)];
    let seq = make_sequence(decisions);

    let paid = FixtureChannel::new(RetrievalChannelTier::Paid).with_all_success(&["a"]);
    let channels: Vec<&dyn BaseRetrievalChannel> = vec![&paid];

    let outcome = execute_batch(&seq, 2, &channels);

    assert_eq!(outcome.channel_tier, RetrievalChannelTier::Paid);
    assert!(outcome.results.iter().all(|r| r.is_success()));
}

// ---------------------------------------------------------------------------
// Web fetch channel unit tests
// ---------------------------------------------------------------------------

#[test]
fn web_fetch_channel_has_correct_tier() {
    let dir = std::env::temp_dir().join("retrieval-test-tier");
    let channel = WebFetchChannel::new(&dir).expect("create");
    assert_eq!(channel.tier(), RetrievalChannelTier::WebFetch);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn web_fetch_channel_readiness_ok_by_default() {
    let dir = std::env::temp_dir().join("retrieval-test-ready");
    let channel = WebFetchChannel::new(&dir).expect("create");
    assert!(channel.readiness().is_ok());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn web_fetch_channel_disabled_readiness_fails() {
    let dir = std::env::temp_dir().join("retrieval-test-disabled");
    let channel = WebFetchChannel::new(&dir)
        .expect("create")
        .with_enabled(false);
    assert!(channel.readiness().is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn web_fetch_channel_fallback_fact() {
    let dir = std::env::temp_dir().join("retrieval-test-fb");
    let channel = WebFetchChannel::new(&dir).expect("create");
    let fact = channel.fallback_fact("timeout");
    assert_eq!(fact.failed_tier, RetrievalChannelTier::WebFetch);
    assert_eq!(fact.next_tier, Some(RetrievalChannelTier::SelfHosted));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn web_fetch_channel_missing_url_is_failure() {
    let dir = std::env::temp_dir().join("retrieval-test-missing");
    let channel = WebFetchChannel::new(&dir).expect("create");
    let batch = RetrievalBatch::new(vec!["ghost".into()], 2);
    let results = channel.retrieve_batch(&batch).expect("batch ok");
    assert_eq!(results.len(), 1);
    assert!(results[0].is_failure());
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// Fixture channel tests
// ---------------------------------------------------------------------------

#[test]
fn fixture_channel_all_success() {
    let channel =
        FixtureChannel::new(RetrievalChannelTier::WebFetch).with_all_success(&["a", "b", "c"]);

    let batch = RetrievalBatch::new(vec!["a".into(), "b".into(), "c".into()], 6);
    let results = channel.retrieve_batch(&batch).expect("batch ok");

    assert_eq!(results.len(), 3);
    assert!(results.iter().all(|r| r.is_success()));
}

#[test]
fn fixture_channel_mixed_results() {
    let channel = FixtureChannel::new(RetrievalChannelTier::SelfHosted)
        .with_response("ok", FixtureResponse::success())
        .with_response("fail", FixtureResponse::network_failure());

    let batch = RetrievalBatch::new(vec!["ok".into(), "fail".into()], 4);
    let results = channel.retrieve_batch(&batch).expect("batch ok");

    assert_eq!(results.len(), 2);
    assert!(results[0].is_success());
    assert!(results[1].is_failure());
}

#[test]
fn fixture_channel_unprogrammed_fails() {
    let channel = FixtureChannel::new(RetrievalChannelTier::WebFetch);
    let batch = RetrievalBatch::new(vec!["unknown".into()], 2);
    let results = channel.retrieve_batch(&batch).expect("batch ok");
    assert!(results[0].is_failure());
}

#[test]
fn fixture_channel_all_readiness_states() {
    let states = vec![
        (FixtureReadiness::Ready, true),
        (FixtureReadiness::Disabled, false),
        (FixtureReadiness::MissingDependency, false),
        (FixtureReadiness::Misconfigured, false),
        (FixtureReadiness::PaidUnconfirmed, false),
    ];

    for (state, expect_ok) in states {
        let channel = FixtureChannel::new(RetrievalChannelTier::WebFetch).with_readiness(state);
        assert_eq!(
            channel.readiness().is_ok(),
            expect_ok,
            "readiness state mismatch"
        );
    }
}

#[test]
fn fixture_channel_preserves_tier() {
    assert_eq!(
        FixtureChannel::new(RetrievalChannelTier::WebFetch).tier(),
        RetrievalChannelTier::WebFetch
    );
    assert_eq!(
        FixtureChannel::new(RetrievalChannelTier::SelfHosted).tier(),
        RetrievalChannelTier::SelfHosted
    );
    assert_eq!(
        FixtureChannel::new(RetrievalChannelTier::Paid).tier(),
        RetrievalChannelTier::Paid
    );
}

// ---------------------------------------------------------------------------
// Executor: edge cases
// ---------------------------------------------------------------------------

#[test]
fn executor_all_channels_exhausted_no_success() {
    let decisions = vec![make_accepted("a", "https://example.com/a.jpg", 5)];
    let seq = make_sequence(decisions);

    let web = FixtureChannel::new(RetrievalChannelTier::WebFetch)
        .with_response("a", FixtureResponse::network_failure());
    let sh = FixtureChannel::new(RetrievalChannelTier::SelfHosted)
        .with_response("a", FixtureResponse::channel_disabled());
    let channels: Vec<&dyn BaseRetrievalChannel> = vec![&web, &sh];

    let outcome = execute_batch(&seq, 2, &channels);

    assert_eq!(outcome.channels_attempted, 2);
    assert!(outcome.execution_blocked.is_some());
    assert!(outcome.results.iter().all(|r| r.is_failure()));
}

#[test]
fn executor_partial_success_stops_fallback() {
    let decisions = vec![
        make_accepted("good", "https://example.com/good.jpg", 5),
        make_accepted("bad", "https://example.com/bad.jpg", 3),
    ];
    let seq = make_sequence(decisions);

    let mixed = FixtureChannel::new(RetrievalChannelTier::WebFetch)
        .with_response("good", FixtureResponse::success())
        .with_response("bad", FixtureResponse::network_failure());
    let backup =
        FixtureChannel::new(RetrievalChannelTier::SelfHosted).with_all_success(&["good", "bad"]);
    let channels: Vec<&dyn BaseRetrievalChannel> = vec![&mixed, &backup];

    let outcome = execute_batch(&seq, 4, &channels);

    // Got some successes → no fallback needed
    assert_eq!(outcome.channels_attempted, 1);
    assert_eq!(outcome.channel_tier, RetrievalChannelTier::WebFetch);
}

#[test]
fn executor_empty_batch_returns_execution_blocked() {
    let seq = RetrievableCandidateSequence::empty();
    let channel = FixtureChannel::new(RetrievalChannelTier::WebFetch);
    let channels: Vec<&dyn BaseRetrievalChannel> = vec![&channel];

    let outcome = execute_batch(&seq, 4, &channels);

    assert!(outcome.execution_blocked.is_some());
    assert_eq!(outcome.channels_attempted, 0);
}

#[test]
fn executor_single_channel_disabled_produces_execution_blocked() {
    let decisions = vec![make_accepted("a", "https://example.com/a.jpg", 5)];
    let seq = make_sequence(decisions);

    let disabled = FixtureChannel::new(RetrievalChannelTier::WebFetch)
        .with_readiness(FixtureReadiness::Disabled);
    let channels: Vec<&dyn BaseRetrievalChannel> = vec![&disabled];

    let outcome = execute_batch(&seq, 2, &channels);

    assert_eq!(outcome.channels_attempted, 1);
    assert!(outcome.channel_attempts[0].abandoned);
}

// ---------------------------------------------------------------------------
// Readiness summary
// ---------------------------------------------------------------------------

#[test]
fn readiness_summary_reports_all_states() {
    let ready = FixtureChannel::new(RetrievalChannelTier::WebFetch);
    let disabled = FixtureChannel::new(RetrievalChannelTier::SelfHosted)
        .with_readiness(FixtureReadiness::Disabled);
    let paid = FixtureChannel::new(RetrievalChannelTier::Paid)
        .with_readiness(FixtureReadiness::PaidUnconfirmed);

    let channels: Vec<&dyn BaseRetrievalChannel> = vec![&ready, &disabled, &paid];
    let summary = summarise_channel_readiness(&channels);

    assert_eq!(summary.len(), 3);
    assert_eq!(summary[0].readiness, RetrievalChannelReadiness::Ready);
    assert_eq!(summary[0].tier, RetrievalChannelTier::WebFetch);
    assert_eq!(summary[1].readiness, RetrievalChannelReadiness::Disabled);
    assert_eq!(
        summary[2].readiness,
        RetrievalChannelReadiness::PaidUnconfirmed
    );
}

// ---------------------------------------------------------------------------
// RetrievalBatch URL mapping
// ---------------------------------------------------------------------------

#[test]
fn batch_carries_urls_for_retrieval() {
    let decisions = vec![
        make_accepted("x", "https://a.example.com/1.jpg", 5),
        make_accepted("y", "https://b.example.com/2.png", 3),
    ];
    let seq = make_sequence(decisions);
    let (batch, _) = RetrievalBatchPlanner::plan(&seq, 4);

    assert_eq!(batch.url_for("x"), Some("https://a.example.com/1.jpg"));
    assert_eq!(batch.url_for("y"), Some("https://b.example.com/2.png"));
    assert_eq!(batch.url_for("z"), None);
}

// ---------------------------------------------------------------------------
// Domain type edge cases
// ---------------------------------------------------------------------------

#[test]
fn retrieval_result_candidate_id_accessor() {
    let success =
        RetrievalResult::Success(image_retrieval::domain::retrieval::RetrievalSuccess::new(
            "c1",
            "/p.jpg",
            RetrievalChannelTier::WebFetch,
            Some("image/jpeg".into()),
            1024,
        ));
    assert_eq!(success.candidate_id(), "c1");

    let failure =
        RetrievalResult::Failure(image_retrieval::domain::retrieval::RetrievalFailure::new(
            "c2",
            RetrievalChannelTier::WebFetch,
            RetrievalFailureCategory::Network,
            "timeout",
            true,
        ));
    assert_eq!(failure.candidate_id(), "c2");
}

#[test]
fn fallback_fact_paid_requires_confirmation_flag() {
    let fact =
        FallbackEligibilityFact::new(RetrievalChannelTier::SelfHosted, "service down", false);
    // Next tier from SelfHosted is Paid → requires_paid_confirmation = true
    assert!(fact.requires_paid_confirmation);
    assert_eq!(fact.next_tier, Some(RetrievalChannelTier::Paid));
}

#[test]
fn fallback_fact_terminal_when_paid_fails() {
    let fact = FallbackEligibilityFact::new(RetrievalChannelTier::Paid, "exhausted", false);
    assert_eq!(fact.next_tier, None);
}

#[test]
fn retrieval_failure_category_allows_fallback_flag() {
    // Network failures are fallbackable
    let f = image_retrieval::domain::retrieval::RetrievalFailure::new(
        "c",
        RetrievalChannelTier::WebFetch,
        RetrievalFailureCategory::Network,
        "timeout",
        true,
    );
    assert!(f.allows_fallback);

    // Access restricted is NOT fallbackable
    let f = image_retrieval::domain::retrieval::RetrievalFailure::new(
        "c",
        RetrievalChannelTier::WebFetch,
        RetrievalFailureCategory::AccessRestricted,
        "403",
        false,
    );
    assert!(!f.allows_fallback);
}

#[test]
fn channel_readiness_display_values() {
    assert_eq!(RetrievalChannelReadiness::Ready.to_string(), "ready");
    assert_eq!(RetrievalChannelReadiness::Disabled.to_string(), "disabled");
    assert_eq!(
        RetrievalChannelReadiness::MissingDependency.to_string(),
        "missing_dependency"
    );
    assert_eq!(
        RetrievalChannelReadiness::Misconfigured.to_string(),
        "misconfigured"
    );
    assert_eq!(
        RetrievalChannelReadiness::PaidUnconfirmed.to_string(),
        "paid_unconfirmed"
    );
}
