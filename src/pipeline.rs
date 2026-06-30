//! Production end-to-end retrieval pipeline.
//!
//! Wires together the real adapters behind their trait boundaries for a
//! single production attempt:
//!
//! 1. Build the provider registry from [`RuntimeConfig`] and the SerpApi
//!    Google Images adapter.
//! 2. Run the weighted-random [`SearchScheduler`] to collect candidates.
//! 3. Apply the [`CandidateQualityGate`] (mechanical + Qwen VLM) to select a
//!    retrievable sequence.
//! 4. Plan a retrieval batch and fetch artifacts via [`WebFetchChannel`].
//! 5. Apply the [`ImageAcceptanceGate`] (mechanical + Qwen VLM) to qualify
//!    downloaded images.
//! 6. Record accepted images and coverage gaps into the [`RunOrchestrator`].
//!
//! The Qwen VLM adapter is constructed once and shared by reference with both
//! gates, satisfying the `&dyn OpenClawEvaluationPort` borrow.

#![allow(deprecated)]

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use crate::domain::candidate::{
    CandidateDecision, CandidateQualityDecision, RetrievableCandidate, RetrievableCandidateBatch,
};
use crate::domain::config::{RetrievalChannelKind, RuntimeConfig, SearchProviderKind};
use crate::domain::delivery::{
    CoverageGapType, DeliveredCandidateQualityEvidence, DeliveredImageAcceptanceEvidence,
    DeliveredImageEvidence, DeliveredImageRecord, PipelineStage, WorkflowFailureCode,
};
use crate::domain::image::{ImageAcceptanceDecision, ImageRecord};
use crate::domain::query_plan::{
    NormalizedQueryPlan, QueryPlanId, QueryProviderPolicy, QueryRetrievalPolicy, ValidatedQueryPlan,
};
use crate::domain::search::{ProviderReadinessRecord, SearchOutcome, SearchSessionOutcome};
use crate::error::Error;
use crate::orchestrator::RunOrchestrator;
#[allow(deprecated)]
use crate::ports::OpenClawEvaluationPort;
use crate::ports::{BaseRetrievalChannel, BaseSearchProvider};
use crate::quality::candidate::gate::CandidateQualityGate;
use crate::quality::image::gate::ImageAcceptanceGate;
use crate::quality::qwen_vlm::QwenVlmEvaluator;
use crate::retrieval::batch_planner::RetrievalBatchPlanner;
use crate::retrieval::channels::{
    paid::PaidChannel, self_hosted::SelfHostedChannel, web_fetch::WebFetchChannel,
};
use crate::search::fixture::FixtureSearchProvider;
use crate::search::registry::{ProviderRegistration, ProviderRegistry};
use crate::search::scheduler::{SearchExecutionContext, SearchScheduler, StdRandom};
use crate::search::serpapi::SerpApiGoogleImagesAdapter;

/// A compact summary of what a single production attempt produced.
#[derive(Debug, Clone, Default)]
pub struct ProductionAttemptSummary {
    /// Unique candidates returned by search.
    pub search_candidate_count: usize,
    /// Candidates that passed the candidate quality gate.
    pub retrievable_candidate_count: usize,
    /// Retrieval jobs planned.
    pub retrieval_job_count: usize,
    /// Retrieval jobs that produced a usable local artifact.
    pub retrieval_complete_count: usize,
    /// Images qualified by the image acceptance gate.
    pub accepted_image_count: usize,
}

/// Execute one full production attempt and record results into the orchestrator.
///
/// Returns a [`ProductionAttemptSummary`] on success, or an exit code on a
/// non-recoverable error (currently only retrieval-channel construction).
pub fn execute_production_attempt(
    config: &RuntimeConfig,
    validated: &ValidatedQueryPlan,
    orchestrator: &mut RunOrchestrator,
) -> Result<ProductionAttemptSummary, i32> {
    let mut summary = ProductionAttemptSummary::default();

    // Construct the Qwen VLM evaluator once; both gates borrow it by reference.
    let evaluator = QwenVlmEvaluator::from_config(&config.vlm_evaluation, validated.quality_tier);
    if let Err(e) = evaluator.readiness() {
        orchestrator.record_gap(
            CoverageGapType::CandidateQualityExecutionBlocked,
            validated.required_count,
            WorkflowFailureCode::CandidateQualityBlocked,
            PipelineStage::CandidateQuality,
            format!("VLM evaluation unavailable: {}", e),
            false,
        );
        return Ok(summary);
    }

    // --- Step 1: provider registry + SerpApi adapter ---
    let registry = build_provider_registry(config);

    // --- Step 2: search ---
    let normalized = normalized_plan_from_validated(
        validated,
        QueryPlanId::new(&orchestrator.state.query_plan_id),
    );
    let scheduler = SearchScheduler::new();
    let mut rng = StdRandom;
    let session = scheduler.run_with_context(
        &normalized,
        &registry,
        &mut rng,
        SearchExecutionContext::new(
            orchestrator.state.full_attempt_count,
            orchestrator.state.retry_count,
        ),
    );
    summary.search_candidate_count = session.candidates.len();

    if session.candidates.is_empty() {
        orchestrator.record_gap(
            CoverageGapType::SearchRecallShortage,
            validated.required_count,
            WorkflowFailureCode::SearchShortage,
            PipelineStage::Search,
            "Search returned no candidates.",
            true,
        );
        return Ok(summary);
    }

    // --- Step 3: candidate quality gate (mechanical + Qwen VLM) ---
    let mut search_outcome = search_outcome_from_session(&session);
    let accepted_candidate_ids: BTreeSet<String> = orchestrator
        .accepted_images
        .iter()
        .map(|img| img.candidate_id.clone())
        .collect();
    if !accepted_candidate_ids.is_empty() {
        search_outcome.candidates.retain(|candidate| {
            !accepted_candidate_ids.contains(&candidate.candidate_id.to_string())
        });
    }
    if search_outcome.candidates.is_empty() {
        orchestrator.record_gap(
            CoverageGapType::SearchRecallShortage,
            validated
                .required_count
                .saturating_sub(orchestrator.accepted_count()),
            WorkflowFailureCode::SearchShortage,
            PipelineStage::Search,
            "Search returned no new candidates after excluding previously accepted images.",
            true,
        );
        return Ok(summary);
    }
    let candidate_gate = CandidateQualityGate::new_with_context(
        &evaluator,
        validated.clone(),
        normalized.query_plan_id.to_string(),
        config.policy.prohibited_domains.clone(),
        false,
    )
    .with_retrievable_target(normalized.retrieval_batch_target as usize);
    let candidate_result = match candidate_gate.evaluate(&search_outcome) {
        Ok(r) => r,
        Err(e) => {
            orchestrator.record_gap(
                CoverageGapType::CandidateQualityExecutionBlocked,
                validated.required_count,
                WorkflowFailureCode::CandidateQualityBlocked,
                PipelineStage::CandidateQuality,
                format!("Candidate quality gate failed: {}", e),
                false,
            );
            return Ok(summary);
        }
    };

    let retrievable = match build_retrievable_batch(
        &candidate_result.retrievable_sequence,
        &candidate_result.quality_decisions,
        validated,
        &normalized.query_plan_id,
        orchestrator.state.full_attempt_count,
        orchestrator.state.retry_count,
    ) {
        Ok(batch) => batch,
        Err(e) => {
            orchestrator.record_gap(
                CoverageGapType::CandidateQualityExecutionBlocked,
                validated.required_count,
                WorkflowFailureCode::CandidateQualityBlocked,
                PipelineStage::CandidateQuality,
                format!("Candidate quality handoff failed: {}", e),
                false,
            );
            return Ok(summary);
        }
    };
    summary.retrievable_candidate_count = retrievable.candidates.len();

    if retrievable.candidates.is_empty() {
        orchestrator.record_gap(
            CoverageGapType::CandidateQualityRejected,
            validated.required_count,
            WorkflowFailureCode::CandidateQualityBlocked,
            PipelineStage::CandidateQuality,
            "No candidates passed the candidate quality gate.",
            true,
        );
        return Ok(summary);
    }

    // --- Step 4: retrieval ---
    let (batch, _shortage) = RetrievalBatchPlanner::plan_from_batch(
        &retrievable,
        &normalized.query_plan_id,
        &effective_retrieval_policy(config),
        robots_unknown_behavior_label(config),
        &config.policy.prohibited_domains,
        false,
    );
    summary.retrieval_job_count = batch.jobs.len();

    let staging_dir = orchestrator.output_dir.join("staging");
    let batch_result = match retrieve_with_configured_channels(config, &batch, staging_dir) {
        Ok(r) => r,
        Err(e) => {
            orchestrator.record_gap(
                CoverageGapType::RetrievalFailed,
                validated.required_count,
                WorkflowFailureCode::RetrievalAllFailed,
                PipelineStage::RetrievalExecution,
                format!("Retrieval batch failed: {}", e),
                true,
            );
            return Ok(summary);
        }
    };

    let images = image_records_from_batch_result(&batch_result);
    summary.retrieval_complete_count = images.len();

    if images.is_empty() {
        orchestrator.record_gap(
            CoverageGapType::RetrievalFailed,
            validated.required_count,
            WorkflowFailureCode::RetrievalAllFailed,
            PipelineStage::RetrievalExecution,
            "No retrieval jobs produced a usable local artifact.",
            true,
        );
        return Ok(summary);
    }

    // --- Step 5: image acceptance gate (mechanical + Qwen VLM) ---
    let image_gate = ImageAcceptanceGate::new(&evaluator, validated.clone());
    let image_result = match image_gate.evaluate(&images) {
        Ok(r) => r,
        Err(e) => {
            orchestrator.record_gap(
                CoverageGapType::ImageAcceptanceExecutionBlocked,
                validated.required_count,
                WorkflowFailureCode::ImageAcceptanceBlocked,
                PipelineStage::ImageAcceptance,
                format!("Image acceptance gate failed: {}", e),
                false,
            );
            return Ok(summary);
        }
    };

    // --- Step 6: record accepted images ---
    let candidate_decisions_by_id: HashMap<String, CandidateDecision> = candidate_result
        .all_decisions
        .iter()
        .map(|decision| (candidate_decision_candidate_id(decision), decision.clone()))
        .collect();
    let candidate_quality_decisions_by_id: HashMap<String, CandidateQualityDecision> =
        candidate_result
            .quality_decisions
            .iter()
            .cloned()
            .map(|decision| (decision.candidate_id.to_string(), decision))
            .collect();
    let accepted_before = orchestrator.accepted_count();
    for decision in &image_result.qualified_images {
        if orchestrator.target_met() {
            break;
        }
        if let ImageAcceptanceDecision::Accepted { image, .. } = decision {
            if let Some(artifact) = batch_result.results.iter().find(|result| {
                result.candidate_id == image.candidate_id && result.is_fully_complete()
            }) {
                if let Some(candidate_decision) =
                    candidate_decisions_by_id.get(&artifact.candidate_id)
                {
                    let delivery_index = orchestrator.accepted_count() as usize;
                    orchestrator.add_accepted_image(delivered_record_from_artifact(
                        artifact,
                        candidate_decision,
                        candidate_quality_decisions_by_id.get(&artifact.candidate_id),
                        decision,
                        delivery_index,
                    ));
                }
            }
        }
    }
    summary.accepted_image_count = orchestrator
        .accepted_count()
        .saturating_sub(accepted_before) as usize;

    // --- Coverage gap if short of target ---
    let accepted = orchestrator.accepted_count();
    if accepted < validated.required_count {
        orchestrator.record_gap(
            CoverageGapType::ImageAcceptanceRejected,
            validated.required_count.saturating_sub(accepted),
            WorkflowFailureCode::PartialDelivery,
            PipelineStage::ImageAcceptance,
            format!(
                "Accepted {} of {} required images.",
                accepted, validated.required_count
            ),
            true,
        );
    }

    Ok(summary)
}

