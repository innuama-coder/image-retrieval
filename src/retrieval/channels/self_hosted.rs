//! Self-hosted retrieval service channel boundary.
//!
//! This channel represents an open-source / self-hosted retrieval service.
//! It is disabled by default and requires configuration to be ready.
//!
//! The adapter contract requires that responses be converted into local
//! artifact files. Remote-only or metadata-only responses are rejected.
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

/// The self-hosted retrieval service channel — tier 2.
///
/// Disabled until a service endpoint and optional credential are configured.
pub struct SelfHostedChannel {
    channel_id: RetrievalChannelId,
    enabled: bool,
    endpoint: Option<String>,
    credential_env: Option<String>,
}

impl SelfHostedChannel {
    /// Create a new self-hosted channel.
    pub fn new() -> Self {
        Self {
            channel_id: RetrievalChannelId::new("self-hosted-default"),
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

impl Default for SelfHostedChannel {
    fn default() -> Self {
        Self::new()
    }
}

impl BaseRetrievalChannel for SelfHostedChannel {
    fn channel_id(&self) -> RetrievalChannelId {
        self.channel_id.clone()
    }

    fn display_name(&self) -> &str {
        "Self-Hosted Retrieval Service"
    }

    fn tier(&self) -> RetrievalChannelTier {
        RetrievalChannelTier::SelfHostedService
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
        if !self.enabled || !config.enabled {
            return RetrievalChannelReadinessReport::disabled(
                self.channel_id.clone(),
                self.display_name(),
                self.tier(),
                RetrievalFailureCode::RetrievalChannelDisabled,
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
                    detail: "no service endpoint configured".into(),
                },
                policy_status: RetrievalPolicyStatus::Allowed,
                failure_code: Some(RetrievalFailureCode::RetrievalChannelMisconfigured),
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
                policy_status: RetrievalPolicyStatus::Allowed,
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
                detail: "self-hosted retrieval service adapter is not implemented".into(),
            },
            policy_status: RetrievalPolicyStatus::Allowed,
            failure_code: Some(RetrievalFailureCode::RetrievalChannelMisconfigured),
            checked_at: String::new(),
            evidence: vec![],
            redaction_applied: false,
        }
    }

    fn retrieve_batch(
        &self,
        batch: &RetrievalBatch,
    ) -> std::result::Result<RetrievalBatchResult, RetrievalError> {
        // Self-hosted channel: not yet connected to a real service.
        // When configured, this would:
        // 1. Send retrieval requests to the configured endpoint
        // 2. Download returned image bytes
        // 3. Write local artifacts
        // 4. Validate sidecar/summary/report if provided by service

        let results: Vec<RetrievalArtifactResult> = batch
            .jobs
            .iter()
            .map(|job| {
                let trace = RetrievalAttemptTrace {
                    attempt_id: format!("attempt-{}-self-hosted", job.retrieval_job_id),
                    retrieval_job_id: job.retrieval_job_id.clone(),
                    query_plan_id: job.query_plan_id.clone(),
                    candidate_id: job.candidate_id.clone(),
                    channel_id: self.channel_id.clone(),
                    channel_tier: RetrievalChannelTier::SelfHostedService,
                    attempt_mode: RetrievalAttemptMode::SelfHostedService,
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
                    policy_reason: Some(
                        "Self-hosted service not yet configured for retrieval".into(),
                    ),
                    artifact_refs: vec![],
                    redaction_applied: false,
                };

                RetrievalArtifactResult::failed(
                    job,
                    &batch.retrieval_batch_id,
                    self.channel_id.clone(),
                    RetrievalChannelTier::SelfHostedService,
                    RetrievalAttemptMode::SelfHostedService,
                    "self-hosted service not configured for retrieval execution",
                    RetrievalFailureCode::RetrievalChannelDisabled,
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

    #[test]
    fn self_hosted_channel_default_disabled() {
        let channel = SelfHostedChannel::new();
        assert_eq!(channel.tier(), RetrievalChannelTier::SelfHostedService);
        assert!(!channel.enabled);
    }

    #[test]
    fn self_hosted_channel_readiness_disabled() {
        let channel = SelfHostedChannel::new();
        let config = RetrievalChannelConfig {
            channel_id: "sh-1".into(),
            channel_kind: crate::domain::config::RetrievalChannelKind::SelfHostedService,
            tier: RetrievalChannelTier::SelfHostedService,
            enabled: false,
            endpoint: None,
            credential_env: None,
            max_batch_size: None,
        };
        let report = channel.readiness(&config);
        assert!(!report.available);
        assert_eq!(
            report.failure_code,
            Some(RetrievalFailureCode::RetrievalChannelDisabled)
        );
    }

    #[test]
    fn self_hosted_channel_readiness_missing_endpoint() {
        let channel = SelfHostedChannel::new().with_enabled(true);
        let config = RetrievalChannelConfig {
            channel_id: "sh-1".into(),
            channel_kind: crate::domain::config::RetrievalChannelKind::SelfHostedService,
            tier: RetrievalChannelTier::SelfHostedService,
            enabled: true,
            endpoint: None,
            credential_env: None,
            max_batch_size: None,
        };
        let report = channel.readiness(&config);
        assert!(!report.available);
    }

    #[test]
    fn self_hosted_channel_with_endpoint_is_not_ready_without_adapter() {
        let channel = SelfHostedChannel::from_config(&RetrievalChannelConfig {
            channel_id: "sh-1".into(),
            channel_kind: crate::domain::config::RetrievalChannelKind::SelfHostedService,
            tier: RetrievalChannelTier::SelfHostedService,
            enabled: true,
            endpoint: Some("https://self-hosted.example.test".into()),
            credential_env: None,
            max_batch_size: None,
        });
        let config = RetrievalChannelConfig {
            channel_id: "sh-1".into(),
            channel_kind: crate::domain::config::RetrievalChannelKind::SelfHostedService,
            tier: RetrievalChannelTier::SelfHostedService,
            enabled: true,
            endpoint: Some("https://self-hosted.example.test".into()),
            credential_env: None,
            max_batch_size: None,
        };

        let report = channel.readiness(&config);

        assert!(!report.available);
        assert_eq!(
            report.failure_code,
            Some(RetrievalFailureCode::RetrievalChannelMisconfigured)
        );
    }
}
