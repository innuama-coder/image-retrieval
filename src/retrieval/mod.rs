//! Retrieval module — batch planning and channel fallback execution (v1.1).
//!
//! This module implements retrieval fallback execution across channels:

#![allow(clippy::too_many_arguments)]
//!
//! 1. `normal_web_fetch.direct_image_fetch`
//! 2. `normal_web_fetch.source_page_resolve`
//! 3. `self_hosted_service`
//! 4. `paid_online_service`
//!
//! # Structure
//!
//! - [`batch_planner`] — plans retrieval batches from TASK-003 retrievable candidates.
//! - [`channels`] — concrete channel implementations.
//! - [`RetrievalExecutor`] — fallback execution orchestrator.
//!
//! References: PRD FR-008/FR-009, LLD §Retrieval,
//! `docs/design/v1.1-TASK-004-retrieval-artifact-channel-design.md`

pub mod batch_planner;
pub mod channels;

pub use batch_planner::RetrievalBatchPlanner;
pub use channels::fixture::FixtureResponse;
pub use channels::FixtureChannel;
pub use channels::PaidChannel;
pub use channels::SelfHostedChannel;
pub use channels::WebFetchChannel;

use crate::domain::candidate::RetrievableCandidateBatch;
use crate::domain::config::RetrievalChannelConfig;
use crate::domain::query_plan::{QueryPlanId, QueryRetrievalPolicy};
use crate::domain::retrieval::{
    FallbackDecisionKind, RetrievalArtifactResult, RetrievalAttemptMode, RetrievalAttemptStatus,
    RetrievalAttemptTrace, RetrievalBatch, RetrievalBatchResult, RetrievalChannelId,
    RetrievalChannelReadinessReport, RetrievalChannelTier, RetrievalDiagnostic,
    RetrievalExecutionBlock, RetrievalFailureCode, RetrievalFallbackDecision, RetrievalSeverity,
    RetrievalStatus,
};
use crate::ports::BaseRetrievalChannel;

/// Fallback order for retrieval attempts.
const FALLBACK_ORDER: &[(RetrievalChannelTier, RetrievalAttemptMode)] = &[
    (
        RetrievalChannelTier::NormalWebFetch,
        RetrievalAttemptMode::DirectImageFetch,
    ),
    (
        RetrievalChannelTier::NormalWebFetch,
        RetrievalAttemptMode::SourcePageResolve,
    ),
    (
        RetrievalChannelTier::SelfHostedService,
        RetrievalAttemptMode::SelfHostedService,
    ),
    (
        RetrievalChannelTier::PaidOnlineService,
        RetrievalAttemptMode::PaidOnlineService,
    ),
];

// ---------------------------------------------------------------------------
// Retrieval executor
// ---------------------------------------------------------------------------

/// Executes retrieval across channels with fallback.
///
/// For each job in the batch, attempts channels in fallback order.
/// Records attempt traces, fallback decisions, policy blocks, and
/// produces a complete [`RetrievalBatchResult`].
pub struct RetrievalExecutor;

