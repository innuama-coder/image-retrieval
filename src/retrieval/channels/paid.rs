//! Paid online retrieval service channel boundary.
//!
//! This channel represents a paid online retrieval service. It is **disabled
//! by default** and requires all of:
//!
//! - `RuntimeConfig.policy.allow_paid_channels = true`
//! - QueryPlan `retrieval_policy.allow_paid = true`
//! - Paid channel config `enabled = true`
//! - Budget or usage boundary configured
//! - Credential env var present when required
//!
//! If any condition is absent, readiness returns
//! `RETRIEVAL_PAID_UNCONFIRMED` and no retrieval is performed.
//!
//! References: `docs/design/v1.1-TASK-004-retrieval-artifact-channel-design.md`

use crate::domain::config::RetrievalChannelConfig;
use crate::domain::retrieval::{
    CredentialStatus, DependencyStatus, RetrievalArtifactResult, RetrievalAttemptMode,
    RetrievalAttemptStatus, RetrievalAttemptTrace, RetrievalBatch, RetrievalBatchResult,
    RetrievalChannelCapabilities, RetrievalChannelId, RetrievalChannelReadinessReport,
    RetrievalChannelTier, RetrievalFailureCode, RetrievalPolicyStatus,
};
use crate::ports::{BaseRetrievalChannel, RetrievalError};

/// The paid online retrieval service channel — tier 3.
///
/// Disabled by default. Requires explicit runtime config enablement,
/// QueryPlan `allow_paid`, and budget boundary before use.
pub struct PaidChannel {
    channel_id: RetrievalChannelId,
    enabled: bool,
    endpoint: Option<String>,
    credential_env: Option<String>,
}

impl PaidChannel {
    /// Create a new paid channel (disabled by default).
    pub fn new() -> Self {
        Self {
            channel_id: RetrievalChannelId::new("paid-default"),
            enabled: false,
            endpoint: None,
            credential_env: None,
        }
    }

    /// Configure from a [`RetrievalChannelConfig`].
    pub fn from_config(config: &RetrievalChannelConfig) -> Self {
        Self {
            channel_id: RetrievalChannelId::new(&config.channel_id),
            enabled: config.enabled,
            endpoint: config.endpoint.clone(),
            credential_env: config.credential_env.clone(),
        }
    }

    /// Set the channel id.
    pub fn with_channel_id(mut self, id: impl Into<String>) -> Self {
        self.channel_id = RetrievalChannelId::new(id);
        self
    }

    /// Enable the channel.
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

impl Default for PaidChannel {
    fn default() -> Self {
        Self::new()
    }
}

impl BaseRetrievalChannel for PaidChannel {
    fn channel_id(&self) -> RetrievalChannelId {
        self.channel_id.clone()
    }

    fn display_name(&self) -> &str {
        "Paid Online Retrieval Service"
    }

    fn tier(&self) -> RetrievalChannelTier {
        RetrievalChannelTier::PaidOnlineService
    }

    fn capabilities(&self) -> RetrievalChannelCapabilities {
        RetrievalChannelCapabilities {
            supports_direct_image_fetch: false,
            supports_source_page_resolve: false,
            fixture_only: false,
            ..Default::default()
        }
    }

