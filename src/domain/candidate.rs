//! Candidate domain types.
//!
//! Covers provider identity, candidate records, source tracking,
//! and candidate-level decisions after mechanical validation and
//! OpenClaw subjective evaluation.
//!
//! References: PRD §搜索与候选产品要求, HLD §Candidate Quality Gate

use serde::{Deserialize, Serialize};

/// Opaque identifier for a search provider.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProviderId(pub String);

impl ProviderId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl std::fmt::Display for ProviderId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A single search result candidate before quality gating.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateRecord {
    /// Opaque id assigned by the system for this candidate.
    pub id: CandidateId,

    /// Which provider produced this candidate.
    pub provider_id: ProviderId,

    /// Original source URL for the candidate image.
    pub source_url: String,

    /// Optional thumbnail URL (may differ from source_url).
    pub thumbnail_url: Option<String>,

    /// Title / alt text from the search result.
    pub title: Option<String>,

    /// Source page URL where the image was found (if available).
    pub page_url: Option<String>,

    /// Image dimensions as reported by the search provider (if available).
    pub dimensions: Option<ImageDimensions>,
}

/// System-assigned candidate identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CandidateId(pub String);

impl CandidateId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl std::fmt::Display for CandidateId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Image dimensions in pixels.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ImageDimensions {
    pub width: u32,
    pub height: u32,
}

/// Source information for traceability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateSource {
    pub provider_id: ProviderId,
    pub search_query: String,
    pub retrieved_at: String,
}

// ---------------------------------------------------------------------------
// Candidate decision — output of the Candidate Quality Gate
// ---------------------------------------------------------------------------

/// The outcome of candidate mechanical validation + OpenClaw subjective
/// evaluation, normalised into an executable status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CandidateDecision {
    /// Candidate passed both mechanical and subjective checks and may enter
    /// the retrievable sequence.
    Accepted {
        candidate: CandidateRecord,
        /// Priority within the retrievable sequence (higher = fetched sooner).
        priority: u32,
    },

    /// Candidate was mechanically blocked (e.g. unreachable, duplicate,
    /// clearly below quality tier). Includes the blocking reason.
    Rejected {
        candidate: CandidateRecord,
        reason: String,
    },

    /// OpenClaw evaluation was uncertain; candidate is excluded from the
    /// retrievable sequence but is not definitively rejected.
    Uncertain {
        candidate: CandidateRecord,
        reason: String,
    },

    /// Insufficient evidence to decide (e.g. OpenClaw was unavailable and
    /// no fallback is allowed in production).
    ExecutionBlocked {
        candidate: CandidateRecord,
        reason: String,
    },
}

impl CandidateDecision {
    /// Returns `true` iff the candidate was accepted and can enter the
    /// retrievable sequence.
    pub fn is_accepted(&self) -> bool {
        matches!(self, Self::Accepted { .. })
    }
}

// ---------------------------------------------------------------------------
// Retrievable candidate sequence
// ---------------------------------------------------------------------------

/// Ordered sequence of candidates that are eligible for retrieval.
///
/// Only candidates with `CandidateDecision::Accepted` may appear here.
/// The sequence is sorted by descending priority.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievableCandidateSequence {
    pub candidates: Vec<CandidateDecision>,
}

impl RetrievableCandidateSequence {
    pub fn empty() -> Self {
        Self {
            candidates: Vec::new(),
        }
    }

    /// Build from a list of decisions, keeping only accepted ones sorted
    /// by descending priority.
    pub fn from_decisions(decisions: Vec<CandidateDecision>) -> Self {
        let mut accepted: Vec<CandidateDecision> =
            decisions.into_iter().filter(|d| d.is_accepted()).collect();
        accepted.sort_by_key(|d| match d {
            CandidateDecision::Accepted { priority, .. } => std::cmp::Reverse(*priority),
            _ => std::cmp::Reverse(0),
        });
        Self {
            candidates: accepted,
        }
    }

    pub fn len(&self) -> usize {
        self.candidates.len()
    }

    pub fn is_empty(&self) -> bool {
        self.candidates.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_candidate(id: &str) -> CandidateRecord {
        CandidateRecord {
            id: CandidateId::new(id),
            provider_id: ProviderId::new("test-provider"),
            source_url: format!("https://example.com/{}", id),
            thumbnail_url: None,
            title: None,
            page_url: None,
            dimensions: None,
        }
    }

    #[test]
    fn candidate_decision_accepted_is_accepted() {
        let c = make_candidate("img-1");
        let d = CandidateDecision::Accepted {
            candidate: c,
            priority: 5,
        };
        assert!(d.is_accepted());
    }

    #[test]
    fn candidate_decision_rejected_is_not_accepted() {
        let c = make_candidate("img-2");
        let d = CandidateDecision::Rejected {
            candidate: c,
            reason: "duplicate".into(),
        };
        assert!(!d.is_accepted());
    }

    #[test]
    fn candidate_decision_uncertain_is_not_accepted() {
        let c = make_candidate("img-3");
        let d = CandidateDecision::Uncertain {
            candidate: c,
            reason: "ambiguous match".into(),
        };
        assert!(!d.is_accepted());
    }

    #[test]
    fn retrievable_sequence_only_includes_accepted() {
        let c1 = make_candidate("img-a");
        let c2 = make_candidate("img-b");
        let c3 = make_candidate("img-c");

        let decisions = vec![
            CandidateDecision::Accepted {
                candidate: c1,
                priority: 3,
            },
            CandidateDecision::Rejected {
                candidate: c2,
                reason: "low quality".into(),
            },
            CandidateDecision::Accepted {
                candidate: c3,
                priority: 7,
            },
        ];

        let seq = RetrievableCandidateSequence::from_decisions(decisions);
        assert_eq!(seq.len(), 2);
        // Higher priority first
        match &seq.candidates[0] {
            CandidateDecision::Accepted { priority, .. } => assert_eq!(*priority, 7),
            _ => panic!("expected accepted"),
        }
    }

    #[test]
    fn empty_sequence_is_empty() {
        let seq = RetrievableCandidateSequence::empty();
        assert!(seq.is_empty());
        assert_eq!(seq.len(), 0);
    }

    #[test]
    fn provider_id_display() {
        let id = ProviderId::new("brave");
        assert_eq!(id.to_string(), "brave");
    }

    #[test]
    fn candidate_id_display() {
        let id = CandidateId::new("cand-001");
        assert_eq!(id.to_string(), "cand-001");
    }
}
