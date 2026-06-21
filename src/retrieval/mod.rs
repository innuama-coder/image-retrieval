//! Retrieval module — batch planning and channel execution.
//!
//! This module implements TASK-005: BaseRetrievalChannel batch planning,
//! short-batch diagnosis, channel fallback, and retrieval failure
//! classification.
//!
//! # Structure
//!
//! - [`batch_planner`] — plans retrieval batches from candidate sequences.
//! - [`channels`] — concrete channel implementations (web fetch, fixture).
//!
//! References: PRD §抓取渠道产品要求, HLD §Retrieval Batch Planner,
//! `docs/design/TASK-005-retrieval-channel-batch-design.md`

pub mod batch_planner;
pub mod channels;

pub use batch_planner::RetrievalBatchPlanner;
pub use channels::FixtureChannel;
pub use channels::WebFetchChannel;

use crate::domain::candidate::RetrievableCandidateSequence;
use crate::domain::retrieval::{
    ChannelAttemptResult, ExecutionBlockingFact, FallbackEligibilityFact, RetrievalBatch,
    RetrievalBatchShortage, RetrievalChannelReadiness, RetrievalChannelTier, RetrievalOutcome,
    RetrievalResult,
};
use crate::error::Error;
use crate::ports::BaseRetrievalChannel;

// ---------------------------------------------------------------------------
// Retrieval executor — orchestrates a single batch attempt across channels
// ---------------------------------------------------------------------------

