//! Delivery domain types.
//!
//! Covers task result status, delivery decisions, and delivery manifest
//! structures for human and automated consumption.
//!
//! References: PRD §交付物产品设计, HLD §Delivery Package Builder

use serde::{Deserialize, Serialize};

/// The final status of a QueryPlan task.
///
/// Maps to PRD task result states: 完整交付 / 有限交付 / 执行阻塞 / 输入拒绝.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    /// All requested images were delivered.
    #[serde(rename = "full_delivery")]
    FullDelivery,

    /// Fewer than requested images were delivered after exhausting retries.
    #[serde(rename = "limited_delivery")]
    LimitedDelivery,

    /// The task was blocked by a missing production dependency or policy.
    #[serde(rename = "execution_blocked")]
    ExecutionBlocked,

    /// The QueryPlan was invalid; no delivery attempt was made.
    #[serde(rename = "input_rejected")]
    InputRejected,
}

/// The orchestrator's delivery decision, containing the final status,
/// accepted images, rejection evidence, and attempt counters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryDecision {
    pub status: TaskStatus,
    pub accepted_images: Vec<super::image::ImageAcceptanceDecision>,
    pub rejected_images: Vec<super::image::ImageAcceptanceDecision>,
    /// Total full attempts made (1 initial + N retries).
    pub full_attempt_count: u32,
    /// Retries beyond the initial attempt (≤ retry_limit).
    pub retry_count: u32,
    /// Human-readable summary of the outcome.
    pub summary: String,
    /// Detailed reason when not a full delivery.
    pub shortfall_reason: Option<String>,
}

impl DeliveryDecision {
    pub fn full_delivery(
        accepted: Vec<super::image::ImageAcceptanceDecision>,
        rejected: Vec<super::image::ImageAcceptanceDecision>,
        full_attempt_count: u32,
        retry_count: u32,
    ) -> Self {
        Self {
            status: TaskStatus::FullDelivery,
            accepted_images: accepted,
            rejected_images: rejected,
            full_attempt_count,
            retry_count,
            summary: format!(
                "Full delivery: {} images accepted after {} attempt(s).",
                full_attempt_count, full_attempt_count
            ),
            shortfall_reason: None,
        }
    }

    pub fn limited_delivery(
        accepted: Vec<super::image::ImageAcceptanceDecision>,
        rejected: Vec<super::image::ImageAcceptanceDecision>,
        full_attempt_count: u32,
        retry_count: u32,
        required_count: u32,
    ) -> Self {
        let accepted_count = accepted.iter().filter(|d| d.is_accepted()).count() as u32;
        let shortfall = required_count.saturating_sub(accepted_count);
        Self {
            status: TaskStatus::LimitedDelivery,
            accepted_images: accepted,
            rejected_images: rejected,
            full_attempt_count,
            retry_count,
            summary: format!(
                "Limited delivery: {} of {} required images delivered after {} attempt(s).",
                accepted_count, required_count, full_attempt_count,
            ),
            shortfall_reason: Some(format!(
                "Shortfall of {} image(s). Retry limit ({}) reached.",
                shortfall, retry_count
            )),
        }
    }

    pub fn execution_blocked(reason: String) -> Self {
        Self {
            status: TaskStatus::ExecutionBlocked,
            accepted_images: vec![],
            rejected_images: vec![],
            full_attempt_count: 0,
            retry_count: 0,
            summary: format!("Execution blocked: {}", reason),
            shortfall_reason: Some(reason),
        }
    }

    pub fn input_rejected(reason: String) -> Self {
        Self {
            status: TaskStatus::InputRejected,
            accepted_images: vec![],
            rejected_images: vec![],
            full_attempt_count: 0,
            retry_count: 0,
            summary: format!("Input rejected: {}", reason),
            shortfall_reason: Some(reason),
        }
    }
}

// ---------------------------------------------------------------------------
// Delivery manifest
// ---------------------------------------------------------------------------

/// Top-level delivery manifest describing what was delivered and why.
///
/// This is the machine-readable summary consumed by both human users and
/// downstream automation workflows.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryManifest {
    pub status: TaskStatus,
    pub required_count: u32,
    pub delivered_count: u32,
    pub full_attempt_count: u32,
    pub retry_count: u32,
    pub summary: String,
    pub shortfall_reason: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::candidate::ImageDimensions;
    use crate::domain::image::{ImageAcceptanceDecision, ImageRecord};

    fn make_accepted(id: &str) -> ImageAcceptanceDecision {
        ImageAcceptanceDecision::Accepted {
            image: ImageRecord {
                candidate_id: id.into(),
                local_path: format!("/tmp/{}.jpg", id),
                content_type: Some("image/jpeg".into()),
                file_size_bytes: 1024,
                dimensions: Some(ImageDimensions {
                    width: 800,
                    height: 600,
                }),
            },
            notes: "good".into(),
        }
    }

    #[test]
    fn full_delivery_status() {
        let accepted = vec![make_accepted("a"), make_accepted("b")];
        let decision = DeliveryDecision::full_delivery(accepted, vec![], 1, 0);
        assert_eq!(decision.status, TaskStatus::FullDelivery);
        assert!(decision.shortfall_reason.is_none());
    }

    #[test]
    fn limited_delivery_status_with_shortfall() {
        let accepted = vec![make_accepted("a")];
        let decision = DeliveryDecision::limited_delivery(accepted, vec![], 4, 3, 3);
        assert_eq!(decision.status, TaskStatus::LimitedDelivery);
        assert!(decision.shortfall_reason.is_some());
    }

    #[test]
    fn execution_blocked_status() {
        let decision = DeliveryDecision::execution_blocked("OpenClaw unavailable".into());
        assert_eq!(decision.status, TaskStatus::ExecutionBlocked);
        assert_eq!(decision.full_attempt_count, 0);
    }

    #[test]
    fn input_rejected_status() {
        let decision = DeliveryDecision::input_rejected("missing description".into());
        assert_eq!(decision.status, TaskStatus::InputRejected);
        assert_eq!(decision.accepted_images.len(), 0);
    }
}
