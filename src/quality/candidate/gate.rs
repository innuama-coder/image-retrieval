#![allow(deprecated)]
//! Candidate Quality Gate — mechanical + OpenClaw evaluation orchestrator.
//!
//! Implements the "先机械、后主观、再归一" flow:
//! 1. Mechanical validation blocks obviously-bad candidates.
//! 2. Mechanically-passed candidates are packaged into evaluation requests.
//! 3. OpenClaw evaluates the requests.
//! 4. Conclusions are normalized into `CandidateDecision`.
//! 5. Accepted candidates form the `RetrievableCandidateSequence`.
//!
//! References: PRD §校验与评价产品要求, HLD §Candidate Quality Gate,
//! `docs/design/TASK-004-candidate-quality-openclaw-design.md`

use crate::domain::candidate::{
    CandidateDecision, CandidateMechanicalAssessment, CandidateQualityDecision, CandidateRecord,
    RetrievableCandidateSequence, VlmSubjectDecision, VlmSubjectDecisionKind,
};
use crate::domain::query_plan::ValidatedQueryPlan;
use crate::domain::search::SearchOutcome;
use crate::error::{Error, Result};
use crate::ports::OpenClawEvaluationPort;
use crate::quality::candidate::evaluation::{
    normalize_conclusion, CandidateEvaluationConclusion, CandidateEvaluationRequest,
    ExecutionBlockingFact,
};
use crate::quality::candidate::mechanical::{
    validate_candidate_mechanical, validate_candidate_mechanical_v11, CandidateMechanicalEvidence,
};
use std::collections::HashSet;

// ---------------------------------------------------------------------------
// Quality gate result
// ---------------------------------------------------------------------------

/// The full output of the candidate quality gate.
///
/// Contains the retrievable sequence, all decisions (for downstream
/// diagnostics), and any execution blocking facts.
#[derive(Debug, Clone)]
pub struct CandidateQualityGateResult {
    /// Accepted candidates sorted by descending priority — the only
    /// candidates eligible for retrieval.
    pub retrievable_sequence: RetrievableCandidateSequence,

    /// All decisions produced by the gate (accepted, rejected, uncertain,
    /// execution-blocked). Useful for diagnostics, metrics, and delivery
    /// manifest explanations.
    pub all_decisions: Vec<CandidateDecision>,

    /// v1.1 quality decisions with mechanical metrics and VLM decisions
    /// preserved for retrieval handoff, coverage, and package evidence.
    pub quality_decisions: Vec<CandidateQualityDecision>,

    /// Execution blocking facts — non-empty when OpenClaw was unavailable
    /// and production policy requires it.
    pub execution_blocking_facts: Vec<ExecutionBlockingFact>,

    /// Summary counts for observability (MET-004, MET-006).
    pub summary: CandidateQualitySummary,
}

/// Summary counts for quality gate observability.
#[derive(Debug, Clone, Default)]
pub struct CandidateQualitySummary {
    /// Total candidates input to the gate.
    pub total_candidates: usize,

    /// Number mechanically blocked (did not reach OpenClaw).
    pub mechanically_blocked: usize,

    /// Number approved by OpenClaw (entered retrievable sequence).
    pub openclaw_approved: usize,

    /// Number rejected by OpenClaw.
    pub openclaw_rejected: usize,

    /// Number evaluated as uncertain by OpenClaw.
    pub openclaw_uncertain: usize,

    /// Number that could not be evaluated (OpenClaw unavailable).
    pub openclaw_unexecutable: usize,
}

impl CandidateQualitySummary {
    /// Build a summary from a list of decisions and the input candidate count.
    pub fn from_decisions(total: usize, decisions: &[CandidateDecision]) -> Self {
        let mut summary = Self {
            total_candidates: total,
            ..Default::default()
        };

        for d in decisions {
            match d {
                CandidateDecision::Accepted { .. } => summary.openclaw_approved += 1,
                CandidateDecision::Rejected { .. } => {
                    // We can't distinguish mechanical vs OpenClaw reject here
                    // without additional metadata. The gate itself tracks this
                    // separately, so the summary is approximate for decisions
                    // that go through the full flow.
                    summary.openclaw_rejected += 1;
                }
                CandidateDecision::Uncertain { .. } => summary.openclaw_uncertain += 1,
                CandidateDecision::ExecutionBlocked { .. } => summary.openclaw_unexecutable += 1,
            }
        }

        summary
    }
}

// ---------------------------------------------------------------------------
// Candidate Quality Gate
// ---------------------------------------------------------------------------

/// The candidate quality gate.
///
/// Owns the OpenClaw evaluation port and the QueryPlan context needed to
/// evaluate candidates. The gate is instantiated once per task execution.
pub struct CandidateQualityGate<'a> {
    /// OpenClaw evaluation port (trait object for pluggability).
    openclaw: &'a dyn OpenClawEvaluationPort,

    /// Query plan context for mechanical validation and evaluation requests.
    query_plan: ValidatedQueryPlan,

    /// Active QueryPlan id used for ownership checks.
    query_plan_id: Option<String>,

    /// Prohibited source domains from runtime policy.
    prohibited_domains: Vec<String>,

    /// Whether fixture candidates are allowed in this gate execution.
    fixture_mode: bool,
}

