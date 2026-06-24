//! Image mechanical acceptance validation.
//!
//! Validates actually-retrieved images against the same mechanical criteria
//! used for candidates but applied to real file artifacts: file integrity,
//! dimensions, content type, corruption detection.
//!
//! References: PRD §校验与评价产品要求, HLD §Image Acceptance Gate,
//! `docs/design/TASK-006-image-acceptance-orchestrator-design.md`

use crate::domain::image::{ImageMechanicalEvidence, ImageRecord};
use crate::domain::query_plan::QualityTier;

// ---------------------------------------------------------------------------
// Image blocking reasons
// ---------------------------------------------------------------------------

/// Reasons an actual retrieved image can be mechanically blocked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageBlockingReason {
    /// File does not exist or cannot be read.
    FileUnreadable { detail: String },

    /// File is zero bytes or below minimum threshold.
    FileTooSmall { actual_bytes: u64, min_bytes: u64 },

    /// Content type is unrecognized or clearly not an image.
    InvalidContentType { content_type: String },

    /// Content type is missing — cannot verify the file is an image.
    MissingContentType,

    /// Image dimensions are below the absolute minimum.
    DimensionsTooSmall {
        width: u32,
        height: u32,
        min_width: u32,
        min_height: u32,
    },

    /// Image is corrupt or structurally invalid.
    Corrupt { detail: String },
}

