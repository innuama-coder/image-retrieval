//! Retrieval integration tests — v1.1 TASK-004.
//!
//! Tests retrieval batch planning, channel fallback execution, artifact
//! completeness, policy blockers, metadata-only rejection, and fixture
//! evidence boundaries.
//!
//! References: `docs/design/v1.1-TASK-004-retrieval-artifact-channel-design.md`

use image_retrieval::domain::candidate::{
    CandidateId, CandidateQualityDecision, CandidateQualityStatus, CandidateRecord, ProviderId,
    RetrievableCandidate, RetrievableCandidateBatch,
};
use image_retrieval::domain::config::{RetrievalChannelConfig, RetrievalChannelKind};
use image_retrieval::domain::query_plan::{QueryPlanId, QueryRetrievalPolicy};
use image_retrieval::domain::retrieval::{
    RetrievalArtifactResult, RetrievalAttemptMode, RetrievalChannelCapabilities,
    RetrievalChannelId, RetrievalChannelReadinessReport, RetrievalChannelTier,
    RetrievalFailureCode, RetrievalJob, RetrievalJobId, RetrievalPolicyContext,
    RetrievalShortageCode, RetrievalStatus,
};
use image_retrieval::ports::{BaseRetrievalChannel, RetrievalError};
use image_retrieval::retrieval::batch_planner::RetrievalBatchPlanner;
use image_retrieval::retrieval::channels::fixture::FixtureChannel;
use image_retrieval::retrieval::channels::paid::PaidChannel;
use image_retrieval::retrieval::channels::self_hosted::SelfHostedChannel;
use image_retrieval::retrieval::{plan_and_execute, FixtureResponse};
use std::path::PathBuf;

// =============================================================================
// Test helpers
// =============================================================================

fn make_retrievable(id: &str, image_url: &str, priority: u32) -> RetrievableCandidate {
    let rec = CandidateRecord::minimal(CandidateId::new(id), ProviderId::new("test"), image_url);
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
        source_page_url: Some(format!("https://example.com/page/{}", id)),
        thumbnail_url: None,
        expected_mime_type: Some("image/jpeg".into()),
        license_hint: None,
        provenance_refs: vec![],
    }
}

fn make_batch_data(candidates: Vec<RetrievableCandidate>) -> RetrievableCandidateBatch {
    let len = candidates.len() as u32;
    RetrievableCandidateBatch {
        query_plan_id: "qp-test".into(),
        full_attempt_count: 1,
        retry_count: 0,
        retrieval_batch_target: (len * 2).max(2),
        candidates,
        rejected_decisions: vec![],
        execution_blocking_facts: vec![],
    }
}

fn default_channel_config(tier: RetrievalChannelTier) -> RetrievalChannelConfig {
    RetrievalChannelConfig {
        channel_id: format!("test-{}", tier),
        channel_kind: RetrievalChannelKind::NormalWebFetch,
        tier,
        enabled: true,
        endpoint: None,
        credential_env: None,
        max_batch_size: None,
    }
}

fn complete_result(job_id: &str, candidate_id: &str) -> RetrievalArtifactResult {
    RetrievalArtifactResult {
        retrieval_job_id: RetrievalJobId::new(job_id),
        retrieval_batch_id: "b-1".into(),
        query_plan_id: "qp-1".into(),
        candidate_id: candidate_id.into(),
        channel_id: RetrievalChannelId::new("wf"),
        channel_tier: RetrievalChannelTier::NormalWebFetch,
        attempt_mode: RetrievalAttemptMode::DirectImageFetch,
        retrieval_status: RetrievalStatus::Complete,
        local_artifact_path: Some(PathBuf::from("/tmp/img.jpg")),
        source_artifact_path: Some(PathBuf::from("/tmp/src.jpg")),
        source_sidecar_path: Some(PathBuf::from("/tmp/sidecar.json")),
        content_summary_path: Some(PathBuf::from("/tmp/summary.json")),
        task_report_path: Some(PathBuf::from("/tmp/report.json")),
        visual_description_path: Some(PathBuf::from("/tmp/vd.json")),
        diagnostics_path: None,
        checksum_sha256: Some("abc".into()),
        content_type_reported: Some("image/jpeg".into()),
        content_type_sniffed: Some("image/jpeg".into()),
        content_type: Some("image/jpeg".into()),
        file_extension: Some("jpg".into()),
        file_size_bytes: Some(1024),
        image_dimensions: None,
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
        redaction_applied: false,
    }
}