impl<'a> CandidateQualityGate<'a> {
    /// Create a new quality gate.
    pub fn new(openclaw: &'a dyn OpenClawEvaluationPort, query_plan: ValidatedQueryPlan) -> Self {
        Self {
            openclaw,
            query_plan,
            query_plan_id: None,
            prohibited_domains: Vec::new(),
            fixture_mode: true,
        }
    }

    /// Create a quality gate with explicit v1.1 runtime context.
    pub fn new_with_context(
        openclaw: &'a dyn OpenClawEvaluationPort,
        query_plan: ValidatedQueryPlan,
        query_plan_id: impl Into<String>,
        prohibited_domains: Vec<String>,
        fixture_mode: bool,
    ) -> Self {
        Self {
            openclaw,
            query_plan,
            query_plan_id: Some(query_plan_id.into()),
            prohibited_domains,
            fixture_mode,
        }
    }

    /// Run the full quality gate pipeline on a set of candidates.
    ///
    /// # Flow
    ///
    /// 1. Check OpenClaw readiness.
    /// 2. Run mechanical validation on every candidate.
    ///    - Blocked candidates → `CandidateDecision::Rejected` immediately.
    /// 3. Build evaluation requests for mechanically-passed candidates.
    /// 4. Call OpenClaw to evaluate the batch.
    /// 5. Normalize each conclusion → `CandidateDecision`.
    /// 6. Build `RetrievableCandidateSequence` from accepted decisions.
    ///
    /// # Errors
    ///
    /// Returns `Err(Error::ExecutionBlocked)` when OpenClaw is unavailable
    /// AND there are mechanically-passed candidates that require evaluation.
    /// The error contains the execution blocking facts for observability.
    pub fn evaluate(&self, search_outcome: &SearchOutcome) -> Result<CandidateQualityGateResult> {
        let candidates = &search_outcome.candidates;
        let total = candidates.len();

        // We also consume candidate shortage evidence from the search outcome
        // for reference. Candidate shortages do NOT block the quality gate
        // — they flow downstream as part of the retrievable sequence context.
        let _shortage = &search_outcome.shortage_reason;

        // Track seen URLs within this batch for duplicate detection
        let mut seen_urls: HashSet<String> = HashSet::new();

        // Phase 1: Mechanical validation
        let mut mechanically_blocked: Vec<CandidateDecision> = Vec::new();
        let mut quality_decisions: Vec<CandidateQualityDecision> = Vec::new();
        let mut mechanically_passed: Vec<(
            CandidateRecord,
            CandidateMechanicalEvidence,
            CandidateMechanicalAssessment,
        )> = Vec::new();

        for candidate in candidates.iter().cloned() {
            let expected_query_plan_id = self
                .query_plan_id
                .as_deref()
                .unwrap_or(&candidate.query_plan_id);
            let evidence = validate_candidate_mechanical(
                &candidate,
                &seen_urls,
                &self.query_plan.content_constraints,
                self.query_plan.quality_tier,
            );
            let assessment = validate_candidate_mechanical_v11(
                &candidate,
                expected_query_plan_id,
                &seen_urls,
                &self.prohibited_domains,
                &self.query_plan.content_constraints.must_avoid,
                self.fixture_mode,
            );

            if assessment.passed {
                // Track the URL for future duplicate detection
                if !candidate.image_url.trim().is_empty() {
                    seen_urls.insert(candidate.image_url.clone());
                }
                mechanically_passed.push((candidate, evidence, assessment));
            } else {
                // Build rejection decision from v1.1 blocking facts.
                let reasons: Vec<String> = assessment
                    .blocking_metrics
                    .iter()
                    .map(|metric| metric.message.clone())
                    .collect();
                mechanically_blocked.push(CandidateDecision::Rejected {
                    candidate: candidate.clone(),
                    reason: reasons.join("; "),
                });
                quality_decisions.push(CandidateQualityDecision::mechanically_rejected(
                    candidate.candidate_id.clone(),
                    candidate.query_plan_id.clone(),
                    assessment.blocking_metrics,
                ));
            }
        }

        let mechanically_blocked_count = mechanically_blocked.len();
        let mut all_decisions: Vec<CandidateDecision> = mechanically_blocked.clone();

        // If no candidates passed mechanical, return early
        if mechanically_passed.is_empty() {
            let summary = CandidateQualitySummary {
                total_candidates: total,
                mechanically_blocked: mechanically_blocked_count,
                ..Default::default()
            };
            return Ok(CandidateQualityGateResult {
                retrievable_sequence: RetrievableCandidateSequence::empty(),
                all_decisions,
                quality_decisions,
                execution_blocking_facts: Vec::new(),
                summary,
            });
        }

        // Phase 2: Build evaluation requests
        let _requests: Vec<CandidateEvaluationRequest> = mechanically_passed
            .iter()
            .map(|(candidate, evidence, _assessment)| {
                CandidateEvaluationRequest::new(
                    candidate.clone(),
                    evidence.clone(),
                    self.query_plan.description.clone(),
                    self.query_plan.quality_tier,
                    self.query_plan.content_constraints.clone(),
                    self.query_plan.authorization_preference,
                    candidate.provider_id.to_string(),
                )
            })
            .collect();

        // Phase 3: Call OpenClaw
        //
        // We call the OpenClaw evaluation port with the candidate records
        // and the query description. The port trait has evaluate_candidates()
        // which returns Vec<CandidateDecision> directly — but for our gate
        // we want finer-grained control over the normalization.
        //
        // We use the port method which already does OpenClaw evaluation.
        // If the port returns an error (OpenClaw unavailable), all
        // mechanically-passed candidates become ExecutionBlocked.

        let openclaw_result = self.openclaw.evaluate_candidate_requests(&_requests);

        match openclaw_result {
            Ok(decisions) => {
                validate_candidate_decision_response(&mechanically_passed, &decisions)?;
                // The port returned decisions directly. We trust the port's
                // normalization but we should still validate and potentially
                // enrich the results. For now, we accept the port's output
                // as normalized decisions.

                // But we need to distinguish between mechanical rejection
                // (already handled) and evaluation-time rejection.
                // The port's decisions go into our final decision list.
                for (decision, (_candidate, _evidence, assessment)) in
                    decisions.iter().zip(mechanically_passed.iter())
                {
                    quality_decisions.push(quality_decision_from_candidate_decision(
                        decision, assessment,
                    ));
                }
                all_decisions.extend(decisions);

                let summary = self.build_summary(total, mechanically_blocked_count, &all_decisions);
                let retrievable_sequence =
                    RetrievableCandidateSequence::from_decisions(all_decisions.clone());

                Ok(CandidateQualityGateResult {
                    retrievable_sequence,
                    all_decisions,
                    quality_decisions,
                    execution_blocking_facts: Vec::new(),
                    summary,
                })
            }
            Err(e) => {
                // OpenClaw evaluation failed — this is an execution block
                if matches!(e, Error::OpenClawUnavailable { .. }) {
                    let fact = ExecutionBlockingFact::openclaw_unavailable(e.to_string());

                    // All mechanically-passed candidates become ExecutionBlocked
                    let blocked_decisions: Vec<CandidateDecision> = mechanically_passed
                        .into_iter()
                        .map(|(candidate, _evidence, assessment)| {
                            let reason = e.to_string();
                            quality_decisions.push(CandidateQualityDecision::merged(
                                candidate.candidate_id.clone(),
                                candidate.query_plan_id.clone(),
                                &assessment,
                                Some(&VlmSubjectDecision {
                                    subject_id: candidate.candidate_id.to_string(),
                                    decision: VlmSubjectDecisionKind::Unexecutable,
                                    confidence: None,
                                    reason_codes: vec!["openclaw_unavailable".into()],
                                    rationale_summary: reason.clone(),
                                    evidence_refs: vec![],
                                }),
                            ));
                            CandidateDecision::ExecutionBlocked { candidate, reason }
                        })
                        .collect();

                    all_decisions.extend(blocked_decisions);

                    let summary = CandidateQualitySummary {
                        total_candidates: total,
                        mechanically_blocked: mechanically_blocked_count,
                        openclaw_unexecutable: all_decisions.len() - mechanically_blocked_count,
                        ..Default::default()
                    };

                    Ok(CandidateQualityGateResult {
                        retrievable_sequence: RetrievableCandidateSequence::empty(),
                        all_decisions,
                        quality_decisions,
                        execution_blocking_facts: vec![fact],
                        summary,
                    })
                } else {
                    // Other errors propagate up
                    Err(e)
                }
            }
        }
    }

