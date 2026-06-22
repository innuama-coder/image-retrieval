//! Retrieval batch planner.
//!
//! Takes a [`RetrievableCandidateSequence`] and a batch target and forms a
//! [`RetrievalBatch`] for one complete retrieval attempt.
//!
//! The batch target is `required_count × 2` per the constitution.
//! When fewer retrievable candidates are available, the planner emits a
//! [`RetrievalBatchShortage`] but does not trigger indefinite back-fill.
//!
//! References: PRD §抓取渠道产品要求 (batch size = required_count × 2),
//! LLD §BaseRetrievalChannel 批次抓取与 fallback 详细设计

use crate::domain::candidate::{CandidateDecision, RetrievableCandidateSequence};
use crate::domain::retrieval::{RetrievalBatch, RetrievalBatchShortage};
use std::collections::HashMap;

/// Plans a retrieval batch from the retrievable candidate sequence.
///
/// # Rules
///
/// 1. Only candidates with `CandidateDecision::Accepted` may enter the batch
///    (the [`RetrievableCandidateSequence`] already guarantees this).
/// 2. The batch takes up to `batch_target` candidates from the front of the
///    sequence (highest priority first).
/// 3. When fewer candidates than `batch_target` are available, the batch is
///    marked as a *short batch* and a [`RetrievalBatchShortage`] is produced.
/// 4. Rejected, uncertain, and execution-blocked candidates are never
///    included — the [`RetrievableCandidateSequence`] already excludes them.
#[derive(Debug, Clone, Default)]
pub struct RetrievalBatchPlanner;

impl RetrievalBatchPlanner {
    /// Plan a retrieval batch from the given sequence.
    ///
    /// Returns the batch and an optional shortage evidence.
    pub fn plan(
        sequence: &RetrievableCandidateSequence,
        batch_target: u32,
    ) -> (RetrievalBatch, Option<RetrievalBatchShortage>) {
        let limit = batch_target as usize;
        let taken: Vec<&CandidateDecision> = sequence.candidates.iter().take(limit).collect();

        let candidate_ids: Vec<String> = taken
            .iter()
            .map(|d| match d {
                CandidateDecision::Accepted { candidate, .. } => candidate.candidate_id.to_string(),
                _ => unreachable!("RetrievableCandidateSequence only contains Accepted"),
            })
            .collect();

        let candidate_urls: HashMap<String, String> = taken
            .iter()
            .map(|d| match d {
                CandidateDecision::Accepted { candidate, .. } => (
                    candidate.candidate_id.to_string(),
                    candidate.image_url.clone(),
                ),
                _ => unreachable!("RetrievableCandidateSequence only contains Accepted"),
            })
            .collect();

        let actual = candidate_ids.len() as u32;
        let shortage = if actual < batch_target {
            Some(RetrievalBatchShortage::new(
                batch_target,
                actual,
                format!(
                    "only {} retrievable candidates available, target was {}",
                    actual, batch_target
                ),
            ))
        } else {
            None
        };

        let batch = RetrievalBatch::new(candidate_ids, batch_target).with_urls(candidate_urls);

        (batch, shortage)
    }

    /// Determine the effective batch target.
    ///
    /// Returns `required_count × 2` with saturation.
    pub fn batch_target_for(required_count: u32) -> u32 {
        required_count.saturating_mul(2)
    }