// =============================================================================
// Batch planner tests
// =============================================================================

#[test]
fn batch_target_for_1_is_2() {
    assert_eq!(RetrievalBatchPlanner::batch_target_for(1), 2);
}

#[test]
fn batch_target_for_4_is_8() {
    assert_eq!(RetrievalBatchPlanner::batch_target_for(4), 8);
}

#[test]
fn batch_target_for_0_is_0() {
    assert_eq!(RetrievalBatchPlanner::batch_target_for(0), 0);
}

#[test]
fn batch_planner_normal_exact_target() {
    let candidates = vec![
        make_retrievable("a", "https://example.com/a.jpg", 5),
        make_retrievable("b", "https://example.com/b.jpg", 4),
    ];
    let rb = RetrievableCandidateBatch {
        retrieval_batch_target: 2,
        ..make_batch_data(candidates)
    };

    let qp_id = QueryPlanId::new("qp-test");
    let policy = QueryRetrievalPolicy::default();
    let (batch, shortage) =
        RetrievalBatchPlanner::plan_from_batch(&rb, &qp_id, &policy, "warn", &[], false);

    assert_eq!(batch.actual_size, 2);
    assert!(!batch.is_short_batch);
    assert_eq!(batch.jobs.len(), 2);
    // Higher priority first
    assert_eq!(batch.jobs[0].candidate_id, "a");
    assert!(shortage.is_none());
}

#[test]
fn batch_planner_short_batch_when_fewer() {
    let candidates = vec![
        make_retrievable("a", "https://example.com/a.jpg", 5),
        make_retrievable("b", "https://example.com/b.jpg", 3),
    ];
    let rb = RetrievableCandidateBatch {
        retrieval_batch_target: 8,
        ..make_batch_data(candidates)
    };

    let qp_id = QueryPlanId::new("qp-test");
    let policy = QueryRetrievalPolicy::default();
    let (batch, _shortage) =
        RetrievalBatchPlanner::plan_from_batch(&rb, &qp_id, &policy, "warn", &[], false);

    assert!(batch.is_short_batch);
    assert_eq!(batch.actual_size, 2);
}

#[test]
fn batch_planner_empty_returns_shortage() {
    let rb = RetrievableCandidateBatch {
        retrieval_batch_target: 4,
        ..make_batch_data(vec![])
    };

    let qp_id = QueryPlanId::new("qp-test");
    let policy = QueryRetrievalPolicy::default();
    let (batch, _shortage) =
        RetrievalBatchPlanner::plan_from_batch(&rb, &qp_id, &policy, "warn", &[], false);

    assert_eq!(batch.actual_size, 0);
    assert!(batch.is_short_batch);
}

// =============================================================================
// RetrievalJob construction tests
// =============================================================================

#[test]
fn retrieval_job_has_unique_id() {
    let rc = make_retrievable("cand-1", "https://example.com/1.jpg", 5);
    let job = RetrievalJob::from_retrievable(
        &rc,
        "batch-1",
        "qp-1",
        1,
        0,
        "qd-1",
        RetrievalPolicyContext::default(),
    );
    assert!(job.retrieval_job_id.to_string().contains("qp-1"));
    assert!(job.retrieval_job_id.to_string().contains("cand-1"));
    assert_eq!(job.candidate_id, "cand-1");
    assert_eq!(job.query_plan_id, "qp-1");
    assert_eq!(job.full_attempt_count, 1);
    assert_eq!(job.retry_count, 0);
}

#[test]
fn retrieval_job_preserves_ownership() {
    let rc = make_retrievable("cand-2", "https://example.com/2.jpg", 10);
    let job = RetrievalJob::from_retrievable(
        &rc,
        "batch-2",
        "qp-2",
        2,
        1,
        "qd-2",
        RetrievalPolicyContext::default(),
    );
    assert_eq!(job.retry_count, 1);
    assert_eq!(job.full_attempt_count, 2);
    assert_eq!(job.target.primary_image_url, "https://example.com/2.jpg");
}

