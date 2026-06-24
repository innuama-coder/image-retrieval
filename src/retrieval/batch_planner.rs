//! Retrieval batch planner — v1.1.
//!
//! Consumes a [`RetrievableCandidateBatch`] from TASK-003 and produces a
//! [`RetrievalBatch`] with structured [`RetrievalJob`] objects.
//!
//! The batch target is `required_image_count × 2`. When fewer retrievable
//! candidates are available, the planner emits a [`RetrievalBatchShortage`].
//!
//! References: PRD FR-007, LLD §Retrieval Batch Planning,
//! `docs/design/v1.1-TASK-004-retrieval-artifact-channel-design.md`

use crate::domain::candidate::RetrievableCandidateBatch;
use crate::domain::query_plan::{QueryPlanId, QueryRetrievalPolicy};
use crate::domain::retrieval::{
    RetrievalBatch, RetrievalBatchShortage, RetrievalJob, RetrievalPolicyContext,
    RetrievalShortageCode,
};

/// Plans a retrieval batch from a TASK-003 `RetrievableCandidateBatch`.
///
/// # Rules
///
/// 1. Only candidates in the retrievable batch may enter (TASK-003 already
///    guarantees mechanical + Qwen approval).
/// 2. The batch takes up to `target_size` candidates, sorted by
///    `retrieval_priority` (higher first).
/// 3. When fewer candidates than target are available, a
///    [`RetrievalBatchShortage`] is produced.
/// 4. Jobs carry full policy context, provenance, and ownership.
#[derive(Debug, Clone, Default)]
pub struct RetrievalBatchPlanner;