fn retrieve_with_configured_channels(
    config: &RuntimeConfig,
    batch: &crate::domain::retrieval::RetrievalBatch,
    staging_dir: std::path::PathBuf,
) -> std::result::Result<crate::domain::retrieval::RetrievalBatchResult, crate::ports::RetrievalError>
{
    let mut channels = build_configured_retrieval_channels(config, &staging_dir)?;
    if channels.is_empty() && config.retrieval_channels.is_empty() {
        channels.push(ConfiguredRetrievalChannel {
            config: crate::domain::config::RetrievalChannelConfig {
                channel_id: "normal_web_fetch".into(),
                channel_kind: RetrievalChannelKind::NormalWebFetch,
                tier: crate::domain::retrieval::RetrievalChannelTier::NormalWebFetch,
                enabled: true,
                endpoint: None,
                credential_env: None,
                max_batch_size: None,
            },
            channel: Box::new(WebFetchChannel::new(staging_dir)?),
        });
    }

    let mut best_results: HashMap<String, crate::domain::retrieval::RetrievalArtifactResult> =
        HashMap::new();
    let mut channel_readiness = Vec::new();
    let mut attempt_trace = Vec::new();
    let mut fallback_decisions = Vec::new();
    let mut execution_blocks = Vec::new();
    let mut diagnostics = Vec::new();
    let mut last_error = None;

    for configured in channels.iter() {
        let readiness = configured.channel.readiness(&configured.config);
        let available = readiness.available;
        channel_readiness.push(readiness);
        if !available {
            continue;
        }

        let pending_jobs = pending_fallback_jobs(batch, &best_results);
        if pending_jobs.is_empty() {
            break;
        }
        let pending_batch = crate::domain::retrieval::RetrievalBatch::new(
            batch.retrieval_batch_id.clone(),
            batch.query_plan_id.clone(),
            batch.full_attempt_count,
            batch.retry_count,
            batch.target_size,
            pending_jobs,
            batch.shortage.clone(),
        );

        match configured.channel.retrieve_batch(&pending_batch) {
            Ok(result) => {
                attempt_trace.extend(result.attempt_trace.clone());
                fallback_decisions.extend(result.fallback_decisions.clone());
                execution_blocks.extend(result.execution_blocking_facts.clone());
                diagnostics.extend(result.diagnostics.clone());
                for item in result.results {
                    let entry = best_results
                        .entry(item.retrieval_job_id.to_string())
                        .or_insert_with(|| item.clone());
                    if !entry.is_fully_complete() && item.is_fully_complete() {
                        *entry = item;
                    }
                }
                if pending_fallback_jobs(batch, &best_results).is_empty() {
                    break;
                }
            }
            Err(e) => {
                last_error = Some(e);
            }
        }
    }

    if best_results.is_empty() {
        return Err(
            last_error.unwrap_or(crate::ports::RetrievalError::Internal {
                message: "no retrieval channel produced results".into(),
            }),
        );
    }

    let results = batch
        .jobs
        .iter()
        .filter_map(|job| best_results.remove(&job.retrieval_job_id.to_string()))
        .collect();

    Ok(crate::domain::retrieval::RetrievalBatchResult::new(
        batch.retrieval_batch_id.clone(),
        batch.query_plan_id.clone(),
        batch.full_attempt_count,
        batch.retry_count,
        batch.target_size,
        channel_readiness,
        results,
        attempt_trace,
        fallback_decisions,
        batch.shortage.clone(),
        execution_blocks,
        diagnostics,
    ))
}

fn pending_fallback_jobs(
    batch: &crate::domain::retrieval::RetrievalBatch,
    best_results: &HashMap<String, crate::domain::retrieval::RetrievalArtifactResult>,
) -> Vec<crate::domain::retrieval::RetrievalJob> {
    batch
        .jobs
        .iter()
        .filter(|job| {
            best_results
                .get(&job.retrieval_job_id.to_string())
                .is_none_or(|result| {
                    !result.is_fully_complete()
                        && result
                            .fetch_trace
                            .last()
                            .is_none_or(|trace| trace.fallback_allowed)
                })
        })
        .cloned()
        .collect()
}