// =============================================================================
// Artifact completeness tests
// =============================================================================

#[test]
fn complete_result_has_all_fields() {
    let result = complete_result("ret-1", "cand-1");
    assert!(result.is_complete());
    assert!(result.has_all_required_paths());
    assert!(result.has_all_integrity_fields());
    assert!(!result.is_metadata_only_result());
}

#[test]
fn missing_paths_prevent_complete() {
    let result = RetrievalArtifactResult {
        retrieval_job_id: RetrievalJobId::new("ret-2"),
        retrieval_batch_id: "b-1".into(),
        query_plan_id: "qp-1".into(),
        candidate_id: "cand-2".into(),
        channel_id: RetrievalChannelId::new("wf"),
        channel_tier: RetrievalChannelTier::NormalWebFetch,
        attempt_mode: RetrievalAttemptMode::DirectImageFetch,
        retrieval_status: RetrievalStatus::Failed,
        local_artifact_path: None,
        source_artifact_path: None,
        source_sidecar_path: None,
        content_summary_path: None,
        task_report_path: None,
        visual_description_path: None,
        diagnostics_path: None,
        checksum_sha256: None,
        content_type_reported: None,
        content_type_sniffed: None,
        content_type: None,
        file_extension: None,
        file_size_bytes: None,
        image_dimensions: None,
        media_type_match: false,
        local_artifact_exists: false,
        source_artifact_exists: false,
        sidecar_valid: false,
        summary_quality_passed: false,
        task_report_valid: false,
        visual_description_valid: false,
        job_ownership_valid: false,
        metadata_only: true,
        fetch_trace: vec![],
        policy_decisions: vec![],
        diagnostics: vec![],
        failure_reason: None,
        redaction_applied: false,
    };

    assert!(!result.is_complete());
    assert!(!result.has_all_required_paths());
    assert!(result.is_metadata_only_result());
}

// =============================================================================
// Metadata-only rejection tests
// =============================================================================

#[test]
fn metadata_only_result_is_not_complete() {
    let channel = FixtureChannel::new(RetrievalChannelTier::NormalWebFetch)
        .with_response("c1", FixtureResponse::metadata_only());

    let rc = make_retrievable("c1", "https://example.com/1.jpg", 5);
    let rb = RetrievableCandidateBatch {
        retrieval_batch_target: 2,
        ..make_batch_data(vec![rc])
    };

    let qp_id = QueryPlanId::new("qp-test");
    let policy = QueryRetrievalPolicy::default();
    let channels: Vec<&dyn BaseRetrievalChannel> = vec![&channel];
    let configs = vec![default_channel_config(RetrievalChannelTier::NormalWebFetch)];

    let result = plan_and_execute(&rb, &qp_id, &policy, "warn", &[], true, &channels, &configs);

    assert_eq!(result.results.len(), 1);
    assert!(!result.results[0].is_complete());
    assert!(result.results[0].is_metadata_only_result());
}

// =============================================================================
// Fallback order tests
// =============================================================================

#[test]
fn fallback_to_self_hosted_on_network_failure() {
    let rc = make_retrievable("a", "https://example.com/a.jpg", 5);
    let rb = RetrievableCandidateBatch {
        retrieval_batch_target: 2,
        ..make_batch_data(vec![rc])
    };

    let qp_id = QueryPlanId::new("qp-test");
    let policy = QueryRetrievalPolicy::default();

    let fail_channel = FixtureChannel::new(RetrievalChannelTier::NormalWebFetch)
        .with_response("a", FixtureResponse::network_failure());
    let success_channel = FixtureChannel::new(RetrievalChannelTier::SelfHostedService)
        .with_response("a", FixtureResponse::success());

    let channels: Vec<&dyn BaseRetrievalChannel> = vec![&fail_channel, &success_channel];
    let configs = vec![
        default_channel_config(RetrievalChannelTier::NormalWebFetch),
        default_channel_config(RetrievalChannelTier::SelfHostedService),
    ];

    let result = plan_and_execute(&rb, &qp_id, &policy, "warn", &[], true, &channels, &configs);

    assert_eq!(result.results.len(), 1);
    assert!(result.results[0].is_complete());
    assert!(!result.fallback_decisions.is_empty());
}

