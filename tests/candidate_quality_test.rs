#![allow(unused_imports)]
//! Integration tests for TASK-003 candidate and image quality with Qwen 3.5 VLM adapter.
//!
//! Covers AC-006, AC-010, AC-012, AC-013 per
//! `docs/design/v1.1-TASK-003-quality-vlm-design.md`.

use std::collections::HashSet;

// =============================================================================
// Candidate v1.1 mechanical validation tests
// =============================================================================

mod candidate_mechanical_v11 {
    use super::*;
    use image_retrieval::domain::candidate::{
        CandidateId, CandidateMechanicalAssessment, CandidateRecord, ProviderId,
    };
    use image_retrieval::domain::metrics::QualityMetricCode;
    use image_retrieval::quality::candidate::mechanical::validate_candidate_mechanical_v11;

    fn make_candidate(
        id: &str,
        url: &str,
        query_plan_id: &str,
        title: Option<&str>,
    ) -> CandidateRecord {
        let cid = CandidateId::new(id);
        CandidateRecord {
            candidate_id: cid.clone(),
            query_plan_id: query_plan_id.into(),
            provider_id: ProviderId::new("test-provider"),
            provider_kind: "test".into(),
            search_request_id: "sr-1".into(),
            search_round: 1,
            provider_rank: 1,
            global_rank_hint: None,
            image_url: url.into(),
            source_page_url: Some("https://example.com/page".into()),
            thumbnail_url: None,
            title: title.map(|s| s.into()),
            snippet: None,
            width: Some(800),
            height: Some(600),
            mime_type: Some("image/jpeg".into()),
            license_hint: Some("CC BY 2.0".into()),
            attribution: None,
            dedupe_key: CandidateRecord::build_dedupe_key(url),
            origin_candidate_ids: vec![cid],
            provenance: image_retrieval::domain::candidate::CandidateProvenance::new(
                1,
                "test query",
                1,
                1,
            ),
            normalization_warnings: Vec::new(),
        }
    }

    // -----------------------------------------------------------------------
    // AC-006: Candidate with missing image URL is mechanically blocked
    // -----------------------------------------------------------------------

    #[test]
    fn candidate_missing_image_url_is_blocked() {
        let c = make_candidate("c1", "", "qp-1", None);
        let assessment =
            validate_candidate_mechanical_v11(&c, "qp-1", &HashSet::new(), &[], &[], false);
        assert!(
            !assessment.passed,
            "candidate with empty URL must be blocked"
        );
        assert!(
            assessment
                .blocking_metrics
                .iter()
                .any(|f| f.code == QualityMetricCode::CandidateImageUrlMissing),
            "must have CANDIDATE_IMAGE_URL_MISSING"
        );
    }

    #[test]
    fn candidate_invalid_url_scheme_is_blocked() {
        let c = make_candidate("c1", "ftp://files.example.com/img.jpg", "qp-1", None);
        let assessment =
            validate_candidate_mechanical_v11(&c, "qp-1", &HashSet::new(), &[], &[], false);
        assert!(!assessment.passed);
        assert!(
            assessment
                .blocking_metrics
                .iter()
                .any(|f| f.code == QualityMetricCode::CandidateImageUrlInvalid),
            "must have CANDIDATE_IMAGE_URL_INVALID"
        );
    }

    #[test]
    fn valid_https_url_passes_mechanical() {
        let c = make_candidate("c1", "https://example.com/img.jpg", "qp-1", Some("Sunset"));
        let assessment =
            validate_candidate_mechanical_v11(&c, "qp-1", &HashSet::new(), &[], &[], false);
        assert!(assessment.passed, "valid candidate should pass mechanical");
    }

    // -----------------------------------------------------------------------
    // AC-006: Query ownership mismatch is mechanically blocked
    // -----------------------------------------------------------------------

    #[test]
    fn candidate_query_ownership_mismatch_is_blocked() {
        let c = make_candidate("c1", "https://example.com/img.jpg", "qp-wrong", None);
        let assessment =
            validate_candidate_mechanical_v11(&c, "qp-correct", &HashSet::new(), &[], &[], false);
        assert!(!assessment.passed);
        assert!(
            assessment
                .blocking_metrics
                .iter()
                .any(|f| f.code == QualityMetricCode::CandidateQueryOwnershipMismatch),
            "must have CANDIDATE_QUERY_OWNERSHIP_MISMATCH"
        );
    }

    // -----------------------------------------------------------------------
    // AC-006: Duplicate candidates are mechanically blocked
    // -----------------------------------------------------------------------

    #[test]
    fn candidate_duplicate_is_blocked() {
        let c = make_candidate("c2", "https://example.com/img.jpg", "qp-1", None);
        let mut seen = HashSet::new();
        seen.insert("https://example.com/img.jpg".to_string());

        let assessment = validate_candidate_mechanical_v11(&c, "qp-1", &seen, &[], &[], false);
        assert!(!assessment.passed);
        assert!(
            assessment
                .blocking_metrics
                .iter()
                .any(|f| f.code == QualityMetricCode::CandidateDuplicateBlocked),
            "must have CANDIDATE_DUPLICATE_BLOCKED"
        );
    }

    // -----------------------------------------------------------------------
    // AC-006: Prohibited source is mechanically blocked
    // -----------------------------------------------------------------------

    #[test]
    fn candidate_prohibited_source_is_blocked() {
        let c = make_candidate("c1", "https://banned-site.com/img.jpg", "qp-1", None);
        let assessment = validate_candidate_mechanical_v11(
            &c,
            "qp-1",
            &HashSet::new(),
            &["banned-site.com".into()],
            &[],
            false,
        );
        assert!(!assessment.passed);
        assert!(
            assessment
                .blocking_metrics
                .iter()
                .any(|f| f.code == QualityMetricCode::CandidateProhibitedSource),
            "must have CANDIDATE_PROHIBITED_SOURCE"
        );
    }

    // -----------------------------------------------------------------------
    // AC-006: Negative scope contradiction is mechanically blocked
    // -----------------------------------------------------------------------

    #[test]
    fn candidate_negative_scope_contradiction_is_blocked() {
        let c = make_candidate(
            "c1",
            "https://example.com/img.jpg",
            "qp-1",
            Some("Beautiful city"),
        );
        let assessment = validate_candidate_mechanical_v11(
            &c,
            "qp-1",
            &HashSet::new(),
            &[],
            &["city".into()],
            false,
        );
        assert!(!assessment.passed);
        assert!(
            assessment
                .blocking_metrics
                .iter()
                .any(|f| f.code == QualityMetricCode::CandidateNegativeScopeContradiction),
            "must have CANDIDATE_NEGATIVE_SCOPE_CONTRADICTION"
        );
    }

    // -----------------------------------------------------------------------
    // Reference metrics: dimensions, license, source page produce reference facts
    // -----------------------------------------------------------------------

