//! Retrieval domain types.
//!
//! Covers retrieval channel tiers, batch planning, retrieval results
//! (success / failure / fallback evidence), channel readiness, and
//! batch shortage diagnosis.
//!
//! References: PRD §抓取渠道产品要求, HLD §Retrieval Batch Planner

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Channel identity
// ---------------------------------------------------------------------------

/// Stable identifier for a retrieval channel.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RetrievalChannelId(pub String);

impl RetrievalChannelId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl std::fmt::Display for RetrievalChannelId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ---------------------------------------------------------------------------
// Channel tiers
// ---------------------------------------------------------------------------

/// The three confirmed retrieval channel tiers (HLD + PRD).
///
/// A possible fourth tier is an open product decision and must not be
/// added without user confirmation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RetrievalChannelTier {
    /// Normal web fetch — the default, lowest-cost channel.
    #[serde(rename = "web_fetch")]
    WebFetch = 1,

    /// Self-hosted open-source service.
    #[serde(rename = "self_hosted")]
    SelfHosted = 2,

    /// Paid online service. Must be explicitly enabled by the user; never
    /// used silently.
    #[serde(rename = "paid")]
    Paid = 3,
}

impl RetrievalChannelTier {
    /// Returns the next tier to try during fallback, or `None` if this is
    /// already the highest confirmed tier.
    pub fn next_fallback(&self) -> Option<Self> {
        match self {
            Self::WebFetch => Some(Self::SelfHosted),
            Self::SelfHosted => Some(Self::Paid),
            Self::Paid => None,
        }
    }
}

impl std::fmt::Display for RetrievalChannelTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Self::WebFetch => "web_fetch",
            Self::SelfHosted => "self_hosted",
            Self::Paid => "paid",
        };
        write!(f, "{}", label)
    }
}

// ---------------------------------------------------------------------------
// Channel readiness
// ---------------------------------------------------------------------------

/// Readiness status of a retrieval channel.
///
/// Per the LLD: ready, disabled, missing_dependency, misconfigured,
/// paid_unconfirmed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RetrievalChannelReadiness {
    /// Channel is ready to serve retrieval requests.
    Ready,

    /// Channel is explicitly disabled in configuration.
    Disabled,

    /// Channel requires a dependency (binary, library, network) that is
    /// not present.
    MissingDependency,

    /// Channel configuration is invalid (e.g. malformed endpoint).
    Misconfigured,

    /// Paid channel requires explicit user confirmation before use.
    PaidUnconfirmed,
}

impl RetrievalChannelReadiness {
    /// Returns `true` if the channel can be used for retrieval.
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready)
    }

    /// Returns `true` if the readiness status is terminal for this task
    /// (cannot become ready without configuration changes).
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Disabled | Self::MissingDependency | Self::Misconfigured
        )
    }
}

impl std::fmt::Display for RetrievalChannelReadiness {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Self::Ready => "ready",
            Self::Disabled => "disabled",
            Self::MissingDependency => "missing_dependency",
            Self::Misconfigured => "misconfigured",
            Self::PaidUnconfirmed => "paid_unconfirmed",
        };
        write!(f, "{}", label)
    }
}

// ---------------------------------------------------------------------------
// Retrieval batch
// ---------------------------------------------------------------------------

/// A planned batch of candidates to retrieve.
///
/// The target size is `required_count × 2`. When fewer retrievable candidates
/// are available this forms a *short batch*.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalBatch {
    /// IDs of the candidates in this batch, in priority order.
    pub candidate_ids: Vec<String>,

    /// Target batch size (required_count × 2).
    pub target_size: u32,

    /// Whether this is a short batch (fewer candidates than target).
    pub is_short_batch: bool,

    /// Map from candidate_id to source_url for retrieval.
    #[serde(default)]
    pub candidate_urls: HashMap<String, String>,
}