// =============================================================================
// Access-restricted blocks fallback tests
// =============================================================================

#[test]
fn access_restricted_stops_fallback() {
    let rc = make_retrievable("a", "https://restricted.example.com/a.jpg", 5);
    let rb = RetrievableCandidateBatch {
        retrieval_batch_target: 2,
        ..make_batch_data(vec![rc])
    };

    let qp_id = QueryPlanId::new("qp-test");
    let policy = QueryRetrievalPolicy::default();

    let restricted_channel = FixtureChannel::new(RetrievalChannelTier::NormalWebFetch)
        .with_response("a", FixtureResponse::access_restricted());
    let higher_channel = FixtureChannel::new(RetrievalChannelTier::SelfHostedService)
        .with_response("a", FixtureResponse::success());

    let channels: Vec<&dyn BaseRetrievalChannel> = vec![&restricted_channel, &higher_channel];
    let configs = vec![
        default_channel_config(RetrievalChannelTier::NormalWebFetch),
        default_channel_config(RetrievalChannelTier::SelfHostedService),
    ];

    let result = plan_and_execute(&rb, &qp_id, &policy, "warn", &[], true, &channels, &configs);

    // Must NOT have fallen back to self-hosted
    assert!(!result.results[0].is_complete());
    assert_eq!(
        result.results[0].retrieval_status,
        RetrievalStatus::AccessRestricted
    );
}

// =============================================================================
// Paid channel disabled tests
// =============================================================================

#[test]
fn paid_channel_skipped_when_not_allowed() {
    let rc = make_retrievable("a", "https://example.com/a.jpg", 5);
    let rb = RetrievableCandidateBatch {
        retrieval_batch_target: 2,
        ..make_batch_data(vec![rc])
    };

    let qp_id = QueryPlanId::new("qp-test");
    let policy = QueryRetrievalPolicy {
        allow_paid: false,
        ..Default::default()
    };

    let fail_channel = FixtureChannel::new(RetrievalChannelTier::NormalWebFetch)
        .with_response("a", FixtureResponse::network_failure());
    let paid_channel = FixtureChannel::new(RetrievalChannelTier::PaidOnlineService)
        .with_response("a", FixtureResponse::success());

    let channels: Vec<&dyn BaseRetrievalChannel> = vec![&fail_channel, &paid_channel];
    let configs = vec![
        default_channel_config(RetrievalChannelTier::NormalWebFetch),
        default_channel_config(RetrievalChannelTier::PaidOnlineService),
    ];

    let result = plan_and_execute(&rb, &qp_id, &policy, "warn", &[], true, &channels, &configs);

    // Paid should be skipped, job fails
    assert!(!result.results[0].is_complete());
}

#[test]
fn paid_channel_readiness_unconfirmed_by_default() {
    let channel = PaidChannel::new();
    let config = RetrievalChannelConfig {
        channel_id: "paid-1".into(),
        channel_kind: RetrievalChannelKind::PaidOnlineService,
        tier: RetrievalChannelTier::PaidOnlineService,
        enabled: false,
        endpoint: None,
        credential_env: None,
        max_batch_size: None,
    };
    let report = channel.readiness(&config);
    assert!(!report.available);
    assert_eq!(
        report.failure_code,
        Some(RetrievalFailureCode::RetrievalPaidUnconfirmed)
    );
}

// =============================================================================
// Channel readiness tests
// =============================================================================

#[test]
fn self_hosted_channel_disabled_by_default() {
    let channel = SelfHostedChannel::new();
    assert_eq!(channel.tier(), RetrievalChannelTier::SelfHostedService);
    assert_eq!(channel.display_name(), "Self-Hosted Retrieval Service");
}

#[test]
fn fixture_channel_provides_capabilities() {
    let channel = FixtureChannel::new(RetrievalChannelTier::NormalWebFetch);
    let caps = channel.capabilities();
    assert!(caps.supports_direct_image_fetch);
    assert!(caps.fixture_only);
}

#[test]
fn channel_readiness_report_ready() {
    let report = RetrievalChannelReadinessReport::ready(
        RetrievalChannelId::new("wf-1"),
        "Web Fetch",
        RetrievalChannelTier::NormalWebFetch,
    );
    assert!(report.available);
    assert!(report.enabled);
    assert!(report.failure_code.is_none());
}