struct ConfiguredRetrievalChannel {
    config: crate::domain::config::RetrievalChannelConfig,
    channel: Box<dyn BaseRetrievalChannel>,
}

fn build_configured_retrieval_channels(
    config: &RuntimeConfig,
    staging_dir: &std::path::Path,
) -> std::result::Result<Vec<ConfiguredRetrievalChannel>, crate::ports::RetrievalError> {
    let mut configs = config.retrieval_channels.clone();
    configs.sort_by_key(|cfg| cfg.tier);

    let mut channels: Vec<ConfiguredRetrievalChannel> = Vec::new();
    for cfg in configs {
        if !cfg.enabled {
            continue;
        }
        let channel: Option<Box<dyn BaseRetrievalChannel>> = match cfg.channel_kind {
            RetrievalChannelKind::NormalWebFetch => Some(Box::new(
                WebFetchChannel::new(staging_dir)?
                    .with_channel_id(cfg.channel_id.clone())
                    .with_enabled(cfg.enabled),
            )),
            RetrievalChannelKind::SelfHostedService => {
                Some(Box::new(SelfHostedChannel::from_config(&cfg)))
            }
            RetrievalChannelKind::PaidOnlineService => {
                Some(Box::new(PaidChannel::from_config(&cfg)))
            }
            RetrievalChannelKind::Fixture | RetrievalChannelKind::Custom(_) => None,
        };
        if let Some(channel) = channel {
            channels.push(ConfiguredRetrievalChannel {
                config: cfg,
                channel,
            });
        }
    }
    Ok(channels)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a provider registry, attaching a production SerpApi adapter to every
/// configured SerpApi Google Images provider.
pub fn build_provider_registry(config: &RuntimeConfig) -> ProviderRegistry {
    let mut registry = ProviderRegistry::new();
    for provider_config in &config.providers {
        let provider_id = crate::domain::candidate::ProviderId::new(&provider_config.provider_id);
        registry.registrations.insert(
            provider_id.clone(),
            ProviderRegistration {
                provider_id: provider_id.clone(),
                display_name: provider_config.provider_id.clone(),
                provider_kind: provider_config.provider_kind.clone(),
                enabled: provider_config.enabled,
                configured_weight: provider_config.weight,
                endpoint: provider_config.endpoint.clone(),
                credential_env: provider_config.credential_env.clone(),
                default_query_params: provider_config.default_query_params.clone(),
                config_fingerprint: String::new(),
                fixture_only: provider_config.provider_kind == SearchProviderKind::Fixture,
            },
        );
        if provider_config.provider_kind == SearchProviderKind::SerpapiGoogleImages {
            registry.adapters.insert(
                provider_id,
                Arc::new(SerpApiGoogleImagesAdapter::from_config(
                    provider_config,
                    true,
                )) as Arc<dyn BaseSearchProvider>,
            );
        } else if provider_config.provider_kind == SearchProviderKind::Fixture {
            registry.adapters.insert(
                provider_id,
                Arc::new(
                    FixtureSearchProvider::new(
                        &provider_config.provider_id,
                        &provider_config.provider_id,
                    )
                    .with_weight(provider_config.weight)
                    .with_enabled(provider_config.enabled),
                ) as Arc<dyn BaseSearchProvider>,
            );
        }
    }
    registry
}

/// Build a downstream-consumable [`NormalizedQueryPlan`] from the legacy
/// [`ValidatedQueryPlan`] used by `cmd_run`.
fn normalized_plan_from_validated(
    validated: &ValidatedQueryPlan,
    query_plan_id: QueryPlanId,
) -> NormalizedQueryPlan {
    let required = validated.required_count.max(1);
    let retry_limit = validated.retry_limit.min(3) as u8;
    NormalizedQueryPlan {
        query_plan_id,
        description: validated.description.clone(),
        query_texts: vec![validated.description.clone()],
        required_image_count: required,
        quality: validated.quality_tier,
        quality_requirements: Default::default(),
        material_types: Vec::new(),
        visual_requirements: Vec::new(),
        negative_scope: Vec::new(),
        source_diversity_requirement: None,
        candidate_target: required.saturating_mul(20),
        retrieval_batch_target: required.saturating_mul(2),
        retry_limit,
        full_attempt_limit: 1 + retry_limit,
        provider_policy: QueryProviderPolicy::default(),
        retrieval_policy: QueryRetrievalPolicy::default(),
        admission_diagnostics: Vec::new(),
    }
}

/// Convert the scheduler's [`SearchSessionOutcome`] into the legacy
/// [`SearchOutcome`] shape consumed by the candidate quality gate.
fn search_outcome_from_session(session: &SearchSessionOutcome) -> SearchOutcome {
    let readiness_summary: Vec<ProviderReadinessRecord> = session
        .readiness_reports
        .iter()
        .map(|r| ProviderReadinessRecord {
            provider_id: r.provider_id.clone(),
            display_name: r.display_name.clone(),
            readiness: map_provider_readiness(&r.status),
            configured_weight: r.configured_weight as i32,
            included_in_table: r.included_in_weight_table,
        })
        .collect();

    SearchOutcome {
        candidates: session.candidates.clone(),
        usage_events: session.usage_events.clone(),
        total_invocations: session.usage_events.len() as u32,
        candidate_target: session.candidate_target,
        target_met: session.target_met,
        shortage_reason: session.shortage_reason.clone(),
        readiness_summary,
    }
}

/// Map a structured [`ProviderReadinessStatus`] to the legacy
/// [`ProviderReadiness`] enum used by [`SearchOutcome`].
fn map_provider_readiness(
    status: &crate::domain::search::ProviderReadinessStatus,
) -> crate::domain::search::ProviderReadiness {
    use crate::domain::search::{ProviderReadiness as R, ProviderReadinessStatus as S};
    match status {
        S::Ready => R::Ready,
        S::Disabled => R::Disabled,
        S::MissingCredentials => R::MissingCredentials,
        S::Misconfigured | S::ConstraintUnsupported => R::Misconfigured,
        S::QuotaExhausted => R::RateLimited,
        S::HealthFailed | S::Retired | S::FixtureOnly | S::Unavailable => R::Unavailable,
    }
}

/// Build a [`RetrievableCandidateBatch`] from the accepted candidates in the
/// candidate quality gate's retrievable sequence.
fn build_retrievable_batch(
    sequence: &crate::domain::candidate::RetrievableCandidateSequence,
    quality_decisions: &[CandidateQualityDecision],
    validated: &ValidatedQueryPlan,
    query_plan_id: &QueryPlanId,
    full_attempt_count: u8,
    retry_count: u8,
) -> crate::error::Result<RetrievableCandidateBatch> {
    let retrieval_batch_target = validated.required_count.saturating_mul(2).max(1);
    let quality_by_candidate: HashMap<String, CandidateQualityDecision> = quality_decisions
        .iter()
        .cloned()
        .map(|decision| (decision.candidate_id.to_string(), decision))
        .collect();

    let mut candidates: Vec<RetrievableCandidate> = Vec::new();
    for decision in &sequence.candidates {
        if let CandidateDecision::Accepted {
            candidate,
            priority,
            ..
        } = decision
        {
            let quality_decision = quality_by_candidate
                .get(&candidate.candidate_id.to_string())
                .cloned()
                .ok_or_else(|| {
                    Error::execution_blocked(format!(
                        "candidate quality decision missing for retrievable candidate {}",
                        candidate.candidate_id
                    ))
                })?;
            candidates.push(RetrievableCandidate {
                candidate: candidate.clone(),
                candidate_quality_decision: quality_decision,
                retrieval_priority: *priority,
                primary_image_url: candidate.image_url.clone(),
                source_page_url: candidate.source_page_url.clone(),
                thumbnail_url: candidate.thumbnail_url.clone(),
                expected_mime_type: candidate.mime_type.clone(),
                license_hint: candidate.license_hint.clone(),
                provenance_refs: Vec::new(),
            });
        }
    }
    let rejected_decisions = quality_decisions
        .iter()
        .filter(|decision| !decision.is_retrievable())
        .cloned()
        .collect();

    Ok(RetrievableCandidateBatch {
        query_plan_id: query_plan_id.to_string(),
        full_attempt_count,
        retry_count,
        retrieval_batch_target,
        candidates,
        rejected_decisions,
        execution_blocking_facts: Vec::new(),
    })
}

/// Resolve the effective retrieval policy used for retrieval planning.
fn effective_retrieval_policy(config: &RuntimeConfig) -> QueryRetrievalPolicy {
    QueryRetrievalPolicy {
        allow_paid: config.policy.allow_paid_channels,
        respect_robots: config.policy.respect_robots,
        allow_login: config.policy.allow_login_required_sources,
        allow_paywalled: config.policy.allow_paywalled_sources,
    }
}

/// Map the configured robots-unknown behaviour to its serialized label.
fn robots_unknown_behavior_label(config: &RuntimeConfig) -> &'static str {
    match config.policy.robots_unknown_behavior {
        crate::domain::config::RobotsUnknownBehavior::Warn => "warn",
        crate::domain::config::RobotsUnknownBehavior::Block => "block",
    }
}