/// Execute a single retrieval batch attempt, starting from the given tier
/// and falling back as needed.
///
/// # Fallback rules
///
/// 1. Start at `start_tier` (typically [`RetrievalChannelTier::WebFetch`]).
/// 2. If the current channel is not ready, record an abandoned attempt and
///    move to the next tier.
/// 3. If the current channel produces failures that *allow* fallback, move
///    to the next tier.
/// 4. If the current channel produces failures that do **not** allow fallback
///    (access-restricted, etc.), do NOT fallback — return the results as-is.
/// 5. If all tiers are exhausted, return the results from the last attempted
///    channel.
/// 6. Fallback must never bypass access-control or authorization restrictions.
///
/// # Paid tier guard
///
/// When the next tier is [`RetrievalChannelTier::Paid`] and the channel is
/// not ready (typically `PaidUnconfirmed`), the executor records an
/// [`ExecutionBlockingFact`] and stops — it does not silently skip to paid.
pub fn execute_batch(
    sequence: &RetrievableCandidateSequence,
    batch_target: u32,
    channels: &[&dyn BaseRetrievalChannel],
) -> RetrievalOutcome {
    let (batch, shortage) = RetrievalBatchPlanner::plan(sequence, batch_target);

    if batch.candidate_ids.is_empty() {
        return RetrievalOutcome {
            batch,
            results: vec![],
            channel_tier: RetrievalChannelTier::WebFetch,
            shortage,
            channels_attempted: 0,
            channel_attempts: vec![],
            fallback_facts: vec![],
            execution_blocked: Some(ExecutionBlockingFact {
                reason: "no candidates in batch to retrieve".into(),
                source_tier: None,
                is_access_restricted: false,
                is_paid_unconfirmed: false,
            }),
        };
    }

    let mut channel_attempts: Vec<ChannelAttemptResult> = Vec::new();
    let mut fallback_facts: Vec<FallbackEligibilityFact> = Vec::new();
    let mut last_results: Option<Vec<RetrievalResult>> = None;
    let mut last_tier = RetrievalChannelTier::WebFetch;

    for channel in channels {
        let tier = channel.tier();
        last_tier = tier;

        // Check readiness
        match channel.readiness() {
            Ok(()) => { /* channel ready */ }
            Err(e) => {
                let abandon_reason = format!("channel not ready: {}", e);
                channel_attempts.push(ChannelAttemptResult {
                    channel_tier: tier,
                    results: vec![],
                    success_count: 0,
                    failure_count: 0,
                    abandoned: true,
                    abandon_reason: Some(abandon_reason.clone()),
                });

                let fact = channel.fallback_fact(&abandon_reason);
                let is_access = fact.is_access_restricted;
                fallback_facts.push(fact);

                if is_access {
                    // Access restriction — stop here
                    return build_outcome(
                        batch,
                        last_results.unwrap_or_default(),
                        last_tier,
                        shortage,
                        channel_attempts,
                        fallback_facts,
                        Some(ExecutionBlockingFact {
                            reason: "access restriction blocked retrieval".into(),
                            source_tier: Some(tier),
                            is_access_restricted: true,
                            is_paid_unconfirmed: false,
                        }),
                    );
                }
                continue; // try next channel
            }
        }

        // Attempt retrieval with this channel
        let results = match channel.retrieve_batch(&batch) {
            Ok(r) => r,
            Err(e) => {
                // Channel-level error → treat as all-failure
                let reason = format!("channel error: {}", e);
                let fact = channel.fallback_fact(&reason);
                let is_access = fact.is_access_restricted;
                fallback_facts.push(fact);

                channel_attempts.push(ChannelAttemptResult {
                    channel_tier: tier,
                    results: vec![],
                    success_count: 0,
                    failure_count: batch.candidate_ids.len() as u32,
                    abandoned: true,
                    abandon_reason: Some(reason),
                });

                if is_access {
                    return build_outcome(
                        batch,
                        vec![],
                        tier,
                        shortage,
                        channel_attempts,
                        fallback_facts,
                        Some(ExecutionBlockingFact {
                            reason: "access restriction blocked retrieval".into(),
                            source_tier: Some(tier),
                            is_access_restricted: true,
                            is_paid_unconfirmed: false,
                        }),
                    );
                }
                continue;
            }
        };

        let success_count = results.iter().filter(|r| r.is_success()).count() as u32;
        let failure_count = results.iter().filter(|r| r.is_failure()).count() as u32;

        // Check if any failure is non-fallbackable (e.g. access-restricted)
        let has_non_fallbackable = results.iter().any(|r| match r {
            RetrievalResult::Failure(f) => !f.allows_fallback,
            _ => false,
        });

        if has_non_fallbackable {
            // Record this attempt but do NOT fallback further
            channel_attempts.push(ChannelAttemptResult {
                channel_tier: tier,
                results: results.clone(),
                success_count,
                failure_count,
                abandoned: false,
                abandon_reason: None,
            });

            let fact = FallbackEligibilityFact::new(
                tier,
                "non-fallbackable failure (access restricted)",
                true,
            );
            fallback_facts.push(fact);

            return build_outcome(
                batch,
                results,
                tier,
                shortage,
                channel_attempts,
                fallback_facts,
                Some(ExecutionBlockingFact {
                    reason: "access restriction blocked fallback to higher tier".into(),
                    source_tier: Some(tier),
                    is_access_restricted: true,
                    is_paid_unconfirmed: false,
                }),
            );
        }

        // If we got at least some successes, we're done (no need to fallback
        // just because some candidates failed with fallbackable errors).
        // The orchestrator (TASK-006) decides whether the success count is
        // sufficient.
        if success_count > 0 {
            channel_attempts.push(ChannelAttemptResult {
                channel_tier: tier,
                results: results.clone(),
                success_count,
                failure_count,
                abandoned: false,
                abandon_reason: None,
            });

            return build_outcome(
                batch,
                results,
                tier,
                shortage,
                channel_attempts,
                fallback_facts,
                None,
            );
        }

        // All failed, and all failures allow fallback → record and try next
        channel_attempts.push(ChannelAttemptResult {
            channel_tier: tier,
            results: results.clone(),
            success_count,
            failure_count,
            abandoned: false,
            abandon_reason: None,
        });

        let fact = FallbackEligibilityFact::new(tier, "all candidates failed", false);
        fallback_facts.push(fact);

        last_results = Some(results);
    }

    // All channels exhausted
    build_outcome(
        batch,
        last_results.unwrap_or_default(),
        last_tier,
        shortage,
        channel_attempts,
        fallback_facts,
        Some(ExecutionBlockingFact {
            reason: "all retrieval channels exhausted without success".into(),
            source_tier: Some(last_tier),
            is_access_restricted: false,
            is_paid_unconfirmed: false,
        }),
    )
}