#[test]
fn channel_readiness_report_fixture_blocked() {
    let report = RetrievalChannelReadinessReport::fixture_blocked(
        RetrievalChannelId::new("fix-1"),
        "Fixture",
        RetrievalChannelTier::NormalWebFetch,
    );
    assert!(!report.available);
    assert_eq!(
        report.failure_code,
        Some(RetrievalFailureCode::RetrievalFixtureNotProduction)
    );
}

// =============================================================================
// Tier serialization tests
// =============================================================================

#[test]
fn tier_serde_canonical_names() {
    let json = r#""normal_web_fetch""#;
    let tier: RetrievalChannelTier = serde_json::from_str(json).expect("deserialize");
    assert_eq!(tier, RetrievalChannelTier::NormalWebFetch);

    let json = r#""self_hosted_service""#;
    let tier: RetrievalChannelTier = serde_json::from_str(json).expect("deserialize");
    assert_eq!(tier, RetrievalChannelTier::SelfHostedService);

    let json = r#""paid_online_service""#;
    let tier: RetrievalChannelTier = serde_json::from_str(json).expect("deserialize");
    assert_eq!(tier, RetrievalChannelTier::PaidOnlineService);
}

#[test]
fn tier_serde_backward_compat_aliases() {
    // Old names still deserialize
    let json = r#""web_fetch""#;
    let tier: RetrievalChannelTier = serde_json::from_str(json).expect("deserialize");
    assert_eq!(tier, RetrievalChannelTier::NormalWebFetch);

    let json = r#""self_hosted""#;
    let tier: RetrievalChannelTier = serde_json::from_str(json).expect("deserialize");
    assert_eq!(tier, RetrievalChannelTier::SelfHostedService);

    let json = r#""paid""#;
    let tier: RetrievalChannelTier = serde_json::from_str(json).expect("deserialize");
    assert_eq!(tier, RetrievalChannelTier::PaidOnlineService);
}

#[test]
fn tier_serializes_to_canonical() {
    assert_eq!(
        serde_json::to_string(&RetrievalChannelTier::NormalWebFetch).unwrap(),
        r#""normal_web_fetch""#
    );
    assert_eq!(
        serde_json::to_string(&RetrievalChannelTier::SelfHostedService).unwrap(),
        r#""self_hosted_service""#
    );
    assert_eq!(
        serde_json::to_string(&RetrievalChannelTier::PaidOnlineService).unwrap(),
        r#""paid_online_service""#
    );
}

// =============================================================================
// RetrievalFailureCode tests
// =============================================================================

#[test]
fn failure_code_display_correct() {
    assert_eq!(
        RetrievalFailureCode::RetrievalDirectFetchNetwork.to_string(),
        "RETRIEVAL_DIRECT_FETCH_NETWORK"
    );
    assert_eq!(
        RetrievalFailureCode::RetrievalAccessRestricted.to_string(),
        "RETRIEVAL_ACCESS_RESTRICTED"
    );
    assert_eq!(
        RetrievalFailureCode::RetrievalPaidUnconfirmed.to_string(),
        "RETRIEVAL_PAID_UNCONFIRMED"
    );
    assert_eq!(
        RetrievalFailureCode::RetrievalMetadataOnly.to_string(),
        "RETRIEVAL_METADATA_ONLY"
    );
}

// =============================================================================
// RetrievalBatchShortage tests
// =============================================================================

#[test]
fn shortage_codes_serde() {
    assert_eq!(
        serde_json::to_string(&RetrievalShortageCode::NoRetrievableCandidates).unwrap(),
        r#""NO_RETRIEVABLE_CANDIDATES""#
    );
    assert_eq!(
        serde_json::to_string(&RetrievalShortageCode::InsufficientRetrievableCandidates).unwrap(),
        r#""INSUFFICIENT_RETRIEVABLE_CANDIDATES""#
    );
}

#[test]
fn shortage_constructs_correctly() {
    let shortage = image_retrieval::domain::retrieval::RetrievalBatchShortage::new(
        "qp-1",
        8,
        2,
        RetrievalShortageCode::InsufficientRetrievableCandidates,
        "only 2 candidates",
    );
    assert_eq!(shortage.query_plan_id, "qp-1");
    assert_eq!(shortage.target_size, 8);
    assert_eq!(shortage.actual_size, 2);
}