    #[test]
    fn candidate_reference_metrics_include_dimensions_license_source() {
        let c = make_candidate("c1", "https://example.com/img.jpg", "qp-1", Some("Sunset"));
        let assessment =
            validate_candidate_mechanical_v11(&c, "qp-1", &HashSet::new(), &[], &[], false);
        assert!(assessment.passed);

        // Reference metrics present
        assert!(
            assessment
                .reference_metrics
                .iter()
                .any(|f| f.code == QualityMetricCode::CandidateDimensionsReported),
            "dimensions reference"
        );
        assert!(
            assessment
                .reference_metrics
                .iter()
                .any(|f| f.code == QualityMetricCode::CandidateLicenseHint),
            "license reference"
        );
        assert!(
            assessment
                .reference_metrics
                .iter()
                .any(|f| f.code == QualityMetricCode::CandidateSourcePagePresent),
            "source page reference"
        );
        assert!(
            assessment
                .reference_metrics
                .iter()
                .any(|f| f.code == QualityMetricCode::CandidateProviderRank),
            "provider rank reference"
        );
        assert!(
            assessment
                .reference_metrics
                .iter()
                .any(|f| f.code == QualityMetricCode::CandidateTextContextMatch),
            "text context reference"
        );
    }

    // -----------------------------------------------------------------------
    // Missing dimensions/license/source page produce reference metrics, not blocking
    // -----------------------------------------------------------------------

    #[test]
    fn missing_dimensions_produce_reference_not_blocking() {
        let mut c = make_candidate("c1", "https://example.com/img.jpg", "qp-1", None);
        c.width = None;
        c.height = None;
        c.license_hint = None;
        c.source_page_url = None;

        let assessment =
            validate_candidate_mechanical_v11(&c, "qp-1", &HashSet::new(), &[], &[], false);
        // Should still pass mechanical — missing metadata is reference, not blocking
        assert!(assessment.passed);
        assert!(assessment.blocking_metrics.is_empty());
        // But reference signals should note the absence
        assert!(!assessment.reference_metrics.is_empty());
    }

    // -----------------------------------------------------------------------
    // Below absolute dimensions is blocking
    // -----------------------------------------------------------------------

    #[test]
    fn candidate_below_absolute_dimensions_is_blocked() {
        let mut c = make_candidate("c1", "https://example.com/img.jpg", "qp-1", None);
        c.width = Some(1);
        c.height = Some(1);

        let assessment =
            validate_candidate_mechanical_v11(&c, "qp-1", &HashSet::new(), &[], &[], false);
        assert!(!assessment.passed);
        assert!(
            assessment
                .blocking_metrics
                .iter()
                .any(|f| f.code == QualityMetricCode::CandidateBelowAbsoluteDimensions),
            "must have CANDIDATE_BELOW_ABSOLUTE_DIMENSIONS"
        );
    }

    // -----------------------------------------------------------------------
    // Fixture candidate in production is blocked
    // -----------------------------------------------------------------------

    #[test]
    fn fixture_candidate_in_production_is_blocked() {
        let mut c = make_candidate("c1", "https://example.com/img.jpg", "qp-1", None);
        c.provider_kind = "fixture".into();

        let assessment =
            validate_candidate_mechanical_v11(&c, "qp-1", &HashSet::new(), &[], &[], false);
        assert!(!assessment.passed);
        assert!(
            assessment
                .blocking_metrics
                .iter()
                .any(|f| f.code == QualityMetricCode::CandidateFixtureNotProduction),
            "must have CANDIDATE_FIXTURE_NOT_PRODUCTION when fixture candidate in non-fixture mode"
        );
    }

    #[test]
    fn fixture_candidate_in_fixture_mode_passes() {
        let mut c = make_candidate("c1", "https://example.com/img.jpg", "qp-1", None);
        c.provider_kind = "fixture".into();

        // In fixture mode, fixture candidates are allowed
        let assessment =
            validate_candidate_mechanical_v11(&c, "qp-1", &HashSet::new(), &[], &[], true);
        assert!(assessment.passed);
    }
}

// =============================================================================
// Image v1.1 mechanical validation tests
// =============================================================================

mod image_mechanical_v11 {
    use super::*;
    use image_retrieval::domain::candidate::{CandidateId, ImageDimensions};
    use image_retrieval::domain::image::{
        ImageMechanicalAssessment, RetrievalArtifactResult, RetrievalStatus,
    };
    use image_retrieval::domain::metrics::QualityMetricCode;
    use image_retrieval::quality::image::mechanical::validate_image_mechanical_v11;

    fn make_complete_result() -> RetrievalArtifactResult {
        RetrievalArtifactResult {
            retrieval_job_id: "ret-1".into(),
            candidate_id: "cand-1".into(),
            query_plan_id: "qp-1".into(),
            channel_id: "web_fetch".into(),
            retrieval_status: RetrievalStatus::Complete,
            local_artifact_path: Some("/tmp/local.jpg".into()),
            source_artifact_path: Some("/tmp/source.jpg".into()),
            source_sidecar_path: Some("/tmp/sidecar.json".into()),
            content_summary_path: Some("/tmp/summary.txt".into()),
            task_report_path: Some("/tmp/report.json".into()),
            visual_description_path: Some("/tmp/vd.txt".into()),
            checksum_sha256: Some("abc123def456".into()),
            content_type: Some("image/jpeg".into()),
            file_size_bytes: Some(4096),
            image_dimensions: Some(ImageDimensions {
                width: 800,
                height: 600,
            }),
            media_type_match: true,
            fetch_trace: vec!["direct_fetch".into()],
            failure_reason: None,
        }
    }

    // -----------------------------------------------------------------------
    // AC-010: Image missing local artifact is mechanically blocked
    // -----------------------------------------------------------------------

    #[test]
    fn image_missing_local_artifact_is_blocked() {
        let mut result = make_complete_result();
        result.local_artifact_path = None;

        let assessment = validate_image_mechanical_v11(&result, "qp-1", "cand-1", false);
        assert!(!assessment.passed);
        assert!(
            assessment
                .blocking_metrics
                .iter()
                .any(|f| f.code == QualityMetricCode::ImageLocalArtifactMissing),
            "must have IMAGE_LOCAL_ARTIFACT_MISSING"
        );
    }

    // -----------------------------------------------------------------------
    // AC-010: Image missing source artifact is mechanically blocked
    // -----------------------------------------------------------------------

    #[test]
    fn image_missing_source_artifact_is_blocked() {
        let mut result = make_complete_result();
        result.source_artifact_path = None;

        let assessment = validate_image_mechanical_v11(&result, "qp-1", "cand-1", false);
        assert!(!assessment.passed);
        assert!(
            assessment
                .blocking_metrics
                .iter()
                .any(|f| f.code == QualityMetricCode::ImageSourceArtifactMissing),
            "must have IMAGE_SOURCE_ARTIFACT_MISSING"
        );
    }

    // -----------------------------------------------------------------------
    // AC-010: Image missing sidecar is mechanically blocked
    // -----------------------------------------------------------------------