    fn readiness(&self, config: &RetrievalChannelConfig) -> RetrievalChannelReadinessReport {
        // Paid channel always requires explicit config + QueryPlan allowance.
        // Without runtime config allow_paid_channels, it's paid_unconfirmed.
        if !config.enabled || !self.enabled {
            return RetrievalChannelReadinessReport::paid_unconfirmed(
                self.channel_id.clone(),
                self.display_name(),
            );
        }

        if self.endpoint.is_none() && config.endpoint.is_none() {
            return RetrievalChannelReadinessReport {
                channel_id: self.channel_id.clone(),
                display_name: self.display_name().into(),
                tier: self.tier(),
                enabled: true,
                available: false,
                included_in_fallback_order: false,
                credential_status: CredentialStatus::NotRequired,
                dependency_status: DependencyStatus::Missing {
                    detail: "no paid service endpoint configured".into(),
                },
                policy_status: RetrievalPolicyStatus::Blocked {
                    reason: "Paid channel requires endpoint and budget configuration.".into(),
                },
                failure_code: Some(RetrievalFailureCode::RetrievalPaidUnconfirmed),
                checked_at: String::new(),
                evidence: vec![],
                redaction_applied: false,
            };
        }

        // Check credential if required
        let cred_env = self
            .credential_env
            .as_deref()
            .or(config.credential_env.as_deref());
        let credential_status = match cred_env {
            Some(env) => match std::env::var(env) {
                Ok(_) => CredentialStatus::Present,
                Err(_) => CredentialStatus::Missing {
                    env_var: env.into(),
                },
            },
            None => CredentialStatus::NotRequired,
        };

        if matches!(&credential_status, CredentialStatus::Missing { .. }) {
            return RetrievalChannelReadinessReport {
                channel_id: self.channel_id.clone(),
                display_name: self.display_name().into(),
                tier: self.tier(),
                enabled: true,
                available: false,
                included_in_fallback_order: false,
                credential_status,
                dependency_status: DependencyStatus::Available,
                policy_status: RetrievalPolicyStatus::Blocked {
                    reason: "Paid channel credential is missing.".into(),
                },
                failure_code: Some(RetrievalFailureCode::RetrievalChannelCredentialMissing),
                checked_at: String::new(),
                evidence: vec![],
                redaction_applied: false,
            };
        }

        RetrievalChannelReadinessReport {
            channel_id: self.channel_id.clone(),
            display_name: self.display_name().into(),
            tier: self.tier(),
            enabled: true,
            available: false,
            included_in_fallback_order: false,
            credential_status,
            dependency_status: DependencyStatus::Missing {
                detail: "paid retrieval service adapter is not implemented".into(),
            },
            policy_status: RetrievalPolicyStatus::Blocked {
                reason: "Paid channel cannot be confirmed until a real adapter is implemented."
                    .into(),
            },
            failure_code: Some(RetrievalFailureCode::RetrievalPaidUnconfirmed),
            checked_at: String::new(),
            evidence: vec![],
            redaction_applied: false,
        }
    }