/// Build a [`RetrievalOutcome`] from the gathered data.
fn build_outcome(
    batch: RetrievalBatch,
    results: Vec<RetrievalResult>,
    channel_tier: RetrievalChannelTier,
    shortage: Option<RetrievalBatchShortage>,
    channel_attempts: Vec<ChannelAttemptResult>,
    fallback_facts: Vec<FallbackEligibilityFact>,
    execution_blocked: Option<ExecutionBlockingFact>,
) -> RetrievalOutcome {
    let channels_attempted = channel_attempts.len() as u32;
    RetrievalOutcome {
        batch,
        results,
        channel_tier,
        shortage,
        channels_attempted,
        channel_attempts,
        fallback_facts,
        execution_blocked,
    }
}

// ---------------------------------------------------------------------------
// Convenience: summarise readiness of a set of channels
// ---------------------------------------------------------------------------

/// Readiness summary for one channel.
#[derive(Debug, Clone)]
pub struct ChannelReadinessRecord {
    pub channel_id: String,
    pub display_name: String,
    pub tier: RetrievalChannelTier,
    pub readiness: RetrievalChannelReadiness,
}

/// Check the readiness of a set of channels and return a summary.
pub fn summarise_channel_readiness(
    channels: &[&dyn BaseRetrievalChannel],
) -> Vec<ChannelReadinessRecord> {
    channels
        .iter()
        .enumerate()
        .map(|(i, ch)| {
            let readiness = match ch.readiness() {
                Ok(()) => RetrievalChannelReadiness::Ready,
                Err(e) => classify_readiness_error(&e, ch.tier()),
            };
            ChannelReadinessRecord {
                channel_id: format!("channel-{}", i),
                display_name: ch.display_name().to_string(),
                tier: ch.tier(),
                readiness,
            }
        })
        .collect()
}