    #[test]
    fn image_missing_sidecar_is_blocked() {
        let mut result = make_complete_result();
        result.source_sidecar_path = None;

        let assessment = validate_image_mechanical_v11(&result, "qp-1", "cand-1", false);
        assert!(!assessment.passed);
        assert!(
            assessment
                .blocking_metrics
                .iter()
                .any(|f| f.code == QualityMetricCode::ImageSidecarMissing),
            "must have IMAGE_SIDECAR_MISSING"
        );
    }

    // -----------------------------------------------------------------------
    // AC-010: Image missing summary is mechanically blocked
    // -----------------------------------------------------------------------

    #[test]
    fn image_missing_summary_is_blocked() {
        let mut result = make_complete_result();
        result.content_summary_path = None;

        let assessment = validate_image_mechanical_v11(&result, "qp-1", "cand-1", false);
        assert!(!assessment.passed);
        assert!(
            assessment
                .blocking_metrics
                .iter()
                .any(|f| f.code == QualityMetricCode::ImageSummaryMissing),
            "must have IMAGE_SUMMARY_MISSING"
        );
    }

    // -----------------------------------------------------------------------
    // AC-010: Image missing task report is mechanically blocked
    // -----------------------------------------------------------------------

    #[test]
    fn image_missing_task_report_is_blocked() {
        let mut result = make_complete_result();
        result.task_report_path = None;

        let assessment = validate_image_mechanical_v11(&result, "qp-1", "cand-1", false);
        assert!(!assessment.passed);
        assert!(
            assessment
                .blocking_metrics
                .iter()
                .any(|f| f.code == QualityMetricCode::ImageTaskReportMissing),
            "must have IMAGE_TASK_REPORT_MISSING"
        );
    }

    // -----------------------------------------------------------------------
    // AC-010: Image missing visual description is mechanically blocked
    // -----------------------------------------------------------------------

    #[test]
    fn image_missing_visual_description_is_blocked() {
        let mut result = make_complete_result();
        result.visual_description_path = None;

        let assessment = validate_image_mechanical_v11(&result, "qp-1", "cand-1", false);
        assert!(!assessment.passed);
        assert!(
            assessment
                .blocking_metrics
                .iter()
                .any(|f| f.code == QualityMetricCode::ImageVisualDescriptionMissing),
            "must have IMAGE_VISUAL_DESCRIPTION_MISSING"
        );
    }

    // -----------------------------------------------------------------------
    // AC-010: Image missing checksum is mechanically blocked
    // -----------------------------------------------------------------------

    #[test]
    fn image_missing_checksum_is_blocked() {
        let mut result = make_complete_result();
        result.checksum_sha256 = None;

        let assessment = validate_image_mechanical_v11(&result, "qp-1", "cand-1", false);
        assert!(!assessment.passed);
        assert!(
            assessment
                .blocking_metrics
                .iter()
                .any(|f| f.code == QualityMetricCode::ImageChecksumMissing),
            "must have IMAGE_CHECKSUM_MISSING"
        );
    }

    // -----------------------------------------------------------------------
    // AC-010: Image with media mismatch is mechanically blocked
    // -----------------------------------------------------------------------

    #[test]
    fn image_media_type_mismatch_is_blocked() {
        let mut result = make_complete_result();
        result.media_type_match = false;

        let assessment = validate_image_mechanical_v11(&result, "qp-1", "cand-1", false);
        assert!(!assessment.passed);
        assert!(
            assessment
                .blocking_metrics
                .iter()
                .any(|f| f.code == QualityMetricCode::ImageMediaTypeMismatch),
            "must have IMAGE_MEDIA_TYPE_MISMATCH"
        );
    }

    // -----------------------------------------------------------------------
    // AC-010: Metadata-only result is mechanically blocked
    // -----------------------------------------------------------------------

    #[test]
    fn image_metadata_only_is_blocked() {
        let mut result = make_complete_result();
        result.local_artifact_path = None;
        result.source_artifact_path = None;

        let assessment = validate_image_mechanical_v11(&result, "qp-1", "cand-1", false);
        assert!(!assessment.passed);
        assert!(
            assessment
                .blocking_metrics
                .iter()
                .any(|f| f.code == QualityMetricCode::ImageMetadataOnlyResult),
            "must have IMAGE_METADATA_ONLY_RESULT"
        );
    }

    // -----------------------------------------------------------------------
    // AC-010: Ownership mismatch (query_plan_id) is mechanically blocked
    // -----------------------------------------------------------------------

    #[test]
    fn image_query_ownership_mismatch_is_blocked() {
        let result = make_complete_result();

        let assessment = validate_image_mechanical_v11(&result, "qp-wrong", "cand-1", false);
        assert!(!assessment.passed);
        assert!(
            assessment
                .blocking_metrics
                .iter()
                .any(|f| f.code == QualityMetricCode::ImageJobOwnershipMismatch),
            "must have IMAGE_JOB_OWNERSHIP_MISMATCH"
        );
    }

    // -----------------------------------------------------------------------
    // AC-010: Ownership mismatch (candidate_id) is mechanically blocked
    // -----------------------------------------------------------------------

    #[test]
    fn image_candidate_ownership_mismatch_is_blocked() {
        let result = make_complete_result();

        let assessment = validate_image_mechanical_v11(&result, "qp-1", "cand-wrong", false);
        assert!(!assessment.passed);
        assert!(
            assessment
                .blocking_metrics
                .iter()
                .any(|f| f.code == QualityMetricCode::ImageJobOwnershipMismatch),
            "must have IMAGE_JOB_OWNERSHIP_MISMATCH for candidate_id mismatch"
        );
    }

    // -----------------------------------------------------------------------
    // AC-010: Not complete retrieval status is blocked
    // -----------------------------------------------------------------------

    #[test]
    fn image_retrieval_not_complete_is_blocked() {
        let mut result = make_complete_result();
        result.retrieval_status = RetrievalStatus::Failed;

        let assessment = validate_image_mechanical_v11(&result, "qp-1", "cand-1", false);
        assert!(!assessment.passed);
        assert!(
            assessment
                .blocking_metrics
                .iter()
                .any(|f| f.code == QualityMetricCode::ImageRetrievalNotComplete),
            "must have IMAGE_RETRIEVAL_NOT_COMPLETE"
        );
    }

    // -----------------------------------------------------------------------
    // AC-010: Fixture retrieval result in production is blocked
    // -----------------------------------------------------------------------

    #[test]
    fn fixture_retrieval_in_production_is_blocked() {
        let mut result = make_complete_result();
        result.channel_id = "fixture".into();

        let assessment = validate_image_mechanical_v11(&result, "qp-1", "cand-1", false);
        assert!(!assessment.passed);
        assert!(
            assessment
                .blocking_metrics
                .iter()
                .any(|f| f.code == QualityMetricCode::ImageFixtureNotProduction),
            "must have IMAGE_FIXTURE_NOT_PRODUCTION"
        );
    }

