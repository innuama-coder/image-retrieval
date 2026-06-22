#![allow(deprecated)]
//! Task orchestrator — drives the image retrieval lifecycle.
//!
//! Implements the state machine defined in
//! `docs/design/TASK-006-image-acceptance-orchestrator-design.md`:
//!
//! ```text
//! InputValidation → Running ↔ Retry → FullDelivery | LimitedDelivery | ExecutionBlocked
//! ```
//!
//! The orchestrator owns the attempt counters, accumulates qualified and
//! rejected images across attempts, and produces the final `DeliveryDecision`.
//!
//! References: PRD §用户旅程与核心流程, HLD §Task Orchestrator,
//! `docs/design/TASK-006-image-acceptance-orchestrator-design.md`

use crate::domain::delivery::DeliveryDecision;
use crate::domain::image::ImageAcceptanceDecision;
use crate::domain::metrics::{MetricEvent, MetricKind};
use crate::domain::query_plan::TaskPlan;
use crate::error::{Diagnostic, DiagnosticItem, DiagnosticLevel, Error, Result};
use crate::ports::OpenClawEvaluationPort;
use crate::quality::image::gate::{ImageAcceptanceGate, ImageAcceptanceGateResult};

// ---------------------------------------------------------------------------
// Orchestrator state
// ---------------------------------------------------------------------------

/// States of the task orchestrator state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrchestratorState {
    InputValidation,
    InputRejected,
    Running,
    Retry,
    FullDelivery,
    LimitedDelivery,
    ExecutionBlocked,
}

impl OrchestratorState {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::InputRejected
                | Self::FullDelivery
                | Self::LimitedDelivery
                | Self::ExecutionBlocked
        )
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::InputValidation => "input_validation",
            Self::InputRejected => "input_rejected",
            Self::Running => "running",
            Self::Retry => "retry",
            Self::FullDelivery => "full_delivery",
            Self::LimitedDelivery => "limited_delivery",
            Self::ExecutionBlocked => "execution_blocked",
        }
    }
}

// ---------------------------------------------------------------------------
// Attempt counter
// ---------------------------------------------------------------------------

/// Tracks full attempts and retries per the LLD specification.
///
/// | Counter | Meaning |
/// |---|---|
/// | `full_attempt_count` | Total full attempts including the initial one. |
/// | `retry_count` | Retries beyond the initial attempt (0..retry_limit). |
#[derive(Debug, Clone, Copy)]
pub struct AttemptCounter {
    pub full_attempt_count: u32,
    pub retry_count: u32,
    pub retry_limit: u32,
}

impl AttemptCounter {
    pub fn new(retry_limit: u32) -> Self {
        Self {
            full_attempt_count: 1,
            retry_count: 0,
            retry_limit,
        }
    }

    pub fn record_retry(&mut self) -> bool {
        if self.retry_count >= self.retry_limit {
            return false;
        }
        self.retry_count += 1;
        self.full_attempt_count += 1;
        true
    }

    pub fn can_retry(&self) -> bool {
        self.retry_count < self.retry_limit
    }

    pub fn is_exhausted(&self) -> bool {
        !self.can_retry()
    }
}

// ---------------------------------------------------------------------------
// Task orchestrator
// ---------------------------------------------------------------------------

pub struct TaskOrchestrator<'a> {
    task_plan: TaskPlan,
    state: OrchestratorState,
    counter: AttemptCounter,
    qualified_images: Vec<ImageAcceptanceDecision>,
    rejected_images: Vec<ImageAcceptanceDecision>,
    execution_block_reason: Option<String>,
    image_gate: ImageAcceptanceGate<'a>,
    diagnostics: Vec<DiagnosticItem>,
    metric_events: Vec<MetricEvent>,
}

impl<'a> TaskOrchestrator<'a> {
    pub fn new(task_plan: TaskPlan, openclaw: &'a dyn OpenClawEvaluationPort) -> Self {
        let retry_limit = task_plan.query_plan.retry_limit;
        let query_plan = task_plan.query_plan.clone();
        let image_gate = ImageAcceptanceGate::new(openclaw, query_plan);

        Self {
            task_plan,
            state: OrchestratorState::Running,
            counter: AttemptCounter::new(retry_limit),
            qualified_images: Vec::new(),
            rejected_images: Vec::new(),
            execution_block_reason: None,
            image_gate,
            diagnostics: Vec::new(),
            metric_events: Vec::new(),
        }
    }

    // --- State accessors ---

    pub fn state(&self) -> OrchestratorState {
        self.state
    }

    pub fn counter(&self) -> &AttemptCounter {
        &self.counter
    }

    pub fn qualified_count(&self) -> usize {
        self.qualified_images
            .iter()
            .filter(|d| d.is_accepted())
            .count()
    }

    pub fn required_count(&self) -> u32 {
        self.task_plan.query_plan.required_count
    }

    pub fn qualified_images(&self) -> &[ImageAcceptanceDecision] {
        &self.qualified_images
    }