    fn retrieve_batch(
        &self,
        batch: &RetrievalBatch,
    ) -> std::result::Result<RetrievalBatchResult, RetrievalError> {
        // Paid channel: not yet connected to a real paid service.
        // When configured and confirmed, this would:
        // 1. Verify budget/usage boundary
        // 2. Send retrieval requests to the paid service
        // 3. Download returned image bytes
        // 4. Write local artifacts
        // 5. Validate evidence against artifact contract

        // Check if ANY job in the batch has paid allowed
        let paid_allowed = batch.jobs.iter().any(|j| j.policy_context.allow_paid);
        if !paid_allowed {
            return Err(RetrievalError::PaidUnconfirmed);
        }

        let results: Vec<RetrievalArtifactResult> = batch
            .jobs
            .iter()
            .map(|job| {
                let trace = RetrievalAttemptTrace {
                    attempt_id: format!("attempt-{}-paid", job.retrieval_job_id),
                    retrieval_job_id: job.retrieval_job_id.clone(),
                    query_plan_id: job.query_plan_id.clone(),
                    candidate_id: job.candidate_id.clone(),
                    channel_id: self.channel_id.clone(),
                    channel_tier: RetrievalChannelTier::PaidOnlineService,
                    attempt_mode: RetrievalAttemptMode::PaidOnlineService,
                    started_at: String::new(),
                    completed_at: None,
                    target_url_redacted: None,
                    source_page_url_redacted: None,
                    final_url_redacted: None,
                    http_status: None,
                    bytes_received: None,
                    status: RetrievalAttemptStatus::Abandoned,
                    failure_code: Some(RetrievalFailureCode::RetrievalPaidUnconfirmed),
                    retryable: false,
                    fallback_allowed: false,
                    policy_reason: Some(
                        "Paid service not yet implemented; requires budget and service endpoint."
                            .into(),
                    ),
                    artifact_refs: vec![],
                    redaction_applied: false,
                };

                RetrievalArtifactResult::failed(
                    job,
                    &batch.retrieval_batch_id,
                    self.channel_id.clone(),
                    RetrievalChannelTier::PaidOnlineService,
                    RetrievalAttemptMode::PaidOnlineService,
                    "paid service not yet available for execution",
                    RetrievalFailureCode::RetrievalPaidUnconfirmed,
                    vec![trace],
                    vec![],
                )
            })
            .collect();

        Ok(RetrievalBatchResult::new(
            batch.retrieval_batch_id.clone(),
            batch.query_plan_id.clone(),
            batch.full_attempt_count,
            batch.retry_count,
            batch.target_size,
            vec![],
            results,
            vec![],
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
    use crate::domain::retrieval::RetrievalJobId;

    #[test]
    fn paid_channel_default_disabled() {
        let channel = PaidChannel::new();
        assert_eq!(channel.tier(), RetrievalChannelTier::PaidOnlineService);
        assert!(!channel.enabled);
    }

    #[test]
    fn paid_channel_readiness_unconfirmed() {
        let channel = PaidChannel::new();
        let config = RetrievalChannelConfig {
            channel_id: "paid-1".into(),
            channel_kind: crate::domain::config::RetrievalChannelKind::PaidOnlineService,
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

    #[test]
    fn paid_channel_with_endpoint_and_credential_is_not_ready_without_adapter() {
        let env_name = format!("IMAGE_RETRIEVAL_PAID_TEST_{}", std::process::id());
        std::env::set_var(&env_name, "dummy");
        let config = RetrievalChannelConfig {
            channel_id: "paid-1".into(),
            channel_kind: crate::domain::config::RetrievalChannelKind::PaidOnlineService,
            tier: RetrievalChannelTier::PaidOnlineService,
            enabled: true,
            endpoint: Some("https://paid.example.test".into()),
            credential_env: Some(env_name.clone()),
            max_batch_size: None,
        };
        let channel = PaidChannel::from_config(&config);

        let report = channel.readiness(&config);

        assert!(!report.available);
        assert_eq!(
            report.failure_code,
            Some(RetrievalFailureCode::RetrievalPaidUnconfirmed)
        );
        std::env::remove_var(env_name);
    }

    #[test]
    fn paid_channel_retrieve_batch_without_paid_allowed() {
        use crate::domain::retrieval::{
            RetrievalJob, RetrievalPolicyContext, RetrievalTarget, RetrievalTargetType,
        };

        let channel = PaidChannel::new().with_enabled(true);
        let job = RetrievalJob {
            retrieval_job_id: RetrievalJobId::new("ret-1"),
            query_plan_id: "qp-1".into(),
            candidate_id: "cand-1".into(),
            full_attempt_count: 1,
            retry_count: 0,
            retrieval_priority: 1,
            target: RetrievalTarget {
                target_type: RetrievalTargetType::Image,
                primary_image_url: "https://example.com/a.jpg".into(),
                alternate_source_page_url: None,
                thumbnail_url: None,
                expected_mime_type: None,
                license_hint: None,
                provider_id: "p1".into(),
                candidate_provenance_refs: vec![],
            },
            candidate_quality_decision_ref: "qd-1".into(),
            requested_outputs: vec![],
            policy_context: RetrievalPolicyContext {
                allow_paid: false, // not allowed
                ..Default::default()
            },
        };
        let batch = RetrievalBatch::new("b-1", "qp-1", 1, 0, 1, vec![job], None);
        let result = channel.retrieve_batch(&batch);
        assert!(result.is_err());
    }
}