    // -----------------------------------------------------------------------
    // Complete result passes mechanical and includes reference metrics
    // -----------------------------------------------------------------------

    #[test]
    fn complete_result_passes_mechanical_with_reference_metrics() {
        let result = make_complete_result();

        let assessment = validate_image_mechanical_v11(&result, "qp-1", "cand-1", false);
        assert!(assessment.passed);
        assert!(assessment.blocking_metrics.is_empty());
        assert!(!assessment.reference_metrics.is_empty());

        // Should have dimensions reference
        assert!(
            assessment
                .reference_metrics
                .iter()
                .any(|f| f.code == QualityMetricCode::ImageDimensionsRef),
            "should have IMAGE_DIMENSIONS reference"
        );
        // Should have content type reference
        assert!(
            assessment
                .reference_metrics
                .iter()
                .any(|f| f.code == QualityMetricCode::ImageContentTypeRef),
            "should have IMAGE_CONTENT_TYPE reference"
        );
        // Should have fetch trace reference
        assert!(
            assessment
                .reference_metrics
                .iter()
                .any(|f| f.code == QualityMetricCode::ImageFetchTraceQuality),
            "should have IMAGE_FETCH_TRACE_QUALITY reference"
        );
    }
}

// =============================================================================
// VlmEvaluationPort tests (readiness, fixture evaluator, error states)
// =============================================================================

mod vlm_evaluation_port {
    use super::*;
    use image_retrieval::domain::candidate::{
        CandidateId, CandidateMechanicalAssessment, CandidateQualityDecision,
        CandidateQualityStatus, QualityPolicyContext, RetrievableCandidateBatch,
        VlmCandidateEvaluationRequest, VlmEvaluationResponse, VlmEvaluatorKind, VlmResponseStatus,
        VlmSubjectDecision, VlmSubjectDecisionKind,
    };
    use image_retrieval::domain::config::VlmEvaluationConfig;
    use image_retrieval::domain::metrics::{
        QualityDiagnostic, QualityDiagnosticCode, QualityExecutionBlock, QualityPhase,
        QualitySeverity,
    };
    use image_retrieval::ports::{
        FixtureVlmEvaluator, VlmEvaluationError, VlmEvaluationFailureCode, VlmEvaluationPort,
        VlmEvaluationReadinessReport,
    };

    // -----------------------------------------------------------------------
    // AC-006: VlmEvaluationPort readiness report
    // -----------------------------------------------------------------------

    #[test]
    fn vlm_readiness_available_when_configured() {
        let evaluator = FixtureVlmEvaluator::always_approve();
        let config = VlmEvaluationConfig {
            fixture_mode: true,
            ..Default::default()
        };
        let report = evaluator.readiness(&config);
        assert!(report.available);
        assert!(report.enabled);
        assert!(report.fixture_mode);
        assert!(report.failure_code.is_none());
    }

    #[test]
    fn vlm_readiness_not_available_when_disabled() {
        let report = VlmEvaluationReadinessReport::not_available(
            VlmEvaluationFailureCode::VlmEvaluationDisabled,
            false,
            vec!["VLM is disabled in config".into()],
        );
        assert!(!report.available);
        assert!(!report.enabled);
        assert_eq!(
            report.failure_code,
            Some(VlmEvaluationFailureCode::VlmEvaluationDisabled)
        );
    }

    #[test]
    fn vlm_readiness_credential_missing() {
        let report = VlmEvaluationReadinessReport::not_available(
            VlmEvaluationFailureCode::VlmEvaluationCredentialMissing,
            false,
            vec!["QWEN_API_TOKEN not set".into()],
        );
        assert!(!report.available);
    }

    // -----------------------------------------------------------------------
    // AC-006: Fixture evaluator blocked in production
    // -----------------------------------------------------------------------

    #[test]
    fn fixture_evaluator_blocked_in_production() {
        let evaluator = FixtureVlmEvaluator::always_approve();
        let config = VlmEvaluationConfig {
            fixture_mode: false,
            ..Default::default()
        };
        let report = evaluator.readiness(&config);
        assert!(!report.available);
        assert_eq!(
            report.failure_code,
            Some(VlmEvaluationFailureCode::VlmEvaluationFixtureNotProduction)
        );
    }

    #[test]
    fn fixture_evaluator_cannot_be_used_in_production_evaluation() {
        let evaluator = FixtureVlmEvaluator::always_approve();
        let request = VlmCandidateEvaluationRequest {
            request_id: "req-1".into(),
            query_plan_id: "qp-1".into(),
            full_attempt_count: 1,
            retry_count: 0,
            semantic_description: "test".into(),
            quality: image_retrieval::domain::query_plan::QualityTier::General,
            quality_requirements: Default::default(),
            visual_requirements: vec![],
            negative_scope: vec![],
            candidates: vec![],
            policy_context: QualityPolicyContext {
                fixture_mode: false,
                ..Default::default()
            },
            model: "qwen-3.5".into(),
            evaluator_provider_id: "fixture_vlm".into(),
            fixture_mode: false,
        };

        let result = evaluator.evaluate_candidates(&request);
        assert!(result.is_err());
        match result {
            Err(VlmEvaluationError::FixtureNotProduction) => {}
            _ => panic!("expected FixtureNotProduction"),
        }
    }

    // -----------------------------------------------------------------------
    // AC-006: Qwen unavailable produces execution-blocked evidence
    // -----------------------------------------------------------------------

    #[test]
    fn vlm_unavailable_produces_execution_block() {
        let block = QualityExecutionBlock::vlm_unavailable(
            QualityPhase::Candidate,
            "QWEN_API_TOKEN not set",
            3,
        );
        assert_eq!(block.dependency, "Qwen 3.5 VLM");
        assert_eq!(block.failure_code, "VLM_EVALUATION_UNAVAILABLE");
        assert!(block.is_permanent);
        assert_eq!(block.pending_subject_count, 3);
    }

    #[test]
    fn vlm_fixture_in_production_produces_execution_block() {
        let block = QualityExecutionBlock::vlm_fixture_in_production(QualityPhase::Image, 2);
        assert_eq!(block.failure_code, "VLM_EVALUATION_FIXTURE_NOT_PRODUCTION");
        assert!(block.is_permanent);
    }

    // -----------------------------------------------------------------------
    // AC-006: VlmEvaluationError covers all required failure modes
    // -----------------------------------------------------------------------