impl RetrievalBatch {
    pub fn new(candidate_ids: Vec<String>, target_size: u32) -> Self {
        let is_short_batch = (candidate_ids.len() as u32) < target_size;
        Self {
            candidate_ids,
            target_size,
            is_short_batch,
            candidate_urls: HashMap::new(),
        }
    }

    pub fn with_urls(mut self, urls: HashMap<String, String>) -> Self {
        self.candidate_urls = urls;
        self
    }

    pub fn actual_size(&self) -> usize {
        self.candidate_ids.len()
    }

    /// Look up the source URL for a candidate in this batch.
    pub fn url_for(&self, candidate_id: &str) -> Option<&str> {
        self.candidate_urls.get(candidate_id).map(|s| s.as_str())
    }
}

// ---------------------------------------------------------------------------
// Batch shortage evidence
// ---------------------------------------------------------------------------

/// Evidence produced when a retrieval batch is shorter than the target.
///
/// This is consumed by downstream tasks (TASK-006, TASK-007) to explain
/// why fewer candidates entered retrieval.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalBatchShortage {
    /// The target batch size that was requested.
    pub target_size: u32,

    /// The actual number of candidates that entered the batch.
    pub actual_size: u32,

    /// Human-readable reason for the shortage.
    pub reason: String,
}

// ---------------------------------------------------------------------------
// Retrieval results
// ---------------------------------------------------------------------------

/// Result of attempting to retrieve a single candidate image.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RetrievalResult {
    /// The image was successfully fetched.
    Success(RetrievalSuccess),

    /// The image could not be fetched; reason and channel information
    /// are recorded for fallback / audit.
    Failure(RetrievalFailure),
}

impl RetrievalResult {
    /// Returns `true` if this is a success.
    pub fn is_success(&self) -> bool {
        matches!(self, Self::Success(_))
    }

    /// Returns `true` if this is a failure.
    pub fn is_failure(&self) -> bool {
        matches!(self, Self::Failure(_))
    }

    /// Extract the candidate_id from either variant.
    pub fn candidate_id(&self) -> &str {
        match self {
            Self::Success(s) => &s.candidate_id,
            Self::Failure(f) => &f.candidate_id,
        }
    }
}

/// A successfully retrieved image.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalSuccess {
    pub candidate_id: String,
    /// Path to the downloaded image file (local temp or delivery staging).
    pub local_path: String,
    /// The channel tier that produced this image.
    pub channel_tier: RetrievalChannelTier,
    /// Content type as reported by the server (e.g. "image/jpeg").
    pub content_type: Option<String>,
    /// File size in bytes.
    pub file_size_bytes: u64,
}

/// A failed retrieval attempt with diagnostic information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalFailure {
    pub candidate_id: String,
    /// The channel tier that was attempted.
    pub channel_tier: RetrievalChannelTier,
    /// Machine-readable failure category.
    pub failure_category: RetrievalFailureCategory,
    /// Human-readable reason.
    pub reason: String,
    /// Whether this failure allows fallback to a higher tier.
    pub allows_fallback: bool,
}

/// Categories of retrieval failures.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RetrievalFailureCategory {
    /// Network error, DNS failure, timeout.
    Network,
    /// HTTP 4xx / 5xx from the remote server.
    HttpStatus,
    /// The response body was not a valid image.
    InvalidContent,
    /// Access control or authorization restriction blocked the fetch.
    AccessRestricted,
    /// The channel is disabled by user configuration or policy.
    ChannelDisabled,
    /// Paid channel attempted without explicit user confirmation.
    PaidNotConfirmed,
    /// The candidate's source URL is not supported by this channel
    /// (e.g. requires JavaScript execution).
    UnsupportedUrl,
    /// Unknown / uncategorized failure.
    Other,
}

// ---------------------------------------------------------------------------
// Fallback eligibility fact
// ---------------------------------------------------------------------------

