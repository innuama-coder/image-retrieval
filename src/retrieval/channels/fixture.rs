//! Fixture retrieval channel for internal testing.
//!
//! Provides a configurable channel that can be programmed to return
//! specific results, simulate failures, and test fallback behaviour
//! without touching real networks.
//!
//! # Non-production
//!
//! This channel is for internal verification only. It must never be used
//! as production delivery evidence (per constitution).
//!
//! References: PRD §fixture/mock 只能用于内部验证

use crate::domain::retrieval::{
    FallbackEligibilityFact, RetrievalBatch, RetrievalChannelReadiness, RetrievalChannelTier,
    RetrievalFailure, RetrievalFailureCategory, RetrievalResult, RetrievalSuccess,
};
use crate::error::{Error, Result};
use crate::ports::BaseRetrievalChannel;
use std::collections::HashMap;

/// Pre-programmed response for a single candidate in the fixture channel.
#[derive(Debug, Clone)]
pub enum FixtureResponse {
    /// Return a successful retrieval with the given parameters.
    Success {
        local_path: String,
        content_type: Option<String>,
        file_size_bytes: u64,
    },
    /// Return a failure with the given parameters.
    Failure {
        failure_category: RetrievalFailureCategory,
        reason: String,
        allows_fallback: bool,
    },
}

impl FixtureResponse {
    /// Create a standard success response.
    pub fn success() -> Self {
        Self::Success {
            local_path: "/tmp/fixture-test.jpg".into(),
            content_type: Some("image/jpeg".into()),
            file_size_bytes: 4096,
        }
    }

    /// Create a standard network failure response.
    pub fn network_failure() -> Self {
        Self::Failure {
            failure_category: RetrievalFailureCategory::Network,
            reason: "simulated network error".into(),
            allows_fallback: true,
        }
    }

    /// Create an access-restricted failure response.
    pub fn access_restricted() -> Self {
        Self::Failure {
            failure_category: RetrievalFailureCategory::AccessRestricted,
            reason: "HTTP 403 — simulated access restriction".into(),
            allows_fallback: false,
        }
    }

    /// Create a channel-disabled failure response.
    pub fn channel_disabled() -> Self {
        Self::Failure {
            failure_category: RetrievalFailureCategory::ChannelDisabled,
            reason: "channel disabled by configuration".into(),
            allows_fallback: true,
        }
    }
}

/// A configurable retrieval channel for testing.
///
/// Each candidate ID can be pre-programmed with a specific response.
/// Candidates not in the fixture map fail with `Other`.
pub struct FixtureChannel {
    /// The tier this fixture pretends to be.
    tier: RetrievalChannelTier,

    /// Human-readable display name.
    name: String,

    /// Pre-programmed responses keyed by candidate ID.
    responses: HashMap<String, FixtureResponse>,

    /// Readiness state to report.
    readiness_state: FixtureReadiness,

    /// Fallback fact override (if set, used instead of auto-generated).
    fallback_fact_override: Option<FallbackEligibilityFact>,
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

impl FixtureReadiness {
    #[allow(dead_code)]
    fn to_domain(&self) -> RetrievalChannelReadiness {
        match self {
            Self::Ready => RetrievalChannelReadiness::Ready,
            Self::Disabled => RetrievalChannelReadiness::Disabled,
            Self::MissingDependency => RetrievalChannelReadiness::MissingDependency,
            Self::Misconfigured => RetrievalChannelReadiness::Misconfigured,
            Self::PaidUnconfirmed => RetrievalChannelReadiness::PaidUnconfirmed,
        }
    }
}

impl FixtureChannel {
    /// Create a new fixture channel for the given tier.
    pub fn new(tier: RetrievalChannelTier) -> Self {
        let name = match tier {
            RetrievalChannelTier::WebFetch => "Fixture Web Fetch",
            RetrievalChannelTier::SelfHosted => "Fixture Self-Hosted",
            RetrievalChannelTier::Paid => "Fixture Paid Service",
        };
        Self {
            tier,
            name: name.into(),
            responses: HashMap::new(),
            readiness_state: FixtureReadiness::Ready,
            fallback_fact_override: None,
        }
    }

    /// Set the readiness state for this fixture.
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

    /// Set a custom display name.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Override the fallback fact.
    pub fn with_fallback_fact(mut self, fact: FallbackEligibilityFact) -> Self {
        self.fallback_fact_override = Some(fact);
        self
    }