    #[test]
    fn vlm_evaluation_error_covers_required_modes() {
        // Disabled
        let err = VlmEvaluationError::Disabled;
        assert_eq!(
            err.to_failure_code(),
            VlmEvaluationFailureCode::VlmEvaluationDisabled
        );

        // Credential missing
        let err = VlmEvaluationError::CredentialMissing {
            env_var: "QWEN_API_TOKEN".into(),
        };
        assert_eq!(
            err.to_failure_code(),
            VlmEvaluationFailureCode::VlmEvaluationCredentialMissing
        );

        // Endpoint missing
        let err = VlmEvaluationError::EndpointMissing;
        assert_eq!(
            err.to_failure_code(),
            VlmEvaluationFailureCode::VlmEvaluationEndpointMissing
        );

        // Timeout
        let err = VlmEvaluationError::Timeout {
            message: "30s timeout".into(),
        };
        assert_eq!(
            err.to_failure_code(),
            VlmEvaluationFailureCode::VlmEvaluationTimeout
        );

        // Invalid response
        let err = VlmEvaluationError::InvalidResponse {
            message: "cardinality mismatch".into(),
        };
        assert_eq!(
            err.to_failure_code(),
            VlmEvaluationFailureCode::VlmEvaluationResponseInvalid
        );

        // Fixture not production
        let err = VlmEvaluationError::FixtureNotProduction;
        assert_eq!(
            err.to_failure_code(),
            VlmEvaluationFailureCode::VlmEvaluationFixtureNotProduction
        );

        // Unavailable
        let err = VlmEvaluationError::Unavailable {
            reason: "general failure".into(),
        };
        assert_eq!(
            err.to_failure_code(),
            VlmEvaluationFailureCode::VlmEvaluationUnavailable
        );
    }

    // -----------------------------------------------------------------------
    // AC-006: VLM response cardinality validation
    // -----------------------------------------------------------------------

    #[test]
    fn vlm_response_cardinality_mismatch_detected() {
        let response = VlmEvaluationResponse {
            request_id: "req-1".into(),
            evaluator_id: "qwen".into(),
            evaluator_kind: VlmEvaluatorKind::Qwen35Vlm,
            status: VlmResponseStatus::Incomplete,
            decisions: vec![VlmSubjectDecision {
                subject_id: "cand-1".into(),
                decision: VlmSubjectDecisionKind::Approve,
                confidence: Some(0.9),
                reason_codes: vec![],
                rationale_summary: "good".into(),
                evidence_refs: vec![],
            }],
            diagnostics: vec![],
            audit_ref: None,
            redaction_applied: false,
        };

        // Submitted 3 candidates but got 1 decision
        let result = response.validate_cardinality(3);
        assert!(result.is_err());
        assert!(response.has_missing_decisions());
    }

    #[test]
    fn vlm_response_valid_cardinality() {
        let response = VlmEvaluationResponse {
            request_id: "req-1".into(),
            evaluator_id: "qwen".into(),
            evaluator_kind: VlmEvaluatorKind::Qwen35Vlm,
            status: VlmResponseStatus::Complete,
            decisions: vec![
                VlmSubjectDecision {
                    subject_id: "cand-1".into(),
                    decision: VlmSubjectDecisionKind::Approve,
                    confidence: Some(0.9),
                    reason_codes: vec![],
                    rationale_summary: "good".into(),
                    evidence_refs: vec![],
                },
                VlmSubjectDecision {
                    subject_id: "cand-2".into(),
                    decision: VlmSubjectDecisionKind::Reject,
                    confidence: Some(0.1),
                    reason_codes: vec![],
                    rationale_summary: "bad".into(),
                    evidence_refs: vec![],
                },
            ],
            diagnostics: vec![],
            audit_ref: None,
            redaction_applied: false,
        };

        assert!(response.validate_cardinality(2).is_ok());
        assert!(!response.has_missing_decisions());
    }

    // -----------------------------------------------------------------------
    // AC-006: Fixture evaluator simulate_unavailable
    // -----------------------------------------------------------------------

    #[test]
    fn fixture_evaluator_simulate_unavailable_returns_error() {
        let evaluator = FixtureVlmEvaluator::unavailable();
        let request = VlmCandidateEvaluationRequest {
            request_id: "req-1".into(),
            query_plan_id: "qp-1".into(),
            full_attempt_count: 1,
            retry_count: 0,
            semantic_description: "test".into(),
            quality: image_retrieval::domain::query_plan::QualityTier::General,
            quality_requirements: Default::default(),
            visual_requirements: vec![],
            negative_scope: vec![],
            candidates: vec![],
            policy_context: QualityPolicyContext {
                fixture_mode: true,
                ..Default::default()
            },
            model: "qwen-3.5".into(),
            evaluator_provider_id: "fixture_vlm".into(),
            fixture_mode: true,
        };

        let result = evaluator.evaluate_candidates(&request);
        assert!(result.is_err());
        match result {
            Err(VlmEvaluationError::Unavailable { .. }) => {}
            _ => panic!("expected Unavailable error"),
        }
    }
}

// =============================================================================
// CandidateQualityDecision and RetrievableCandidateBatch tests
// =============================================================================

mod candidate_quality_decision {
    use super::*;
    use image_retrieval::domain::candidate::{
        CandidateId, CandidateMechanicalAssessment, CandidateQualityDecision,
        CandidateQualityStatus, RetrievableCandidateBatch,
    };
    use image_retrieval::domain::metrics::QualityMetricCode;

    // -----------------------------------------------------------------------
    // Decision merging: mechanical pass + VLM approve = Retrievable
    // -----------------------------------------------------------------------

    #[test]
    fn mechanical_pass_plus_vlm_approve_equals_retrievable() {
        use image_retrieval::domain::candidate::{VlmSubjectDecision, VlmSubjectDecisionKind};

        let mechanical = CandidateMechanicalAssessment::pass(CandidateId::new("cand-1"), "qp-1");
        let vlm = VlmSubjectDecision {
            subject_id: "cand-1".into(),
            decision: VlmSubjectDecisionKind::Approve,
            confidence: Some(0.95),
            reason_codes: vec!["semantic_match".into()],
            rationale_summary: "Image matches query description".into(),
            evidence_refs: vec![],
        };

        let decision = CandidateQualityDecision::merged(
            CandidateId::new("cand-1"),
            "qp-1",
            &mechanical,
            Some(&vlm),
        );

        assert!(decision.is_retrievable());
        assert_eq!(decision.final_status, CandidateQualityStatus::Retrievable);
        assert!(decision.mechanical_passed);
        assert!(decision.vlm_passed);
    }

    // -----------------------------------------------------------------------
    // Decision merging: mechanical pass + VLM reject = SubjectivelyRejected
    // -----------------------------------------------------------------------

    #[test]
    fn mechanical_pass_plus_vlm_reject_equals_subjectively_rejected() {
        use image_retrieval::domain::candidate::{VlmSubjectDecision, VlmSubjectDecisionKind};

        let mechanical = CandidateMechanicalAssessment::pass(CandidateId::new("cand-1"), "qp-1");
        let vlm = VlmSubjectDecision {
            subject_id: "cand-1".into(),
            decision: VlmSubjectDecisionKind::Reject,
            confidence: Some(0.1),
            reason_codes: vec!["mismatch".into()],
            rationale_summary: "Image does not match query".into(),
            evidence_refs: vec![],
        };

        let decision = CandidateQualityDecision::merged(
            CandidateId::new("cand-1"),
            "qp-1",
            &mechanical,
            Some(&vlm),
        );

        assert!(!decision.is_retrievable());
        assert_eq!(
            decision.final_status,
            CandidateQualityStatus::SubjectivelyRejected
        );
    }