    pub fn rejected_images(&self) -> &[ImageAcceptanceDecision] {
        &self.rejected_images
    }

    pub fn execution_block_reason(&self) -> Option<&str> {
        self.execution_block_reason.as_deref()
    }

    pub fn diagnostics(&self) -> &[DiagnosticItem] {
        &self.diagnostics
    }

    pub fn metric_events(&self) -> &[MetricEvent] {
        &self.metric_events
    }

    pub fn task_plan(&self) -> &TaskPlan {
        &self.task_plan
    }

    // --- State transitions ---

    pub fn reject_input(&mut self, reason: impl Into<String>) {
        self.state = OrchestratorState::InputRejected;
        self.diagnostics.push(DiagnosticItem {
            level: DiagnosticLevel::Error,
            category: "input_rejection".into(),
            message: reason.into(),
        });
        self.emit_task_outcome_event();
    }

    /// Process the output of an image acceptance gate invocation.
    pub fn process_image_acceptance(
        &mut self,
        gate_result: ImageAcceptanceGateResult,
    ) -> Result<OrchestratorState> {
        // Check for execution block from image acceptance
        if !gate_result.execution_blocking_facts.is_empty() {
            let fact = &gate_result.execution_blocking_facts[0];
            self.state = OrchestratorState::ExecutionBlocked;
            self.execution_block_reason = Some(fact.reason.clone());
            self.diagnostics.push(DiagnosticItem {
                level: DiagnosticLevel::Error,
                category: "execution_blocked".into(),
                message: format!("OpenClaw image evaluation unavailable: {}", fact.reason),
            });
            self.emit_task_outcome_event();
            return Ok(self.state);
        }

        // Accumulate qualified and rejected images
        for decision in &gate_result.all_decisions {
            match decision {
                ImageAcceptanceDecision::Accepted { .. } => {
                    self.qualified_images.push(decision.clone());
                }
                ImageAcceptanceDecision::MechanicallyRejected { .. }
                | ImageAcceptanceDecision::SubjectivelyRejected { .. } => {
                    self.rejected_images.push(decision.clone());
                }
                ImageAcceptanceDecision::ExecutionBlocked { reason } => {
                    self.state = OrchestratorState::ExecutionBlocked;
                    self.execution_block_reason = Some(reason.clone());
                    self.diagnostics.push(DiagnosticItem {
                        level: DiagnosticLevel::Error,
                        category: "execution_blocked".into(),
                        message: format!(
                            "OpenClaw image evaluation returned execution-blocked: {}",
                            reason
                        ),
                    });
                    self.emit_task_outcome_event();
                    return Ok(self.state);
                }
            }
        }

        // Record diagnostics for this attempt
        self.diagnostics.push(DiagnosticItem {
            level: DiagnosticLevel::Info,
            category: "image_acceptance".into(),
            message: format!(
                "attempt {}: {} qualified, {} rejected (mechanical: {}, approved: {}, rejected: {}, uncertain: {})",
                self.counter.full_attempt_count,
                self.qualified_count(),
                self.rejected_images.len(),
                gate_result.summary.mechanically_blocked,
                gate_result.summary.openclaw_approved,
                gate_result.summary.openclaw_rejected,
                gate_result.summary.openclaw_uncertain,
            ),
        });

        self.emit_rejection_events(&gate_result);

        // Decide next state
        let qualified_count = self.qualified_count() as u32;
        let required = self.required_count();

        if qualified_count >= required {
            self.state = OrchestratorState::FullDelivery;
            self.diagnostics.push(DiagnosticItem {
                level: DiagnosticLevel::Info,
                category: "delivery_decision".into(),
                message: format!(
                    "full delivery: {} qualified images meet requirement of {} after {} attempt(s)",
                    qualified_count, required, self.counter.full_attempt_count,
                ),
            });
            self.emit_task_outcome_event();
            self.emit_qualified_achievement_event();
        } else if self.counter.can_retry() {
            self.state = OrchestratorState::Retry;
            self.diagnostics.push(DiagnosticItem {
                level: DiagnosticLevel::Warning,
                category: "retry".into(),
                message: format!(
                    "retry needed: {} of {} qualified after attempt {} (retry {}/{})",
                    qualified_count,
                    required,
                    self.counter.full_attempt_count,
                    self.counter.retry_count + 1,
                    self.counter.retry_limit,
                ),
            });
        } else {
            self.state = OrchestratorState::LimitedDelivery;
            let shortfall = required.saturating_sub(qualified_count);
            self.diagnostics.push(DiagnosticItem {
                level: DiagnosticLevel::Warning,
                category: "delivery_decision".into(),
                message: format!(
                    "limited delivery: {} of {} qualified after {} attempt(s) (retries exhausted); shortfall of {}",
                    qualified_count, required, self.counter.full_attempt_count, shortfall,
                ),
            });
            self.emit_task_outcome_event();
            self.emit_qualified_achievement_event();
        }

        Ok(self.state)
    }