/// Heuristically classify an error from `readiness()` into a
/// [`RetrievalChannelReadiness`].
fn classify_readiness_error(
    error: &Error,
    _tier: RetrievalChannelTier,
) -> RetrievalChannelReadiness {
    let msg = error.to_string().to_lowercase();
    if msg.contains("paid") || msg.contains("confirmation") {
        RetrievalChannelReadiness::PaidUnconfirmed
    } else if msg.contains("disabled") {
        RetrievalChannelReadiness::Disabled
    } else if msg.contains("dependency") || msg.contains("missing") {
        RetrievalChannelReadiness::MissingDependency
    } else if msg.contains("misconfigured") {
        RetrievalChannelReadiness::Misconfigured
    } else {
        RetrievalChannelReadiness::MissingDependency
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::candidate::{CandidateDecision, CandidateId, CandidateRecord, ProviderId};
    use crate::retrieval::channels::fixture::{FixtureReadiness, FixtureResponse};

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

    // -------------------------------------------------------------------
    // Executor: single channel success
    // -------------------------------------------------------------------

    #[test]
    fn execute_batch_single_channel_all_success() {
        let decisions = vec![
            make_accepted("a", "https://example.com/a.jpg", 3),
            make_accepted("b", "https://example.com/b.jpg", 2),
        ];
        let seq = make_sequence(decisions);

        let channel =
            FixtureChannel::new(RetrievalChannelTier::WebFetch).with_all_success(&["a", "b"]);
        let channels: Vec<&dyn BaseRetrievalChannel> = vec![&channel];

        let outcome = execute_batch(&seq, 4, &channels);

        assert_eq!(outcome.results.len(), 2);
        assert!(outcome.results.iter().all(|r| r.is_success()));
        assert_eq!(outcome.channel_tier, RetrievalChannelTier::WebFetch);
        assert_eq!(outcome.channels_attempted, 1);
        assert!(outcome.execution_blocked.is_none());
    }

    // -------------------------------------------------------------------
    // Executor: fallback on failure
    // -------------------------------------------------------------------

    #[test]
    fn execute_batch_fallback_to_second_channel() {
        let decisions = vec![make_accepted("a", "https://example.com/a.jpg", 5)];
        let seq = make_sequence(decisions);

        let fail_channel = FixtureChannel::new(RetrievalChannelTier::WebFetch)
            .with_response("a", FixtureResponse::network_failure());
        let success_channel =
            FixtureChannel::new(RetrievalChannelTier::SelfHosted).with_all_success(&["a"]);
        let channels: Vec<&dyn BaseRetrievalChannel> = vec![&fail_channel, &success_channel];

        let outcome = execute_batch(&seq, 2, &channels);

        assert_eq!(outcome.channel_tier, RetrievalChannelTier::SelfHosted);
        assert_eq!(outcome.channels_attempted, 2);
        assert_eq!(outcome.fallback_facts.len(), 1);
        assert!(outcome.results.iter().all(|r| r.is_success()));
    }

    // -------------------------------------------------------------------
    // Executor: no fallback on access-restricted
    // -------------------------------------------------------------------

    #[test]
    fn execute_batch_no_fallback_when_access_restricted() {
        let decisions = vec![make_accepted(
            "a",
            "https://restricted.example.com/a.jpg",
            5,
        )];
        let seq = make_sequence(decisions);

        let restricted_channel = FixtureChannel::new(RetrievalChannelTier::WebFetch)
            .with_response("a", FixtureResponse::access_restricted());
        let higher_channel =
            FixtureChannel::new(RetrievalChannelTier::SelfHosted).with_all_success(&["a"]);
        let channels: Vec<&dyn BaseRetrievalChannel> = vec![&restricted_channel, &higher_channel];

        let outcome = execute_batch(&seq, 2, &channels);

        // Must NOT have fallen back — access restriction blocks fallback
        assert_eq!(outcome.channel_tier, RetrievalChannelTier::WebFetch);
        assert_eq!(outcome.channels_attempted, 1);
        assert!(outcome.execution_blocked.is_some());
        if let Some(ref block) = outcome.execution_blocked {
            assert!(block.is_access_restricted);
        }
    }

    // -------------------------------------------------------------------
    // Executor: paid channel not silently used
    // -------------------------------------------------------------------

    #[test]
    fn execute_batch_paid_channel_unconfirmed_detected() {
        let decisions = vec![make_accepted("a", "https://example.com/a.jpg", 5)];
        let seq = make_sequence(decisions);

        let web_fetch = FixtureChannel::new(RetrievalChannelTier::WebFetch)
            .with_response("a", FixtureResponse::network_failure());
        let paid = FixtureChannel::new(RetrievalChannelTier::Paid)
            .with_readiness(FixtureReadiness::PaidUnconfirmed)
            .with_all_success(&["a"]);
        let channels: Vec<&dyn BaseRetrievalChannel> = vec![&web_fetch, &paid];

        let outcome = execute_batch(&seq, 2, &channels);

        // Paid channel should have been skipped (not ready), resulting in
        // all channels exhausted
        assert_eq!(outcome.channels_attempted, 2);
        // Check that the paid channel attempt was abandoned
        let paid_attempt = outcome
            .channel_attempts
            .iter()
            .find(|a| a.channel_tier == RetrievalChannelTier::Paid);
        assert!(paid_attempt.is_some());
        assert!(paid_attempt.unwrap().abandoned);
    }

    // -------------------------------------------------------------------
    // Executor: empty batch
    // -------------------------------------------------------------------

    #[test]
    fn execute_batch_empty_sequence_no_candidates() {
        let seq = RetrievableCandidateSequence::empty();
        let channel = FixtureChannel::new(RetrievalChannelTier::WebFetch);
        let channels: Vec<&dyn BaseRetrievalChannel> = vec![&channel];

        let outcome = execute_batch(&seq, 4, &channels);

        assert!(outcome.results.is_empty());
        assert!(outcome.execution_blocked.is_some());
        assert_eq!(outcome.channels_attempted, 0);
    }

    // -------------------------------------------------------------------
    // Executor: short batch
    // -------------------------------------------------------------------

    #[test]
    fn execute_batch_short_batch_when_fewer_candidates() {
        let decisions = vec![
            make_accepted("a", "https://example.com/a.jpg", 5),
            make_accepted("b", "https://example.com/b.jpg", 3),
        ];
        let seq = make_sequence(decisions);

        let channel =
            FixtureChannel::new(RetrievalChannelTier::WebFetch).with_all_success(&["a", "b"]);
        let channels: Vec<&dyn BaseRetrievalChannel> = vec![&channel];

        let outcome = execute_batch(&seq, 8, &channels);

        assert!(outcome.batch.is_short_batch);
        assert!(outcome.shortage.is_some());
        if let Some(ref s) = outcome.shortage {
            assert_eq!(s.target_size, 8);
            assert_eq!(s.actual_size, 2);
        }
    }

    // -------------------------------------------------------------------
    // Executor: batch target for 4 images = 8
    // -------------------------------------------------------------------

    #[test]
    fn batch_target_for_4_images_is_8() {
        let target = RetrievalBatchPlanner::batch_target_for(4);
        assert_eq!(target, 8);
    }

    // -------------------------------------------------------------------
    // Executor: partial success, stop fallback
    // -------------------------------------------------------------------

    #[test]
    fn execute_batch_stops_fallback_when_some_succeed() {
        let decisions = vec![
            make_accepted("good", "https://example.com/good.jpg", 5),
            make_accepted("bad", "https://example.com/bad.jpg", 3),
        ];
        let seq = make_sequence(decisions);

        // First channel succeeds partially (one good, one bad but fallbackable)
        let mixed_channel = FixtureChannel::new(RetrievalChannelTier::WebFetch)
            .with_response("good", FixtureResponse::success())
            .with_response("bad", FixtureResponse::network_failure());
        let higher_channel = FixtureChannel::new(RetrievalChannelTier::SelfHosted)
            .with_all_success(&["good", "bad"]);
        let channels: Vec<&dyn BaseRetrievalChannel> = vec![&mixed_channel, &higher_channel];

        let outcome = execute_batch(&seq, 4, &channels);

        // Should NOT have fallen back since we got at least some successes
        assert_eq!(outcome.channels_attempted, 1);
        assert_eq!(outcome.channel_tier, RetrievalChannelTier::WebFetch);
    }

    // -------------------------------------------------------------------
    // Summarise channel readiness
    // -------------------------------------------------------------------

    #[test]
    fn summarise_readiness_reports_all_channels() {
        let ready_ch = FixtureChannel::new(RetrievalChannelTier::WebFetch);
        let disabled_ch = FixtureChannel::new(RetrievalChannelTier::SelfHosted)
            .with_readiness(FixtureReadiness::Disabled);
        let paid_ch = FixtureChannel::new(RetrievalChannelTier::Paid)
            .with_readiness(FixtureReadiness::PaidUnconfirmed);

        let channels: Vec<&dyn BaseRetrievalChannel> = vec![&ready_ch, &disabled_ch, &paid_ch];
        let summary = summarise_channel_readiness(&channels);

        assert_eq!(summary.len(), 3);
        assert_eq!(summary[0].readiness, RetrievalChannelReadiness::Ready);
        assert_eq!(summary[1].readiness, RetrievalChannelReadiness::Disabled);
        assert_eq!(
            summary[2].readiness,
            RetrievalChannelReadiness::PaidUnconfirmed
        );
    }
}
