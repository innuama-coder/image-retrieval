//! Retrieval domain types.
//!
//! Covers retrieval channel tiers, batch planning, and retrieval results
//! (success / failure / fallback evidence).
//!
//! References: PRD §抓取渠道产品要求, HLD §Retrieval Batch Planner

use serde::{Deserialize, Serialize};

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

// ---------------------------------------------------------------------------
// Retrieval batch
// ---------------------------------------------------------------------------

/// A planned batch of candidates to retrieve.
///
/// The target size is `required_count × 2`. When fewer retrievable candidates
/// are available this forms a *short batch*.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalBatch {
    /// IDs of the candidates in this batch.
    pub candidate_ids: Vec<String>,

    /// Target batch size (required_count × 2).
    pub target_size: u32,

    /// Whether this is a short batch (fewer candidates than target).
    pub is_short_batch: bool,
}

impl RetrievalBatch {
    pub fn new(candidate_ids: Vec<String>, target_size: u32) -> Self {
        let is_short_batch = (candidate_ids.len() as u32) < target_size;
        Self {
            candidate_ids,
            target_size,
            is_short_batch,
        }
    }

    pub fn actual_size(&self) -> usize {
        self.candidate_ids.len()
    }
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