    pub fn record_retry(&mut self) -> Result<()> {
        if !self.counter.record_retry() {
            return Err(Error::execution_blocked(
                "retry limit exhausted — cannot retry",
            ));
        }
        self.state = OrchestratorState::Running;
        self.diagnostics.push(DiagnosticItem {
            level: DiagnosticLevel::Info,
            category: "retry".into(),
            message: format!(
                "starting retry attempt {} (retry {}/{})",
                self.counter.full_attempt_count, self.counter.retry_count, self.counter.retry_limit,
            ),
        });
        Ok(())
    }

    pub fn block_execution(&mut self, reason: impl Into<String>) {
        self.state = OrchestratorState::ExecutionBlocked;
        let reason = reason.into();
        self.execution_block_reason = Some(reason.clone());
        self.diagnostics.push(DiagnosticItem {
            level: DiagnosticLevel::Error,
            category: "execution_blocked".into(),
            message: reason,
        });
        self.emit_task_outcome_event();
    }

    pub fn can_retry(&self) -> bool {
        self.counter.can_retry() && self.state == OrchestratorState::Retry
    }

    // --- Delivery decision ---

    pub fn build_delivery_decision(&self) -> DeliveryDecision {
        match self.state {
            OrchestratorState::FullDelivery => DeliveryDecision::full_delivery(
                self.qualified_images.clone(),
                self.rejected_images.clone(),
                self.counter.full_attempt_count,
                self.counter.retry_count,
            ),
            OrchestratorState::LimitedDelivery => DeliveryDecision::limited_delivery(
                self.qualified_images.clone(),
                self.rejected_images.clone(),
                self.counter.full_attempt_count,
                self.counter.retry_count,
                self.required_count(),
            ),
            OrchestratorState::ExecutionBlocked => DeliveryDecision::execution_blocked(
                self.execution_block_reason
                    .clone()
                    .unwrap_or_else(|| "unknown reason".into()),
            ),
            OrchestratorState::InputRejected => DeliveryDecision::input_rejected(
                self.execution_block_reason
                    .clone()
                    .unwrap_or_else(|| "input rejected".into()),
            ),
            OrchestratorState::InputValidation | OrchestratorState::Running => {
                DeliveryDecision::execution_blocked("task not completed".into())
            }
            OrchestratorState::Retry => {
                DeliveryDecision::execution_blocked("task still retrying".into())
            }
        }
    }

    pub fn build_diagnostic(&self) -> Diagnostic {
        let status = self.state.label().to_string();
        let summary = match self.state {
            OrchestratorState::FullDelivery => format!(
                "Full delivery: {} qualified images delivered after {} attempt(s).",
                self.qualified_count(),
                self.counter.full_attempt_count,
            ),
            OrchestratorState::LimitedDelivery => {
                let shortfall = self
                    .required_count()
                    .saturating_sub(self.qualified_count() as u32);
                format!(
                    "Limited delivery: {} of {} required images delivered after {} attempt(s); shortfall of {}.",
                    self.qualified_count(),
                    self.required_count(),
                    self.counter.full_attempt_count,
                    shortfall,
                )
            }
            OrchestratorState::ExecutionBlocked => format!(
                "Execution blocked: {}",
                self.execution_block_reason
                    .as_deref()
                    .unwrap_or("unknown reason")
            ),
            OrchestratorState::InputRejected => format!(
                "Input rejected: {}",
                self.execution_block_reason
                    .as_deref()
                    .unwrap_or("unknown reason")
            ),
            _ => format!(
                "Task in progress: {} (attempt {}/{})",
                self.state.label(),
                self.counter.full_attempt_count,
                self.counter.retry_limit + 1,
            ),
        };

        Diagnostic {
            status,
            summary,
            items: self.diagnostics.clone(),
        }
    }

    // --- Image acceptance gate access ---