/// Convert completed retrieval artifacts into [`ImageRecord`] inputs for the
/// image acceptance gate. Only jobs with a usable local artifact are kept.
fn image_records_from_batch_result(
    batch_result: &crate::domain::retrieval::RetrievalBatchResult,
) -> Vec<ImageRecord> {
    batch_result
        .results
        .iter()
        .filter_map(|result| {
            let local_path = result.local_artifact_path.as_ref()?;
            let path_str = local_path.display().to_string();
            // Prefer the result-reported size; fall back to filesystem metadata.
            let file_size_bytes = result
                .file_size_bytes
                .or_else(|| std::fs::metadata(local_path).ok().map(|m| m.len()))
                .unwrap_or(0);
            Some(ImageRecord {
                candidate_id: result.candidate_id.clone(),
                local_path: path_str,
                content_type: result.content_type.clone(),
                file_size_bytes,
                dimensions: result.image_dimensions,
                reference_metrics: image_reference_metrics_from_artifact(result, file_size_bytes),
            })
        })
        .collect()
}

fn image_reference_metrics_from_artifact(
    result: &crate::domain::retrieval::RetrievalArtifactResult,
    file_size_bytes: u64,
) -> Vec<serde_json::Value> {
    let mut metrics = vec![
        serde_json::json!({
            "kind": "retrieval_channel",
            "channel_id": result.channel_id.to_string(),
            "channel_tier": result.channel_tier.to_string(),
            "retrieval_status": retrieval_status_label(result.retrieval_status),
        }),
        serde_json::json!({
            "kind": "file_size",
            "value": file_size_bytes,
        }),
    ];

    if let Some(dimensions) = result.image_dimensions {
        metrics.push(serde_json::json!({
            "kind": "image_dimensions",
            "width": dimensions.width,
            "height": dimensions.height,
        }));
    }
    if let Some(path) = &result.source_sidecar_path {
        metrics.push(serde_json::json!({
            "kind": "source_sidecar_path",
            "path": path.display().to_string(),
        }));
    }
    if let Some(path) = &result.content_summary_path {
        metrics.push(serde_json::json!({
            "kind": "content_summary_path",
            "path": path.display().to_string(),
        }));
    }
    if let Some(path) = &result.visual_description_path {
        metrics.push(serde_json::json!({
            "kind": "visual_description_path",
            "path": path.display().to_string(),
        }));
    }
    if !result.fetch_trace.is_empty() {
        metrics.push(serde_json::json!({
            "kind": "fetch_trace_count",
            "value": result.fetch_trace.len(),
        }));
    }
    metrics.push(serde_json::json!({
        "kind": "media_type_match",
        "value": result.media_type_match,
    }));

    metrics
}

/// Build a [`DeliveredImageRecord`] from a complete retrieval artifact.
fn delivered_record_from_artifact(
    artifact: &crate::domain::retrieval::RetrievalArtifactResult,
    candidate_decision: &CandidateDecision,
    candidate_quality_decision: Option<&CandidateQualityDecision>,
    image_decision: &ImageAcceptanceDecision,
    index: usize,
) -> DeliveredImageRecord {
    let (width, height) = match &artifact.image_dimensions {
        Some(d) => (Some(d.width), Some(d.height)),
        None => (None, None),
    };
    let path = |value: &Option<std::path::PathBuf>| {
        value
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default()
    };
    DeliveredImageRecord {
        delivered_image_id: format!("delivered-{}-{}", artifact.query_plan_id, index + 1),
        query_plan_id: artifact.query_plan_id.clone(),
        candidate_id: artifact.candidate_id.clone(),
        retrieval_job_id: artifact.retrieval_job_id.to_string(),
        package_image_path: path(&artifact.local_artifact_path),
        local_artifact_path: path(&artifact.local_artifact_path),
        source_artifact_path: path(&artifact.source_artifact_path),
        source_sidecar_path: path(&artifact.source_sidecar_path),
        content_summary_path: path(&artifact.content_summary_path),
        task_report_path: path(&artifact.task_report_path),
        visual_description_path: path(&artifact.visual_description_path),
        checksum_sha256: artifact.checksum_sha256.clone().unwrap_or_default(),
        content_type: artifact.content_type.clone().unwrap_or_default(),
        file_size_bytes: artifact.file_size_bytes.unwrap_or(0),
        width,
        height,
        candidate_quality_decision_ref: format!(
            "candidate-quality-decision-{}",
            artifact.candidate_id
        ),
        image_acceptance_decision_ref: format!(
            "image-acceptance-decision-{}",
            artifact.candidate_id
        ),
        manifest_entry_ref: format!("manifest-{:04}", index + 1),
        evidence: delivered_evidence_from_decisions(
            artifact,
            candidate_decision,
            candidate_quality_decision,
            image_decision,
        ),
    }
}

fn candidate_decision_candidate_id(decision: &CandidateDecision) -> String {
    match decision {
        CandidateDecision::Accepted { candidate, .. }
        | CandidateDecision::Rejected { candidate, .. }
        | CandidateDecision::Uncertain { candidate, .. }
        | CandidateDecision::ExecutionBlocked { candidate, .. } => {
            candidate.candidate_id.to_string()
        }
    }
}

fn delivered_evidence_from_decisions(
    artifact: &crate::domain::retrieval::RetrievalArtifactResult,
    candidate_decision: &CandidateDecision,
    candidate_quality_decision: Option<&CandidateQualityDecision>,
    image_decision: &ImageAcceptanceDecision,
) -> DeliveredImageEvidence {
    let provider_id = match candidate_decision {
        CandidateDecision::Accepted { candidate, .. }
        | CandidateDecision::Rejected { candidate, .. }
        | CandidateDecision::Uncertain { candidate, .. }
        | CandidateDecision::ExecutionBlocked { candidate, .. } => {
            candidate.provider_id.to_string()
        }
    };

    DeliveredImageEvidence {
        provider_id,
        channel_id: artifact.channel_id.to_string(),
        channel_tier: artifact.channel_tier.to_string(),
        retrieval_status: retrieval_status_label(artifact.retrieval_status),
        media_type_match: artifact.media_type_match,
        fetch_trace: artifact
            .fetch_trace
            .iter()
            .filter_map(|trace| serde_json::to_value(trace).ok())
            .collect(),
        failure_reason: artifact
            .failure_reason
            .as_ref()
            .and_then(|reason| serde_json::to_value(reason).ok()),
        candidate_decision: candidate_quality_decision
            .map(|decision| {
                candidate_quality_evidence_from_decision(
                    decision,
                    candidate_decision_vlm_evidence(candidate_decision),
                )
            })
            .unwrap_or_else(|| candidate_quality_evidence(candidate_decision)),
        image_decision: image_acceptance_evidence(image_decision, artifact),
    }
}

