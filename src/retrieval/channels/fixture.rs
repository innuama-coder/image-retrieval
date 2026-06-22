//! Fixture retrieval channel for internal testing.
//!
//! Provides a configurable channel for deterministic retrieval tests.
//! Fixture channels are **test-only** — production retrieval must reject
//! fixture evidence.
//!
//! References: `docs/design/v1.1-TASK-004-retrieval-artifact-channel-design.md`

#![allow(clippy::too_many_arguments)]

use crate::domain::config::RetrievalChannelConfig;
use crate::domain::retrieval::{
    RetrievalArtifactResult, RetrievalAttemptMode, RetrievalAttemptStatus, RetrievalAttemptTrace,
    RetrievalBatch, RetrievalBatchResult, RetrievalChannelCapabilities, RetrievalChannelId,
    RetrievalChannelReadinessReport, RetrievalChannelTier, RetrievalFailureCode, RetrievalJobId,
    RetrievalStatus,
};
use crate::ports::{BaseRetrievalChannel, RetrievalError};
use std::collections::HashMap;
use std::path::PathBuf;

/// Pre-programmed response for a single candidate in the fixture channel.
#[derive(Debug, Clone)]
pub enum FixtureResponse {
    /// Return a successful complete artifact result.
    Complete {
        local_path: PathBuf,
        source_path: PathBuf,
        sidecar_path: PathBuf,
        summary_path: PathBuf,
        report_path: PathBuf,
        visual_desc_path: PathBuf,
        checksum: String,
        content_type: String,
        file_size: u64,
    },
    /// Return a failure with the given parameters.
    Failure {
        failure_code: RetrievalFailureCode,
        reason: String,
        allows_fallback: bool,
    },
    /// Return a metadata-only result (rejected).
    MetadataOnly { reason: String },
    /// Return an access-restricted result.
    AccessRestricted { reason: String },
}

impl FixtureResponse {
    /// Standard success.
    pub fn success() -> Self {
        Self::Complete {
            local_path: PathBuf::from("/tmp/fixture/img.jpg"),
            source_path: PathBuf::from("/tmp/fixture/src.jpg"),
            sidecar_path: PathBuf::from("/tmp/fixture/sidecar.json"),
            summary_path: PathBuf::from("/tmp/fixture/summary.json"),
            report_path: PathBuf::from("/tmp/fixture/report.json"),
            visual_desc_path: PathBuf::from("/tmp/fixture/vd.json"),
            checksum: "fixture-sha256-abc123".into(),
            content_type: "image/jpeg".into(),
            file_size: 4096,
        }
    }

    /// Standard network failure.
    pub fn network_failure() -> Self {
        Self::Failure {
            failure_code: RetrievalFailureCode::RetrievalDirectFetchNetwork,
            reason: "simulated network error".into(),
            allows_fallback: true,
        }
    }

    /// Access-restricted failure.
    pub fn access_restricted() -> Self {
        Self::AccessRestricted {
            reason: "HTTP 403 — simulated access restriction".into(),
        }
    }

    /// Metadata-only response.
    pub fn metadata_only() -> Self {
        Self::MetadataOnly {
            reason: "remote-only metadata response".into(),
        }
    }
}

/// Readiness configuration for the fixture channel.
#[derive(Debug, Clone)]
pub enum FixtureReadiness {
    Ready,
    Disabled,
    MissingDependency,
    Misconfigured,
    PaidUnconfirmed,
}

/// A configurable retrieval channel for testing.
pub struct FixtureChannel {
    channel_id: RetrievalChannelId,
    tier: RetrievalChannelTier,
    name: String,
    responses: HashMap<String, FixtureResponse>,
    readiness_state: FixtureReadiness,
    fixture_only: bool,
}

impl FixtureChannel {
    /// Create a new fixture channel for the given tier.
    pub fn new(tier: RetrievalChannelTier) -> Self {
        let name = match tier {
            RetrievalChannelTier::NormalWebFetch => "Fixture Normal Web Fetch",
            RetrievalChannelTier::SelfHostedService => "Fixture Self-Hosted",
            RetrievalChannelTier::PaidOnlineService => "Fixture Paid Service",
        };
        let channel_id = RetrievalChannelId::new(format!("fixture-{}", tier));
        Self {
            channel_id,
            tier,
            name: name.into(),
            responses: HashMap::new(),
            readiness_state: FixtureReadiness::Ready,
            fixture_only: true,
        }
    }

    /// Set the readiness state.
    pub fn with_readiness(mut self, state: FixtureReadiness) -> Self {
        self.readiness_state = state;
        self
    }