    // -----------------------------------------------------------------------
    // Decision merging: mechanical pass + VLM uncertain = SubjectivelyUncertain
    // -----------------------------------------------------------------------

    #[test]
    fn mechanical_pass_plus_vlm_uncertain_equals_subjectively_uncertain() {
        use image_retrieval::domain::candidate::{VlmSubjectDecision, VlmSubjectDecisionKind};

        let mechanical = CandidateMechanicalAssessment::pass(CandidateId::new("cand-1"), "qp-1");
        let vlm = VlmSubjectDecision {
            subject_id: "cand-1".into(),
            decision: VlmSubjectDecisionKind::Uncertain,
            confidence: None,
            reason_codes: vec!["ambiguous".into()],
            rationale_summary: "Cannot determine if image matches".into(),
            evidence_refs: vec![],
        };

        let decision = CandidateQualityDecision::merged(
            CandidateId::new("cand-1"),
            "qp-1",
            &mechanical,
            Some(&vlm),
        );

        assert!(!decision.is_retrievable());
        assert_eq!(
            decision.final_status,
            CandidateQualityStatus::SubjectivelyUncertain
        );
    }

    // -----------------------------------------------------------------------
    // Decision merging: mechanical pass + VLM unexecutable = ExecutionBlocked
    // -----------------------------------------------------------------------

    #[test]
    fn mechanical_pass_plus_vlm_unexecutable_equals_execution_blocked() {
        use image_retrieval::domain::candidate::{VlmSubjectDecision, VlmSubjectDecisionKind};

        let mechanical = CandidateMechanicalAssessment::pass(CandidateId::new("cand-1"), "qp-1");
        let vlm = VlmSubjectDecision {
            subject_id: "cand-1".into(),
            decision: VlmSubjectDecisionKind::Unexecutable,
            confidence: None,
            reason_codes: vec![],
            rationale_summary: "VLM unavailable".into(),
            evidence_refs: vec![],
        };

        let decision = CandidateQualityDecision::merged(
            CandidateId::new("cand-1"),
            "qp-1",
            &mechanical,
            Some(&vlm),
        );

        assert!(!decision.is_retrievable());
        assert_eq!(
            decision.final_status,
            CandidateQualityStatus::ExecutionBlocked
        );
    }

    // -----------------------------------------------------------------------
    // Decision merging: VLM unavailable (None) = ExecutionBlocked
    // -----------------------------------------------------------------------

    #[test]
    fn mechanical_pass_plus_no_vlm_equals_execution_blocked() {
        let mechanical = CandidateMechanicalAssessment::pass(CandidateId::new("cand-1"), "qp-1");

        let decision = CandidateQualityDecision::merged(
            CandidateId::new("cand-1"),
            "qp-1",
            &mechanical,
            None, // VLM unavailable
        );

        assert!(!decision.is_retrievable());
        assert_eq!(
            decision.final_status,
            CandidateQualityStatus::ExecutionBlocked
        );
    }

    // -----------------------------------------------------------------------
    // Mechanically rejected decision
    // -----------------------------------------------------------------------

    #[test]
    fn mechanically_rejected_decision() {
        use image_retrieval::domain::metrics::MetricFact;

        let fact = MetricFact::candidate_blocking(
            QualityMetricCode::CandidateImageUrlMissing,
            "cand-1",
            "qp-1",
            "image URL missing",
        );

        let decision = CandidateQualityDecision::mechanically_rejected(
            CandidateId::new("cand-1"),
            "qp-1",
            vec![fact],
        );

        assert!(!decision.is_retrievable());
        assert_eq!(
            decision.final_status,
            CandidateQualityStatus::MechanicallyRejected
        );
        assert!(!decision.mechanical_passed);
        assert!(!decision.vlm_passed);
        assert!(decision.vlm_decision.is_none()); // never reached VLM
    }

    // -----------------------------------------------------------------------
    // RetrievableCandidateBatch preserves rejected decisions
    // -----------------------------------------------------------------------

    #[test]
    fn retrievable_batch_preserves_rejected_decisions() {
        use image_retrieval::domain::candidate::RetrievableCandidate;

        let batch = RetrievableCandidateBatch {
            query_plan_id: "qp-1".into(),
            full_attempt_count: 1,
            retry_count: 0,
            retrieval_batch_target: 4,
            candidates: vec![RetrievableCandidate {
                candidate: image_retrieval::domain::candidate::CandidateRecord::minimal(
                    CandidateId::new("cand-1"),
                    image_retrieval::domain::candidate::ProviderId::new("p1"),
                    "https://example.com/1.jpg",
                ),
                candidate_quality_decision: CandidateQualityDecision::merged(
                    CandidateId::new("cand-1"),
                    "qp-1",
                    &CandidateMechanicalAssessment::pass(CandidateId::new("cand-1"), "qp-1"),
                    Some(&image_retrieval::domain::candidate::VlmSubjectDecision {
                        subject_id: "cand-1".into(),
                        decision:
                            image_retrieval::domain::candidate::VlmSubjectDecisionKind::Approve,
                        confidence: Some(0.9),
                        reason_codes: vec![],
                        rationale_summary: "good".into(),
                        evidence_refs: vec![],
                    }),
                ),
                retrieval_priority: 5,
                primary_image_url: "https://example.com/1.jpg".into(),
                source_page_url: None,
                thumbnail_url: None,
                expected_mime_type: None,
                license_hint: None,
                provenance_refs: vec![],
            }],
            rejected_decisions: vec![CandidateQualityDecision::mechanically_rejected(
                CandidateId::new("cand-2"),
                "qp-1",
                vec![
                    image_retrieval::domain::metrics::MetricFact::candidate_blocking(
                        QualityMetricCode::CandidateDuplicateBlocked,
                        "cand-2",
                        "qp-1",
                        "duplicate",
                    ),
                ],
            )],
            execution_blocking_facts: vec![],
        };

        assert_eq!(batch.len(), 1);
        assert!(!batch.is_empty());
        assert_eq!(batch.rejected_decisions.len(), 1);
        assert_eq!(batch.retrieval_batch_target, 4);
    }

    #[test]
    fn empty_retrievable_batch() {
        let batch = RetrievableCandidateBatch::empty("qp-1", 1, 0, 4);
        assert!(batch.is_empty());
        assert_eq!(batch.len(), 0);
    }
}

// =============================================================================
// ImageAcceptanceDecisionV11 tests
// =============================================================================