fn candidate_quality_evidence_from_decision(
    decision: &CandidateQualityDecision,
    evaluator_evidence: Option<&crate::domain::candidate::VlmDecisionEvidence>,
) -> DeliveredCandidateQualityEvidence {
    DeliveredCandidateQualityEvidence {
        mechanical_passed: decision.mechanical_passed,
        vlm_passed: decision.vlm_passed,
        final_status: serde_json::to_value(decision.final_status)
            .ok()
            .and_then(|value| value.as_str().map(ToOwned::to_owned))
            .unwrap_or_else(|| format!("{:?}", decision.final_status).to_lowercase()),
        priority: decision.priority,
        blocking_metrics: decision
            .blocking_metrics
            .iter()
            .filter_map(|metric| serde_json::to_value(metric).ok())
            .collect(),
        reference_metrics: decision
            .reference_metrics
            .iter()
            .filter_map(|metric| serde_json::to_value(metric).ok())
            .collect(),
        vlm_decision: decision
            .vlm_decision
            .as_ref()
            .map(|decision| vlm_subject_decision_package_evidence(decision, evaluator_evidence)),
    }
}

fn candidate_decision_vlm_evidence(
    decision: &CandidateDecision,
) -> Option<&crate::domain::candidate::VlmDecisionEvidence> {
    match decision {
        CandidateDecision::Accepted { vlm_evidence, .. } => vlm_evidence.as_ref(),
        CandidateDecision::Rejected { .. }
        | CandidateDecision::Uncertain { .. }
        | CandidateDecision::ExecutionBlocked { .. } => None,
    }
}

fn vlm_subject_decision_package_evidence(
    decision: &crate::domain::candidate::VlmSubjectDecision,
    evaluator_evidence: Option<&crate::domain::candidate::VlmDecisionEvidence>,
) -> serde_json::Value {
    let mut value = serde_json::to_value(decision).unwrap_or(serde_json::Value::Null);
    if let Some(obj) = value.as_object_mut() {
        if let Some(evidence) = evaluator_evidence {
            obj.entry("provider_id")
                .or_insert_with(|| serde_json::Value::String(evidence.provider_id.clone()));
            if let Some(model) = &evidence.model {
                obj.entry("model")
                    .or_insert_with(|| serde_json::Value::String(model.clone()));
            }
            obj.entry("evidence_source")
                .or_insert_with(|| serde_json::Value::String(evidence.evidence_source.clone()));
            if let Some(raw_verdict) = &evidence.raw_verdict {
                obj.entry("raw_verdict")
                    .or_insert_with(|| serde_json::Value::String(raw_verdict.clone()));
            }
        }
    }
    value
}

fn candidate_quality_evidence(decision: &CandidateDecision) -> DeliveredCandidateQualityEvidence {
    match decision {
        CandidateDecision::Accepted {
            candidate,
            priority,
            vlm_evidence,
        } => DeliveredCandidateQualityEvidence {
            mechanical_passed: true,
            vlm_passed: true,
            final_status: "retrievable".into(),
            priority: *priority,
            blocking_metrics: vec![],
            reference_metrics: vec![serde_json::json!({
                "kind": "search_provider",
                "provider_id": candidate.provider_id.to_string(),
                "provider_rank": candidate.provider_rank,
                "search_request_id": candidate.search_request_id,
            })],
            vlm_decision: vlm_evidence
                .as_ref()
                .map(|evidence| evidence.to_json_value()),
        },
        CandidateDecision::Rejected { reason, .. } => DeliveredCandidateQualityEvidence {
            mechanical_passed: false,
            vlm_passed: false,
            final_status: "rejected".into(),
            priority: 0,
            blocking_metrics: vec![serde_json::json!({"reason": reason})],
            reference_metrics: vec![],
            vlm_decision: None,
        },
        CandidateDecision::Uncertain { .. } => DeliveredCandidateQualityEvidence {
            mechanical_passed: true,
            vlm_passed: false,
            final_status: "subjectively_uncertain".into(),
            priority: 0,
            blocking_metrics: vec![],
            reference_metrics: vec![],
            vlm_decision: None,
        },
        CandidateDecision::ExecutionBlocked { .. } => DeliveredCandidateQualityEvidence {
            mechanical_passed: true,
            vlm_passed: false,
            final_status: "execution_blocked".into(),
            priority: 0,
            blocking_metrics: vec![],
            reference_metrics: vec![],
            vlm_decision: None,
        },
    }
}

fn image_acceptance_evidence(
    decision: &ImageAcceptanceDecision,
    artifact: &crate::domain::retrieval::RetrievalArtifactResult,
) -> DeliveredImageAcceptanceEvidence {
    match decision {
        ImageAcceptanceDecision::Accepted {
            image,
            notes,
            vlm_evidence,
        } => DeliveredImageAcceptanceEvidence {
            mechanical_passed: true,
            vlm_passed: true,
            artifact_complete: artifact.is_fully_complete(),
            final_status: "accepted".into(),
            blocking_reasons: vec![],
            reference_metrics: image_reference_metrics(image),
            vlm_decision: vlm_evidence.as_ref().map(|evidence| {
                let mut value = evidence.to_json_value();
                if let Some(obj) = value.as_object_mut() {
                    obj.insert("notes".into(), serde_json::Value::String(notes.clone()));
                }
                value
            }),
        },
        ImageAcceptanceDecision::MechanicallyRejected { evidence, .. } => {
            DeliveredImageAcceptanceEvidence {
                mechanical_passed: false,
                vlm_passed: false,
                artifact_complete: artifact.is_fully_complete(),
                final_status: "mechanically_rejected".into(),
                blocking_reasons: evidence.blocking_findings.clone(),
                reference_metrics: evidence
                    .reference_findings
                    .iter()
                    .map(|finding| serde_json::json!({"kind": "mechanical_reference", "value": finding}))
                    .collect(),
                vlm_decision: None,
            }
        }
        ImageAcceptanceDecision::SubjectivelyRejected {
            mechanical_evidence,
            ..
        } => DeliveredImageAcceptanceEvidence {
            mechanical_passed: true,
            vlm_passed: false,
            artifact_complete: artifact.is_fully_complete(),
            final_status: "subjectively_rejected".into(),
            blocking_reasons: vec![],
            reference_metrics: mechanical_evidence
                .reference_findings
                .iter()
                .map(|finding| serde_json::json!({"kind": "mechanical_reference", "value": finding}))
                .collect(),
            vlm_decision: None,
        },
        ImageAcceptanceDecision::ExecutionBlocked { .. } => DeliveredImageAcceptanceEvidence {
            mechanical_passed: true,
            vlm_passed: false,
            artifact_complete: artifact.is_fully_complete(),
            final_status: "execution_blocked".into(),
            blocking_reasons: vec![],
            reference_metrics: vec![],
            vlm_decision: None,
        },
    }
}

fn image_reference_metrics(image: &ImageRecord) -> Vec<serde_json::Value> {
    let mut metrics = image.reference_metrics.clone();
    push_unique_metric(
        &mut metrics,
        serde_json::json!({
            "kind": "file_size_bytes",
            "value": image.file_size_bytes,
        }),
    );
    if let Some(content_type) = &image.content_type {
        push_unique_metric(
            &mut metrics,
            serde_json::json!({
                "kind": "content_type",
                "value": content_type,
            }),
        );
    }
    if let Some(dimensions) = image.dimensions {
        push_unique_metric(
            &mut metrics,
            serde_json::json!({
                "kind": "image_dimensions",
                "width": dimensions.width,
                "height": dimensions.height,
            }),
        );
    }
    metrics
}

fn push_unique_metric(metrics: &mut Vec<serde_json::Value>, metric: serde_json::Value) {
    if !metrics.iter().any(|existing| existing == &metric) {
        metrics.push(metric);
    }
}

