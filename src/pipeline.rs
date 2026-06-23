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

use std::sync::Arc;

use crate::domain::candidate::{
    CandidateDecision, CandidateQualityDecision, CandidateQualityStatus, RetrievableCandidate,
    RetrievableCandidateBatch,
};
use crate::domain::config::{RuntimeConfig, SearchProviderKind};
use crate::domain::delivery::{
    CoverageGapType, DeliveredImageRecord, PipelineStage, WorkflowFailureCode,
};
use crate::domain::image::ImageRecord;
use crate::domain::query_plan::{
    NormalizedQueryPlan, QueryPlanId, QueryProviderPolicy, QueryRetrievalPolicy, ValidatedQueryPlan,
};
use crate::domain::search::{ProviderReadinessRecord, SearchOutcome, SearchSessionOutcome};
use crate::orchestrator::RunOrchestrator;
use crate::ports::{BaseRetrievalChannel, BaseSearchProvider};
use crate::quality::candidate::gate::CandidateQualityGate;
use crate::quality::image::gate::ImageAcceptanceGate;
use crate::quality::qwen_vlm::QwenVlmEvaluator;
use crate::retrieval::batch_planner::RetrievalBatchPlanner;
use crate::retrieval::channels::web_fetch::WebFetchChannel;
use crate::search::registry::ProviderRegistry;
use crate::search::scheduler::{SearchScheduler, StdRandom};
use crate::search::serpapi::SerpApiGoogleImagesAdapter;

/// Internal-error exit code returned when an unrecoverable error occurs while
/// wiring the pipeline (mirrors `main::exit_code::INTERNAL_ERROR`).
const INTERNAL_ERROR_EXIT_CODE: i32 = 70;

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

    // --- Step 1: provider registry + SerpApi adapter ---
    let registry = build_provider_registry(config);

    // --- Step 2: search ---
    let normalized = normalized_plan_from_validated(validated);
    let scheduler = SearchScheduler::new();
    let mut rng = StdRandom;
    let session = scheduler.run(&normalized, &registry, &mut rng);
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
    let search_outcome = search_outcome_from_session(&session);
    let candidate_gate = CandidateQualityGate::new(&evaluator, validated.clone());
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

    let retrievable = build_retrievable_batch(&candidate_result.retrievable_sequence, validated);
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
    let channel = WebFetchChannel::new(staging_dir).map_err(|e| {
        eprintln!("Error: cannot initialize retrieval channel: {}", e);
        INTERNAL_ERROR_EXIT_CODE
    })?;

    let batch_result = match channel.retrieve_batch(&batch) {
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
    for (index, decision) in image_result.qualified_images.iter().enumerate() {
        if let crate::domain::image::ImageAcceptanceDecision::Accepted { image, notes } = decision {
            orchestrator.add_accepted_image(delivered_record_from_image(
                image,
                notes,
                &normalized.query_plan_id,
                index,
            ));
        }
    }
    summary.accepted_image_count = image_result.qualified_images.len();

    // --- Coverage gap if short of target ---
    let accepted = summary.accepted_image_count as u32;
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

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a provider registry, attaching a production SerpApi adapter to every
/// configured SerpApi Google Images provider.
fn build_provider_registry(config: &RuntimeConfig) -> ProviderRegistry {
    ProviderRegistry::from_config(config, |kind| match kind {
        SearchProviderKind::SerpapiGoogleImages => {
            // The closure only receives the kind, so we resolve the
            // credential env from the matching provider config when present.
            let provider_cfg = config
                .providers
                .iter()
                .find(|p| p.provider_kind == SearchProviderKind::SerpapiGoogleImages);
            let adapter = match provider_cfg {
                Some(cfg) => SerpApiGoogleImagesAdapter::from_config(cfg, true),
                None => SerpApiGoogleImagesAdapter::default_production(),
            };
            Some(Arc::new(adapter) as Arc<dyn BaseSearchProvider>)
        }
        _ => None,
    })
}

/// Build a downstream-consumable [`NormalizedQueryPlan`] from the legacy
/// [`ValidatedQueryPlan`] used by `cmd_run`.
fn normalized_plan_from_validated(validated: &ValidatedQueryPlan) -> NormalizedQueryPlan {
    let required = validated.required_count.max(1);
    let retry_limit = validated.retry_limit.min(3) as u8;
    NormalizedQueryPlan {
        query_plan_id: QueryPlanId::generate(),
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
    validated: &ValidatedQueryPlan,
) -> RetrievableCandidateBatch {
    let query_plan_id = validated.description.chars().take(8).collect::<String>();
    let retrieval_batch_target = validated.required_count.saturating_mul(2).max(1);

    let candidates: Vec<RetrievableCandidate> = sequence
        .candidates
        .iter()
        .filter_map(|decision| match decision {
            CandidateDecision::Accepted {
                candidate,
                priority,
            } => {
                let quality_decision = CandidateQualityDecision {
                    candidate_id: candidate.candidate_id.clone(),
                    query_plan_id: query_plan_id.clone(),
                    mechanical_passed: true,
                    vlm_passed: true,
                    final_status: CandidateQualityStatus::Retrievable,
                    priority: *priority,
                    blocking_metrics: Vec::new(),
                    reference_metrics: Vec::new(),
                    vlm_decision: None,
                    diagnostics: Vec::new(),
                };
                Some(RetrievableCandidate {
                    candidate: candidate.clone(),
                    candidate_quality_decision: quality_decision,
                    retrieval_priority: *priority,
                    primary_image_url: candidate.image_url.clone(),
                    source_page_url: candidate.source_page_url.clone(),
                    thumbnail_url: candidate.thumbnail_url.clone(),
                    expected_mime_type: candidate.mime_type.clone(),
                    license_hint: candidate.license_hint.clone(),
                    provenance_refs: Vec::new(),
                })
            }
            _ => None,
        })
        .collect();

    RetrievableCandidateBatch {
        query_plan_id,
        full_attempt_count: 1,
        retry_count: 0,
        retrieval_batch_target,
        candidates,
        rejected_decisions: Vec::new(),
        execution_blocking_facts: Vec::new(),
    }
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
            })
        })
        .collect()
}

/// Build a [`DeliveredImageRecord`] from an accepted image. Artifact sidecar
/// paths that are not produced by the web-fetch channel are left empty.
fn delivered_record_from_image(
    image: &ImageRecord,
    notes: &str,
    query_plan_id: &QueryPlanId,
    index: usize,
) -> DeliveredImageRecord {
    let (width, height) = match &image.dimensions {
        Some(d) => (Some(d.width), Some(d.height)),
        None => (None, None),
    };
    DeliveredImageRecord {
        delivered_image_id: format!("delivered-{}-{}", query_plan_id, index + 1),
        query_plan_id: query_plan_id.to_string(),
        candidate_id: image.candidate_id.clone(),
        retrieval_job_id: String::new(),
        package_image_path: image.local_path.clone(),
        local_artifact_path: image.local_path.clone(),
        source_artifact_path: String::new(),
        source_sidecar_path: String::new(),
        content_summary_path: String::new(),
        task_report_path: String::new(),
        visual_description_path: String::new(),
        checksum_sha256: String::new(),
        content_type: image.content_type.clone().unwrap_or_default(),
        file_size_bytes: image.file_size_bytes,
        width,
        height,
        candidate_quality_decision_ref: String::new(),
        image_acceptance_decision_ref: notes.to_string(),
        manifest_entry_ref: String::new(),
    }
}