impl RetrievalBatchPlanner {
    /// Plan a retrieval batch from a TASK-003 retrievable candidate batch.
    ///
    /// Returns the batch and an optional shortage evidence.
    pub fn plan_from_batch(
        retrievable_batch: &RetrievableCandidateBatch,
        query_plan_id: &QueryPlanId,
        retrieval_policy: &QueryRetrievalPolicy,
        robots_unknown_behavior: &str,
        prohibited_domains: &[String],
        fixture_mode: bool,
    ) -> (RetrievalBatch, Option<RetrievalBatchShortage>) {
        let target_size = retrievable_batch.retrieval_batch_target;
        let full_attempt_count = retrievable_batch.full_attempt_count;
        let retry_count = retrievable_batch.retry_count;

        let batch_id = format!("batch-{}-attempt-{}", query_plan_id, full_attempt_count);

        // Take at most target_size candidates (already sorted by priority)
        let limit = target_size as usize;
        let taken = if retrievable_batch.candidates.len() > limit {
            &retrievable_batch.candidates[..limit]
        } else {
            &retrievable_batch.candidates
        };

        let policy_context = RetrievalPolicyContext {
            allow_paid: retrieval_policy.allow_paid,
            respect_robots: retrieval_policy.respect_robots,
            allow_login: retrieval_policy.allow_login,
            allow_paywalled: retrieval_policy.allow_paywalled,
            prohibited_domains: prohibited_domains.to_vec(),
            robots_unknown_behavior: robots_unknown_behavior.to_string(),
            fixture_mode,
        };

        let jobs: Vec<RetrievalJob> = taken
            .iter()
            .map(|rc| {
                RetrievalJob::from_retrievable(
                    rc,
                    &batch_id,
                    &query_plan_id.to_string(),
                    full_attempt_count,
                    retry_count,
                    format!("candidate-quality-decision-{}", rc.candidate.candidate_id),
                    policy_context.clone(),
                )
            })
            .collect();

        let actual_size = jobs.len() as u32;
        let shortage = if actual_size < target_size {
            let (shortage_code, reason) = if actual_size == 0 {
                (
                    RetrievalShortageCode::NoRetrievableCandidates,
                    format!(
                        "no retrievable candidates available, target was {}",
                        target_size
                    ),
                )
            } else {
                (
                    RetrievalShortageCode::InsufficientRetrievableCandidates,
                    format!(
                        "only {} retrievable candidates available, target was {}",
                        actual_size, target_size
                    ),
                )
            };

            let mut shortage = RetrievalBatchShortage::new(
                query_plan_id.to_string(),
                target_size,
                actual_size,
                shortage_code,
                reason,
            );
            shortage.candidate_quality_blockers = retrievable_batch
                .rejected_decisions
                .iter()
                .map(|d| format!("{}: {:?}", d.candidate_id, d.final_status))
                .collect();
            shortage.search_shortage_ref =
                Some(format!("candidate-quality-outcome-{}", query_plan_id));
            Some(shortage)
        } else {
            None
        };

        let batch = RetrievalBatch::new(
            batch_id,
            query_plan_id.to_string(),
            full_attempt_count,
            retry_count,
            target_size,
            jobs,
            shortage.clone(),
        );

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
    use crate::domain::candidate::{
        CandidateId, CandidateQualityDecision, CandidateQualityStatus, CandidateRecord, ProviderId,
        RetrievableCandidate, RetrievableCandidateBatch,
    };

    fn make_retrievable(id: &str, image_url: &str, priority: u32) -> RetrievableCandidate {
        let rec =
            CandidateRecord::minimal(CandidateId::new(id), ProviderId::new("test"), image_url);
        RetrievableCandidate {
            candidate: rec,
            candidate_quality_decision: CandidateQualityDecision {
                candidate_id: CandidateId::new(id),
                query_plan_id: "qp-1".into(),
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
            source_page_url: None,
            thumbnail_url: None,
            expected_mime_type: Some("image/jpeg".into()),
            license_hint: None,
            provenance_refs: vec![],
        }
    }

    fn make_batch(candidates: Vec<RetrievableCandidate>) -> RetrievableCandidateBatch {
        let len = candidates.len() as u32;
        RetrievableCandidateBatch {
            query_plan_id: "qp-1".into(),
            full_attempt_count: 1,
            retry_count: 0,
            retrieval_batch_target: len * 2, // ensure enough room
            candidates,
            rejected_decisions: vec![],
            execution_blocking_facts: vec![],
        }
    }

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
    fn plan_normal_batch_exact_target() {
        let candidates = vec![
            make_retrievable("a", "https://example.com/a.jpg", 5),
            make_retrievable("b", "https://example.com/b.jpg", 4),
        ];
        let rb = RetrievableCandidateBatch {
            retrieval_batch_target: 2,
            ..make_batch(candidates)
        };

        let qp_id = QueryPlanId::new("qp-1");
        let policy = QueryRetrievalPolicy::default();
        let (batch, shortage) =
            RetrievalBatchPlanner::plan_from_batch(&rb, &qp_id, &policy, "warn", &[], false);

        assert_eq!(batch.actual_size, 2);
        assert!(!batch.is_short_batch);
        assert_eq!(batch.target_size, 2);
        assert!(shortage.is_none());
        assert_eq!(batch.jobs.len(), 2);
        assert_eq!(batch.jobs[0].candidate_id, "a"); // higher priority first
    }

    #[test]
    fn plan_short_batch_when_fewer_candidates() {
        let candidates = vec![
            make_retrievable("a", "https://example.com/a.jpg", 5),
            make_retrievable("b", "https://example.com/b.jpg", 3),
        ];
        let rb = RetrievableCandidateBatch {
            retrieval_batch_target: 8,
            ..make_batch(candidates)
        };

        let qp_id = QueryPlanId::new("qp-1");
        let policy = QueryRetrievalPolicy::default();
        let (batch, _shortage) =
            RetrievalBatchPlanner::plan_from_batch(&rb, &qp_id, &policy, "warn", &[], false);

        assert!(batch.is_short_batch);
        assert_eq!(batch.actual_size, 2);
        assert_eq!(batch.target_size, 8);
    }

    #[test]
    fn plan_empty_batch() {
        let rb = RetrievableCandidateBatch {
            query_plan_id: "qp-1".into(),
            full_attempt_count: 1,
            retry_count: 0,
            retrieval_batch_target: 4,
            candidates: vec![],
            rejected_decisions: vec![],
            execution_blocking_facts: vec![],
        };

        let qp_id = QueryPlanId::new("qp-1");
        let policy = QueryRetrievalPolicy::default();
        let (batch, _shortage) =
            RetrievalBatchPlanner::plan_from_batch(&rb, &qp_id, &policy, "warn", &[], false);

        assert_eq!(batch.actual_size, 0);
        assert!(batch.is_short_batch);
    }
}