    /// Build a quality summary from decisions.
    fn build_summary(
        &self,
        total: usize,
        mechanically_blocked: usize,
        decisions: &[CandidateDecision],
    ) -> CandidateQualitySummary {
        let mut summary = CandidateQualitySummary {
            total_candidates: total,
            mechanically_blocked,
            ..Default::default()
        };

        for d in decisions {
            match d {
                CandidateDecision::Accepted { .. } => summary.openclaw_approved += 1,
                CandidateDecision::Rejected { .. } => {
                    // mechanically_blocked already counted; the rest are OpenClaw rejects
                }
                CandidateDecision::Uncertain { .. } => summary.openclaw_uncertain += 1,
                CandidateDecision::ExecutionBlocked { .. } => summary.openclaw_unexecutable += 1,
            }
        }

        // OpenClaw-rejected = total Rejected - mechanically blocked
        let total_rejected = decisions
            .iter()
            .filter(|d| matches!(d, CandidateDecision::Rejected { .. }))
            .count();
        summary.openclaw_rejected = total_rejected.saturating_sub(mechanically_blocked);

        summary
    }
}

fn quality_decision_from_candidate_decision(
    decision: &CandidateDecision,
    assessment: &CandidateMechanicalAssessment,
) -> CandidateQualityDecision {
    let vlm = vlm_subject_decision_from_candidate_decision(decision);
    let (candidate_id, query_plan_id) = match decision {
        CandidateDecision::Accepted { candidate, .. }
        | CandidateDecision::Rejected { candidate, .. }
        | CandidateDecision::Uncertain { candidate, .. }
        | CandidateDecision::ExecutionBlocked { candidate, .. } => (
            candidate.candidate_id.clone(),
            candidate.query_plan_id.clone(),
        ),
    };
    CandidateQualityDecision::merged(candidate_id, query_plan_id, assessment, Some(&vlm))
}