    /// Register a response for a specific candidate ID.
    pub fn with_response(
        mut self,
        candidate_id: impl Into<String>,
        response: FixtureResponse,
    ) -> Self {
        self.responses.insert(candidate_id.into(), response);
        self
    }

    /// Bulk-load all given IDs to succeed.
    pub fn with_all_success(mut self, candidate_ids: &[&str]) -> Self {
        for id in candidate_ids {
            self.responses
                .insert(id.to_string(), FixtureResponse::success());
        }
        self
    }

    fn make_trace(
        job_id: &RetrievalJobId,
        query_plan_id: &str,
        candidate_id: &str,
        channel_id: &RetrievalChannelId,
        tier: RetrievalChannelTier,
        mode: RetrievalAttemptMode,
        status: RetrievalAttemptStatus,
        failure_code: Option<RetrievalFailureCode>,
    ) -> RetrievalAttemptTrace {
        RetrievalAttemptTrace {
            attempt_id: format!("attempt-{}-{}", job_id, "fixture"),
            retrieval_job_id: job_id.clone(),
            query_plan_id: query_plan_id.to_string(),
            candidate_id: candidate_id.to_string(),
            channel_id: channel_id.clone(),
            channel_tier: tier,
            attempt_mode: mode,
            started_at: String::new(),
            completed_at: None,
            target_url_redacted: None,
            source_page_url_redacted: None,
            final_url_redacted: None,
            http_status: None,
            bytes_received: None,
            status,
            failure_code,
            retryable: true,
            fallback_allowed: true,
            policy_reason: None,
            artifact_refs: vec![],
            redaction_applied: false,
        }
    }
}

impl BaseRetrievalChannel for FixtureChannel {
    fn channel_id(&self) -> RetrievalChannelId {
        self.channel_id.clone()
    }

    fn display_name(&self) -> &str {
        &self.name
    }

    fn tier(&self) -> RetrievalChannelTier {
        self.tier
    }

    fn capabilities(&self) -> RetrievalChannelCapabilities {
        RetrievalChannelCapabilities {
            fixture_only: self.fixture_only,
            ..Default::default()
        }
    }

    fn readiness(&self, _config: &RetrievalChannelConfig) -> RetrievalChannelReadinessReport {
        match self.readiness_state {
            FixtureReadiness::Ready => RetrievalChannelReadinessReport::ready(
                self.channel_id.clone(),
                self.name.clone(),
                self.tier,
            ),
            FixtureReadiness::Disabled => RetrievalChannelReadinessReport::disabled(
                self.channel_id.clone(),
                self.name.clone(),
                self.tier,
                RetrievalFailureCode::RetrievalChannelDisabled,
            ),
            FixtureReadiness::MissingDependency => RetrievalChannelReadinessReport::disabled(
                self.channel_id.clone(),
                self.name.clone(),
                self.tier,
                RetrievalFailureCode::RetrievalChannelDependencyMissing,
            ),
            FixtureReadiness::Misconfigured => RetrievalChannelReadinessReport::disabled(
                self.channel_id.clone(),
                self.name.clone(),
                self.tier,
                RetrievalFailureCode::RetrievalChannelMisconfigured,
            ),
            FixtureReadiness::PaidUnconfirmed => RetrievalChannelReadinessReport::paid_unconfirmed(
                self.channel_id.clone(),
                self.name.clone(),
            ),
        }
    }