fn retrieval_status_label(status: crate::domain::retrieval::RetrievalStatus) -> String {
    serde_json::to_value(status)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| format!("{:?}", status).to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::candidate::{
        CandidateId, CandidateQualityDecision, CandidateQualityStatus, CandidateRecord,
        ImageDimensions, ProviderId, RetrievableCandidateSequence,
    };
    use crate::domain::config::{
        RetrievalChannelConfig, SearchProviderConfig, VlmEvaluationConfig,
    };
    use crate::domain::metrics::{MetricFact, QualityMetricCode};
    use crate::domain::retrieval::{
        RetrievalArtifactResult, RetrievalAttemptMode, RetrievalAttemptStatus,
        RetrievalAttemptTrace, RetrievalChannelId, RetrievalChannelTier, RetrievalJob,
        RetrievalJobId, RetrievalPolicyContext, RetrievalStatus, RetrievalTarget,
        RetrievalTargetType,
    };
    use crate::domain::search::ProviderReadinessStatus;
    use std::path::PathBuf;

    fn complete_artifact() -> RetrievalArtifactResult {
        RetrievalArtifactResult {
            retrieval_job_id: RetrievalJobId::new("job-1"),
            retrieval_batch_id: "batch-1".into(),
            query_plan_id: "qp-1".into(),
            candidate_id: "cand-1".into(),
            channel_id: RetrievalChannelId::new("web"),
            channel_tier: RetrievalChannelTier::NormalWebFetch,
            attempt_mode: RetrievalAttemptMode::DirectImageFetch,
            retrieval_status: RetrievalStatus::Complete,
            local_artifact_path: Some(PathBuf::from("staging/image.jpg")),
            source_artifact_path: Some(PathBuf::from("staging/source.html")),
            source_sidecar_path: Some(PathBuf::from("staging/sidecar.json")),
            content_summary_path: Some(PathBuf::from("staging/summary.txt")),
            task_report_path: Some(PathBuf::from("staging/report.json")),
            visual_description_path: Some(PathBuf::from("staging/visual.txt")),
            diagnostics_path: None,
            checksum_sha256: Some(
                "sha256-ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad".into(),
            ),
            content_type_reported: Some("image/jpeg".into()),
            content_type_sniffed: Some("image/jpeg".into()),
            content_type: Some("image/jpeg".into()),
            file_extension: Some("jpg".into()),
            file_size_bytes: Some(123),
            image_dimensions: Some(ImageDimensions {
                width: 10,
                height: 20,
            }),
            media_type_match: true,
            local_artifact_exists: true,
            source_artifact_exists: true,
            sidecar_valid: true,
            summary_quality_passed: true,
            task_report_valid: true,
            visual_description_valid: true,
            job_ownership_valid: true,
            metadata_only: false,
            fetch_trace: vec![],
            policy_decisions: vec![],
            diagnostics: vec![],
            failure_reason: None,
            redaction_applied: true,
        }
    }

    #[test]
    fn retrievable_batch_preserves_candidate_quality_decisions_and_rejections() {
        let accepted_candidate = CandidateRecord::minimal(
            CandidateId::new("cand-1"),
            ProviderId::new("fixture_search"),
            "https://example.com/image.jpg",
        );
        let rejected_candidate = CandidateRecord::minimal(
            CandidateId::new("cand-2"),
            ProviderId::new("fixture_search"),
            "https://example.com/other.jpg",
        );
        let accepted_decision = CandidateDecision::Accepted {
            candidate: accepted_candidate.clone(),
            priority: 11,
            vlm_evidence: Some(crate::domain::candidate::VlmDecisionEvidence::new(
                "approve",
                "fixture_vlm",
                "qwen3-vl-plus",
                "unit_test",
            )),
        };
        let rejected_decision = CandidateDecision::Rejected {
            candidate: rejected_candidate.clone(),
            reason: "mechanically rejected".into(),
        };
        let reference_metric = MetricFact::candidate_reference(
            QualityMetricCode::CandidateDimensionsReported,
            "cand-1",
            "qp-1",
            "dimensions: 1920x1080",
        );
        let accepted_quality = CandidateQualityDecision {
            candidate_id: CandidateId::new("cand-1"),
            query_plan_id: "qp-1".into(),
            mechanical_passed: true,
            vlm_passed: true,
            final_status: CandidateQualityStatus::Retrievable,
            priority: 11,
            blocking_metrics: vec![],
            reference_metrics: vec![reference_metric],
            vlm_decision: Some(crate::domain::candidate::VlmSubjectDecision {
                subject_id: "cand-1".into(),
                decision: crate::domain::candidate::VlmSubjectDecisionKind::Approve,
                confidence: Some(0.9),
                reason_codes: vec!["fixture_approve".into()],
                rationale_summary: "approved".into(),
                evidence_refs: vec![],
            }),
            diagnostics: vec![],
        };
        let rejected_quality = CandidateQualityDecision::mechanically_rejected(
            CandidateId::new("cand-2"),
            "qp-1",
            vec![MetricFact::candidate_blocking(
                QualityMetricCode::CandidateImageUrlInvalid,
                "cand-2",
                "qp-1",
                "invalid",
            )],
        );
        let validated = ValidatedQueryPlan {
            description: "sunset".into(),
            required_count: 1,
            quality_tier: crate::domain::query_plan::QualityTier::General,
            content_constraints: Default::default(),
            authorization_preference: Default::default(),
            output_preference: Default::default(),
            retry_limit: 3,
        };

        let batch = build_retrievable_batch(
            &RetrievableCandidateSequence::from_decisions(vec![
                accepted_decision,
                rejected_decision,
            ]),
            &[accepted_quality, rejected_quality],
            &validated,
            &QueryPlanId::new("qp-1"),
            1,
            0,
        )
        .unwrap();

        assert_eq!(batch.candidates.len(), 1);
        assert_eq!(
            batch.candidates[0]
                .candidate_quality_decision
                .reference_metrics
                .len(),
            1
        );
        assert!(batch.candidates[0]
            .candidate_quality_decision
            .vlm_decision
            .is_some());
        assert_eq!(batch.rejected_decisions.len(), 1);
        assert_eq!(
            batch.rejected_decisions[0].final_status,
            CandidateQualityStatus::MechanicallyRejected
        );
    }

    #[test]
    fn retrievable_batch_rejects_missing_candidate_quality_decision() {
        let accepted_candidate = CandidateRecord::minimal(
            CandidateId::new("cand-missing-quality"),
            ProviderId::new("fixture_search"),
            "https://example.com/image.jpg",
        );
        let accepted_decision = CandidateDecision::Accepted {
            candidate: accepted_candidate,
            priority: 11,
            vlm_evidence: Some(crate::domain::candidate::VlmDecisionEvidence::new(
                "approve",
                "fixture_vlm",
                "qwen3-vl-plus",
                "unit_test",
            )),
        };
        let validated = ValidatedQueryPlan {
            description: "sunset".into(),
            required_count: 1,
            quality_tier: crate::domain::query_plan::QualityTier::General,
            content_constraints: Default::default(),
            authorization_preference: Default::default(),
            output_preference: Default::default(),
            retry_limit: 3,
        };

        let err = build_retrievable_batch(
            &RetrievableCandidateSequence::from_decisions(vec![accepted_decision]),
            &[],
            &validated,
            &QueryPlanId::new("qp-1"),
            1,
            0,
        )
        .expect_err("missing candidate quality decision must block handoff");

        assert!(err.to_string().contains("candidate quality decision"));
    }

    #[test]
    fn delivered_record_from_artifact_preserves_required_evidence_fields() {
        let artifact = complete_artifact();
        let candidate = CandidateRecord::minimal(
            CandidateId::new("cand-1"),
            ProviderId::new("fixture_search"),
            "https://example.com/image.jpg",
        );
        let candidate_decision = CandidateDecision::Accepted {
            candidate,
            priority: 7,
            vlm_evidence: Some(crate::domain::candidate::VlmDecisionEvidence::new(
                "approve",
                "fixture_vlm",
                "qwen3-vl-plus",
                "pipeline_unit_test",
            )),
        };
        let image_decision = ImageAcceptanceDecision::Accepted {
            image: ImageRecord {
                candidate_id: "cand-1".into(),
                local_path: "staging/image.jpg".into(),
                content_type: Some("image/jpeg".into()),
                file_size_bytes: 123,
                dimensions: Some(ImageDimensions {
                    width: 10,
                    height: 20,
                }),
                reference_metrics: vec![],
            },
            notes: "Qwen VLM approved".into(),
            vlm_evidence: Some(crate::domain::candidate::VlmDecisionEvidence::new(
                "approve",
                "fixture_vlm",
                "qwen3-vl-plus",
                "pipeline_unit_test",
            )),
        };
        let record = delivered_record_from_artifact(
            &artifact,
            &candidate_decision,
            None,
            &image_decision,
            0,
        );

        assert_eq!(record.retrieval_job_id, "job-1");
        assert_eq!(record.local_artifact_path, "staging/image.jpg");
        assert_eq!(record.source_artifact_path, "staging/source.html");
        assert_eq!(record.source_sidecar_path, "staging/sidecar.json");
        assert_eq!(record.content_summary_path, "staging/summary.txt");
        assert_eq!(record.task_report_path, "staging/report.json");
        assert_eq!(record.visual_description_path, "staging/visual.txt");
        assert_eq!(
            record.checksum_sha256,
            "sha256-ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(record.width, Some(10));
        assert_eq!(record.height, Some(20));
        assert_eq!(record.evidence.provider_id, "fixture_search");
        assert_eq!(record.evidence.channel_id, "web");
        assert_eq!(record.evidence.channel_tier, "normal_web_fetch");
        assert_eq!(record.evidence.retrieval_status, "complete");
        assert_eq!(
            record
                .evidence
                .candidate_decision
                .vlm_decision
                .as_ref()
                .and_then(|v| v.get("provider_id"))
                .and_then(|v| v.as_str()),
            Some("fixture_vlm")
        );
        assert_eq!(
            record
                .evidence
                .image_decision
                .vlm_decision
                .as_ref()
                .and_then(|v| v.get("provider_id"))
                .and_then(|v| v.as_str()),
            Some("fixture_vlm")
        );
    }

    #[test]
    fn delivered_record_merges_candidate_quality_decision_with_vlm_provider_evidence() {
        let artifact = complete_artifact();
        let candidate = CandidateRecord::minimal(
            CandidateId::new("cand-1"),
            ProviderId::new("fixture_search"),
            "https://example.com/image.jpg",
        );
        let candidate_decision = CandidateDecision::Accepted {
            candidate,
            priority: 7,
            vlm_evidence: Some(crate::domain::candidate::VlmDecisionEvidence::new(
                "approve",
                "qwen_3_5_vlm",
                "qwen3-vl-plus",
                "qwen_candidate_text_relevance",
            )),
        };
        let candidate_quality_decision = CandidateQualityDecision {
            candidate_id: CandidateId::new("cand-1"),
            query_plan_id: "qp-1".into(),
            mechanical_passed: true,
            vlm_passed: true,
            final_status: CandidateQualityStatus::Retrievable,
            priority: 7,
            blocking_metrics: vec![],
            reference_metrics: vec![],
            vlm_decision: Some(crate::domain::candidate::VlmSubjectDecision {
                subject_id: "cand-1".into(),
                decision: crate::domain::candidate::VlmSubjectDecisionKind::Approve,
                confidence: Some(0.95),
                reason_codes: vec!["candidate_relevance_score_0.95".into()],
                rationale_summary: "metadata matches the query".into(),
                evidence_refs: vec![],
            }),
            diagnostics: vec![],
        };
        let image_decision = ImageAcceptanceDecision::Accepted {
            image: ImageRecord {
                candidate_id: "cand-1".into(),
                local_path: "staging/image.jpg".into(),
                content_type: Some("image/jpeg".into()),
                file_size_bytes: 123,
                dimensions: Some(ImageDimensions {
                    width: 10,
                    height: 20,
                }),
                reference_metrics: vec![],
            },
            notes: "Qwen VLM approved".into(),
            vlm_evidence: Some(crate::domain::candidate::VlmDecisionEvidence::new(
                "approve",
                "qwen_3_5_vlm",
                "qwen3-vl-plus",
                "qwen_image_evaluation",
            )),
        };

        let record = delivered_record_from_artifact(
            &artifact,
            &candidate_decision,
            Some(&candidate_quality_decision),
            &image_decision,
            0,
        );
        let vlm = record
            .evidence
            .candidate_decision
            .vlm_decision
            .as_ref()
            .expect("candidate quality vlm evidence");

        assert_eq!(vlm["decision"], "approve");
        assert_eq!(vlm["provider_id"], "qwen_3_5_vlm");
        assert_eq!(vlm["model"], "qwen3-vl-plus");
        let confidence = vlm["confidence"].as_f64().expect("confidence");
        assert!((confidence - 0.95).abs() < 0.001);
        assert_eq!(vlm["evidence_source"], "qwen_candidate_text_relevance");
    }

    #[test]
    fn delivered_record_preserves_image_reference_metrics_used_by_vlm() {
        let artifact = complete_artifact();
        let candidate = CandidateRecord::minimal(
            CandidateId::new("cand-1"),
            ProviderId::new("fixture_search"),
            "https://example.com/image.jpg",
        );
        let candidate_decision = CandidateDecision::Accepted {
            candidate,
            priority: 7,
            vlm_evidence: Some(crate::domain::candidate::VlmDecisionEvidence::new(
                "approve",
                "fixture_vlm",
                "qwen3-vl-plus",
                "pipeline_unit_test",
            )),
        };
        let image_decision = ImageAcceptanceDecision::Accepted {
            image: ImageRecord {
                candidate_id: "cand-1".into(),
                local_path: "staging/image.jpg".into(),
                content_type: Some("image/jpeg".into()),
                file_size_bytes: 123,
                dimensions: Some(ImageDimensions {
                    width: 10,
                    height: 20,
                }),
                reference_metrics: vec![
                    serde_json::json!({
                        "kind": "retrieval_channel",
                        "channel_id": "web",
                        "channel_tier": "normal_web_fetch",
                    }),
                    serde_json::json!({
                        "kind": "source_sidecar_path",
                        "path": "staging/sidecar.json",
                    }),
                    serde_json::json!({
                        "kind": "mechanical_reference",
                        "value": "dimensions accepted for quality tier",
                    }),
                ],
            },
            notes: "Qwen VLM approved".into(),
            vlm_evidence: Some(crate::domain::candidate::VlmDecisionEvidence::new(
                "approve",
                "fixture_vlm",
                "qwen3-vl-plus",
                "pipeline_unit_test",
            )),
        };

        let record = delivered_record_from_artifact(
            &artifact,
            &candidate_decision,
            None,
            &image_decision,
            0,
        );
        let metrics = &record.evidence.image_decision.reference_metrics;

        assert!(metrics
            .iter()
            .any(|metric| metric["kind"] == "retrieval_channel"));
        assert!(metrics
            .iter()
            .any(|metric| metric["kind"] == "source_sidecar_path"));
        assert!(metrics
            .iter()
            .any(|metric| metric["kind"] == "mechanical_reference"));
    }

    #[test]
    fn image_records_from_batch_result_preserve_reference_metrics_for_vlm() {
        let artifact = complete_artifact();
        let batch_result = crate::domain::retrieval::RetrievalBatchResult::new(
            "batch-1",
            "qp-1",
            1,
            0,
            1,
            vec![],
            vec![artifact],
            vec![],
            vec![],
            None,
            vec![],
            vec![],
        );

        let images = image_records_from_batch_result(&batch_result);
        let image_json = serde_json::to_value(&images[0]).unwrap();
        let reference_metrics = image_json["reference_metrics"].as_array().unwrap();

        assert!(reference_metrics.iter().any(|metric| {
            metric["kind"] == "retrieval_channel" && metric["channel_id"] == "web"
        }));
        assert!(reference_metrics
            .iter()
            .any(|metric| metric["kind"] == "source_sidecar_path"));
        assert!(reference_metrics
            .iter()
            .any(|metric| metric["kind"] == "visual_description_path"));
    }

    #[test]
    fn provider_registry_uses_each_same_kind_provider_config() {
        let missing_env = format!("IMAGE_RETRIEVAL_MISSING_{}", std::process::id());
        let present_env = format!("IMAGE_RETRIEVAL_PRESENT_{}", std::process::id());
        std::env::remove_var(&missing_env);
        std::env::set_var(&present_env, "dummy-key");

        let config = RuntimeConfig {
            providers: vec![
                SearchProviderConfig {
                    provider_id: "serpapi_missing".into(),
                    provider_kind: SearchProviderKind::SerpapiGoogleImages,
                    enabled: true,
                    weight: 1,
                    endpoint: Some("https://serpapi.com/search".into()),
                    credential_env: Some(missing_env),
                    default_query_params: Default::default(),
                },
                SearchProviderConfig {
                    provider_id: "serpapi_present".into(),
                    provider_kind: SearchProviderKind::SerpapiGoogleImages,
                    enabled: true,
                    weight: 1,
                    endpoint: Some("https://serpapi.com/search".into()),
                    credential_env: Some(present_env.clone()),
                    default_query_params: Default::default(),
                },
            ],
            retrieval_channels: vec![],
            vlm_evaluation: VlmEvaluationConfig::default(),
            policy: Default::default(),
            quality_defaults: Default::default(),
            output: Default::default(),
            limits: Default::default(),
        };

        let registry = build_provider_registry(&config);
        let reports = registry.evaluate_readiness();
        let present = reports
            .iter()
            .find(|r| r.provider_id.to_string() == "serpapi_present")
            .unwrap();

        assert_eq!(present.status, ProviderReadinessStatus::Ready);
        std::env::remove_var(present_env);
    }

    #[test]
    fn retrieval_channel_sorting_keeps_channel_config_pairing() {
        let dir = std::env::temp_dir().join(format!("channel-pairing-{}", std::process::id()));
        let config = RuntimeConfig {
            providers: vec![],
            retrieval_channels: vec![
                RetrievalChannelConfig {
                    channel_id: "paid".into(),
                    channel_kind: RetrievalChannelKind::PaidOnlineService,
                    tier: RetrievalChannelTier::PaidOnlineService,
                    enabled: true,
                    endpoint: Some("https://paid.example.test".into()),
                    credential_env: None,
                    max_batch_size: None,
                },
                RetrievalChannelConfig {
                    channel_id: "web".into(),
                    channel_kind: RetrievalChannelKind::NormalWebFetch,
                    tier: RetrievalChannelTier::NormalWebFetch,
                    enabled: true,
                    endpoint: None,
                    credential_env: None,
                    max_batch_size: None,
                },
            ],
            vlm_evaluation: VlmEvaluationConfig::default(),
            policy: Default::default(),
            quality_defaults: Default::default(),
            output: Default::default(),
            limits: Default::default(),
        };

        let channels = build_configured_retrieval_channels(&config, &dir).unwrap();

        assert_eq!(channels[0].channel.channel_id().to_string(), "web");
        assert_eq!(channels[0].config.channel_id, "web");
        assert_eq!(channels[1].channel.channel_id().to_string(), "paid");
        assert_eq!(channels[1].config.channel_id, "paid");

        let _ = std::fs::remove_dir_all(&dir);
    }

    fn make_job(id: &str) -> RetrievalJob {
        RetrievalJob {
            retrieval_job_id: RetrievalJobId::new(format!("job-{}", id)),
            query_plan_id: "qp-1".into(),
            candidate_id: id.into(),
            full_attempt_count: 1,
            retry_count: 0,
            retrieval_priority: 1,
            target: RetrievalTarget {
                target_type: RetrievalTargetType::Image,
                primary_image_url: format!("https://example.com/{}.jpg", id),
                alternate_source_page_url: None,
                thumbnail_url: None,
                expected_mime_type: None,
                license_hint: None,
                provider_id: "provider".into(),
                candidate_provenance_refs: vec![],
            },
            candidate_quality_decision_ref: format!("quality-{}", id),
            requested_outputs: vec![],
            policy_context: RetrievalPolicyContext::default(),
        }
    }

    fn complete_artifact_for_job(job: &RetrievalJob) -> RetrievalArtifactResult {
        let mut artifact = complete_artifact();
        artifact.retrieval_job_id = job.retrieval_job_id.clone();
        artifact.query_plan_id = job.query_plan_id.clone();
        artifact.candidate_id = job.candidate_id.clone();
        artifact.fetch_trace = vec![RetrievalAttemptTrace {
            attempt_id: format!("attempt-{}", job.retrieval_job_id),
            retrieval_job_id: job.retrieval_job_id.clone(),
            query_plan_id: job.query_plan_id.clone(),
            candidate_id: job.candidate_id.clone(),
            channel_id: RetrievalChannelId::new("web"),
            channel_tier: RetrievalChannelTier::NormalWebFetch,
            attempt_mode: RetrievalAttemptMode::DirectImageFetch,
            started_at: String::new(),
            completed_at: Some(String::new()),
            target_url_redacted: None,
            source_page_url_redacted: None,
            final_url_redacted: None,
            http_status: Some(200),
            bytes_received: Some(123),
            status: RetrievalAttemptStatus::Succeeded,
            failure_code: None,
            retryable: false,
            fallback_allowed: false,
            policy_reason: None,
            artifact_refs: vec![],
            redaction_applied: true,
        }];
        artifact
    }

    fn failed_artifact_for_job(
        job: &RetrievalJob,
        fallback_allowed: bool,
    ) -> RetrievalArtifactResult {
        RetrievalArtifactResult::failed(
            job,
            "batch-1",
            RetrievalChannelId::new("web"),
            RetrievalChannelTier::NormalWebFetch,
            RetrievalAttemptMode::DirectImageFetch,
            "network failure",
            crate::domain::retrieval::RetrievalFailureCode::RetrievalDirectFetchNetwork,
            vec![RetrievalAttemptTrace {
                attempt_id: format!("attempt-{}", job.retrieval_job_id),
                retrieval_job_id: job.retrieval_job_id.clone(),
                query_plan_id: job.query_plan_id.clone(),
                candidate_id: job.candidate_id.clone(),
                channel_id: RetrievalChannelId::new("web"),
                channel_tier: RetrievalChannelTier::NormalWebFetch,
                attempt_mode: RetrievalAttemptMode::DirectImageFetch,
                started_at: String::new(),
                completed_at: None,
                target_url_redacted: None,
                source_page_url_redacted: None,
                final_url_redacted: None,
                http_status: None,
                bytes_received: None,
                status: RetrievalAttemptStatus::Failed,
                failure_code: Some(
                    crate::domain::retrieval::RetrievalFailureCode::RetrievalDirectFetchNetwork,
                ),
                retryable: true,
                fallback_allowed,
                policy_reason: None,
                artifact_refs: vec![],
                redaction_applied: true,
            }],
            vec![],
        )
    }

    #[test]
    fn pending_fallback_jobs_excludes_completed_jobs() {
        let job_a = make_job("a");
        let job_b = make_job("b");
        let batch = crate::domain::retrieval::RetrievalBatch::new(
            "batch-1",
            "qp-1",
            1,
            0,
            2,
            vec![job_a.clone(), job_b.clone()],
            None,
        );
        let mut best_results = HashMap::new();
        best_results.insert(
            job_a.retrieval_job_id.to_string(),
            complete_artifact_for_job(&job_a),
        );
        best_results.insert(
            job_b.retrieval_job_id.to_string(),
            failed_artifact_for_job(&job_b, true),
        );

        let pending = pending_fallback_jobs(&batch, &best_results);

        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].retrieval_job_id, job_b.retrieval_job_id);
    }

    #[test]
    fn pending_fallback_jobs_respects_non_fallbackable_failure() {
        let job = make_job("a");
        let batch = crate::domain::retrieval::RetrievalBatch::new(
            "batch-1",
            "qp-1",
            1,
            0,
            1,
            vec![job.clone()],
            None,
        );
        let mut best_results = HashMap::new();
        best_results.insert(
            job.retrieval_job_id.to_string(),
            failed_artifact_for_job(&job, false),
        );

        let pending = pending_fallback_jobs(&batch, &best_results);

        assert!(pending.is_empty());
    }
}