    /// Bulk-load responses: all given IDs succeed, others fail.
    pub fn with_all_success(mut self, candidate_ids: &[&str]) -> Self {
        for id in candidate_ids {
            self.responses
                .insert(id.to_string(), FixtureResponse::success());
        }
        self
    }
}

impl BaseRetrievalChannel for FixtureChannel {
    fn tier(&self) -> RetrievalChannelTier {
        self.tier
    }

    fn display_name(&self) -> &str {
        &self.name
    }

    fn readiness(&self) -> Result<()> {
        match self.readiness_state {
            FixtureReadiness::Ready => Ok(()),
            FixtureReadiness::Disabled => Err(Error::retrieval_failure(
                None::<&str>,
                self.tier.to_string(),
                "fixture channel is disabled",
            )),
            FixtureReadiness::MissingDependency => Err(Error::retrieval_failure(
                None::<&str>,
                self.tier.to_string(),
                "fixture channel missing dependency",
            )),
            FixtureReadiness::Misconfigured => Err(Error::retrieval_failure(
                None::<&str>,
                self.tier.to_string(),
                "fixture channel misconfigured",
            )),
            FixtureReadiness::PaidUnconfirmed => Err(Error::retrieval_failure(
                None::<&str>,
                self.tier.to_string(),
                "paid channel requires explicit user confirmation",
            )),
        }
    }

    fn retrieve_batch(&self, batch: &RetrievalBatch) -> Result<Vec<RetrievalResult>> {
        let results: Vec<RetrievalResult> = batch
            .candidate_ids
            .iter()
            .map(|cid| match self.responses.get(cid.as_str()) {
                Some(FixtureResponse::Success {
                    local_path,
                    content_type,
                    file_size_bytes,
                }) => RetrievalResult::Success(RetrievalSuccess::new(
                    cid,
                    local_path.clone(),
                    self.tier,
                    content_type.clone(),
                    *file_size_bytes,
                )),
                Some(FixtureResponse::Failure {
                    failure_category,
                    reason,
                    allows_fallback,
                }) => RetrievalResult::Failure(RetrievalFailure::new(
                    cid,
                    self.tier,
                    failure_category.clone(),
                    reason.clone(),
                    *allows_fallback,
                )),
                None => {
                    // Unprogrammed candidate → generic failure
                    RetrievalResult::Failure(RetrievalFailure::new(
                        cid,
                        self.tier,
                        RetrievalFailureCategory::Other,
                        format!(
                            "no fixture response programmed for '{}' on {} channel",
                            cid, self.tier
                        ),
                        true,
                    ))
                }
            })
            .collect();

        Ok(results)
    }