    fn retrieve_batch(
        &self,
        batch: &RetrievalBatch,
    ) -> std::result::Result<RetrievalBatchResult, RetrievalError> {
        let mut results: Vec<RetrievalArtifactResult> = Vec::new();
        let mut all_traces: Vec<RetrievalAttemptTrace> = Vec::new();

        for job in &batch.jobs {
            let cid = &job.candidate_id;
            let response = self.responses.get(cid);

            match response {
                Some(FixtureResponse::Complete {
                    local_path,
                    source_path,
                    sidecar_path,
                    summary_path,
                    report_path,
                    visual_desc_path,
                    checksum,
                    content_type,
                    file_size,
                }) => {
                    let trace = Self::make_trace(
                        &job.retrieval_job_id,
                        &job.query_plan_id,
                        cid,
                        &self.channel_id,
                        self.tier,
                        RetrievalAttemptMode::DirectImageFetch,
                        RetrievalAttemptStatus::Succeeded,
                        None,
                    );
                    all_traces.push(trace.clone());

                    let result = RetrievalArtifactResult {
                        retrieval_job_id: job.retrieval_job_id.clone(),
                        retrieval_batch_id: batch.retrieval_batch_id.clone(),
                        query_plan_id: job.query_plan_id.clone(),
                        candidate_id: cid.clone(),
                        channel_id: self.channel_id.clone(),
                        channel_tier: self.tier,
                        attempt_mode: RetrievalAttemptMode::DirectImageFetch,
                        retrieval_status: RetrievalStatus::Complete,
                        local_artifact_path: Some(local_path.clone()),
                        source_artifact_path: Some(source_path.clone()),
                        source_sidecar_path: Some(sidecar_path.clone()),
                        content_summary_path: Some(summary_path.clone()),
                        task_report_path: Some(report_path.clone()),
                        visual_description_path: Some(visual_desc_path.clone()),
                        diagnostics_path: None,
                        checksum_sha256: Some(checksum.clone()),
                        content_type_reported: Some(content_type.clone()),
                        content_type_sniffed: Some(content_type.clone()),
                        content_type: Some(content_type.clone()),
                        file_extension: Some("jpg".into()),
                        file_size_bytes: Some(*file_size),
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
                        fetch_trace: vec![trace],
                        policy_decisions: vec![],
                        diagnostics: vec![],
                        failure_reason: None,
                        redaction_applied: false,
                    };
                    results.push(result);
                }

                Some(FixtureResponse::Failure {
                    failure_code,
                    reason,
                    allows_fallback,
                }) => {
                    let status = match failure_code {
                        RetrievalFailureCode::RetrievalAccessRestricted => {
                            RetrievalAttemptStatus::AccessRestricted
                        }
                        _ => RetrievalAttemptStatus::Failed,
                    };
                    let trace = Self::make_trace(
                        &job.retrieval_job_id,
                        &job.query_plan_id,
                        cid,
                        &self.channel_id,
                        self.tier,
                        RetrievalAttemptMode::DirectImageFetch,
                        status,
                        Some(failure_code.clone()),
                    );
                    all_traces.push(trace.clone());

                    let mut result = RetrievalArtifactResult::failed(
                        job,
                        &batch.retrieval_batch_id,
                        self.channel_id.clone(),
                        self.tier,
                        RetrievalAttemptMode::DirectImageFetch,
                        reason,
                        failure_code.clone(),
                        vec![trace],
                        vec![],
                    );
                    if !allows_fallback {
                        result.retrieval_status = RetrievalStatus::AccessRestricted;
                    }
                    results.push(result);
                }

                Some(FixtureResponse::MetadataOnly { reason }) => {
                    let trace = Self::make_trace(
                        &job.retrieval_job_id,
                        &job.query_plan_id,
                        cid,
                        &self.channel_id,
                        self.tier,
                        RetrievalAttemptMode::DirectImageFetch,
                        RetrievalAttemptStatus::MetadataOnlyRejected,
                        Some(RetrievalFailureCode::RetrievalMetadataOnly),
                    );
                    all_traces.push(trace.clone());

                    let result = RetrievalArtifactResult::failed(
                        job,
                        &batch.retrieval_batch_id,
                        self.channel_id.clone(),
                        self.tier,
                        RetrievalAttemptMode::DirectImageFetch,
                        reason,
                        RetrievalFailureCode::RetrievalMetadataOnly,
                        vec![trace],
                        vec![],
                    );
                    results.push(result);
                }

                Some(FixtureResponse::AccessRestricted { reason }) => {
                    let trace = Self::make_trace(
                        &job.retrieval_job_id,
                        &job.query_plan_id,
                        cid,
                        &self.channel_id,
                        self.tier,
                        RetrievalAttemptMode::DirectImageFetch,
                        RetrievalAttemptStatus::AccessRestricted,
                        Some(RetrievalFailureCode::RetrievalAccessRestricted),
                    );
                    all_traces.push(trace.clone());

                    let mut result = RetrievalArtifactResult::failed(
                        job,
                        &batch.retrieval_batch_id,
                        self.channel_id.clone(),
                        self.tier,
                        RetrievalAttemptMode::DirectImageFetch,
                        reason,
                        RetrievalFailureCode::RetrievalAccessRestricted,
                        vec![trace],
                        vec![],
                    );
                    result.retrieval_status = RetrievalStatus::AccessRestricted;
                    results.push(result);
                }

                None => {
                    let trace = Self::make_trace(
                        &job.retrieval_job_id,
                        &job.query_plan_id,
                        cid,
                        &self.channel_id,
                        self.tier,
                        RetrievalAttemptMode::DirectImageFetch,
                        RetrievalAttemptStatus::Failed,
                        Some(RetrievalFailureCode::RetrievalUnavailable),
                    );
                    all_traces.push(trace.clone());

                    let result = RetrievalArtifactResult::failed(
                        job,
                        &batch.retrieval_batch_id,
                        self.channel_id.clone(),
                        self.tier,
                        RetrievalAttemptMode::DirectImageFetch,
                        format!("no fixture response programmed for '{}'", cid),
                        RetrievalFailureCode::RetrievalUnavailable,
                        vec![trace],
                        vec![],
                    );
                    results.push(result);
                }
            }
        }

        Ok(RetrievalBatchResult::new(
            batch.retrieval_batch_id.clone(),
            batch.query_plan_id.clone(),
            batch.full_attempt_count,
            batch.retry_count,
            batch.target_size,
            vec![],
            results,
            all_traces,
            vec![],
            batch.shortage.clone(),
            vec![],
            vec![],
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::config::RetrievalChannelConfig;
    use crate::domain::retrieval::{
        RetrievalJob, RetrievalPolicyContext, RetrievalTarget, RetrievalTargetType,
    };

    fn make_job(id: &str, cid: &str, qp_id: &str, url: &str) -> RetrievalJob {
        RetrievalJob {
            retrieval_job_id: RetrievalJobId::new(id),
            query_plan_id: qp_id.into(),
            candidate_id: cid.into(),
            full_attempt_count: 1,
            retry_count: 0,
            retrieval_priority: 5,
            target: RetrievalTarget {
                target_type: RetrievalTargetType::Image,
                primary_image_url: url.into(),
                alternate_source_page_url: None,
                thumbnail_url: None,
                expected_mime_type: None,
                license_hint: None,
                provider_id: "p1".into(),
                candidate_provenance_refs: vec![],
            },
            candidate_quality_decision_ref: "qd-1".into(),
            requested_outputs: vec![],
            policy_context: RetrievalPolicyContext::default(),
        }
    }

    fn make_batch(jobs: Vec<RetrievalJob>) -> RetrievalBatch {
        RetrievalBatch::new("b-1", "qp-1", 1, 0, jobs.len() as u32, jobs, None)
    }

    #[test]
    fn fixture_channel_returns_complete_result() {
        let channel = FixtureChannel::new(RetrievalChannelTier::NormalWebFetch)
            .with_response("c1", FixtureResponse::success());

        let batch = make_batch(vec![make_job(
            "ret-1",
            "c1",
            "qp-1",
            "https://example.com/1.jpg",
        )]);
        let result = channel.retrieve_batch(&batch).expect("batch should work");

        assert_eq!(result.results.len(), 1);
        assert!(result.results[0].is_complete());
        assert!(result.results[0].has_all_required_paths());
        assert_eq!(
            result.results[0].checksum_sha256,
            Some("fixture-sha256-abc123".into())
        );
    }

    #[test]
    fn fixture_channel_returns_failure() {
        let channel = FixtureChannel::new(RetrievalChannelTier::NormalWebFetch)
            .with_response("c2", FixtureResponse::network_failure());

        let batch = make_batch(vec![make_job(
            "ret-2",
            "c2",
            "qp-1",
            "https://example.com/2.jpg",
        )]);
        let result = channel.retrieve_batch(&batch).expect("batch should work");

        assert_eq!(result.results.len(), 1);
        assert!(!result.results[0].is_complete());
    }

    #[test]
    fn fixture_channel_access_restricted() {
        let channel = FixtureChannel::new(RetrievalChannelTier::NormalWebFetch)
            .with_response("c3", FixtureResponse::access_restricted());

        let batch = make_batch(vec![make_job(
            "ret-3",
            "c3",
            "qp-1",
            "https://restricted.example.com/3.jpg",
        )]);
        let result = channel.retrieve_batch(&batch).expect("batch should work");

        assert_eq!(
            result.results[0].retrieval_status,
            RetrievalStatus::AccessRestricted
        );
    }

    #[test]
    fn fixture_channel_metadata_only_rejected() {
        let channel = FixtureChannel::new(RetrievalChannelTier::NormalWebFetch)
            .with_response("c4", FixtureResponse::metadata_only());

        let batch = make_batch(vec![make_job(
            "ret-4",
            "c4",
            "qp-1",
            "https://example.com/4.jpg",
        )]);
        let result = channel.retrieve_batch(&batch).expect("batch should work");

        assert!(result.results[0].is_metadata_only_result());
    }

    #[test]
    fn fixture_channel_readiness() {
        let channel = FixtureChannel::new(RetrievalChannelTier::NormalWebFetch);
        let config = RetrievalChannelConfig {
            channel_id: "test".into(),
            channel_kind: crate::domain::config::RetrievalChannelKind::Fixture,
            tier: RetrievalChannelTier::NormalWebFetch,
            enabled: true,
            endpoint: None,
            credential_env: None,
            max_batch_size: None,
        };
        let report = channel.readiness(&config);
        assert!(report.available);
        assert!(channel.capabilities().fixture_only);
    }
}