impl ImageBlockingReason {
    /// Human-readable label for metrics / diagnostics.
    pub fn label(&self) -> &'static str {
        match self {
            Self::FileUnreadable { .. } => "file_unreadable",
            Self::FileTooSmall { .. } => "file_too_small",
            Self::InvalidContentType { .. } => "invalid_content_type",
            Self::MissingContentType => "missing_content_type",
            Self::DimensionsTooSmall { .. } => "dimensions_too_small",
            Self::Corrupt { .. } => "corrupt",
        }
    }

    /// Human-readable description.
    pub fn description(&self) -> String {
        match self {
            Self::FileUnreadable { detail } => {
                format!("image file unreadable: {}", detail)
            }
            Self::FileTooSmall {
                actual_bytes,
                min_bytes,
            } => {
                format!(
                    "file too small: {} bytes (minimum {} bytes)",
                    actual_bytes, min_bytes
                )
            }
            Self::InvalidContentType { content_type } => {
                format!("invalid content type: {}", content_type)
            }
            Self::MissingContentType => "missing content type".into(),
            Self::DimensionsTooSmall {
                width,
                height,
                min_width,
                min_height,
            } => {
                format!(
                    "dimensions {}x{} below minimum {}x{}",
                    width, height, min_width, min_height
                )
            }
            Self::Corrupt { detail } => {
                format!("image appears corrupt: {}", detail)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Image reference signals
// ---------------------------------------------------------------------------

/// Non-blocking signals for image mechanical acceptance.
///
/// These inform OpenClaw subjective evaluation and risk explanations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageReferenceSignal {
    /// File is present but dimensions are small (below recommendation
    /// but above absolute minimum).
    SmallDimensions { width: u32, height: u32 },

    /// Content type is recognized but not in the preferred set.
    NonPreferredContentType { content_type: String },

    /// File size is large (may indicate high-resolution image).
    LargeFileSize { file_size_bytes: u64 },

    /// No dimensions could be determined (reference-only concern).
    NoDimensions,
}

impl ImageReferenceSignal {
    pub fn label(&self) -> &'static str {
        match self {
            Self::SmallDimensions { .. } => "small_dimensions",
            Self::NonPreferredContentType { .. } => "non_preferred_content_type",
            Self::LargeFileSize { .. } => "large_file_size",
            Self::NoDimensions => "no_dimensions",
        }
    }
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// Absolute minimum dimensions for any accepted image.
const ABSOLUTE_MIN_WIDTH: u32 = 2;
const ABSOLUTE_MIN_HEIGHT: u32 = 2;

/// Recommended minimum dimensions (below this produces a reference signal).
const RECOMMENDED_MIN_WIDTH: u32 = 50;
const RECOMMENDED_MIN_HEIGHT: u32 = 50;

/// Minimum file size in bytes.
const MIN_FILE_SIZE_BYTES: u64 = 1;

/// Preferred image content types.
const PREFERRED_CONTENT_TYPES: &[&str] = &["image/jpeg", "image/png", "image/webp", "image/gif"];

/// Run mechanical validation on an actually-retrieved image.
///
/// Returns `ImageMechanicalEvidence` with blocking findings (image is
/// rejected) or reference signals (image passes mechanical).
pub fn validate_image_mechanical(
    image: &ImageRecord,
    _quality_tier: QualityTier,
) -> ImageMechanicalEvidence {
    let mut blocking: Vec<ImageBlockingReason> = Vec::new();
    let mut reference: Vec<ImageReferenceSignal> = Vec::new();

    // 1. File size check
    if image.file_size_bytes < MIN_FILE_SIZE_BYTES {
        blocking.push(ImageBlockingReason::FileTooSmall {
            actual_bytes: image.file_size_bytes,
            min_bytes: MIN_FILE_SIZE_BYTES,
        });
    }

    // 2. Content type check
    match &image.content_type {
        None => {
            blocking.push(ImageBlockingReason::MissingContentType);
        }
        Some(ct) => {
            let normalized = ct.trim().to_lowercase();
            // Must be an image type
            if !normalized.starts_with("image/") {
                blocking.push(ImageBlockingReason::InvalidContentType {
                    content_type: ct.clone(),
                });
            } else if !PREFERRED_CONTENT_TYPES
                .iter()
                .any(|pref| normalized == *pref)
            {
                reference.push(ImageReferenceSignal::NonPreferredContentType {
                    content_type: ct.clone(),
                });
            }
        }
    }

    // 3. Dimensions check
    match &image.dimensions {
        None => {
            reference.push(ImageReferenceSignal::NoDimensions);
        }
        Some(dims) => {
            if dims.width < ABSOLUTE_MIN_WIDTH || dims.height < ABSOLUTE_MIN_HEIGHT {
                blocking.push(ImageBlockingReason::DimensionsTooSmall {
                    width: dims.width,
                    height: dims.height,
                    min_width: ABSOLUTE_MIN_WIDTH,
                    min_height: ABSOLUTE_MIN_HEIGHT,
                });
            } else if dims.width < RECOMMENDED_MIN_WIDTH || dims.height < RECOMMENDED_MIN_HEIGHT {
                reference.push(ImageReferenceSignal::SmallDimensions {
                    width: dims.width,
                    height: dims.height,
                });
            }
        }
    }

    // 4. Large file size (reference only)
    const LARGE_FILE_THRESHOLD: u64 = 50 * 1024 * 1024; // 50 MB
    if image.file_size_bytes > LARGE_FILE_THRESHOLD {
        reference.push(ImageReferenceSignal::LargeFileSize {
            file_size_bytes: image.file_size_bytes,
        });
    }

    // Convert to domain evidence type
    let blocking_findings: Vec<String> = blocking.iter().map(|r| r.description()).collect();
    let reference_findings: Vec<String> = reference.iter().map(|r| r.label().to_string()).collect();

    ImageMechanicalEvidence {
        blocking_findings,
        reference_findings,
    }
}

// ---------------------------------------------------------------------------
// v1.1 image mechanical validation — artifact-aware
// ---------------------------------------------------------------------------

/// Run v1.1 mechanical validation on a retrieved image artifact.
///
/// Checks for all required artifact evidence fields per the detailed design:
/// retrieval status, local artifact, source artifact, sidecar, summary,
/// task report, visual description, checksum, content type, media type match,
/// file size, dimensions, ownership, and fixture mode.
pub fn validate_image_mechanical_v11(
    result: &crate::domain::image::RetrievalArtifactResult,
    expected_query_plan_id: &str,
    expected_candidate_id: &str,
    fixture_mode: bool,
) -> crate::domain::image::ImageMechanicalAssessment {
    use crate::domain::candidate::CandidateId;
    use crate::domain::image::ImageMechanicalAssessment;
    use crate::domain::metrics::{MetricFact, QualityMetricCode};

    let candidate_id = CandidateId::new(expected_candidate_id);
    let retrieval_job_id = result.retrieval_job_id.clone();
    let query_plan_id = expected_query_plan_id.to_string();
    let mut blocking = Vec::new();
    let mut reference = Vec::new();

    // 1. Retrieval not complete
    if !result.is_complete() {
        blocking.push(MetricFact::image_blocking(
            QualityMetricCode::ImageRetrievalNotComplete,
            &retrieval_job_id,
            &query_plan_id,
            "retrieval status is not complete",
        ));
    }

    // 2. Local artifact missing
    if result.local_artifact_path.is_none() {
        blocking.push(MetricFact::image_blocking(
            QualityMetricCode::ImageLocalArtifactMissing,
            &retrieval_job_id,
            &query_plan_id,
            "local artifact path is missing",
        ));
    }

    // 3. Source artifact missing
    if result.source_artifact_path.is_none() {
        blocking.push(MetricFact::image_blocking(
            QualityMetricCode::ImageSourceArtifactMissing,
            &retrieval_job_id,
            &query_plan_id,
            "source artifact path is missing",
        ));
    }

    // 4. Sidecar missing
    if result.source_sidecar_path.is_none() {
        blocking.push(MetricFact::image_blocking(
            QualityMetricCode::ImageSidecarMissing,
            &retrieval_job_id,
            &query_plan_id,
            "source sidecar path is missing",
        ));
    }

    // 5. Content summary missing
    if result.content_summary_path.is_none() {
        blocking.push(MetricFact::image_blocking(
            QualityMetricCode::ImageSummaryMissing,
            &retrieval_job_id,
            &query_plan_id,
            "content summary path is missing",
        ));
    }

    // 6. Task report missing
    if result.task_report_path.is_none() {
        blocking.push(MetricFact::image_blocking(
            QualityMetricCode::ImageTaskReportMissing,
            &retrieval_job_id,
            &query_plan_id,
            "task report path is missing",
        ));
    }

    // 7. Visual description missing
    if result.visual_description_path.is_none() {
        blocking.push(MetricFact::image_blocking(
            QualityMetricCode::ImageVisualDescriptionMissing,
            &retrieval_job_id,
            &query_plan_id,
            "visual description path is missing",
        ));
    }

    // 8. Checksum missing
    if result.checksum_sha256.is_none() {
        blocking.push(MetricFact::image_blocking(
            QualityMetricCode::ImageChecksumMissing,
            &retrieval_job_id,
            &query_plan_id,
            "checksum is missing",
        ));
    }

    // 9. Content type invalid
    match &result.content_type {
        None => {
            blocking.push(MetricFact::image_blocking(
                QualityMetricCode::ImageContentTypeInvalid,
                &retrieval_job_id,
                &query_plan_id,
                "content type is missing",
            ));
        }
        Some(ct) if !ct.starts_with("image/") => {
            blocking.push(MetricFact::image_blocking(
                QualityMetricCode::ImageContentTypeInvalid,
                &retrieval_job_id,
                &query_plan_id,
                format!("content type '{}' is not image-compatible", ct),
            ));
        }
        _ => {}
    }

    // 10. Media type mismatch
    if !result.media_type_match {
        blocking.push(MetricFact::image_blocking(
            QualityMetricCode::ImageMediaTypeMismatch,
            &retrieval_job_id,
            &query_plan_id,
            "media type does not match expected type",
        ));
    }

    // 11. File size too small
    if let Some(size) = result.file_size_bytes {
        if size == 0 {
            blocking.push(
                MetricFact::image_blocking(
                    QualityMetricCode::ImageFileEmptyOrTooSmall,
                    &retrieval_job_id,
                    &query_plan_id,
                    "file is empty (0 bytes)",
                )
                .with_value("0"),
            );
        }
    }

    // 12. Dimensions too small
    if let Some(dims) = &result.image_dimensions {
        const MIN_WIDTH: u32 = 2;
        const MIN_HEIGHT: u32 = 2;
        if dims.width < MIN_WIDTH || dims.height < MIN_HEIGHT {
            blocking.push(
                MetricFact::image_blocking(
                    QualityMetricCode::ImageDimensionsTooSmall,
                    &retrieval_job_id,
                    &query_plan_id,
                    format!(
                        "dimensions {}x{} below minimum {}x{}",
                        dims.width, dims.height, MIN_WIDTH, MIN_HEIGHT
                    ),
                )
                .with_value(format!("{}x{}", dims.width, dims.height))
                .with_threshold(format!("{}x{}", MIN_WIDTH, MIN_HEIGHT)),
            );
        }
    }

    // 13. Ownership mismatch
    if result.query_plan_id != expected_query_plan_id {
        blocking.push(MetricFact::image_blocking(
            QualityMetricCode::ImageJobOwnershipMismatch,
            &retrieval_job_id,
            &query_plan_id,
            format!(
                "retrieval job query_plan_id '{}' != expected '{}'",
                result.query_plan_id, expected_query_plan_id
            ),
        ));
    }
    if result.candidate_id != expected_candidate_id {
        blocking.push(MetricFact::image_blocking(
            QualityMetricCode::ImageJobOwnershipMismatch,
            &retrieval_job_id,
            &query_plan_id,
            format!(
                "retrieval job candidate_id '{}' != expected '{}'",
                result.candidate_id, expected_candidate_id
            ),
        ));
    }

    // 14. Metadata-only result
    if result.is_metadata_only() {
        blocking.push(MetricFact::image_blocking(
            QualityMetricCode::ImageMetadataOnlyResult,
            &retrieval_job_id,
            &query_plan_id,
            "result is metadata-only — no actual image artifact",
        ));
    }

    // 15. Fixture check
    if !fixture_mode && result.channel_id == "fixture" {
        blocking.push(MetricFact::image_blocking(
            QualityMetricCode::ImageFixtureNotProduction,
            &retrieval_job_id,
            &query_plan_id,
            "fixture retrieval result cannot be used in production mode",
        ));
    }

    // --- Reference metrics ---

    // Dimensions
    if let Some(dims) = &result.image_dimensions {
        reference.push(MetricFact::image_reference(
            QualityMetricCode::ImageDimensionsRef,
            &retrieval_job_id,
            &query_plan_id,
            format!("dimensions: {}x{}", dims.width, dims.height),
        ));
    }

    // File size
    if let Some(size) = result.file_size_bytes {
        let note: String = if size > 50 * 1024 * 1024 {
            "large file (>50MB)".into()
        } else {
            "normal size".into()
        };
        reference.push(
            MetricFact::image_reference(
                QualityMetricCode::ImageFileSizeRef,
                &retrieval_job_id,
                &query_plan_id,
                note,
            )
            .with_value(size.to_string()),
        );
    }

    // Content type
    if let Some(ref ct) = result.content_type {
        reference.push(MetricFact::image_reference(
            QualityMetricCode::ImageContentTypeRef,
            &retrieval_job_id,
            &query_plan_id,
            format!("content type: {}", ct),
        ));
    }

    // Fetch trace quality
    reference.push(MetricFact::image_reference(
        QualityMetricCode::ImageFetchTraceQuality,
        &retrieval_job_id,
        &query_plan_id,
        format!("fetch attempts: {}", result.fetch_trace.len()),
    ));

    ImageMechanicalAssessment {
        candidate_id,
        retrieval_job_id,
        query_plan_id,
        passed: blocking.is_empty(),
        blocking_metrics: blocking,
        reference_metrics: reference,
        evaluated_at: String::new(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::candidate::ImageDimensions;
    use crate::domain::image::ImageRecord;
    use crate::domain::query_plan::QualityTier;

    fn make_image(
        content_type: Option<&str>,
        file_size: u64,
        width: u32,
        height: u32,
    ) -> ImageRecord {
        ImageRecord {
            candidate_id: "test-c1".into(),
            local_path: "/tmp/test.jpg".into(),
            content_type: content_type.map(|s| s.into()),
            file_size_bytes: file_size,
            dimensions: Some(ImageDimensions { width, height }),
            reference_metrics: vec![],
        }
    }

    fn make_image_no_dims(content_type: Option<&str>, file_size: u64) -> ImageRecord {
        ImageRecord {
            candidate_id: "test-c1".into(),
            local_path: "/tmp/test.jpg".into(),
            content_type: content_type.map(|s| s.into()),
            file_size_bytes: file_size,
            dimensions: None,
            reference_metrics: vec![],
        }
    }

    #[test]
    fn valid_image_passes_mechanical() {
        let img = make_image(Some("image/jpeg"), 4096, 800, 600);
        let evidence = validate_image_mechanical(&img, QualityTier::General);
        assert!(evidence.passed_mechanical());
        assert!(evidence.blocking_findings.is_empty());
        // Reference signals for valid image should be empty (no issues)
    }

    #[test]
    fn zero_byte_file_is_blocked() {
        let img = make_image(Some("image/jpeg"), 0, 800, 600);
        let evidence = validate_image_mechanical(&img, QualityTier::General);
        assert!(!evidence.passed_mechanical());
        assert!(!evidence.blocking_findings.is_empty());
        assert!(evidence.blocking_findings[0].contains("too small"));
    }

    #[test]
    fn missing_content_type_is_blocked() {
        let img = make_image(None, 4096, 800, 600);
        let evidence = validate_image_mechanical(&img, QualityTier::General);
        assert!(!evidence.passed_mechanical());
        assert!(evidence
            .blocking_findings
            .iter()
            .any(|f| f.contains("missing content type")));
    }

    #[test]
    fn non_image_content_type_is_blocked() {
        let img = make_image(Some("text/html"), 4096, 800, 600);
        let evidence = validate_image_mechanical(&img, QualityTier::General);
        assert!(!evidence.passed_mechanical());
        assert!(evidence
            .blocking_findings
            .iter()
            .any(|f| f.contains("invalid content type")));
    }

    #[test]
    fn non_preferred_content_type_produces_reference_signal() {
        let img = make_image(Some("image/bmp"), 4096, 800, 600);
        let evidence = validate_image_mechanical(&img, QualityTier::General);
        assert!(evidence.passed_mechanical());
        assert!(!evidence.reference_findings.is_empty());
    }

    #[test]
    fn dimensions_below_absolute_minimum_blocked() {
        let img = make_image(Some("image/jpeg"), 4096, 1, 1);
        let evidence = validate_image_mechanical(&img, QualityTier::General);
        assert!(!evidence.passed_mechanical());
        assert!(evidence
            .blocking_findings
            .iter()
            .any(|f| f.contains("dimensions")));
    }

    #[test]
    fn small_dimensions_produce_reference_signal() {
        let img = make_image(Some("image/jpeg"), 4096, 40, 40);
        let evidence = validate_image_mechanical(&img, QualityTier::General);
        assert!(evidence.passed_mechanical());
        assert!(!evidence.reference_findings.is_empty());
    }

    #[test]
    fn missing_dimensions_produces_reference_signal() {
        let img = make_image_no_dims(Some("image/jpeg"), 4096);
        let evidence = validate_image_mechanical(&img, QualityTier::General);
        assert!(evidence.passed_mechanical());
        assert!(evidence
            .reference_findings
            .contains(&"no_dimensions".to_string()));
    }

    #[test]
    fn large_file_size_produces_reference_signal() {
        let img = make_image(Some("image/jpeg"), 100 * 1024 * 1024, 8000, 6000);
        let evidence = validate_image_mechanical(&img, QualityTier::General);
        assert!(evidence.passed_mechanical());
        assert!(evidence
            .reference_findings
            .contains(&"large_file_size".to_string()));
    }

    #[test]
    fn multiple_blocking_reasons_recorded() {
        // Zero bytes + missing content type
        let img = ImageRecord {
            candidate_id: "bad".into(),
            local_path: "/tmp/bad".into(),
            content_type: None,
            file_size_bytes: 0,
            dimensions: None,
            reference_metrics: vec![],
        };
        let evidence = validate_image_mechanical(&img, QualityTier::General);
        assert!(!evidence.passed_mechanical());
        // At least 2 blocking findings
        assert!(evidence.blocking_findings.len() >= 2);
    }

    #[test]
    fn image_blocking_reason_labels() {
        assert_eq!(
            ImageBlockingReason::FileUnreadable { detail: "x".into() }.label(),
            "file_unreadable"
        );
        assert_eq!(
            ImageBlockingReason::FileTooSmall {
                actual_bytes: 0,
                min_bytes: 1
            }
            .label(),
            "file_too_small"
        );
        assert_eq!(
            ImageBlockingReason::InvalidContentType {
                content_type: "x".into()
            }
            .label(),
            "invalid_content_type"
        );
        assert_eq!(
            ImageBlockingReason::MissingContentType.label(),
            "missing_content_type"
        );
        assert_eq!(
            ImageBlockingReason::DimensionsTooSmall {
                width: 1,
                height: 1,
                min_width: 2,
                min_height: 2
            }
            .label(),
            "dimensions_too_small"
        );
        assert_eq!(
            ImageBlockingReason::Corrupt { detail: "x".into() }.label(),
            "corrupt"
        );
    }

    #[test]
    fn image_reference_signal_labels() {
        assert_eq!(
            ImageReferenceSignal::SmallDimensions {
                width: 40,
                height: 40
            }
            .label(),
            "small_dimensions"
        );
        assert_eq!(
            ImageReferenceSignal::NonPreferredContentType {
                content_type: "image/bmp".into()
            }
            .label(),
            "non_preferred_content_type"
        );
        assert_eq!(
            ImageReferenceSignal::LargeFileSize {
                file_size_bytes: 100_000_000
            }
            .label(),
            "large_file_size"
        );
        assert_eq!(ImageReferenceSignal::NoDimensions.label(), "no_dimensions");
    }
}
