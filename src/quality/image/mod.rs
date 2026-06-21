//! Image quality gate module.
//!
//! Contains:
//! - [`mechanical`] — Image mechanical acceptance validation.
//! - [`evaluation`] — OpenClaw image evaluation request/response types and normalization.
//! - [`gate`] — Image acceptance gate orchestrator.
//!
//! References: PRD §图片验收与重试收口, HLD §Image Acceptance Gate,
//! `docs/design/TASK-006-image-acceptance-orchestrator-design.md`

pub mod evaluation;
pub mod gate;
pub mod mechanical;

// Re-export public API
pub use evaluation::{
    normalize_image_conclusion, normalize_image_conclusions, ImageEvaluationConclusion,
    ImageEvaluationRequest, ImageExecutionBlockingFact,
};
pub use gate::{
    evaluate_images_with_conclusions, ImageAcceptanceGate, ImageAcceptanceGateResult,
    ImageAcceptanceSummary,
};
pub use mechanical::{validate_image_mechanical, ImageBlockingReason, ImageReferenceSignal};