    fn fallback_fact(&self, reason: &str) -> FallbackEligibilityFact {
        if let Some(ref fact) = self.fallback_fact_override {
            return fact.clone();
        }
        FallbackEligibilityFact::new(self.tier, reason, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_channel_returns_programmed_success() {
        let channel = FixtureChannel::new(RetrievalChannelTier::WebFetch)
            .with_response("c1", FixtureResponse::success());

        let batch = RetrievalBatch::new(vec!["c1".into()], 2);
        let results = channel.retrieve_batch(&batch).expect("batch should work");
        assert_eq!(results.len(), 1);
        assert!(results[0].is_success());
        if let RetrievalResult::Success(s) = &results[0] {
            assert_eq!(s.candidate_id, "c1");
            assert_eq!(s.channel_tier, RetrievalChannelTier::WebFetch);
        }
    }

    #[test]
    fn fixture_channel_returns_programmed_failure() {
        let channel = FixtureChannel::new(RetrievalChannelTier::WebFetch)
            .with_response("c2", FixtureResponse::access_restricted());

        let batch = RetrievalBatch::new(vec!["c2".into()], 2);
        let results = channel.retrieve_batch(&batch).expect("batch should work");
        assert!(results[0].is_failure());
        if let RetrievalResult::Failure(f) = &results[0] {
            assert_eq!(
                f.failure_category,
                RetrievalFailureCategory::AccessRestricted
            );
            assert!(!f.allows_fallback);
        }
    }

    #[test]
    fn fixture_channel_unprogrammed_candidate_fails() {
        let channel = FixtureChannel::new(RetrievalChannelTier::WebFetch);

        let batch = RetrievalBatch::new(vec!["unknown".into()], 2);
        let results = channel.retrieve_batch(&batch).expect("batch should work");
        assert!(results[0].is_failure());
    }

    #[test]
    fn fixture_channel_mixed_results() {
        let channel = FixtureChannel::new(RetrievalChannelTier::SelfHosted)
            .with_response("good", FixtureResponse::success())
            .with_response("bad", FixtureResponse::network_failure());

        let batch = RetrievalBatch::new(vec!["good".into(), "bad".into()], 4);
        let results = channel.retrieve_batch(&batch).expect("batch should work");
        assert_eq!(results.len(), 2);
        assert!(results[0].is_success());
        assert!(results[1].is_failure());
    }

    #[test]
    fn fixture_channel_readiness_ready() {
        let channel = FixtureChannel::new(RetrievalChannelTier::WebFetch);
        assert!(channel.readiness().is_ok());
    }

    #[test]
    fn fixture_channel_readiness_disabled() {
        let channel = FixtureChannel::new(RetrievalChannelTier::WebFetch)
            .with_readiness(FixtureReadiness::Disabled);
        assert!(channel.readiness().is_err());
    }

    #[test]
    fn fixture_channel_readiness_paid_unconfirmed() {
        let channel = FixtureChannel::new(RetrievalChannelTier::Paid)
            .with_readiness(FixtureReadiness::PaidUnconfirmed);
        let result = channel.readiness();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("paid"));
    }

    #[test]
    fn fixture_channel_tier_reported() {
        let wf = FixtureChannel::new(RetrievalChannelTier::WebFetch);
        assert_eq!(wf.tier(), RetrievalChannelTier::WebFetch);

        let sh = FixtureChannel::new(RetrievalChannelTier::SelfHosted);
        assert_eq!(sh.tier(), RetrievalChannelTier::SelfHosted);

        let pd = FixtureChannel::new(RetrievalChannelTier::Paid);
        assert_eq!(pd.tier(), RetrievalChannelTier::Paid);
    }

    #[test]
    fn fixture_channel_fallback_fact() {
        let channel = FixtureChannel::new(RetrievalChannelTier::WebFetch);
        let fact = channel.fallback_fact("test");
        assert_eq!(fact.failed_tier, RetrievalChannelTier::WebFetch);
        assert_eq!(fact.next_tier, Some(RetrievalChannelTier::SelfHosted));
    }

    #[test]
    fn fixture_channel_fallback_fact_override() {
        let custom = FallbackEligibilityFact {
            failed_tier: RetrievalChannelTier::SelfHosted,
            next_tier: None,
            reason: "custom override".into(),
            is_access_restricted: true,
            requires_paid_confirmation: false,
        };
        let channel =
            FixtureChannel::new(RetrievalChannelTier::WebFetch).with_fallback_fact(custom);

        let fact = channel.fallback_fact("ignored");
        assert!(fact.is_access_restricted);
        assert_eq!(fact.reason, "custom override");
    }

    #[test]
    fn fixture_channel_with_all_success() {
        let channel =
            FixtureChannel::new(RetrievalChannelTier::WebFetch).with_all_success(&["a", "b", "c"]);

        let batch = RetrievalBatch::new(vec!["a".into(), "b".into(), "c".into()], 6);
        let results = channel.retrieve_batch(&batch).expect("batch should work");
        assert_eq!(results.len(), 3);
        assert!(results.iter().all(|r| r.is_success()));
    }

    #[test]
    fn fixture_response_constructors() {
        let success = FixtureResponse::success();
        match success {
            FixtureResponse::Success { content_type, .. } => {
                assert_eq!(content_type, Some("image/jpeg".into()));
            }
            _ => panic!("expected success"),
        }

        let net_fail = FixtureResponse::network_failure();
        match net_fail {
            FixtureResponse::Failure {
                failure_category,
                allows_fallback,
                ..
            } => {
                assert_eq!(failure_category, RetrievalFailureCategory::Network);
                assert!(allows_fallback);
            }
            _ => panic!("expected failure"),
        }

        let access = FixtureResponse::access_restricted();
        match access {
            FixtureResponse::Failure {
                failure_category,
                allows_fallback,
                ..
            } => {
                assert_eq!(failure_category, RetrievalFailureCategory::AccessRestricted);
                assert!(!allows_fallback);
            }
            _ => panic!("expected failure"),
        }
    }
}