// =============================================================================
// RetrievalBatch tests
// =============================================================================

#[test]
fn retrieval_batch_is_short_when_jobs_less_than_target() {
    let rc = make_retrievable("a", "https://example.com/a.jpg", 5);
    let rb = RetrievableCandidateBatch {
        retrieval_batch_target: 8,
        ..make_batch_data(vec![rc])
    };
    let qp_id = QueryPlanId::new("qp-test");
    let policy = QueryRetrievalPolicy::default();
    let (batch, _) =
        RetrievalBatchPlanner::plan_from_batch(&rb, &qp_id, &policy, "warn", &[], false);
    assert!(batch.is_short_batch);
    assert_eq!(batch.actual_size, 1);
    assert_eq!(batch.target_size, 8);
}

// =============================================================================
// RetrievalChannelCapabilities tests
// =============================================================================

#[test]
fn capabilities_default_values() {
    let caps = RetrievalChannelCapabilities::default();
    assert!(caps.supports_direct_image_fetch);
    assert!(caps.supports_source_page_resolve);
    assert!(caps.supports_checksum);
    assert!(!caps.fixture_only);
}

// =============================================================================
// RetrievalAttemptMode tests
// =============================================================================

#[test]
fn attempt_mode_is_normal_web() {
    assert!(RetrievalAttemptMode::DirectImageFetch.is_normal_web());
    assert!(RetrievalAttemptMode::SourcePageResolve.is_normal_web());
    assert!(!RetrievalAttemptMode::SelfHostedService.is_normal_web());
    assert!(!RetrievalAttemptMode::PaidOnlineService.is_normal_web());
}

// =============================================================================
// RetrievalStatus tests
// =============================================================================

#[test]
fn status_is_complete() {
    assert!(RetrievalStatus::Complete.is_complete());
    assert!(!RetrievalStatus::Failed.is_complete());
    assert!(!RetrievalStatus::Partial.is_complete());
}

#[test]
fn status_is_terminal_failure() {
    assert!(RetrievalStatus::Failed.is_terminal_failure());
    assert!(RetrievalStatus::PolicyBlocked.is_terminal_failure());
    assert!(!RetrievalStatus::Complete.is_terminal_failure());
    assert!(!RetrievalStatus::Partial.is_terminal_failure());
}

// =============================================================================
// RetrievalError tests
// =============================================================================

#[test]
fn retrieval_error_to_failure_code() {
    assert_eq!(
        RetrievalError::AccessRestricted {
            message: "denied".into()
        }
        .to_failure_code(),
        RetrievalFailureCode::RetrievalAccessRestricted
    );
    assert_eq!(
        RetrievalError::PaidUnconfirmed.to_failure_code(),
        RetrievalFailureCode::RetrievalPaidUnconfirmed
    );
    assert_eq!(
        RetrievalError::Network {
            message: "timeout".into()
        }
        .to_failure_code(),
        RetrievalFailureCode::RetrievalDirectFetchNetwork
    );
}

#[test]
fn retrieval_error_allows_fallback() {
    assert!(RetrievalError::Network {
        message: "timeout".into()
    }
    .allows_fallback());
    assert!(!RetrievalError::AccessRestricted {
        message: "403".into()
    }
    .allows_fallback());
    assert!(!RetrievalError::PaidUnconfirmed.allows_fallback());
}

// =============================================================================
// Test all four channels satisfy the BaseRetrievalChannel trait
// =============================================================================

#[test]
fn all_channels_satisfy_the_trait() {
    fn _assert_trait(_ch: &dyn BaseRetrievalChannel) {}

    let web_fetch = image_retrieval::retrieval::WebFetchChannel::new("/tmp/test-wf-ret").unwrap();
    let fixture = FixtureChannel::new(RetrievalChannelTier::NormalWebFetch);
    let self_hosted = SelfHostedChannel::new();
    let paid = PaidChannel::new();

    _assert_trait(&web_fetch);
    _assert_trait(&fixture);
    _assert_trait(&self_hosted);
    _assert_trait(&paid);

    // Cleanup
    let _ = std::fs::remove_dir_all("/tmp/test-wf-ret");
}