mod image_acceptance_decision_v11 {
    use super::*;
    use image_retrieval::domain::candidate::{
        CandidateId, VlmSubjectDecision, VlmSubjectDecisionKind,
    };
    use image_retrieval::domain::image::{
        ImageAcceptanceDecisionV11, ImageAcceptanceOutcome, ImageAcceptanceStatus,
        ImageArtifactRefs, ImageMechanicalAssessment,
    };
    use image_retrieval::domain::metrics::{MetricFact, QualityMetricCode, QualitySummary};

    // -----------------------------------------------------------------------
    // VLM image approve + mechanical pass = DeliveredQualified
    // -----------------------------------------------------------------------

    #[test]
    fn image_vlm_approve_plus_mechanical_pass_equals_delivered_qualified() {
        let mechanical =
            ImageMechanicalAssessment::pass(CandidateId::new("cand-1"), "ret-1", "qp-1");
        let vlm = VlmSubjectDecision {
            subject_id: "cand-1".into(),
            decision: VlmSubjectDecisionKind::Approve,
            confidence: Some(0.95),
            reason_codes: vec!["visual_match".into()],
            rationale_summary: "Image matches query".into(),
            evidence_refs: vec![],
        };

        let artifact_refs = ImageArtifactRefs {
            local_artifact_path: Some("/tmp/local.jpg".into()),
            source_artifact_path: Some("/tmp/source.jpg".into()),
            source_sidecar_path: Some("/tmp/sidecar.json".into()),
            content_summary_path: Some("/tmp/summary.txt".into()),
            task_report_path: Some("/tmp/report.json".into()),
            visual_description_path: Some("/tmp/vd.txt".into()),
            checksum_sha256: Some("abc123".into()),
        };

        let decision = ImageAcceptanceDecisionV11::merged(
            CandidateId::new("cand-1"),
            "ret-1",
            "qp-1",
            &mechanical,
            Some(&vlm),
            artifact_refs,
        );

        assert!(decision.is_delivered_qualified());
        assert_eq!(
            decision.final_status,
            ImageAcceptanceStatus::DeliveredQualified
        );
        assert!(decision.mechanical_passed);
        assert!(decision.vlm_passed);
    }

    // -----------------------------------------------------------------------
    // VLM image reject does not count as delivered
    // -----------------------------------------------------------------------

    #[test]
    fn image_vlm_reject_not_delivered() {
        let mechanical =
            ImageMechanicalAssessment::pass(CandidateId::new("cand-1"), "ret-1", "qp-1");
        let vlm = VlmSubjectDecision {
            subject_id: "cand-1".into(),
            decision: VlmSubjectDecisionKind::Reject,
            confidence: Some(0.1),
            reason_codes: vec![],
            rationale_summary: "poor match".into(),
            evidence_refs: vec![],
        };

        let decision = ImageAcceptanceDecisionV11::merged(
            CandidateId::new("cand-1"),
            "ret-1",
            "qp-1",
            &mechanical,
            Some(&vlm),
            ImageArtifactRefs::default(),
        );

        assert!(!decision.is_delivered_qualified());
        assert_eq!(
            decision.final_status,
            ImageAcceptanceStatus::SubjectivelyRejected
        );
    }

    // -----------------------------------------------------------------------
    // VLM image uncertain does not count as delivered
    // -----------------------------------------------------------------------

    #[test]
    fn image_vlm_uncertain_not_delivered() {
        let mechanical =
            ImageMechanicalAssessment::pass(CandidateId::new("cand-1"), "ret-1", "qp-1");
        let vlm = VlmSubjectDecision {
            subject_id: "cand-1".into(),
            decision: VlmSubjectDecisionKind::Uncertain,
            confidence: None,
            reason_codes: vec![],
            rationale_summary: "ambiguous".into(),
            evidence_refs: vec![],
        };

        let decision = ImageAcceptanceDecisionV11::merged(
            CandidateId::new("cand-1"),
            "ret-1",
            "qp-1",
            &mechanical,
            Some(&vlm),
            ImageArtifactRefs::default(),
        );

        assert!(!decision.is_delivered_qualified());
        assert_eq!(
            decision.final_status,
            ImageAcceptanceStatus::SubjectivelyUncertain
        );
    }

    // -----------------------------------------------------------------------
    // Mechanically rejected is not delivered (even if VLM would approve)
    // -----------------------------------------------------------------------

    #[test]
    fn mechanical_block_prevents_delivery_even_with_vlm_approve() {
        use image_retrieval::domain::metrics::MetricFact;

        let mechanical = ImageMechanicalAssessment::blocked(
            CandidateId::new("cand-1"),
            "ret-1",
            "qp-1",
            vec![MetricFact::image_blocking(
                QualityMetricCode::ImageChecksumMissing,
                "ret-1",
                "qp-1",
                "checksum missing",
            )],
        );
        let vlm = VlmSubjectDecision {
            subject_id: "cand-1".into(),
            decision: VlmSubjectDecisionKind::Approve,
            confidence: Some(0.9),
            reason_codes: vec![],
            rationale_summary: "good".into(),
            evidence_refs: vec![],
        };

        let decision = ImageAcceptanceDecisionV11::merged(
            CandidateId::new("cand-1"),
            "ret-1",
            "qp-1",
            &mechanical,
            Some(&vlm),
            ImageArtifactRefs::default(),
        );

        // VLM approve cannot override mechanical rejection
        assert!(!decision.is_delivered_qualified());
        assert_eq!(
            decision.final_status,
            ImageAcceptanceStatus::MechanicallyRejected
        );
    }

    // -----------------------------------------------------------------------
    // ImageAcceptanceOutcome separates accepted from rejected
    // -----------------------------------------------------------------------

    #[test]
    fn image_acceptance_outcome_separates_accepted() {
        let mech1 = ImageMechanicalAssessment::pass(CandidateId::new("cand-1"), "ret-1", "qp-1");
        let mech2 = ImageMechanicalAssessment::pass(CandidateId::new("cand-2"), "ret-2", "qp-1");

        let vlm_approve = VlmSubjectDecision {
            subject_id: "cand-1".into(),
            decision: VlmSubjectDecisionKind::Approve,
            confidence: Some(0.9),
            reason_codes: vec![],
            rationale_summary: "good".into(),
            evidence_refs: vec![],
        };
        let vlm_reject = VlmSubjectDecision {
            subject_id: "cand-2".into(),
            decision: VlmSubjectDecisionKind::Reject,
            confidence: Some(0.1),
            reason_codes: vec![],
            rationale_summary: "bad".into(),
            evidence_refs: vec![],
        };

        let artifacts = ImageArtifactRefs {
            checksum_sha256: Some("abc".into()),
            ..Default::default()
        };

        let d1 = ImageAcceptanceDecisionV11::merged(
            CandidateId::new("cand-1"),
            "ret-1",
            "qp-1",
            &mech1,
            Some(&vlm_approve),
            artifacts.clone(),
        );
        let d2 = ImageAcceptanceDecisionV11::merged(
            CandidateId::new("cand-2"),
            "ret-2",
            "qp-1",
            &mech2,
            Some(&vlm_reject),
            artifacts,
        );

        let outcome = ImageAcceptanceOutcome::new(
            "qp-1",
            1,
            0,
            vec![d1, d2],
            vec![],
            vec![],
            QualitySummary::default(),
        );

        assert_eq!(outcome.accepted_images.len(), 1);
        assert_eq!(outcome.decisions.len(), 2);
        assert_eq!(outcome.full_attempt_count, 1);
        assert_eq!(outcome.retry_count, 0);
    }
}