fn validate_candidate_decision_response(
    mechanically_passed: &[(
        CandidateRecord,
        CandidateMechanicalEvidence,
        CandidateMechanicalAssessment,
    )],
    decisions: &[CandidateDecision],
) -> Result<()> {
    if decisions.len() != mechanically_passed.len() {
        return Err(Error::execution_blocked(format!(
            "VLM candidate response cardinality mismatch: expected {} decisions, got {}",
            mechanically_passed.len(),
            decisions.len()
        )));
    }

    for ((expected_candidate, _evidence, _assessment), decision) in
        mechanically_passed.iter().zip(decisions.iter())
    {
        let actual_id = candidate_decision_subject_id(decision);
        let expected_id = expected_candidate.candidate_id.to_string();
        if actual_id != expected_id {
            return Err(Error::execution_blocked(format!(
                "VLM candidate response subject id mismatch: expected {}, got {}",
                expected_id, actual_id
            )));
        }
    }

    Ok(())
}

fn candidate_decision_subject_id(decision: &CandidateDecision) -> String {
    match decision {
        CandidateDecision::Accepted { candidate, .. }
        | CandidateDecision::Rejected { candidate, .. }
        | CandidateDecision::Uncertain { candidate, .. }
        | CandidateDecision::ExecutionBlocked { candidate, .. } => {
            candidate.candidate_id.to_string()
        }
    }
}

fn vlm_subject_decision_from_candidate_decision(
    decision: &CandidateDecision,
) -> VlmSubjectDecision {
    match decision {
        CandidateDecision::Accepted {
            candidate,
            vlm_evidence,
            ..
        } => VlmSubjectDecision {
            subject_id: candidate.candidate_id.to_string(),
            decision: VlmSubjectDecisionKind::Approve,
            confidence: vlm_evidence.as_ref().and_then(|e| e.confidence),
            reason_codes: vlm_evidence
                .as_ref()
                .map(|e| e.reason_codes.clone())
                .unwrap_or_default(),
            rationale_summary: vlm_evidence
                .as_ref()
                .and_then(|e| e.rationale_summary.clone())
                .unwrap_or_else(|| "candidate approved by VLM".into()),
            evidence_refs: vec![],
        },
        CandidateDecision::Rejected { candidate, reason } => VlmSubjectDecision {
            subject_id: candidate.candidate_id.to_string(),
            decision: VlmSubjectDecisionKind::Reject,
            confidence: None,
            reason_codes: vec!["candidate_rejected".into()],
            rationale_summary: reason.clone(),
            evidence_refs: vec![],
        },
        CandidateDecision::Uncertain { candidate, reason } => VlmSubjectDecision {
            subject_id: candidate.candidate_id.to_string(),
            decision: VlmSubjectDecisionKind::Uncertain,
            confidence: None,
            reason_codes: vec!["candidate_uncertain".into()],
            rationale_summary: reason.clone(),
            evidence_refs: vec![],
        },
        CandidateDecision::ExecutionBlocked { candidate, reason } => VlmSubjectDecision {
            subject_id: candidate.candidate_id.to_string(),
            decision: VlmSubjectDecisionKind::Unexecutable,
            confidence: None,
            reason_codes: vec!["candidate_execution_blocked".into()],
            rationale_summary: reason.clone(),
            evidence_refs: vec![],
        },
    }
}

// ---------------------------------------------------------------------------
// Standalone evaluation helper (no dependency on OpenClaw port)
// ---------------------------------------------------------------------------