    /// Return the maximum number of candidates this planner will ever put
    /// in a single batch.
    pub fn max_batch_size_for(required_count: u32) -> usize {
        Self::batch_target_for(required_count) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::candidate::{CandidateId, CandidateRecord, ProviderId};

    fn make_accepted(id: &str, url: &str, priority: u32) -> CandidateDecision {
        CandidateDecision::Accepted {
            candidate: CandidateRecord::minimal(CandidateId::new(id), ProviderId::new("test"), url),
            priority,
        }
    }

    fn make_sequence(decisions: Vec<CandidateDecision>) -> RetrievableCandidateSequence {
        RetrievableCandidateSequence::from_decisions(decisions)
    }

    // -------------------------------------------------------------------
    // Normal batch (target met)
    // -------------------------------------------------------------------

    #[test]
    fn normal_batch_exact_target() {
        let decisions = vec![
            make_accepted("a", "https://example.com/a.jpg", 5),
            make_accepted("b", "https://example.com/b.jpg", 4),
            make_accepted("c", "https://example.com/c.jpg", 3),
            make_accepted("d", "https://example.com/d.jpg", 2),
        ];
        let seq = make_sequence(decisions);
        let (batch, shortage) = RetrievalBatchPlanner::plan(&seq, 4);

        assert_eq!(batch.actual_size(), 4);
        assert!(!batch.is_short_batch);
        assert_eq!(batch.target_size, 4);
        assert_eq!(batch.candidate_ids, vec!["a", "b", "c", "d"]);
        assert!(shortage.is_none());
    }

    #[test]
    fn normal_batch_more_than_target() {
        let mut decisions = Vec::new();
        for i in 0..10 {
            decisions.push(make_accepted(
                &format!("c{}", i),
                &format!("https://example.com/c{}.jpg", i),
                (10 - i) as u32,
            ));
        }
        let seq = make_sequence(decisions);
        let (batch, shortage) = RetrievalBatchPlanner::plan(&seq, 4);

        // Should take exactly 4, highest priority first
        assert_eq!(batch.actual_size(), 4);
        assert!(!batch.is_short_batch);
        assert_eq!(batch.target_size, 4);
        assert_eq!(batch.candidate_ids, vec!["c0", "c1", "c2", "c3"]);
        assert!(shortage.is_none());
    }

    // -------------------------------------------------------------------
    // Short batch
    // -------------------------------------------------------------------

    #[test]
    fn short_batch_fewer_than_target() {
        let decisions = vec![
            make_accepted("a", "https://example.com/a.jpg", 3),
            make_accepted("b", "https://example.com/b.jpg", 2),
        ];
        let seq = make_sequence(decisions);
        let (batch, shortage) = RetrievalBatchPlanner::plan(&seq, 8);

        assert_eq!(batch.actual_size(), 2);
        assert!(batch.is_short_batch);
        assert_eq!(batch.target_size, 8);

        let s = shortage.expect("should have shortage");
        assert_eq!(s.target_size, 8);
        assert_eq!(s.actual_size, 2);
        assert!(s.reason.contains("only 2"));
    }

    #[test]
    fn empty_batch_when_no_candidates() {
        let seq = RetrievableCandidateSequence::empty();
        let (batch, shortage) = RetrievalBatchPlanner::plan(&seq, 8);

        assert_eq!(batch.actual_size(), 0);
        assert!(batch.is_short_batch);
        assert!(shortage.is_some());
    }

    // -------------------------------------------------------------------
    // URL mapping
    // -------------------------------------------------------------------

    #[test]
    fn batch_includes_urls() {
        let decisions = vec![
            make_accepted("img-1", "https://example.com/1.jpg", 5),
            make_accepted("img-2", "https://example.com/2.jpg", 3),
        ];
        let seq = make_sequence(decisions);
        let (batch, _) = RetrievalBatchPlanner::plan(&seq, 2);

        assert_eq!(batch.url_for("img-1"), Some("https://example.com/1.jpg"));
        assert_eq!(batch.url_for("img-2"), Some("https://example.com/2.jpg"));
    }

    // -------------------------------------------------------------------
    // Batch target derivation
    // -------------------------------------------------------------------

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
    fn batch_target_for_3_is_6() {
        assert_eq!(RetrievalBatchPlanner::batch_target_for(3), 6);
    }

    #[test]
    fn max_batch_size_for_4_is_8() {
        assert_eq!(RetrievalBatchPlanner::max_batch_size_for(4), 8);
    }
}