    pub fn image_gate(&self) -> &ImageAcceptanceGate<'a> {
        &self.image_gate
    }

    pub fn accept_images(
        &mut self,
        images: &[crate::domain::image::ImageRecord],
    ) -> Result<OrchestratorState> {
        let gate_result = self.image_gate.evaluate(images)?;
        self.process_image_acceptance(gate_result)
    }

    // --- Metric events ---

    fn emit_task_outcome_event(&mut self) {
        let outcome_label = self.state.label();
        self.metric_events.push(
            MetricEvent::new(
                MetricKind::TaskOutcome,
                format!("task_outcome_{}", outcome_label),
                1.0,
            )
            .with_meta("state", outcome_label),
        );
    }

    fn emit_qualified_achievement_event(&mut self) {
        let qualified = self.qualified_count() as f64;
        let required = self.required_count() as f64;
        self.metric_events.push(
            MetricEvent::new(
                MetricKind::QualifiedImageAchievement,
                "qualified_image_achievement",
                qualified,
            )
            .with_denominator(required),
        );
    }

    fn emit_rejection_events(&mut self, gate_result: &ImageAcceptanceGateResult) {
        if gate_result.summary.mechanically_blocked > 0 {
            self.metric_events.push(
                MetricEvent::new(
                    MetricKind::RejectionReason,
                    "image_mechanical_rejection",
                    gate_result.summary.mechanically_blocked as f64,
                )
                .with_meta("reason", "mechanical"),
            );
        }
        if gate_result.summary.openclaw_rejected > 0 {
            self.metric_events.push(
                MetricEvent::new(
                    MetricKind::RejectionReason,
                    "image_openclaw_rejection",
                    gate_result.summary.openclaw_rejected as f64,
                )
                .with_meta("reason", "openclaw_rejected"),
            );
        }
        if gate_result.summary.openclaw_uncertain > 0 {
            self.metric_events.push(
                MetricEvent::new(
                    MetricKind::RejectionReason,
                    "image_openclaw_uncertain",
                    gate_result.summary.openclaw_uncertain as f64,
                )
                .with_meta("reason", "openclaw_uncertain"),
            );
        }
        if gate_result.summary.openclaw_approved > 0
            || gate_result.summary.openclaw_rejected > 0
            || gate_result.summary.openclaw_uncertain > 0
        {
            let total_evaluated = (gate_result.summary.openclaw_approved
                + gate_result.summary.openclaw_rejected
                + gate_result.summary.openclaw_uncertain) as f64;
            self.metric_events.push(
                MetricEvent::new(
                    MetricKind::OpenClawEvaluationRate,
                    "image_openclaw_pass_rate",
                    gate_result.summary.openclaw_approved as f64,
                )
                .with_denominator(total_evaluated),
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Convenience: input validation entry point
// ---------------------------------------------------------------------------

pub fn validate_and_create_orchestrator<'a>(
    input: crate::domain::query_plan::QueryPlanInput,
    openclaw: &'a dyn OpenClawEvaluationPort,
) -> std::result::Result<TaskOrchestrator<'a>, DeliveryDecision> {
    use crate::domain::query_plan::validate_query_plan;

    match validate_query_plan(input) {
        crate::domain::query_plan::ValidationOutcome::Valid { plan, .. } => {
            let task_plan = TaskPlan::from_validated(plan);
            Ok(TaskOrchestrator::new(task_plan, openclaw))
        }
        crate::domain::query_plan::ValidationOutcome::Rejected(rejection) => {
            Err(DeliveryDecision::input_rejected(rejection.summary))
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::candidate::ImageDimensions;
    use crate::domain::delivery::TaskStatus;
    use crate::domain::image::{ImageMechanicalEvidence, ImageRecord};
    use crate::domain::query_plan::{
        AuthorizationPreference, ContentConstraints, OutputPreference, QualityTier, QueryPlanInput,
        ValidatedQueryPlan,
    };
    use crate::quality::image::evaluation::ImageEvaluationConclusion;
    use crate::quality::image::gate::evaluate_images_with_conclusions;
    use std::cell::RefCell;

    // -----------------------------------------------------------------------
    // Fixture OpenClaw evaluator
    // -----------------------------------------------------------------------

    struct FixtureImageEvaluator {
        conclusions: RefCell<Vec<ImageEvaluationConclusion>>,
    }

    impl FixtureImageEvaluator {
        fn new(conclusions: Vec<ImageEvaluationConclusion>) -> Self {
            Self {
                conclusions: RefCell::new(conclusions),
            }
        }
    }

    impl OpenClawEvaluationPort for FixtureImageEvaluator {
        fn readiness(&self) -> Result<()> {
            Ok(())
        }

        fn evaluate_candidates(
            &self,
            _candidates: &[crate::domain::candidate::CandidateRecord],
            _description: &str,
        ) -> Result<Vec<crate::domain::candidate::CandidateDecision>> {
            Ok(vec![])
        }

        fn evaluate_images(
            &self,
            images: &[ImageRecord],
            _description: &str,
        ) -> Result<Vec<ImageAcceptanceDecision>> {
            let conclusions = self.conclusions.borrow().clone();
            let mech = ImageMechanicalEvidence {
                blocking_findings: vec![],
                reference_findings: vec![],
            };

            let passed: Vec<(ImageRecord, ImageMechanicalEvidence)> = images
                .iter()
                .cloned()
                .map(|img| (img, mech.clone()))
                .collect();

            Ok(evaluate_images_with_conclusions(passed, conclusions))
        }
    }

    struct UnavailableEvaluator;

    impl OpenClawEvaluationPort for UnavailableEvaluator {
        fn readiness(&self) -> Result<()> {
            Err(Error::openclaw_unavailable(
                "no production endpoint configured",
            ))
        }

        fn evaluate_candidates(
            &self,
            _candidates: &[crate::domain::candidate::CandidateRecord],
            _description: &str,
        ) -> Result<Vec<crate::domain::candidate::CandidateDecision>> {
            Err(Error::openclaw_unavailable(
                "no production endpoint configured",
            ))
        }

        fn evaluate_images(
            &self,
            _images: &[ImageRecord],
            _description: &str,
        ) -> Result<Vec<ImageAcceptanceDecision>> {
            Err(Error::openclaw_unavailable(
                "no production endpoint configured",
            ))
        }
    }

    // -----------------------------------------------------------------------
    // Test helpers
    // -----------------------------------------------------------------------

    fn make_image(id: &str, width: u32, height: u32) -> ImageRecord {
        ImageRecord {
            candidate_id: id.into(),
            local_path: format!("/tmp/{}.jpg", id),
            content_type: Some("image/jpeg".into()),
            file_size_bytes: 4096,
            dimensions: Some(ImageDimensions { width, height }),
        }
    }

    fn make_task_plan(required_count: u32) -> TaskPlan {
        let plan = ValidatedQueryPlan {
            description: "sunset over mountains".into(),
            required_count,
            quality_tier: QualityTier::General,
            content_constraints: ContentConstraints::default(),
            authorization_preference: AuthorizationPreference::Default,
            output_preference: OutputPreference::Human,
            retry_limit: 3,
        };
        TaskPlan::from_validated(plan)
    }

    fn make_approve_conclusion() -> ImageEvaluationConclusion {
        ImageEvaluationConclusion::Approve {
            notes: Some("good match".into()),
        }
    }

    fn make_reject_conclusion() -> ImageEvaluationConclusion {
        ImageEvaluationConclusion::Reject {
            reason: "not matching".into(),
        }
    }

    fn make_uncertain_conclusion() -> ImageEvaluationConclusion {
        ImageEvaluationConclusion::Uncertain {
            reason: "ambiguous".into(),
        }
    }

    // -----------------------------------------------------------------------
    // Full delivery tests
    // -----------------------------------------------------------------------

    #[test]
    fn full_delivery_when_qualified_images_meet_requirement() {
        let task_plan = make_task_plan(2);
        let evaluator =
            FixtureImageEvaluator::new(vec![make_approve_conclusion(), make_approve_conclusion()]);
        let mut orchestrator = TaskOrchestrator::new(task_plan, &evaluator);

        let images = vec![make_image("img-1", 800, 600), make_image("img-2", 800, 600)];
        let state = orchestrator.accept_images(&images).unwrap();

        assert_eq!(state, OrchestratorState::FullDelivery);
        assert_eq!(orchestrator.qualified_count(), 2);
        assert_eq!(orchestrator.counter().full_attempt_count, 1);
        assert_eq!(orchestrator.counter().retry_count, 0);

        let decision = orchestrator.build_delivery_decision();
        assert_eq!(decision.status, TaskStatus::FullDelivery);
        assert!(decision.shortfall_reason.is_none());
    }

    #[test]
    fn immediate_full_delivery_when_requirement_met_on_first_attempt() {
        let task_plan = make_task_plan(1);
        let evaluator = FixtureImageEvaluator::new(vec![make_approve_conclusion()]);
        let mut orchestrator = TaskOrchestrator::new(task_plan, &evaluator);

        let images = vec![make_image("img-1", 800, 600)];
        let state = orchestrator.accept_images(&images).unwrap();

        assert_eq!(state, OrchestratorState::FullDelivery);
        assert_eq!(orchestrator.counter().full_attempt_count, 1);
        assert_eq!(orchestrator.counter().retry_count, 0);
    }

    // -----------------------------------------------------------------------
    // Dual pass tests
    // -----------------------------------------------------------------------

    #[test]
    fn only_mechanical_and_openclaw_dual_pass_counts_as_qualified() {
        let task_plan = make_task_plan(2);
        let evaluator =
            FixtureImageEvaluator::new(vec![make_approve_conclusion(), make_reject_conclusion()]);
        let mut orchestrator = TaskOrchestrator::new(task_plan, &evaluator);

        let images = vec![make_image("img-1", 800, 600), make_image("img-2", 800, 600)];
        let state = orchestrator.accept_images(&images).unwrap();

        assert_eq!(state, OrchestratorState::Retry);
        assert_eq!(orchestrator.qualified_count(), 1);
    }

    #[test]
    fn openclaw_uncertain_does_not_count_as_qualified() {
        let task_plan = make_task_plan(1);
        let evaluator = FixtureImageEvaluator::new(vec![make_uncertain_conclusion()]);
        let mut orchestrator = TaskOrchestrator::new(task_plan, &evaluator);

        let images = vec![make_image("img-1", 800, 600)];
        let state = orchestrator.accept_images(&images).unwrap();

        assert_eq!(state, OrchestratorState::Retry);
        assert_eq!(orchestrator.qualified_count(), 0);
    }

    // -----------------------------------------------------------------------
    // Execution blocked tests
    // -----------------------------------------------------------------------

    #[test]
    fn openclaw_unavailable_enters_execution_blocked() {
        let task_plan = make_task_plan(2);
        let evaluator = UnavailableEvaluator;
        let mut orchestrator = TaskOrchestrator::new(task_plan, &evaluator);

        let images = vec![make_image("img-1", 800, 600)];
        let state = orchestrator.accept_images(&images).unwrap();

        assert_eq!(state, OrchestratorState::ExecutionBlocked);
        assert!(orchestrator.execution_block_reason().is_some());

        let decision = orchestrator.build_delivery_decision();
        assert_eq!(decision.status, TaskStatus::ExecutionBlocked);
    }

    #[test]
    fn execution_blocked_by_retrieval_fact() {
        let task_plan = make_task_plan(2);
        let evaluator = FixtureImageEvaluator::new(vec![]);
        let mut orchestrator = TaskOrchestrator::new(task_plan, &evaluator);

        orchestrator.block_execution("all retrieval channels blocked by access restriction");
        assert_eq!(orchestrator.state(), OrchestratorState::ExecutionBlocked);
        assert!(orchestrator.execution_block_reason().is_some());

        let decision = orchestrator.build_delivery_decision();
        assert_eq!(decision.status, TaskStatus::ExecutionBlocked);
    }

    // -----------------------------------------------------------------------
    // Retry and limited delivery tests
    // -----------------------------------------------------------------------

    #[test]
    fn insufficient_triggers_retry() {
        let task_plan = make_task_plan(2);
        let evaluator =
            FixtureImageEvaluator::new(vec![make_approve_conclusion(), make_reject_conclusion()]);
        let mut orchestrator = TaskOrchestrator::new(task_plan, &evaluator);

        let images = vec![make_image("img-1", 800, 600), make_image("img-2", 800, 600)];
        let state = orchestrator.accept_images(&images).unwrap();
        assert_eq!(state, OrchestratorState::Retry);
        assert_eq!(orchestrator.qualified_count(), 1);
        assert_eq!(orchestrator.counter().full_attempt_count, 1);
    }

    #[test]
    fn retry_increments_both_counters() {
        let task_plan = make_task_plan(3);
        let evaluator = FixtureImageEvaluator::new(vec![make_approve_conclusion()]);
        let mut orchestrator = TaskOrchestrator::new(task_plan, &evaluator);

        let images = vec![make_image("img-1", 800, 600)];
        let state = orchestrator.accept_images(&images).unwrap();
        assert_eq!(state, OrchestratorState::Retry);

        orchestrator.record_retry().unwrap();
        assert_eq!(orchestrator.counter().full_attempt_count, 2);
        assert_eq!(orchestrator.counter().retry_count, 1);

        orchestrator.record_retry().unwrap();
        assert_eq!(orchestrator.counter().full_attempt_count, 3);
        assert_eq!(orchestrator.counter().retry_count, 2);

        orchestrator.record_retry().unwrap();
        assert_eq!(orchestrator.counter().full_attempt_count, 4);
        assert_eq!(orchestrator.counter().retry_count, 3);
        assert!(!orchestrator.counter().can_retry());
        assert!(orchestrator.counter().is_exhausted());
    }

    #[test]
    fn limited_delivery_when_insufficient_after_all_retries() {
        let task_plan = make_task_plan(5);
        let evaluator = FixtureImageEvaluator::new(vec![make_approve_conclusion()]);
        let mut orchestrator = TaskOrchestrator::new(task_plan, &evaluator);

        for i in 0..4 {
            if i > 0 {
                orchestrator.record_retry().unwrap();
            }
            let img = make_image(&format!("img-{}", i + 1), 800, 600);
            let state = orchestrator.accept_images(&[img]).unwrap();
            if i < 3 {
                assert_eq!(state, OrchestratorState::Retry);
            } else {
                assert_eq!(state, OrchestratorState::LimitedDelivery);
            }
        }

        assert_eq!(orchestrator.counter().full_attempt_count, 4);
        assert_eq!(orchestrator.counter().retry_count, 3);
        assert!(orchestrator.counter().is_exhausted());
        assert_eq!(orchestrator.qualified_count(), 4);

        let decision = orchestrator.build_delivery_decision();
        assert_eq!(decision.status, TaskStatus::LimitedDelivery);
        assert!(decision.shortfall_reason.is_some());
    }

    #[test]
    fn limited_delivery_can_be_zero_images() {
        let task_plan = make_task_plan(2);
        let evaluator =
            FixtureImageEvaluator::new(vec![make_reject_conclusion(), make_reject_conclusion()]);
        let mut orchestrator = TaskOrchestrator::new(task_plan, &evaluator);

        let images = vec![make_image("img-1", 800, 600), make_image("img-2", 800, 600)];
        let state = orchestrator.accept_images(&images).unwrap();
        assert_eq!(state, OrchestratorState::Retry);

        for _ in 0..3 {
            orchestrator.record_retry().unwrap();
            let imgs = vec![make_image("rx", 800, 600)];
            let state = orchestrator.accept_images(&imgs).unwrap();
            if orchestrator.counter().retry_count < 3 {
                assert_eq!(state, OrchestratorState::Retry);
            } else {
                assert_eq!(state, OrchestratorState::LimitedDelivery);
            }
        }

        assert_eq!(orchestrator.qualified_count(), 0);

        let decision = orchestrator.build_delivery_decision();
        assert_eq!(decision.status, TaskStatus::LimitedDelivery);
        assert_eq!(decision.accepted_images.len(), 0);
    }

    #[test]
    fn limited_delivery_after_all_retries_with_insufficient_accumulation() {
        let task_plan = make_task_plan(5);
        let evaluator = FixtureImageEvaluator::new(vec![make_approve_conclusion()]);
        let mut orchestrator = TaskOrchestrator::new(task_plan, &evaluator);

        let state = orchestrator
            .accept_images(&[make_image("img-1", 800, 600)])
            .unwrap();
        assert_eq!(state, OrchestratorState::Retry);

        orchestrator.record_retry().unwrap();
        let state = orchestrator
            .accept_images(&[make_image("img-2", 800, 600)])
            .unwrap();
        assert_eq!(state, OrchestratorState::Retry);

        orchestrator.record_retry().unwrap();
        let state = orchestrator
            .accept_images(&[make_image("img-3", 800, 600)])
            .unwrap();
        assert_eq!(state, OrchestratorState::Retry);

        orchestrator.record_retry().unwrap();
        let state = orchestrator
            .accept_images(&[make_image("img-4", 800, 600)])
            .unwrap();

        assert_eq!(state, OrchestratorState::LimitedDelivery);
        assert_eq!(orchestrator.qualified_count(), 4);
        assert_eq!(orchestrator.counter().full_attempt_count, 4);
        assert_eq!(orchestrator.counter().retry_count, 3);
        assert!(orchestrator.counter().is_exhausted());

        let decision = orchestrator.build_delivery_decision();
        assert_eq!(decision.status, TaskStatus::LimitedDelivery);
    }

    // -----------------------------------------------------------------------
    // Counter tests
    // -----------------------------------------------------------------------

    #[test]
    fn full_attempt_count_and_retry_count_are_distinct() {
        let task_plan = make_task_plan(1);
        let evaluator = FixtureImageEvaluator::new(vec![make_reject_conclusion()]);
        let mut orchestrator = TaskOrchestrator::new(task_plan, &evaluator);

        assert_eq!(orchestrator.counter().full_attempt_count, 1);
        assert_eq!(orchestrator.counter().retry_count, 0);
        assert_ne!(
            orchestrator.counter().full_attempt_count,
            orchestrator.counter().retry_count
        );

        orchestrator
            .accept_images(&[make_image("img-1", 800, 600)])
            .unwrap();
        orchestrator.record_retry().unwrap();
        assert_eq!(orchestrator.counter().full_attempt_count, 2);
        assert_eq!(orchestrator.counter().retry_count, 1);
        assert_ne!(
            orchestrator.counter().full_attempt_count,
            orchestrator.counter().retry_count
        );
    }

    #[test]
    fn attempt_counter_new_starts_at_one() {
        let counter = AttemptCounter::new(3);
        assert_eq!(counter.full_attempt_count, 1);
        assert_eq!(counter.retry_count, 0);
        assert!(counter.can_retry());
        assert!(!counter.is_exhausted());
    }

    #[test]
    fn attempt_counter_exhausted_after_max_retries() {
        let mut counter = AttemptCounter::new(2);
        assert!(counter.record_retry());
        assert_eq!(counter.retry_count, 1);
        assert!(counter.can_retry());

        assert!(counter.record_retry());
        assert_eq!(counter.retry_count, 2);
        assert!(!counter.can_retry());
        assert!(counter.is_exhausted());

        assert!(!counter.record_retry());
        assert_eq!(counter.retry_count, 2);
    }

    #[test]
    fn attempt_counter_zero_retry_limit() {
        let mut counter = AttemptCounter::new(0);
        assert!(!counter.can_retry());
        assert!(counter.is_exhausted());
        assert!(!counter.record_retry());
    }

    #[test]
    fn record_retry_fails_when_exhausted() {
        let task_plan = make_task_plan(1);
        let evaluator = FixtureImageEvaluator::new(vec![make_reject_conclusion()]);
        let mut orchestrator = TaskOrchestrator::new(task_plan, &evaluator);

        for _ in 0..3 {
            orchestrator.record_retry().unwrap();
        }

        assert!(orchestrator.record_retry().is_err());
    }

    // -----------------------------------------------------------------------
    // State tests
    // -----------------------------------------------------------------------

    #[test]
    fn terminal_states_are_terminal() {
        assert!(OrchestratorState::InputRejected.is_terminal());
        assert!(OrchestratorState::FullDelivery.is_terminal());
        assert!(OrchestratorState::LimitedDelivery.is_terminal());
        assert!(OrchestratorState::ExecutionBlocked.is_terminal());
    }

    #[test]
    fn non_terminal_states_are_not_terminal() {
        assert!(!OrchestratorState::InputValidation.is_terminal());
        assert!(!OrchestratorState::Running.is_terminal());
        assert!(!OrchestratorState::Retry.is_terminal());
    }

    #[test]
    fn state_labels() {
        assert_eq!(
            OrchestratorState::InputValidation.label(),
            "input_validation"
        );
        assert_eq!(OrchestratorState::InputRejected.label(), "input_rejected");
        assert_eq!(OrchestratorState::Running.label(), "running");
        assert_eq!(OrchestratorState::Retry.label(), "retry");
        assert_eq!(OrchestratorState::FullDelivery.label(), "full_delivery");
        assert_eq!(
            OrchestratorState::LimitedDelivery.label(),
            "limited_delivery"
        );
        assert_eq!(
            OrchestratorState::ExecutionBlocked.label(),
            "execution_blocked"
        );
    }

    // -----------------------------------------------------------------------
    // Mechanical rejection
    // -----------------------------------------------------------------------

    #[test]
    fn mechanically_blocked_images_are_rejected_not_qualified() {
        let task_plan = make_task_plan(1);
        let evaluator = FixtureImageEvaluator::new(vec![]);
        let mut orchestrator = TaskOrchestrator::new(task_plan, &evaluator);

        let bad_image = ImageRecord {
            candidate_id: "bad".into(),
            local_path: "/tmp/bad".into(),
            content_type: None,
            file_size_bytes: 0,
            dimensions: None,
        };

        let state = orchestrator.accept_images(&[bad_image]).unwrap();
        assert_eq!(state, OrchestratorState::Retry);
        assert_eq!(orchestrator.qualified_count(), 0);
        assert_eq!(orchestrator.rejected_images().len(), 1);
    }

    // -----------------------------------------------------------------------
    // Input validation entry point
    // -----------------------------------------------------------------------

    #[test]
    fn validate_and_create_orchestrator_valid_input() {
        let input = QueryPlanInput {
            description: "sunset".into(),
            required_image_count: 2,
            ..Default::default()
        };
        let evaluator = FixtureImageEvaluator::new(vec![]);
        let result = validate_and_create_orchestrator(input, &evaluator);
        assert!(result.is_ok());
        let orchestrator = result.unwrap();
        assert_eq!(orchestrator.state(), OrchestratorState::Running);
        assert_eq!(orchestrator.required_count(), 2);
    }

    #[test]
    fn validate_and_create_orchestrator_rejects_empty_description() {
        let input = QueryPlanInput {
            description: "".into(),
            ..Default::default()
        };
        let evaluator = FixtureImageEvaluator::new(vec![]);
        let result = validate_and_create_orchestrator(input, &evaluator);
        assert!(result.is_err());
        match result {
            Err(decision) => {
                assert_eq!(decision.status, TaskStatus::InputRejected);
            }
            Ok(_) => panic!("expected rejection"),
        }
    }

    // -----------------------------------------------------------------------
    // Diagnostic and metric events
    // -----------------------------------------------------------------------

    #[test]
    fn orchestrator_emits_task_outcome_event_on_terminal_state() {
        let task_plan = make_task_plan(1);
        let evaluator = FixtureImageEvaluator::new(vec![make_approve_conclusion()]);
        let mut orchestrator = TaskOrchestrator::new(task_plan, &evaluator);

        orchestrator
            .accept_images(&[make_image("img-1", 800, 600)])
            .unwrap();

        let events = orchestrator.metric_events();
        assert!(!events.is_empty());
        let outcome_event = events
            .iter()
            .find(|e| e.kind == MetricKind::TaskOutcome)
            .expect("should emit task outcome event");
        assert!(outcome_event.label.contains("full_delivery"));
    }

    #[test]
    fn orchestrator_builds_diagnostic_with_items() {
        let task_plan = make_task_plan(1);
        let evaluator = FixtureImageEvaluator::new(vec![make_approve_conclusion()]);
        let mut orchestrator = TaskOrchestrator::new(task_plan, &evaluator);

        orchestrator
            .accept_images(&[make_image("img-1", 800, 600)])
            .unwrap();

        let diagnostic = orchestrator.build_diagnostic();
        assert_eq!(diagnostic.status, "full_delivery");
        assert!(!diagnostic.items.is_empty());
        assert!(diagnostic.summary.contains("Full delivery"));
    }

    // -----------------------------------------------------------------------
    // Accumulation across calls
    // -----------------------------------------------------------------------

    #[test]
    fn qualified_images_accumulate_across_calls() {
        let task_plan = make_task_plan(3);
        let evaluator =
            FixtureImageEvaluator::new(vec![make_approve_conclusion(), make_approve_conclusion()]);
        let mut orchestrator = TaskOrchestrator::new(task_plan, &evaluator);

        let images1 = vec![make_image("img-1", 800, 600), make_image("img-2", 800, 600)];
        orchestrator.accept_images(&images1).unwrap();
        assert_eq!(orchestrator.qualified_count(), 2);

        orchestrator.record_retry().unwrap();
        assert_eq!(orchestrator.qualified_count(), 2);
        assert_eq!(orchestrator.counter().full_attempt_count, 2);
    }
}