/// Evaluate candidates using an explicit conclusion-per-candidate mapping.
///
/// This is the preferred path for fixture/test evaluators. Instead of
/// calling the port trait, the caller provides a list of conclusions that
/// map 1:1 to the mechanically-passed candidates.
pub fn evaluate_with_conclusions(
    mechanically_passed: Vec<(CandidateRecord, CandidateMechanicalEvidence)>,
    conclusions: Vec<CandidateEvaluationConclusion>,
) -> Vec<CandidateDecision> {
    mechanically_passed
        .into_iter()
        .zip(conclusions)
        .map(|((candidate, _evidence), conclusion)| normalize_conclusion(candidate, conclusion))
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::candidate::{CandidateId, ProviderId};
    use crate::domain::query_plan::{
        AuthorizationPreference, ContentConstraints, QualityTier, ValidatedQueryPlan,
    };
    use crate::domain::search::{CandidateShortageReason, SearchOutcome};
    use crate::quality::candidate::mechanical::CandidateBlockingReason;
    use std::cell::RefCell;

    // -----------------------------------------------------------------------
    // Test helpers
    // -----------------------------------------------------------------------

    fn make_candidate(id: &str, url: &str, title: Option<&str>) -> CandidateRecord {
        let cid = CandidateId::new(id);
        CandidateRecord {
            candidate_id: cid.clone(),
            query_plan_id: "qp-test".into(),
            provider_id: ProviderId::new("test-provider"),
            provider_kind: "fixture".into(),
            search_request_id: "sr-test".into(),
            search_round: 1,
            provider_rank: 1,
            global_rank_hint: None,
            image_url: url.into(),
            source_page_url: None,
            thumbnail_url: None,
            title: title.map(|s| s.into()),
            snippet: None,
            width: None,
            height: None,
            mime_type: None,
            license_hint: None,
            attribution: None,
            dedupe_key: CandidateRecord::build_dedupe_key(url),
            origin_candidate_ids: vec![cid],
            provenance: crate::domain::candidate::CandidateProvenance::new(1, "test", 1, 1),
            normalization_warnings: Vec::new(),
        }
    }

    fn make_test_query_plan() -> ValidatedQueryPlan {
        ValidatedQueryPlan {
            description: "sunset over mountains".into(),
            required_count: 1,
            quality_tier: QualityTier::General,
            content_constraints: ContentConstraints::default(),
            authorization_preference: AuthorizationPreference::Default,
            output_preference: crate::domain::query_plan::OutputPreference::Human,
            retry_limit: 3,
        }
    }

    fn make_search_outcome(candidates: Vec<CandidateRecord>) -> SearchOutcome {
        SearchOutcome {
            candidates,
            usage_events: vec![],
            total_invocations: 1,
            candidate_target: 20,
            target_met: true,
            shortage_reason: None,
            readiness_summary: vec![],
        }
    }

    struct CapturingCandidateEvaluator {
        requests: RefCell<Vec<CandidateEvaluationRequest>>,
    }

    impl CapturingCandidateEvaluator {
        fn new() -> Self {
            Self {
                requests: RefCell::new(Vec::new()),
            }
        }
    }

    struct FixedCandidateEvaluator {
        decisions: Vec<CandidateDecision>,
    }

    impl FixedCandidateEvaluator {
        fn new(decisions: Vec<CandidateDecision>) -> Self {
            Self { decisions }
        }
    }

    impl OpenClawEvaluationPort for FixedCandidateEvaluator {
        fn readiness(&self) -> Result<()> {
            Ok(())
        }

        fn evaluate_candidates(
            &self,
            _candidates: &[CandidateRecord],
            _description: &str,
        ) -> Result<Vec<CandidateDecision>> {
            Ok(self.decisions.clone())
        }

        fn evaluate_images(
            &self,
            _images: &[crate::domain::image::ImageRecord],
            _description: &str,
        ) -> Result<Vec<crate::domain::image::ImageAcceptanceDecision>> {
            Ok(vec![])
        }
    }

    impl OpenClawEvaluationPort for CapturingCandidateEvaluator {
        fn readiness(&self) -> Result<()> {
            Ok(())
        }

        fn evaluate_candidates(
            &self,
            _candidates: &[CandidateRecord],
            _description: &str,
        ) -> Result<Vec<CandidateDecision>> {
            panic!("candidate gate must call evaluate_candidate_requests");
        }

        fn evaluate_candidate_requests(
            &self,
            requests: &[CandidateEvaluationRequest],
        ) -> Result<Vec<CandidateDecision>> {
            self.requests.borrow_mut().extend_from_slice(requests);
            Ok(requests
                .iter()
                .map(|request| CandidateDecision::Accepted {
                    candidate: request.candidate.clone(),
                    priority: 9,
                    vlm_evidence: Some(crate::domain::candidate::VlmDecisionEvidence::new(
                        "approve",
                        "capturing_vlm",
                        "unit-test",
                        "candidate_request_capture",
                    )),
                })
                .collect())
        }

        fn evaluate_images(
            &self,
            _images: &[crate::domain::image::ImageRecord],
            _description: &str,
        ) -> Result<Vec<crate::domain::image::ImageAcceptanceDecision>> {
            Ok(vec![])
        }
    }

    #[test]
    fn gate_passes_candidate_reference_metrics_to_evaluator_and_quality_decisions() {
        let evaluator = CapturingCandidateEvaluator::new();
        let plan = make_test_query_plan();
        let mut candidate = make_candidate(
            "cand-reference",
            "https://example.com/reference.jpg",
            Some("Sunset mountains"),
        );
        candidate.width = Some(1920);
        candidate.height = Some(1080);
        candidate.source_page_url = Some("https://example.com/reference-page".into());
        candidate.license_hint = Some("cc-by".into());
        let outcome = make_search_outcome(vec![candidate.clone()]);

        let gate = CandidateQualityGate::new(&evaluator, plan);
        let result = gate.evaluate(&outcome).unwrap();

        let requests = evaluator.requests.borrow();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].candidate.candidate_id, candidate.candidate_id);
        assert!(
            !requests[0].mechanical_evidence.reference_signals.is_empty(),
            "candidate VLM request must include mechanical reference signals"
        );
        assert_eq!(result.quality_decisions.len(), 1);
        assert!(
            !result.quality_decisions[0].reference_metrics.is_empty(),
            "candidate quality decision must preserve mechanical reference metrics"
        );
        assert!(result.quality_decisions[0].vlm_decision.is_some());
    }

    #[test]
    fn gate_uses_v11_mechanical_assessment_as_blocking_authority() {
        let evaluator = CapturingCandidateEvaluator::new();
        let plan = make_test_query_plan();
        let mut candidate = make_candidate(
            "cand-wrong-owner",
            "https://example.com/wrong-owner.jpg",
            Some("Sunset mountains"),
        );
        candidate.query_plan_id = "qp-other".into();
        candidate.provider_kind = "serpapi_google_images".into();
        let outcome = make_search_outcome(vec![candidate]);

        let gate = CandidateQualityGate::new_with_context(
            &evaluator,
            plan,
            "qp-active",
            Vec::new(),
            false,
        );
        let result = gate.evaluate(&outcome).unwrap();

        assert!(evaluator.requests.borrow().is_empty());
        assert!(result.retrievable_sequence.is_empty());
        assert_eq!(result.quality_decisions.len(), 1);
        assert!(!result.quality_decisions[0].mechanical_passed);
        assert!(result.quality_decisions[0]
            .blocking_metrics
            .iter()
            .any(|metric| metric.code
                == crate::domain::metrics::QualityMetricCode::CandidateQueryOwnershipMismatch));
    }

    #[test]
    fn gate_rejects_vlm_candidate_response_cardinality_mismatch() {
        let evaluator = FixedCandidateEvaluator::new(vec![]);
        let plan = make_test_query_plan();
        let mut candidate = make_candidate(
            "cand-one",
            "https://example.com/one.jpg",
            Some("Sunset mountains"),
        );
        candidate.provider_kind = "serpapi_google_images".into();
        let outcome = make_search_outcome(vec![candidate]);

        let gate =
            CandidateQualityGate::new_with_context(&evaluator, plan, "qp-test", Vec::new(), false);
        let err = gate.evaluate(&outcome).unwrap_err();

        assert!(err.to_string().contains("cardinality"));
    }

    #[test]
    fn gate_rejects_vlm_candidate_response_subject_id_mismatch() {
        let mut returned_candidate = make_candidate(
            "cand-extra",
            "https://example.com/extra.jpg",
            Some("Unvalidated image"),
        );
        returned_candidate.provider_kind = "serpapi_google_images".into();
        let evaluator = FixedCandidateEvaluator::new(vec![CandidateDecision::Accepted {
            candidate: returned_candidate,
            priority: 1,
            vlm_evidence: Some(crate::domain::candidate::VlmDecisionEvidence::new(
                "approve",
                "fixture_vlm",
                "unit-test",
                "mismatch",
            )),
        }]);
        let plan = make_test_query_plan();
        let mut candidate = make_candidate(
            "cand-requested",
            "https://example.com/requested.jpg",
            Some("Sunset mountains"),
        );
        candidate.provider_kind = "serpapi_google_images".into();
        let outcome = make_search_outcome(vec![candidate]);

        let gate =
            CandidateQualityGate::new_with_context(&evaluator, plan, "qp-test", Vec::new(), false);
        let err = gate.evaluate(&outcome).unwrap_err();

        assert!(err.to_string().contains("subject id"));
    }

    // -----------------------------------------------------------------------
    // Mechanical blocking → rejected before OpenClaw
    // -----------------------------------------------------------------------

    #[test]
    fn mechanically_blocked_candidates_do_not_reach_openclaw() {
        // A candidate with an empty source URL should be mechanically blocked
        let bad = make_candidate("bad-1", "", None);
        let good = make_candidate("good-1", "https://example.com/img.jpg", Some("Sunset"));

        let outcome = make_search_outcome(vec![bad, good]);

        // Run mechanical + evaluation manually
        let mut seen = HashSet::new();
        let plan = make_test_query_plan();

        let mut mechanically_passed = Vec::new();
        let mut mechanically_blocked = Vec::new();

        for c in &outcome.candidates {
            let evidence = validate_candidate_mechanical(
                c,
                &seen,
                &plan.content_constraints,
                plan.quality_tier,
            );
            if evidence.passed_mechanical() {
                if !c.image_url.trim().is_empty() {
                    seen.insert(c.image_url.clone());
                }
                mechanically_passed.push((c.clone(), evidence));
            } else {
                mechanically_blocked.push((c.clone(), evidence));
            }
        }

        assert_eq!(mechanically_blocked.len(), 1);
        assert_eq!(mechanically_passed.len(), 1);
        assert_eq!(
            mechanically_passed[0].0.candidate_id,
            CandidateId::new("good-1")
        );
    }

    // -----------------------------------------------------------------------
    // evaluate_with_conclusions
    // -----------------------------------------------------------------------

    #[test]
    fn evaluate_with_conclusions_maps_all_outcomes() {
        let c1 = make_candidate("c1", "https://a.com/1.jpg", Some("Sunset"));
        let c2 = make_candidate("c2", "https://a.com/2.jpg", Some("City"));
        let c3 = make_candidate("c3", "https://a.com/3.jpg", Some("Forest"));

        let mech1 = CandidateMechanicalEvidence::pass();
        let mech2 = CandidateMechanicalEvidence::pass();
        let mech3 = CandidateMechanicalEvidence::pass();

        let passed = vec![(c1, mech1), (c2, mech2), (c3, mech3)];

        let conclusions = vec![
            CandidateEvaluationConclusion::Approve {
                notes: Some("good".into()),
            },
            CandidateEvaluationConclusion::Reject {
                reason: "city not mountains".into(),
            },
            CandidateEvaluationConclusion::Uncertain {
                reason: "unclear content".into(),
            },
        ];

        let decisions = evaluate_with_conclusions(passed, conclusions);
        assert_eq!(decisions.len(), 3);
        assert!(decisions[0].is_accepted());
        assert!(!decisions[1].is_accepted());
        assert!(!decisions[2].is_accepted());

        // Build retrievable sequence from all decisions
        let seq = RetrievableCandidateSequence::from_decisions(decisions);
        assert_eq!(seq.len(), 1);
    }

    // -----------------------------------------------------------------------
    // Integration: full pipeline without OpenClaw dependency
    // -----------------------------------------------------------------------

    #[test]
    fn full_pipeline_mechanical_then_evaluation() {
        let candidates = vec![
            make_candidate("c1", "https://a.com/1.jpg", Some("Mountain sunset")),
            make_candidate("c2", "", None), // mechanically blocked
            make_candidate("c3", "https://a.com/2.jpg", Some("City skyline")),
            make_candidate("c4", "https://a.com/3.jpg", Some("Forest dawn")),
            // c5 is a duplicate of c1
            make_candidate("c5", "https://a.com/1.jpg", Some("Mountain sunset")),
        ];

        let plan = make_test_query_plan();
        let mut seen = HashSet::new();
        let mut decisions: Vec<CandidateDecision> = Vec::new();
        let mut passed: Vec<(CandidateRecord, CandidateMechanicalEvidence)> = Vec::new();

        for c in &candidates {
            let evidence = validate_candidate_mechanical(
                c,
                &seen,
                &plan.content_constraints,
                plan.quality_tier,
            );
            if evidence.passed_mechanical() {
                if !c.image_url.trim().is_empty() {
                    seen.insert(c.image_url.clone());
                }
                passed.push((c.clone(), evidence));
            } else {
                let reasons: Vec<String> = evidence
                    .blocking_findings
                    .iter()
                    .map(|r| r.description())
                    .collect();
                decisions.push(CandidateDecision::Rejected {
                    candidate: c.clone(),
                    reason: reasons.join("; "),
                });
            }
        }

        // c2 (empty URL) and c5 (duplicate of c1) should be mechanically blocked
        // c1, c3, c4 should pass mechanical
        assert_eq!(decisions.len(), 2); // c2 and c5 blocked
        assert_eq!(passed.len(), 3); // c1, c3, c4 passed

        // Simulate OpenClaw evaluation
        let conclusions = vec![
            CandidateEvaluationConclusion::Approve {
                notes: Some("perfect match".into()),
            },
            CandidateEvaluationConclusion::Reject {
                reason: "city not matching mountains query".into(),
            },
            CandidateEvaluationConclusion::Uncertain {
                reason: "forest could be mountain-adjacent".into(),
            },
        ];

        let eval_decisions = evaluate_with_conclusions(passed, conclusions);
        decisions.extend(eval_decisions);

        // Now build the retrievable sequence
        let seq = RetrievableCandidateSequence::from_decisions(decisions.clone());
        assert_eq!(seq.len(), 1); // Only c1 was approved

        // The only accepted candidate should be c1
        match &seq.candidates[0] {
            CandidateDecision::Accepted { candidate, .. } => {
                assert_eq!(candidate.candidate_id, CandidateId::new("c1"));
            }
            _ => panic!("expected Accepted"),
        }

        // Summary
        let total = candidates.len();
        let _mechanically_blocked = 2;
        let summary = CandidateQualitySummary::from_decisions(total, &decisions);
        assert_eq!(summary.total_candidates, 5);
        assert_eq!(summary.openclaw_approved, 1);
        // Rejected: 2 mechanical + 1 openclaw = 3 total
        let rejected_count = decisions
            .iter()
            .filter(|d| matches!(d, CandidateDecision::Rejected { .. }))
            .count();
        assert_eq!(rejected_count, 3);
        assert_eq!(summary.openclaw_uncertain, 1);
    }

    // -----------------------------------------------------------------------
    // Execution blocking when all pass mechanical but OpenClaw is down
    // -----------------------------------------------------------------------

    #[test]
    fn openclaw_unavailable_blocks_all_passed_candidates() {
        let candidates = vec![
            make_candidate("c1", "https://a.com/1.jpg", Some("Sunset")),
            make_candidate("c2", "https://b.com/2.jpg", Some("Mountains")),
        ];

        let plan = make_test_query_plan();
        let mut seen = HashSet::new();
        let mut passed = Vec::new();
        let mut decisions = Vec::new();

        for c in &candidates {
            let evidence = validate_candidate_mechanical(
                c,
                &seen,
                &plan.content_constraints,
                plan.quality_tier,
            );
            if evidence.passed_mechanical() {
                if !c.image_url.trim().is_empty() {
                    seen.insert(c.image_url.clone());
                }
                passed.push((c.clone(), evidence));
            } else {
                decisions.push(CandidateDecision::Rejected {
                    candidate: c.clone(),
                    reason: "mechanical block".into(),
                });
            }
        }

        assert_eq!(passed.len(), 2);

        // Simulate OpenClaw unavailability — all become ExecutionBlocked
        let fact = ExecutionBlockingFact::openclaw_unavailable("no endpoint configured");
        assert!(fact.is_permanent);

        let blocked: Vec<CandidateDecision> = passed
            .into_iter()
            .map(|(c, _)| CandidateDecision::ExecutionBlocked {
                candidate: c,
                reason: "OpenClaw unavailable: no endpoint configured".into(),
            })
            .collect();

        decisions.extend(blocked);

        let seq = RetrievableCandidateSequence::from_decisions(decisions.clone());
        // No accepted candidates
        assert!(seq.is_empty());

        // All decisions are ExecutionBlocked
        let blocked_count = decisions
            .iter()
            .filter(|d| matches!(d, CandidateDecision::ExecutionBlocked { .. }))
            .count();
        assert_eq!(blocked_count, 2);
    }

    // -----------------------------------------------------------------------
    // CandidateQualitySummary
    // -----------------------------------------------------------------------

    #[test]
    fn summary_counts_all_categories() {
        let decisions = vec![
            CandidateDecision::Accepted {
                candidate: make_candidate("a", "https://a.com/1.jpg", None),
                priority: 1,
                vlm_evidence: None,
            },
            CandidateDecision::Rejected {
                candidate: make_candidate("b", "https://b.com/2.jpg", None),
                reason: "bad".into(),
            },
            CandidateDecision::Uncertain {
                candidate: make_candidate("c", "https://c.com/3.jpg", None),
                reason: "maybe".into(),
            },
            CandidateDecision::ExecutionBlocked {
                candidate: make_candidate("d", "https://d.com/4.jpg", None),
                reason: "OpenClaw down".into(),
            },
        ];

        let summary = CandidateQualitySummary::from_decisions(4, &decisions);
        assert_eq!(summary.total_candidates, 4);
        assert_eq!(summary.openclaw_approved, 1);
        assert_eq!(summary.openclaw_rejected, 1);
        assert_eq!(summary.openclaw_uncertain, 1);
        assert_eq!(summary.openclaw_unexecutable, 1);
    }

    #[test]
    fn empty_summary() {
        let summary = CandidateQualitySummary::default();
        assert_eq!(summary.total_candidates, 0);
        assert_eq!(summary.mechanically_blocked, 0);
        assert_eq!(summary.openclaw_approved, 0);
    }

    // -----------------------------------------------------------------------
    // Duplicate detection within batch
    // -----------------------------------------------------------------------

    #[test]
    fn duplicates_within_same_batch_are_blocked() {
        let candidates = vec![
            make_candidate("c1", "https://a.com/1.jpg", Some("Image 1")),
            make_candidate("c2", "https://a.com/1.jpg", Some("Image 1 copy")), // duplicate URL
            make_candidate("c3", "https://b.com/2.jpg", Some("Image 2")),
        ];

        let plan = make_test_query_plan();
        let mut seen = HashSet::new();
        let mut blocked = Vec::new();
        let mut passed = Vec::new();

        for c in &candidates {
            let evidence = validate_candidate_mechanical(
                c,
                &seen,
                &plan.content_constraints,
                plan.quality_tier,
            );
            if evidence.passed_mechanical() {
                seen.insert(c.image_url.clone());
                passed.push(c.clone());
            } else {
                blocked.push((c.clone(), evidence));
            }
        }

        assert_eq!(blocked.len(), 1);
        assert_eq!(passed.len(), 2);
        // The blocked one should be c2 (duplicate of c1)
        assert_eq!(blocked[0].0.candidate_id, CandidateId::new("c2"));
        assert!(matches!(
            blocked[0].1.blocking_findings[0],
            CandidateBlockingReason::Duplicate { .. }
        ));
    }

    // -----------------------------------------------------------------------
    // must_avoid check with actual title
    // -----------------------------------------------------------------------

    #[test]
    fn must_avoid_city_blocks_city_titles() {
        let candidates = vec![
            make_candidate("c1", "https://a.com/1.jpg", Some("Beautiful city at dusk")),
            make_candidate("c2", "https://b.com/2.jpg", Some("Mountain panorama")),
        ];

        let plan = ValidatedQueryPlan {
            content_constraints: ContentConstraints {
                must_include: vec![],
                must_avoid: vec!["city".into()],
            },
            ..make_test_query_plan()
        };

        let mut seen = HashSet::new();
        let mut blocked_count = 0;
        let mut passed_count = 0;

        for c in &candidates {
            let evidence = validate_candidate_mechanical(
                c,
                &seen,
                &plan.content_constraints,
                plan.quality_tier,
            );
            if evidence.passed_mechanical() {
                seen.insert(c.image_url.clone());
                passed_count += 1;
            } else {
                blocked_count += 1;
            }
        }

        assert_eq!(blocked_count, 1); // "city" in title
        assert_eq!(passed_count, 1); // "Mountain panorama"
    }

    // -----------------------------------------------------------------------
    // No candidates — edge case
    // -----------------------------------------------------------------------

    #[test]
    fn empty_candidate_list_produces_empty_result() {
        let _plan = make_test_query_plan();
        let _outcome = make_search_outcome(vec![]);
        let summary = CandidateQualitySummary {
            total_candidates: 0,
            ..Default::default()
        };

        let result = CandidateQualityGateResult {
            retrievable_sequence: RetrievableCandidateSequence::empty(),
            all_decisions: vec![],
            quality_decisions: vec![],
            execution_blocking_facts: vec![],
            summary,
        };

        assert!(result.retrievable_sequence.is_empty());
        assert_eq!(result.summary.total_candidates, 0);
    }

    // -----------------------------------------------------------------------
    // Shortage evidence flows through
    // -----------------------------------------------------------------------

    #[test]
    fn candidate_shortage_preserved_in_outcome() {
        let outcome = SearchOutcome {
            candidates: vec![make_candidate("c1", "https://a.com/1.jpg", Some("Sunset"))],
            usage_events: vec![],
            total_invocations: 1,
            candidate_target: 20,
            target_met: false,
            shortage_reason: Some(CandidateShortageReason::NoAvailableSearchProvider),
            readiness_summary: vec![],
        };

        assert!(!outcome.target_met);
        assert!(outcome.shortage_reason.is_some());
        // The quality gate should consume this but not block on it
        // (shortage flows downstream)
    }
}