/// A fact produced by the retrieval layer for the orchestrator / policy
/// layer to decide whether fallback should proceed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FallbackEligibilityFact {
    /// The tier that just failed.
    pub failed_tier: RetrievalChannelTier,

    /// The tier that would be tried next.
    pub next_tier: Option<RetrievalChannelTier>,

    /// Why the current tier failed.
    pub reason: String,

    /// Whether the failure is due to access-control / authorization limits.
    /// When `true`, fallback must NOT be used to bypass the restriction.
    pub is_access_restricted: bool,

    /// Whether the next tier is a paid tier that requires user confirmation.
    pub requires_paid_confirmation: bool,
}

// ---------------------------------------------------------------------------
// Retrieval outcome — aggregated result of one batch attempt
// ---------------------------------------------------------------------------

/// The aggregated result of a single retrieval batch attempt through one
/// or more channels.
///
/// Produced by the retrieval executor and consumed by the orchestrator
/// (TASK-006) and delivery (TASK-007).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalOutcome {
    /// The batch that was planned for this attempt.
    pub batch: RetrievalBatch,

    /// All per-candidate retrieval results from the successful channel
    /// (or the last attempted channel).
    pub results: Vec<RetrievalResult>,

    /// The channel tier that produced the final results.
    pub channel_tier: RetrievalChannelTier,

    /// Shortage evidence if the batch was short.
    pub shortage: Option<RetrievalBatchShortage>,

    /// How many channels were attempted (including fallback).
    pub channels_attempted: u32,

    /// Ordered list of channel attempts (first to last).
    pub channel_attempts: Vec<ChannelAttemptResult>,

    /// All fallback eligibility facts produced during the attempt.
    pub fallback_facts: Vec<FallbackEligibilityFact>,

    /// Execution-blocking fact if the attempt could not proceed due to
    /// policy or dependency unavailability.
    pub execution_blocked: Option<ExecutionBlockingFact>,
}

/// Record of one channel's attempt within a retrieval batch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelAttemptResult {
    /// The channel tier that was attempted.
    pub channel_tier: RetrievalChannelTier,

    /// Per-candidate results from this channel.
    pub results: Vec<RetrievalResult>,

    /// Number of successes from this channel.
    pub success_count: u32,

    /// Number of failures from this channel.
    pub failure_count: u32,

    /// Whether the attempt was abandoned (channel not ready, etc.).
    pub abandoned: bool,

    /// Reason if abandoned.
    pub abandon_reason: Option<String>,
}

/// Fact produced when a retrieval attempt cannot proceed at all.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionBlockingFact {
    /// Why retrieval is blocked.
    pub reason: String,

    /// The channel tier that was the source of the block (if applicable).
    pub source_tier: Option<RetrievalChannelTier>,

    /// Whether the block is due to access-control / authorization.
    pub is_access_restricted: bool,

    /// Whether the block is due to paid tier not being confirmed.
    pub is_paid_unconfirmed: bool,
}

// ---------------------------------------------------------------------------
// Convenience constructors
// ---------------------------------------------------------------------------

impl RetrievalSuccess {
    pub fn new(
        candidate_id: impl Into<String>,
        local_path: impl Into<String>,
        channel_tier: RetrievalChannelTier,
        content_type: Option<String>,
        file_size_bytes: u64,
    ) -> Self {
        Self {
            candidate_id: candidate_id.into(),
            local_path: local_path.into(),
            channel_tier,
            content_type,
            file_size_bytes,
        }
    }
}

impl RetrievalFailure {
    pub fn new(
        candidate_id: impl Into<String>,
        channel_tier: RetrievalChannelTier,
        failure_category: RetrievalFailureCategory,
        reason: impl Into<String>,
        allows_fallback: bool,
    ) -> Self {
        Self {
            candidate_id: candidate_id.into(),
            channel_tier,
            failure_category,
            reason: reason.into(),
            allows_fallback,
        }
    }
}

impl FallbackEligibilityFact {
    pub fn new(
        failed_tier: RetrievalChannelTier,
        reason: impl Into<String>,
        is_access_restricted: bool,
    ) -> Self {
        let next_tier = failed_tier.next_fallback();
        let requires_paid_confirmation = matches!(next_tier, Some(RetrievalChannelTier::Paid));
        Self {
            failed_tier,
            next_tier,
            reason: reason.into(),
            is_access_restricted,
            requires_paid_confirmation,
        }
    }
}