// =============================================================================
// Quality trace links and contracts
// =============================================================================

mod quality_trace_links {
    use super::*;
    use image_retrieval::domain::candidate::{
        CandidateId, CandidateMechanicalAssessment, CandidateQualityDecision,
        CandidateQualityStatus, RetrievableCandidateBatch, VlmSubjectDecision,
        VlmSubjectDecisionKind,
    };
    use image_retrieval::domain::metrics::{
        MetricFact, QualityDiagnostic, QualityDiagnosticCode, QualityExecutionBlock,
        QualityMetricCode, QualityPhase, QualitySeverity,
    };

    // -----------------------------------------------------------------------
    // Trace links: query_plan_id links through all quality outputs
    // -----------------------------------------------------------------------

    #[test]
    fn query_plan_id_traces_through_candidate_quality() {
        let mechanical =
            CandidateMechanicalAssessment::pass(CandidateId::new("cand-1"), "qp-trace-001");
        assert_eq!(mechanical.query_plan_id, "qp-trace-001");

        let vlm = VlmSubjectDecision {
            subject_id: "cand-1".into(),
            decision: VlmSubjectDecisionKind::Approve,
            confidence: Some(0.9),
            reason_codes: vec![],
            rationale_summary: "good".into(),
            evidence_refs: vec![],
        };

        let decision = CandidateQualityDecision::merged(
            CandidateId::new("cand-1"),
            "qp-trace-001",
            &mechanical,
            Some(&vlm),
        );
        assert_eq!(decision.query_plan_id, "qp-trace-001");

        // Fact also carries query_plan_id
        let fact = MetricFact::candidate_blocking(
            QualityMetricCode::CandidateImageUrlMissing,
            "cand-1",
            "qp-trace-001",
            "missing URL",
        );
        assert_eq!(fact.query_plan_id, "qp-trace-001");
    }

    // -----------------------------------------------------------------------
    // Trace links: candidate_id links search to quality to retrieval
    // -----------------------------------------------------------------------

    #[test]
    fn candidate_id_traces_through_quality_to_retrieval_batch() {
        let batch = RetrievableCandidateBatch {
            query_plan_id: "qp-1".into(),
            full_attempt_count: 1,
            retry_count: 0,
            retrieval_batch_target: 4,
            candidates: vec![],
            rejected_decisions: vec![CandidateQualityDecision::mechanically_rejected(
                CandidateId::new("cand-rejected"),
                "qp-1",
                vec![],
            )],
            execution_blocking_facts: vec![],
        };

        // Rejected decisions carry candidate_id for coverage/gap explanation
        assert_eq!(
            batch.rejected_decisions[0].candidate_id,
            CandidateId::new("cand-rejected")
        );
    }

    // -----------------------------------------------------------------------
    // Trace links: retrieval_job_id links retrieval to image acceptance
    // -----------------------------------------------------------------------

    #[test]
    fn retrieval_job_id_traces_through_image_acceptance() {
        use image_retrieval::domain::image::{
            ImageAcceptanceDecisionV11, ImageArtifactRefs, ImageMechanicalAssessment,
        };

        let mechanical =
            ImageMechanicalAssessment::pass(CandidateId::new("cand-1"), "ret-job-123", "qp-1");
        assert_eq!(mechanical.retrieval_job_id, "ret-job-123");

        let decision = ImageAcceptanceDecisionV11::mechanically_rejected(
            CandidateId::new("cand-1"),
            "ret-job-123",
            "qp-1",
            vec![],
        );
        assert_eq!(decision.retrieval_job_id, "ret-job-123");
    }

    // -----------------------------------------------------------------------
    // QualityDiagnostic preserves trace links
    // -----------------------------------------------------------------------

    #[test]
    fn quality_diagnostic_preserves_trace_links() {
        let diag = QualityDiagnostic::new(
            QualityDiagnosticCode::QualityVlmEvaluationUnavailable,
            QualitySeverity::Blocker,
            QualityPhase::Candidate,
            "qp-1",
            "VLM unavailable in production",
        )
        .with_subject("cand-1");

        assert_eq!(diag.query_plan_id, "qp-1");
        assert_eq!(diag.subject_id, Some("cand-1".into()));
    }

    // -----------------------------------------------------------------------
    // QualityExecutionBlock preserves phase and count
    // -----------------------------------------------------------------------

    #[test]
    fn quality_execution_block_preserves_phase() {
        let block = QualityExecutionBlock::vlm_unavailable(QualityPhase::Candidate, "no token", 5);
        assert_eq!(block.phase, QualityPhase::Candidate);
        assert_eq!(block.pending_subject_count, 5);

        let block = QualityExecutionBlock::vlm_unavailable(QualityPhase::Image, "no endpoint", 3);
        assert_eq!(block.phase, QualityPhase::Image);
    }
}

// =============================================================================
// Redaction and security tests
// =============================================================================

mod redaction_security {
    use super::*;
    use image_retrieval::domain::metrics::{
        MetricFact, QualityDiagnostic, QualityDiagnosticCode, QualityMetricCode, QualityPhase,
        QualitySeverity,
    };

    #[test]
    fn metric_fact_marks_redaction() {
        let fact = MetricFact::candidate_reference(
            QualityMetricCode::CandidateLicenseHint,
            "cand-1",
            "qp-1",
            "license: CC BY 2.0",
        )
        .with_redacted();
        assert!(fact.redacted);
    }

    #[test]
    fn quality_diagnostic_marks_redaction() {
        let diag = QualityDiagnostic::new(
            QualityDiagnosticCode::QualitySensitiveDataRedacted,
            QualitySeverity::Warning,
            QualityPhase::Candidate,
            "qp-1",
            "sensitive data removed from output",
        )
        .with_redacted();
        assert!(diag.redacted);
        assert_eq!(
            diag.code,
            QualityDiagnosticCode::QualitySensitiveDataRedacted
        );
    }

    #[test]
    fn vlm_evaluation_response_marks_redaction() {
        use image_retrieval::domain::candidate::{
            VlmEvaluationResponse, VlmEvaluatorKind, VlmResponseStatus,
        };

        let response = VlmEvaluationResponse {
            request_id: "req-1".into(),
            evaluator_id: "qwen".into(),
            evaluator_kind: VlmEvaluatorKind::Qwen35Vlm,
            status: VlmResponseStatus::Complete,
            decisions: vec![],
            diagnostics: vec![],
            audit_ref: None,
            redaction_applied: true,
        };
        assert!(response.redaction_applied);
    }
}