impl RetrievalExecutor {
    /// Execute a retrieval batch using the configured channels.
    ///
    /// Returns a [`RetrievalBatchResult`] with per-job artifact evidence.
    pub fn execute(
        batch: &RetrievalBatch,
        channels: &[&dyn BaseRetrievalChannel],
        channel_configs: &[RetrievalChannelConfig],
        retrieval_policy: &QueryRetrievalPolicy,
        _robots_unknown_behavior: &str,
        _prohibited_domains: &[String],
        fixture_mode: bool,
    ) -> RetrievalBatchResult {
        let mut all_results: Vec<RetrievalArtifactResult> = Vec::new();
        let mut all_traces: Vec<RetrievalAttemptTrace> = Vec::new();
        let mut all_fallback_decisions: Vec<RetrievalFallbackDecision> = Vec::new();
        let mut all_diagnostics: Vec<RetrievalDiagnostic> = Vec::new();
        let mut execution_blocks: Vec<RetrievalExecutionBlock> = Vec::new();

        // Evaluate channel readiness
        let channel_readiness: Vec<RetrievalChannelReadinessReport> = channels
            .iter()
            .zip(channel_configs.iter())
            .map(|(ch, cfg)| ch.readiness(cfg))
            .collect();

        // Check for fixture-in-production
        if !fixture_mode {
            for ch in channels {
                let caps = ch.capabilities();
                if caps.fixture_only {
                    execution_blocks.push(RetrievalExecutionBlock {
                        query_plan_id: batch.query_plan_id.clone(),
                        retrieval_batch_id: Some(batch.retrieval_batch_id.clone()),
                        dependency: format!("{} (fixture channel)", ch.display_name()),
                        failure_code: RetrievalFailureCode::RetrievalFixtureNotProduction,
                        reason: format!(
                            "Fixture channel '{}' cannot be used in production mode.",
                            ch.display_name()
                        ),
                        is_permanent: true,
                        pending_job_count: batch.jobs.len(),
                    });
                }
            }
        }

        // Build a lookup: tier -> channel
        let channel_map: std::collections::BTreeMap<
            RetrievalChannelTier,
            &dyn BaseRetrievalChannel,
        > = channels.iter().map(|ch| (ch.tier(), *ch)).collect();

        // Process each job through fallback order
        for job in &batch.jobs {
            let mut job_complete = false;
            let mut job_traces: Vec<RetrievalAttemptTrace> = Vec::new();

            for &(tier, mode) in FALLBACK_ORDER {
                if job_complete {
                    break;
                }

                // Check if this tier is available
                let channel = match channel_map.get(&tier) {
                    Some(ch) => *ch,
                    None => continue,
                };

                // Check channel readiness
                let is_ready = channel_readiness
                    .iter()
                    .any(|r| r.channel_id == channel.channel_id() && r.available);

                // For paid tier, check policy
                if tier == RetrievalChannelTier::PaidOnlineService && !retrieval_policy.allow_paid {
                    let fb = RetrievalFallbackDecision {
                        retrieval_job_id: job.retrieval_job_id.clone(),
                        from_tier: get_previous_tier(tier),
                        from_attempt_mode: get_previous_mode(tier, mode),
                        to_tier: Some(tier),
                        to_attempt_mode: Some(mode),
                        decision: FallbackDecisionKind::StopPaidUnconfirmed,
                        reason_code: RetrievalFailureCode::RetrievalPaidUnconfirmed,
                        policy_reason: Some(
                            "Paid retrieval is not allowed by QueryPlan policy.".into(),
                        ),
                    };
                    all_fallback_decisions.push(fb);

                    all_diagnostics.push(
                        RetrievalDiagnostic::new(
                            RetrievalFailureCode::RetrievalPaidUnconfirmed,
                            RetrievalSeverity::Blocker,
                            &job.query_plan_id,
                            "Paid channel skipped: not allowed by policy",
                        )
                        .with_job(job.retrieval_job_id.to_string())
                        .with_candidate(&job.candidate_id),
                    );
                    break;
                }

                if !is_ready {
                    // Record abandoned attempt
                    let trace = RetrievalAttemptTrace {
                        attempt_id: format!("attempt-{}-{}-abandoned", job.retrieval_job_id, tier),
                        retrieval_job_id: job.retrieval_job_id.clone(),
                        query_plan_id: job.query_plan_id.clone(),
                        candidate_id: job.candidate_id.clone(),
                        channel_id: channel.channel_id(),
                        channel_tier: tier,
                        attempt_mode: mode,
                        started_at: String::new(),
                        completed_at: None,
                        target_url_redacted: None,
                        source_page_url_redacted: None,
                        final_url_redacted: None,
                        http_status: None,
                        bytes_received: None,
                        status: RetrievalAttemptStatus::Abandoned,
                        failure_code: Some(RetrievalFailureCode::RetrievalChannelDisabled),
                        retryable: true,
                        fallback_allowed: true,
                        policy_reason: Some(format!("Channel {} not ready", tier)),
                        artifact_refs: vec![],
                        redaction_applied: false,
                    };
                    job_traces.push(trace);
                    continue;
                }

                // Attempt retrieval through this channel
                // The channel's retrieve_batch processes all jobs, but we only
                // want the result for this specific job. We create a single-job
                // sub-batch for the channel.
                let sub_batch = RetrievalBatch::new(
                    format!("{}-{}", batch.retrieval_batch_id, job.retrieval_job_id),
                    &batch.query_plan_id,
                    batch.full_attempt_count,
                    batch.retry_count,
                    1,
                    vec![job.clone()],
                    None,
                );

                match channel.retrieve_batch(&sub_batch) {
                    Ok(sub_result) => {
                        if let Some(job_result) = sub_result.results.first() {
                            let mut result = job_result.clone();
                            result.retrieval_batch_id = batch.retrieval_batch_id.clone();

                            if !result.fetch_trace.is_empty() {
                                job_traces.extend(result.fetch_trace.clone());
                            }

                            if result.is_complete() {
                                job_complete = true;
                                all_results.push(result);
                            } else if result.retrieval_status == RetrievalStatus::AccessRestricted {
                                // Access-restricted: record and stop fallback
                                let fb = RetrievalFallbackDecision {
                                    retrieval_job_id: job.retrieval_job_id.clone(),
                                    from_tier: tier,
                                    from_attempt_mode: mode,
                                    to_tier: None,
                                    to_attempt_mode: None,
                                    decision: FallbackDecisionKind::StopAccessRestricted,
                                    reason_code: RetrievalFailureCode::RetrievalAccessRestricted,
                                    policy_reason: Some(
                                        "Access restricted; cannot escalate to higher tier.".into(),
                                    ),
                                };
                                all_fallback_decisions.push(fb);
                                all_results.push(result);
                                job_complete = true;
                            } else {
                                // Failed but fallbackable
                                if let Some(next) = next_fallback(tier, mode) {
                                    let fb = RetrievalFallbackDecision {
                                        retrieval_job_id: job.retrieval_job_id.clone(),
                                        from_tier: tier,
                                        from_attempt_mode: mode,
                                        to_tier: Some(next.0),
                                        to_attempt_mode: Some(next.1),
                                        decision: FallbackDecisionKind::Proceed,
                                        reason_code: result
                                            .failure_reason
                                            .as_ref()
                                            .map(|f| f.code.clone())
                                            .unwrap_or(RetrievalFailureCode::RetrievalUnavailable),
                                        policy_reason: None,
                                    };
                                    all_fallback_decisions.push(fb);
                                } else {
                                    let fb = RetrievalFallbackDecision {
                                        retrieval_job_id: job.retrieval_job_id.clone(),
                                        from_tier: tier,
                                        from_attempt_mode: mode,
                                        to_tier: None,
                                        to_attempt_mode: None,
                                        decision: FallbackDecisionKind::StopNoHigherTier,
                                        reason_code: RetrievalFailureCode::RetrievalUnavailable,
                                        policy_reason: Some(
                                            "No higher tier available for fallback.".into(),
                                        ),
                                    };
                                    all_fallback_decisions.push(fb);
                                    all_results.push(result);
                                    job_complete = true;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        let failure_code = e.to_failure_code();
                        let allows_fallback = e.allows_fallback();
                        let trace = RetrievalAttemptTrace {
                            attempt_id: format!("attempt-{}-{}-error", job.retrieval_job_id, tier),
                            retrieval_job_id: job.retrieval_job_id.clone(),
                            query_plan_id: job.query_plan_id.clone(),
                            candidate_id: job.candidate_id.clone(),
                            channel_id: channel.channel_id(),
                            channel_tier: tier,
                            attempt_mode: mode,
                            started_at: String::new(),
                            completed_at: None,
                            target_url_redacted: None,
                            source_page_url_redacted: None,
                            final_url_redacted: None,
                            http_status: None,
                            bytes_received: None,
                            status: RetrievalAttemptStatus::Failed,
                            failure_code: Some(failure_code.clone()),
                            retryable: allows_fallback,
                            fallback_allowed: allows_fallback,
                            policy_reason: None,
                            artifact_refs: vec![],
                            redaction_applied: false,
                        };
                        job_traces.push(trace.clone());

                        if !allows_fallback {
                            let failed_result = RetrievalArtifactResult::failed(
                                job,
                                &batch.retrieval_batch_id,
                                channel.channel_id(),
                                tier,
                                mode,
                                e.to_string(),
                                failure_code,
                                vec![trace],
                                vec![],
                            );
                            let fb = RetrievalFallbackDecision {
                                retrieval_job_id: job.retrieval_job_id.clone(),
                                from_tier: tier,
                                from_attempt_mode: mode,
                                to_tier: None,
                                to_attempt_mode: None,
                                decision: FallbackDecisionKind::StopAccessRestricted,
                                reason_code: RetrievalFailureCode::RetrievalAccessRestricted,
                                policy_reason: Some(
                                    "Access restricted; cannot escalate to higher tier.".into(),
                                ),
                            };
                            all_fallback_decisions.push(fb);
                            all_results.push(failed_result);
                            job_complete = true;
                        }
                    }
                }
            }

            // If job never completed and we ran out of fallback options
            if !job_complete {
                let failed_result = RetrievalArtifactResult::failed(
                    job,
                    &batch.retrieval_batch_id,
                    RetrievalChannelId::new("exhausted"),
                    RetrievalChannelTier::NormalWebFetch,
                    RetrievalAttemptMode::DirectImageFetch,
                    "all retrieval channels exhausted without success",
                    RetrievalFailureCode::RetrievalUnavailable,
                    job_traces.clone(),
                    vec![],
                );
                all_results.push(failed_result);
            }

            all_traces.extend(job_traces);
        }

        RetrievalBatchResult::new(
            &batch.retrieval_batch_id,
            &batch.query_plan_id,
            batch.full_attempt_count,
            batch.retry_count,
            batch.target_size,
            channel_readiness,
            all_results,
            all_traces,
            all_fallback_decisions,
            batch.shortage.clone(),
            execution_blocks,
            all_diagnostics,
        )
    }
}

// ---------------------------------------------------------------------------
// Fallback order helpers
// ---------------------------------------------------------------------------

/// Get the previous tier in fallback order.
fn get_previous_tier(current: RetrievalChannelTier) -> RetrievalChannelTier {
    match current {
        RetrievalChannelTier::SelfHostedService => RetrievalChannelTier::NormalWebFetch,
        RetrievalChannelTier::PaidOnlineService => RetrievalChannelTier::SelfHostedService,
        _ => RetrievalChannelTier::NormalWebFetch,
    }
}

/// Get the previous attempt mode for fallback decision recording.
fn get_previous_mode(
    tier: RetrievalChannelTier,
    mode: RetrievalAttemptMode,
) -> RetrievalAttemptMode {
    match (tier, mode) {
        (RetrievalChannelTier::NormalWebFetch, RetrievalAttemptMode::SourcePageResolve) => {
            RetrievalAttemptMode::DirectImageFetch
        }
        (RetrievalChannelTier::SelfHostedService, _) => RetrievalAttemptMode::SourcePageResolve,
        (RetrievalChannelTier::PaidOnlineService, _) => RetrievalAttemptMode::SelfHostedService,
        _ => mode,
    }
}

/// Return the next (tier, mode) in fallback order, or None if exhausted.
fn next_fallback(
    tier: RetrievalChannelTier,
    mode: RetrievalAttemptMode,
) -> Option<(RetrievalChannelTier, RetrievalAttemptMode)> {
    for i in 0..FALLBACK_ORDER.len() {
        if FALLBACK_ORDER[i] == (tier, mode) {
            if i + 1 < FALLBACK_ORDER.len() {
                return Some(FALLBACK_ORDER[i + 1]);
            }
            return None;
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Convenience: plan and execute in one call
// ---------------------------------------------------------------------------

/// Plan a batch from TASK-003 candidates and execute retrieval.
pub fn plan_and_execute(
    retrievable_batch: &RetrievableCandidateBatch,
    query_plan_id: &QueryPlanId,
    retrieval_policy: &QueryRetrievalPolicy,
    robots_unknown_behavior: &str,
    prohibited_domains: &[String],
    fixture_mode: bool,
    channels: &[&dyn BaseRetrievalChannel],
    channel_configs: &[RetrievalChannelConfig],
) -> RetrievalBatchResult {
    let (batch, _shortage) = RetrievalBatchPlanner::plan_from_batch(
        retrievable_batch,
        query_plan_id,
        retrieval_policy,
        robots_unknown_behavior,
        prohibited_domains,
        fixture_mode,
    );

    RetrievalExecutor::execute(
        &batch,
        channels,
        channel_configs,
        retrieval_policy,
        robots_unknown_behavior,
        prohibited_domains,
        fixture_mode,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::candidate::{
        CandidateId, CandidateQualityDecision, CandidateQualityStatus, CandidateRecord, ProviderId,
        RetrievableCandidate, RetrievableCandidateBatch,
    };
    use crate::domain::config::RetrievalChannelConfig;
    use crate::retrieval::channels::fixture::FixtureChannel;

    fn make_retrievable(id: &str, image_url: &str, priority: u32) -> RetrievableCandidate {
        let rec =
            CandidateRecord::minimal(CandidateId::new(id), ProviderId::new("test"), image_url);
        RetrievableCandidate {
            candidate: rec,
            candidate_quality_decision: CandidateQualityDecision {
                candidate_id: CandidateId::new(id),
                query_plan_id: "qp-test".into(),
                mechanical_passed: true,
                vlm_passed: true,
                final_status: CandidateQualityStatus::Retrievable,
                priority,
                blocking_metrics: vec![],
                reference_metrics: vec![],
                vlm_decision: None,
                diagnostics: vec![],
            },
            retrieval_priority: priority,
            primary_image_url: image_url.into(),
            source_page_url: None,
            thumbnail_url: None,
            expected_mime_type: Some("image/jpeg".into()),
            license_hint: None,
            provenance_refs: vec![],
        }
    }

    fn make_batch(candidates: Vec<RetrievableCandidate>) -> RetrievableCandidateBatch {
        let len = candidates.len() as u32;
        RetrievableCandidateBatch {
            query_plan_id: "qp-test".into(),
            full_attempt_count: 1,
            retry_count: 0,
            retrieval_batch_target: len * 2,
            candidates,
            rejected_decisions: vec![],
            execution_blocking_facts: vec![],
        }
    }

    fn default_config(tier: RetrievalChannelTier) -> RetrievalChannelConfig {
        RetrievalChannelConfig {
            channel_id: format!("test-{}", tier),
            channel_kind: crate::domain::config::RetrievalChannelKind::NormalWebFetch,
            tier,
            enabled: true,
            endpoint: None,
            credential_env: None,
            max_batch_size: None,
        }
    }

    // -------------------------------------------------------------------
    // Fallback order tests
    // -------------------------------------------------------------------

    #[test]
    fn fallback_order_is_correct() {
        assert_eq!(FALLBACK_ORDER[0].0, RetrievalChannelTier::NormalWebFetch);
        assert_eq!(FALLBACK_ORDER[0].1, RetrievalAttemptMode::DirectImageFetch);
        assert_eq!(FALLBACK_ORDER[1].1, RetrievalAttemptMode::SourcePageResolve);
        assert_eq!(FALLBACK_ORDER[2].0, RetrievalChannelTier::SelfHostedService);
        assert_eq!(FALLBACK_ORDER[3].0, RetrievalChannelTier::PaidOnlineService);
    }

    #[test]
    fn next_fallback_chain() {
        assert_eq!(
            next_fallback(
                RetrievalChannelTier::NormalWebFetch,
                RetrievalAttemptMode::DirectImageFetch
            ),
            Some((
                RetrievalChannelTier::NormalWebFetch,
                RetrievalAttemptMode::SourcePageResolve
            ))
        );
        assert_eq!(
            next_fallback(
                RetrievalChannelTier::PaidOnlineService,
                RetrievalAttemptMode::PaidOnlineService
            ),
            None
        );
    }

    // -------------------------------------------------------------------
    // Executor: fixture channel success
    // -------------------------------------------------------------------

    #[test]
    fn executor_fixture_single_success() {
        let candidates = vec![make_retrievable("a", "https://example.com/a.jpg", 5)];
        let rb = RetrievableCandidateBatch {
            retrieval_batch_target: 2,
            ..make_batch(candidates)
        };

        let qp_id = QueryPlanId::new("qp-test");
        let policy = QueryRetrievalPolicy::default();
        let channel = FixtureChannel::new(RetrievalChannelTier::NormalWebFetch).with_response(
            "a",
            crate::retrieval::channels::fixture::FixtureResponse::success(),
        );
        let channels: Vec<&dyn BaseRetrievalChannel> = vec![&channel];
        let configs = vec![default_config(RetrievalChannelTier::NormalWebFetch)];

        let result = plan_and_execute(
            &rb,
            &qp_id,
            &policy,
            "warn",
            &[],
            true, // fixture mode
            &channels,
            &configs,
        );

        assert_eq!(result.results.len(), 1);
        assert!(result.results[0].is_complete());
    }

    // -------------------------------------------------------------------
    // Executor: fixture channel failure with fallback
    // -------------------------------------------------------------------

    #[test]
    fn executor_fallback_on_failure() {
        let candidates = vec![make_retrievable("a", "https://example.com/a.jpg", 5)];
        let rb = RetrievableCandidateBatch {
            retrieval_batch_target: 2,
            ..make_batch(candidates)
        };

        let qp_id = QueryPlanId::new("qp-test");
        let policy = QueryRetrievalPolicy::default();

        let fail_channel = FixtureChannel::new(RetrievalChannelTier::NormalWebFetch).with_response(
            "a",
            crate::retrieval::channels::fixture::FixtureResponse::network_failure(),
        );
        let success_channel = FixtureChannel::new(RetrievalChannelTier::SelfHostedService)
            .with_response(
                "a",
                crate::retrieval::channels::fixture::FixtureResponse::success(),
            );

        let channels: Vec<&dyn BaseRetrievalChannel> = vec![&fail_channel, &success_channel];
        let configs = vec![
            default_config(RetrievalChannelTier::NormalWebFetch),
            default_config(RetrievalChannelTier::SelfHostedService),
        ];

        let result = plan_and_execute(&rb, &qp_id, &policy, "warn", &[], true, &channels, &configs);

        assert_eq!(result.results.len(), 1);
        // Should have succeeded via fallback
        assert!(result.results[0].is_complete());
        assert!(!result.fallback_decisions.is_empty());
    }

    // -------------------------------------------------------------------
    // Executor: access-restricted stops fallback
    // -------------------------------------------------------------------

    #[test]
    fn executor_no_fallback_on_access_restricted() {
        let candidates = vec![make_retrievable(
            "a",
            "https://restricted.example.com/a.jpg",
            5,
        )];
        let rb = RetrievableCandidateBatch {
            retrieval_batch_target: 2,
            ..make_batch(candidates)
        };

        let qp_id = QueryPlanId::new("qp-test");
        let policy = QueryRetrievalPolicy::default();

        let restricted_channel = FixtureChannel::new(RetrievalChannelTier::NormalWebFetch)
            .with_response(
                "a",
                crate::retrieval::channels::fixture::FixtureResponse::access_restricted(),
            );
        let success_channel = FixtureChannel::new(RetrievalChannelTier::SelfHostedService)
            .with_response(
                "a",
                crate::retrieval::channels::fixture::FixtureResponse::success(),
            );

        let channels: Vec<&dyn BaseRetrievalChannel> = vec![&restricted_channel, &success_channel];
        let configs = vec![
            default_config(RetrievalChannelTier::NormalWebFetch),
            default_config(RetrievalChannelTier::SelfHostedService),
        ];

        let result = plan_and_execute(&rb, &qp_id, &policy, "warn", &[], true, &channels, &configs);

        // Must NOT have fallen back — access restriction blocks fallback
        assert!(!result.results[0].is_complete());
    }

    // -------------------------------------------------------------------
    // Executor: paid channel skipped when not allowed
    // -------------------------------------------------------------------

    #[test]
    fn executor_paid_skipped_when_not_allowed() {
        let candidates = vec![make_retrievable("a", "https://example.com/a.jpg", 5)];
        let rb = RetrievableCandidateBatch {
            retrieval_batch_target: 2,
            ..make_batch(candidates)
        };

        let qp_id = QueryPlanId::new("qp-test");
        let policy = QueryRetrievalPolicy {
            allow_paid: false,
            ..Default::default()
        };

        let fail_channel = FixtureChannel::new(RetrievalChannelTier::NormalWebFetch).with_response(
            "a",
            crate::retrieval::channels::fixture::FixtureResponse::network_failure(),
        );
        let paid_channel = FixtureChannel::new(RetrievalChannelTier::PaidOnlineService)
            .with_response(
                "a",
                crate::retrieval::channels::fixture::FixtureResponse::success(),
            );

        let channels: Vec<&dyn BaseRetrievalChannel> = vec![&fail_channel, &paid_channel];
        let configs = vec![
            default_config(RetrievalChannelTier::NormalWebFetch),
            default_config(RetrievalChannelTier::PaidOnlineService),
        ];

        let result = plan_and_execute(&rb, &qp_id, &policy, "warn", &[], true, &channels, &configs);

        // Paid channel should be skipped, job should fail
        assert!(!result.results[0].is_complete());
    }

    // -------------------------------------------------------------------
    // Executor: metadata-only rejected
    // -------------------------------------------------------------------

    #[test]
    fn executor_metadata_only_rejected() {
        let candidates = vec![make_retrievable("a", "https://example.com/a.jpg", 5)];
        let rb = RetrievableCandidateBatch {
            retrieval_batch_target: 2,
            ..make_batch(candidates)
        };

        let qp_id = QueryPlanId::new("qp-test");
        let policy = QueryRetrievalPolicy::default();

        let metadata_channel = FixtureChannel::new(RetrievalChannelTier::NormalWebFetch)
            .with_response(
                "a",
                crate::retrieval::channels::fixture::FixtureResponse::metadata_only(),
            );

        let channels: Vec<&dyn BaseRetrievalChannel> = vec![&metadata_channel];
        let configs = vec![default_config(RetrievalChannelTier::NormalWebFetch)];

        let result = plan_and_execute(&rb, &qp_id, &policy, "warn", &[], true, &channels, &configs);

        assert!(!result.results[0].is_complete());
        assert!(result.results[0].is_metadata_only_result());
    }

    // -------------------------------------------------------------------
    // Batch target test
    // -------------------------------------------------------------------

    #[test]
    fn batch_target_for_4_images_is_8() {
        assert_eq!(RetrievalBatchPlanner::batch_target_for(4), 8);
    }

    #[test]
    fn batch_target_for_1_is_2() {
        assert_eq!(RetrievalBatchPlanner::batch_target_for(1), 2);
    }
}