impl RetrievalBatchShortage {
    pub fn new(target_size: u32, actual_size: u32, reason: impl Into<String>) -> Self {
        Self {
            target_size,
            actual_size,
            reason: reason.into(),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Tier tests
    // -----------------------------------------------------------------------

    #[test]
    fn tier_ordering() {
        assert!(RetrievalChannelTier::WebFetch < RetrievalChannelTier::SelfHosted);
        assert!(RetrievalChannelTier::SelfHosted < RetrievalChannelTier::Paid);
    }

    #[test]
    fn fallback_chain() {
        assert_eq!(
            RetrievalChannelTier::WebFetch.next_fallback(),
            Some(RetrievalChannelTier::SelfHosted)
        );
        assert_eq!(
            RetrievalChannelTier::SelfHosted.next_fallback(),
            Some(RetrievalChannelTier::Paid)
        );
        assert_eq!(RetrievalChannelTier::Paid.next_fallback(), None);
    }

    #[test]
    fn tier_display() {
        assert_eq!(RetrievalChannelTier::WebFetch.to_string(), "web_fetch");
        assert_eq!(RetrievalChannelTier::SelfHosted.to_string(), "self_hosted");
        assert_eq!(RetrievalChannelTier::Paid.to_string(), "paid");
    }

    // -----------------------------------------------------------------------
    // Channel ID tests
    // -----------------------------------------------------------------------

    #[test]
    fn channel_id_construction() {
        let id = RetrievalChannelId::new("web-fetch-default");
        assert_eq!(id.to_string(), "web-fetch-default");
    }

    #[test]
    fn channel_id_equality() {
        let a = RetrievalChannelId::new("ch1");
        let b = RetrievalChannelId::new("ch1");
        let c = RetrievalChannelId::new("ch2");
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    // -----------------------------------------------------------------------
    // Readiness tests
    // -----------------------------------------------------------------------

    #[test]
    fn readiness_is_ready() {
        assert!(RetrievalChannelReadiness::Ready.is_ready());
        assert!(!RetrievalChannelReadiness::Disabled.is_ready());
        assert!(!RetrievalChannelReadiness::MissingDependency.is_ready());
        assert!(!RetrievalChannelReadiness::Misconfigured.is_ready());
        assert!(!RetrievalChannelReadiness::PaidUnconfirmed.is_ready());
    }

    #[test]
    fn readiness_terminal() {
        assert!(RetrievalChannelReadiness::Disabled.is_terminal());
        assert!(RetrievalChannelReadiness::MissingDependency.is_terminal());
        assert!(RetrievalChannelReadiness::Misconfigured.is_terminal());
        assert!(!RetrievalChannelReadiness::Ready.is_terminal());
        assert!(!RetrievalChannelReadiness::PaidUnconfirmed.is_terminal());
    }

    #[test]
    fn readiness_display() {
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

    // -----------------------------------------------------------------------
    // Batch tests
    // -----------------------------------------------------------------------

    #[test]
    fn retrieval_batch_normal() {
        let batch = RetrievalBatch::new(vec!["a".into(), "b".into(), "c".into(), "d".into()], 4);
        assert!(!batch.is_short_batch);
        assert_eq!(batch.actual_size(), 4);
    }

    #[test]
    fn retrieval_batch_short() {
        let batch = RetrievalBatch::new(vec!["a".into(), "b".into()], 8);
        assert!(batch.is_short_batch);
        assert_eq!(batch.actual_size(), 2);
    }

    #[test]
    fn retrieval_batch_with_urls() {
        let mut urls = HashMap::new();
        urls.insert("a".to_string(), "https://example.com/a.jpg".to_string());
        urls.insert("b".to_string(), "https://example.com/b.jpg".to_string());

        let batch = RetrievalBatch::new(vec!["a".into(), "b".into()], 4).with_urls(urls);

        assert_eq!(batch.url_for("a"), Some("https://example.com/a.jpg"));
        assert_eq!(batch.url_for("b"), Some("https://example.com/b.jpg"));
        assert_eq!(batch.url_for("c"), None);
    }

    #[test]
    fn retrieval_batch_url_for_missing() {
        let batch = RetrievalBatch::new(vec!["x".into()], 2);
        assert_eq!(batch.url_for("x"), None);
    }

    // -----------------------------------------------------------------------
    // Shortage tests
    // -----------------------------------------------------------------------

    #[test]
    fn batch_shortage_records_gap() {
        let shortage = RetrievalBatchShortage::new(8, 3, "only 3 retrievable candidates available");
        assert_eq!(shortage.target_size, 8);
        assert_eq!(shortage.actual_size, 3);
        assert!(shortage.reason.contains("retrievable"));
    }

    // -----------------------------------------------------------------------
    // RetrievalResult tests
    // -----------------------------------------------------------------------

    #[test]
    fn retrieval_result_is_success() {
        let s = RetrievalResult::Success(RetrievalSuccess::new(
            "c1",
            "/tmp/img.jpg",
            RetrievalChannelTier::WebFetch,
            Some("image/jpeg".into()),
            4096,
        ));
        assert!(s.is_success());
        assert!(!s.is_failure());
        assert_eq!(s.candidate_id(), "c1");
    }

    #[test]
    fn retrieval_result_is_failure() {
        let f = RetrievalResult::Failure(RetrievalFailure::new(
            "c2",
            RetrievalChannelTier::WebFetch,
            RetrievalFailureCategory::Network,
            "connection timeout",
            true,
        ));
        assert!(f.is_failure());
        assert!(!f.is_success());
        assert_eq!(f.candidate_id(), "c2");
    }

    // -----------------------------------------------------------------------
    // RetrievalSuccess tests
    // -----------------------------------------------------------------------

    #[test]
    fn retrieval_success_fields() {
        let s = RetrievalSuccess::new(
            "c1",
            "/tmp/test.png",
            RetrievalChannelTier::WebFetch,
            Some("image/png".into()),
            8192,
        );
        assert_eq!(s.candidate_id, "c1");
        assert_eq!(s.local_path, "/tmp/test.png");
        assert_eq!(s.channel_tier, RetrievalChannelTier::WebFetch);
        assert_eq!(s.content_type, Some("image/png".into()));
        assert_eq!(s.file_size_bytes, 8192);
    }

    // -----------------------------------------------------------------------
    // RetrievalFailure tests
    // -----------------------------------------------------------------------

    #[test]
    fn retrieval_failure_network() {
        let f = RetrievalFailure::new(
            "c3",
            RetrievalChannelTier::WebFetch,
            RetrievalFailureCategory::Network,
            "DNS resolution failed",
            true,
        );
        assert_eq!(f.failure_category, RetrievalFailureCategory::Network);
        assert!(f.allows_fallback);
    }

    #[test]
    fn retrieval_failure_access_restricted_no_fallback() {
        let f = RetrievalFailure::new(
            "c4",
            RetrievalChannelTier::WebFetch,
            RetrievalFailureCategory::AccessRestricted,
            "HTTP 403 — access denied by site policy",
            false,
        );
        assert_eq!(
            f.failure_category,
            RetrievalFailureCategory::AccessRestricted
        );
        assert!(!f.allows_fallback);
    }

    // -----------------------------------------------------------------------
    // FallbackEligibilityFact tests
    // -----------------------------------------------------------------------

    #[test]
    fn fallback_fact_web_to_self_hosted() {
        let fact =
            FallbackEligibilityFact::new(RetrievalChannelTier::WebFetch, "network timeout", false);
        assert_eq!(fact.failed_tier, RetrievalChannelTier::WebFetch);
        assert_eq!(fact.next_tier, Some(RetrievalChannelTier::SelfHosted));
        assert!(!fact.is_access_restricted);
        assert!(!fact.requires_paid_confirmation);
    }

    #[test]
    fn fallback_fact_self_hosted_to_paid() {
        let fact = FallbackEligibilityFact::new(
            RetrievalChannelTier::SelfHosted,
            "service unavailable",
            false,
        );
        assert_eq!(fact.failed_tier, RetrievalChannelTier::SelfHosted);
        assert_eq!(fact.next_tier, Some(RetrievalChannelTier::Paid));
        assert!(fact.requires_paid_confirmation);
    }

    #[test]
    fn fallback_fact_paid_terminal() {
        let fact = FallbackEligibilityFact::new(
            RetrievalChannelTier::Paid,
            "paid service exhausted",
            false,
        );
        assert_eq!(fact.failed_tier, RetrievalChannelTier::Paid);
        assert_eq!(fact.next_tier, None);
    }

    #[test]
    fn fallback_fact_access_restricted() {
        let fact = FallbackEligibilityFact::new(
            RetrievalChannelTier::WebFetch,
            "HTTP 403 Forbidden",
            true,
        );
        assert!(fact.is_access_restricted);
        // Even though there's a next tier, access-restricted means don't fallback
        assert_eq!(fact.next_tier, Some(RetrievalChannelTier::SelfHosted));
    }

    // -----------------------------------------------------------------------
    // RetrievalOutcome tests
    // -----------------------------------------------------------------------

    #[test]
    fn retrieval_outcome_empty_batch() {
        let batch = RetrievalBatch::new(vec![], 4);
        let outcome = RetrievalOutcome {
            batch: batch.clone(),
            results: vec![],
            channel_tier: RetrievalChannelTier::WebFetch,
            shortage: Some(RetrievalBatchShortage::new(4, 0, "no candidates")),
            channels_attempted: 0,
            channel_attempts: vec![],
            fallback_facts: vec![],
            execution_blocked: None,
        };
        assert!(outcome.results.is_empty());
        assert!(outcome.shortage.is_some());
    }

    // -----------------------------------------------------------------------
    // ExecutionBlockingFact tests
    // -----------------------------------------------------------------------

    #[test]
    fn execution_blocked_by_paid_unconfirmed() {
        let fact = ExecutionBlockingFact {
            reason: "paid channel requires user confirmation".into(),
            source_tier: Some(RetrievalChannelTier::Paid),
            is_access_restricted: false,
            is_paid_unconfirmed: true,
        };
        assert!(fact.is_paid_unconfirmed);
        assert!(!fact.is_access_restricted);
    }

    #[test]
    fn execution_blocked_by_access_restriction() {
        let fact = ExecutionBlockingFact {
            reason: "all channels blocked by access restriction".into(),
            source_tier: Some(RetrievalChannelTier::WebFetch),
            is_access_restricted: true,
            is_paid_unconfirmed: false,
        };
        assert!(fact.is_access_restricted);
        assert!(!fact.is_paid_unconfirmed);
    }

    // -----------------------------------------------------------------------
    // ChannelAttemptResult tests
    // -----------------------------------------------------------------------

    #[test]
    fn channel_attempt_records_counts() {
        let attempt = ChannelAttemptResult {
            channel_tier: RetrievalChannelTier::WebFetch,
            results: vec![],
            success_count: 3,
            failure_count: 1,
            abandoned: false,
            abandon_reason: None,
        };
        assert_eq!(attempt.success_count, 3);
        assert_eq!(attempt.failure_count, 1);
        assert!(!attempt.abandoned);
    }

    #[test]
    fn channel_attempt_abandoned() {
        let attempt = ChannelAttemptResult {
            channel_tier: RetrievalChannelTier::WebFetch,
            results: vec![],
            success_count: 0,
            failure_count: 0,
            abandoned: true,
            abandon_reason: Some("channel not ready".into()),
        };
        assert!(attempt.abandoned);
    }
}
