//! Image acceptance domain types.
//!
//! Covers image records, mechanical acceptance evidence, and the final
//! image acceptance decision after OpenClaw subjective evaluation.
//!
//! References: PRD §校验与评价产品要求, HLD §Image Acceptance Gate

use serde::{Deserialize, Serialize};

/// A locally-fetched image that is ready for acceptance checks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageRecord {
    /// Candidate id this image was fetched from.
    pub candidate_id: String,

    /// Path to the local image file.
    pub local_path: String,

    /// Content type (e.g. "image/jpeg").
    pub content_type: Option<String>,

    /// File size in bytes.
    pub file_size_bytes: u64,

    /// Actual image dimensions determined by reading the file.
    pub dimensions: Option<super::candidate::ImageDimensions>,
}

// ---------------------------------------------------------------------------
// Mechanical evidence
// ---------------------------------------------------------------------------

/// Evidence produced by mechanical image validation.
///
/// Evidence is split into two classes per the constitution:
/// - **Blocking**: grounds for automatic rejection.
/// - **Reference**: supplementary information for subjective evaluation
///   and risk explanation. Reference evidence alone cannot reject an image
///   unless a product policy explicitly makes it blocking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageMechanicalEvidence {
    /// Blocking findings — any non-empty list means the image is rejected.
    pub blocking_findings: Vec<String>,

    /// Reference findings — fed into OpenClaw evaluation and risk/policy
    /// explanations.
    pub reference_findings: Vec<String>,
}

impl ImageMechanicalEvidence {
    /// Returns `true` when there are no blocking findings.
    pub fn passed_mechanical(&self) -> bool {
        self.blocking_findings.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Image acceptance decision
// ---------------------------------------------------------------------------

/// The final verdict for a single retrieved image after both mechanical
/// acceptance and OpenClaw subjective evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ImageAcceptanceDecision {
    /// Both mechanical and subjective checks passed — the image counts
    /// toward the delivery quota.
    Accepted {
        image: ImageRecord,
        /// Quality/relevance notes for the delivery manifest.
        notes: String,
    },

    /// Mechanical check blocked the image.
    MechanicallyRejected {
        image: ImageRecord,
        evidence: ImageMechanicalEvidence,
    },

    /// Mechanical check passed but OpenClaw subjective evaluation rejected
    /// or was uncertain.
    SubjectivelyRejected {
        image: ImageRecord,
        mechanical_evidence: ImageMechanicalEvidence,
        reason: String,
    },

    /// OpenClaw evaluation could not be performed (production dependency
    /// unavailable). This is a task-level execution block, not a per-image
    /// rejection.
    ExecutionBlocked { reason: String },
}

impl ImageAcceptanceDecision {
    /// Returns `true` iff the image is accepted and qualified for delivery.
    pub fn is_accepted(&self) -> bool {
        matches!(self, Self::Accepted { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::candidate::{CandidateId, ImageDimensions};

    fn make_image() -> ImageRecord {
        ImageRecord {
            candidate_id: CandidateId::new("c1").to_string(),
            local_path: "/tmp/test.jpg".into(),
            content_type: Some("image/jpeg".into()),
            file_size_bytes: 1024,
            dimensions: Some(ImageDimensions {
                width: 800,
                height: 600,
            }),
        }
    }

    #[test]
    fn mechanical_evidence_passes_when_no_blocking() {
        let evidence = ImageMechanicalEvidence {
            blocking_findings: vec![],
            reference_findings: vec!["low resolution".into()],
        };
        assert!(evidence.passed_mechanical());
    }

    #[test]
    fn mechanical_evidence_fails_on_blocking() {
        let evidence = ImageMechanicalEvidence {
            blocking_findings: vec!["file corrupted".into()],
            reference_findings: vec![],
        };
        assert!(!evidence.passed_mechanical());
    }

    #[test]
    fn accepted_decision_is_accepted() {
        let d = ImageAcceptanceDecision::Accepted {
            image: make_image(),
            notes: "good match".into(),
        };
        assert!(d.is_accepted());
    }

    #[test]
    fn rejected_decisions_are_not_accepted() {
        let img = make_image();
        assert!(!ImageAcceptanceDecision::MechanicallyRejected {
            image: img.clone(),
            evidence: ImageMechanicalEvidence {
                blocking_findings: vec!["corrupt".into()],
                reference_findings: vec![],
            },
        }
        .is_accepted());

        assert!(!ImageAcceptanceDecision::SubjectivelyRejected {
            image: img.clone(),
            mechanical_evidence: ImageMechanicalEvidence {
                blocking_findings: vec![],
                reference_findings: vec![],
            },
            reason: "does not match description".into(),
        }
        .is_accepted());

        assert!(!ImageAcceptanceDecision::ExecutionBlocked {
            reason: "OpenClaw unavailable".into(),
        }
        .is_accepted());
    }
}
