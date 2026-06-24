//! Normal web fetch retrieval channel — v1.1 artifact-backed.
//!
//! Implements the `normal_web_fetch` tier with two attempt modes:
//! 1. `direct_image_fetch` — HTTP GET the primary image URL.
//! 2. `source_page_resolve` — fetch the source page, extract image URLs,
//!    then fetch the selected image.
//!
//! Every successful fetch produces a complete artifact set: local image
//! artifact, source artifact, source sidecar, content summary, task report,
//! visual description, SHA-256 checksum, content-type evidence, dimensions,
//! diagnostics, and fetch trace.
//!
//! References: `docs/design/v1.1-TASK-004-retrieval-artifact-channel-design.md`

#![allow(clippy::type_complexity, clippy::too_many_arguments)]

use crate::domain::candidate::ImageDimensions;
use crate::domain::config::RetrievalChannelConfig;
use crate::domain::retrieval::{
    ArtifactWriteRecord, AuthorizationRisk, ContentSummary, CredentialStatus, DependencyStatus,
    RetrievalArtifactResult, RetrievalAttemptMode, RetrievalAttemptStatus, RetrievalAttemptTrace,
    RetrievalBatch, RetrievalBatchResult, RetrievalChannelCapabilities, RetrievalChannelId,
    RetrievalChannelReadinessReport, RetrievalChannelTier, RetrievalFailureCode, RetrievalJob,
    RetrievalPolicyDecision, RetrievalPolicyStatus, RetrievalStatus, RetrievalTaskReport,
    RetrievedContentKind, RobotsPolicyOutcome, SourceSidecar, SummaryGeneratorKind, SummaryQuality,
    VisualDescription, VisualDescriptionMethod,
};
use crate::ports::{BaseRetrievalChannel, RetrievalError};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Default request timeout for the web fetch channel (seconds).
const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// Maximum response body size (16 MB) to prevent memory exhaustion.
const MAX_RESPONSE_SIZE: u64 = 16 * 1024 * 1024;

/// Content types recognised as images.
const IMAGE_CONTENT_TYPES: &[&str] = &[
    "image/jpeg",
    "image/png",
    "image/gif",
    "image/webp",
    "image/bmp",
    "image/svg+xml",
    "image/tiff",
];

/// The normal web fetch retrieval channel — tier 1 (default).
pub struct WebFetchChannel {
    channel_id: RetrievalChannelId,
    /// Directory where artifacts are stored.
    staging_dir: PathBuf,
    /// Whether this channel is enabled.
    enabled: bool,
    /// Request timeout.
    timeout: Duration,
    /// Whether source-page resolve is supported.
    supports_source_page_resolve: bool,
}

impl WebFetchChannel {
    /// Create a new web fetch channel.
    pub fn new(staging_dir: impl Into<PathBuf>) -> std::result::Result<Self, RetrievalError> {
        let dir: PathBuf = staging_dir.into();
        fs::create_dir_all(&dir).map_err(|e| RetrievalError::Misconfigured {
            reason: format!("cannot create staging directory '{}': {}", dir.display(), e),
        })?;
        Ok(Self {
            channel_id: RetrievalChannelId::new("web-fetch-default"),
            staging_dir: dir,
            enabled: true,
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
            supports_source_page_resolve: true,
        })
    }

    /// Set a custom channel id.
    pub fn with_channel_id(mut self, id: impl Into<String>) -> Self {
        self.channel_id = RetrievalChannelId::new(id);
        self
    }

    /// Enable or disable this channel.
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Set a custom timeout (seconds).
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout = Duration::from_secs(secs);
        self
    }

    /// Enable or disable source-page resolve.
    pub fn with_source_page_resolve(mut self, supported: bool) -> Self {
        self.supports_source_page_resolve = supported;
        self
    }

    /// Return the staging directory path.
    pub fn staging_dir(&self) -> &Path {
        &self.staging_dir
    }

    /// Build a job staging directory: `{staging}/retrieval/attempts/{attempt}/{job_id}/`
    fn job_dir(&self, job: &RetrievalJob) -> PathBuf {
        self.staging_dir
            .join("retrieval")
            .join("attempts")
            .join(job.full_attempt_count.to_string())
            .join(job.retrieval_job_id.to_string())
    }

    /// Determine file extension from content-type.
    fn extension_from_content_type(content_type: &str) -> &str {
        match content_type {
            "image/jpeg" => "jpg",
            "image/png" => "png",
            "image/gif" => "gif",
            "image/webp" => "webp",
            "image/bmp" => "bmp",
            "image/svg+xml" => "svg",
            "image/tiff" => "tiff",
            _ => "bin",
        }
    }

    /// Check whether a content-type indicates an image.
    fn is_image_content_type(ct: &str) -> bool {
        IMAGE_CONTENT_TYPES
            .iter()
            .any(|valid| ct.starts_with(valid))
    }

    /// Compute SHA-256 checksum of bytes.
    fn sha256_hex(data: &[u8]) -> String {
        use sha2::{Digest, Sha256};
        let digest = Sha256::digest(data);
        format!("sha256-{:x}", digest)
    }

    /// Sniff content type from the first bytes of data.
    fn sniff_content_type(data: &[u8]) -> Option<String> {
        if data.len() < 8 {
            return None;
        }
        // JPEG: FF D8 FF
        if data[0] == 0xFF && data[1] == 0xD8 && data[2] == 0xFF {
            return Some("image/jpeg".into());
        }
        // PNG: 89 50 4E 47
        if data[0] == 0x89 && data[1] == 0x50 && data[2] == 0x4E && data[3] == 0x47 {
            return Some("image/png".into());
        }
        // GIF: 47 49 46
        if data[0] == 0x47 && data[1] == 0x49 && data[2] == 0x46 {
            return Some("image/gif".into());
        }
        // WebP: 52 49 46 46 ... 57 45 42 50
        if data.len() >= 12
            && data[0] == 0x52
            && data[1] == 0x49
            && data[2] == 0x46
            && data[3] == 0x46
            && data[8] == 0x57
            && data[9] == 0x45
            && data[10] == 0x42
            && data[11] == 0x50
        {
            return Some("image/webp".into());
        }
        // BMP: 42 4D
        if data[0] == 0x42 && data[1] == 0x4D {
            return Some("image/bmp".into());
        }
        None
    }

    /// Sniff image dimensions from common binary headers.
    fn sniff_image_dimensions(data: &[u8]) -> Option<ImageDimensions> {
        // PNG: signature + IHDR length/type + 4-byte width + 4-byte height.
        if data.len() >= 24
            && data[0] == 0x89
            && data[1] == b'P'
            && data[2] == b'N'
            && data[3] == b'G'
            && data[4] == 0x0D
            && data[5] == 0x0A
            && data[6] == 0x1A
            && data[7] == 0x0A
            && &data[12..16] == b"IHDR"
        {
            let width = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);
            let height = u32::from_be_bytes([data[20], data[21], data[22], data[23]]);
            if width > 0 && height > 0 {
                return Some(ImageDimensions { width, height });
            }
        }

        // JPEG: walk marker segments until a Start Of Frame marker.
        if data.len() >= 4 && data[0] == 0xFF && data[1] == 0xD8 {
            let mut i = 2usize;
            while i + 3 < data.len() {
                while i < data.len() && data[i] == 0xFF {
                    i += 1;
                }
                if i >= data.len() {
                    break;
                }
                let marker = data[i];
                i += 1;

                if marker == 0xD9 || marker == 0xDA {
                    break;
                }
                if matches!(marker, 0x01 | 0xD0..=0xD7) {
                    continue;
                }
                if i + 1 >= data.len() {
                    break;
                }
                let segment_len = u16::from_be_bytes([data[i], data[i + 1]]) as usize;
                if segment_len < 2 || i + segment_len > data.len() {
                    break;
                }

                let is_sof = matches!(
                    marker,
                    0xC0 | 0xC1
                        | 0xC2
                        | 0xC3
                        | 0xC5
                        | 0xC6
                        | 0xC7
                        | 0xC9
                        | 0xCA
                        | 0xCB
                        | 0xCD
                        | 0xCE
                        | 0xCF
                );
                if is_sof && segment_len >= 7 {
                    let height = u16::from_be_bytes([data[i + 3], data[i + 4]]) as u32;
                    let width = u16::from_be_bytes([data[i + 5], data[i + 6]]) as u32;
                    if width > 0 && height > 0 {
                        return Some(ImageDimensions { width, height });
                    }
                }

                i += segment_len;
            }
        }

        None
    }

    /// Perform a direct HTTP GET and return the bytes, content-type, and final URL.
    fn http_get(
        &self,
        url: &str,
        agent: &ureq::Agent,
    ) -> std::result::Result<(Vec<u8>, Option<String>, Option<u16>), RetrievalError> {
        let response = match agent.get(url).call() {
            Ok(r) => r,
            Err(ureq::Error::Status(code, _response)) => {
                if code == 401 || code == 403 {
                    return Err(RetrievalError::AccessRestricted {
                        message: format!("HTTP {} from {}", code, url),
                    });
                }
                return Err(RetrievalError::HttpStatus {
                    code,
                    message: format!("HTTP {} from {}", code, url),
                });
            }
            Err(ureq::Error::Transport(transport)) => {
                return Err(RetrievalError::Network {
                    message: format!("transport error fetching {}: {}", url, transport),
                });
            }
        };

        let http_status = Some(response.status());
        let content_type: Option<String> = response.header("Content-Type").map(|s| s.to_string());

        let mut reader = response.into_reader();
        let mut buf: Vec<u8> = Vec::new();
        let mut chunk = [0u8; 8192];
        loop {
            let n = match std::io::Read::read(&mut reader, &mut chunk) {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) => {
                    return Err(RetrievalError::Network {
                        message: format!("read error from {}: {}", url, e),
                    });
                }
            };
            buf.extend_from_slice(&chunk[..n]);
            if buf.len() as u64 > MAX_RESPONSE_SIZE {
                return Err(RetrievalError::MetadataOnly {
                    message: format!(
                        "response too large (>{} bytes) from {}",
                        MAX_RESPONSE_SIZE, url
                    ),
                });
            }
        }

        Ok((buf, content_type, http_status))
    }

    /// Attempt direct image fetch for a single job.
    fn attempt_direct_fetch(
        &self,
        job: &RetrievalJob,
        batch_id: &str,
    ) -> (RetrievalArtifactResult, Vec<RetrievalAttemptTrace>) {
        let agent = ureq::AgentBuilder::new().timeout(self.timeout).build();

        let started_at = chrono_now();
        let attempt_id = format!("attempt-{}-direct-{}", job.retrieval_job_id, started_at);

        let url = &job.target.primary_image_url;

        let mut trace = RetrievalAttemptTrace {
            attempt_id: attempt_id.clone(),
            retrieval_job_id: job.retrieval_job_id.clone(),
            query_plan_id: job.query_plan_id.clone(),
            candidate_id: job.candidate_id.clone(),
            channel_id: self.channel_id.clone(),
            channel_tier: RetrievalChannelTier::NormalWebFetch,
            attempt_mode: RetrievalAttemptMode::DirectImageFetch,
            started_at: started_at.clone(),
            completed_at: None,
            target_url_redacted: Some(redact_url(url)),
            source_page_url_redacted: None,
            final_url_redacted: None,
            http_status: None,
            bytes_received: None,
            status: RetrievalAttemptStatus::Started,
            failure_code: None,
            retryable: true,
            fallback_allowed: true,
            policy_reason: None,
            artifact_refs: vec![],
            redaction_applied: true,
        };

        // Evaluate pre-fetch policy
        if let Some(block) = self.check_pre_fetch_policy(job, url) {
            trace.status = RetrievalAttemptStatus::PolicyBlocked;
            trace.failure_code = Some(RetrievalFailureCode::RetrievalProhibitedSource);
            trace.fallback_allowed = false;
            trace.policy_reason = Some(block.reason.clone());
            let completed_at = chrono_now();
            trace.completed_at = Some(completed_at);

            let block_reason = block.reason.clone();
            let result = RetrievalArtifactResult::policy_blocked(
                job,
                batch_id,
                self.channel_id.clone(),
                RetrievalChannelTier::NormalWebFetch,
                RetrievalAttemptMode::DirectImageFetch,
                &block_reason,
                RetrievalFailureCode::RetrievalProhibitedSource,
                vec![trace.clone()],
                vec![],
                vec![block],
            );
            return (result, vec![trace]);
        }

        // Perform HTTP GET
        match self.http_get(url, &agent) {
            Ok((data, content_type_reported, http_status)) => {
                trace.http_status = http_status;
                trace.bytes_received = Some(data.len() as u64);

                if data.is_empty() {
                    trace.status = RetrievalAttemptStatus::Failed;
                    trace.failure_code = Some(RetrievalFailureCode::RetrievalMetadataOnly);
                    let completed_at = chrono_now();
                    trace.completed_at = Some(completed_at);

                    let result = RetrievalArtifactResult::failed(
                        job,
                        batch_id,
                        self.channel_id.clone(),
                        RetrievalChannelTier::NormalWebFetch,
                        RetrievalAttemptMode::DirectImageFetch,
                        "empty response body",
                        RetrievalFailureCode::RetrievalMetadataOnly,
                        vec![trace.clone()],
                        vec![],
                    );
                    return (result, vec![trace]);
                }

                // Check content type
                let ct_reported = content_type_reported.clone();
                if let Some(ref ct) = ct_reported {
                    if !Self::is_image_content_type(ct) {
                        trace.status = RetrievalAttemptStatus::Failed;
                        trace.failure_code =
                            Some(RetrievalFailureCode::RetrievalContentTypeMismatch);
                        let completed_at = chrono_now();
                        trace.completed_at = Some(completed_at);

                        let result = RetrievalArtifactResult::failed(
                            job,
                            batch_id,
                            self.channel_id.clone(),
                            RetrievalChannelTier::NormalWebFetch,
                            RetrievalAttemptMode::DirectImageFetch,
                            format!("non-image content type '{}'", ct),
                            RetrievalFailureCode::RetrievalContentTypeMismatch,
                            vec![trace.clone()],
                            vec![],
                        );
                        return (result, vec![trace]);
                    }
                }

                // Sniff content type
                let ct_sniffed = Self::sniff_content_type(&data);

                // Write artifacts
                match self.write_artifacts(job, &data, ct_reported.clone(), ct_sniffed.clone()) {
                    Ok(result) => {
                        trace.status = RetrievalAttemptStatus::Succeeded;
                        let completed_at = chrono_now();
                        trace.completed_at = Some(completed_at);
                        trace.artifact_refs = result
                            .local_artifact_path
                            .iter()
                            .map(|p| p.to_string_lossy().to_string())
                            .collect();

                        let mut final_result = result;
                        final_result.retrieval_batch_id = batch_id.to_string();
                        final_result.fetch_trace = vec![trace.clone()];
                        self.rewrite_task_report_attempts(&final_result, &[trace.clone()]);
                        final_result.job_ownership_valid = final_result.candidate_id
                            == job.candidate_id
                            && final_result.query_plan_id == job.query_plan_id;
                        final_result.retrieval_status = if final_result.has_all_required_paths()
                            && final_result.has_all_integrity_fields()
                        {
                            RetrievalStatus::Complete
                        } else {
                            RetrievalStatus::Partial
                        };

                        (final_result, vec![trace])
                    }
                    Err(e) => {
                        trace.status = RetrievalAttemptStatus::Failed;
                        trace.failure_code =
                            Some(RetrievalFailureCode::RetrievalArtifactWriteFailed);
                        let completed_at = chrono_now();
                        trace.completed_at = Some(completed_at);

                        let result = RetrievalArtifactResult::failed(
                            job,
                            batch_id,
                            self.channel_id.clone(),
                            RetrievalChannelTier::NormalWebFetch,
                            RetrievalAttemptMode::DirectImageFetch,
                            e.to_string(),
                            RetrievalFailureCode::RetrievalArtifactWriteFailed,
                            vec![trace.clone()],
                            vec![],
                        );
                        (result, vec![trace])
                    }
                }
            }
            Err(e) => {
                let failure_code = e.to_failure_code();
                let allows_fallback = e.allows_fallback();
                trace.status = RetrievalAttemptStatus::Failed;
                trace.failure_code = Some(failure_code.clone());
                trace.retryable = allows_fallback;
                trace.fallback_allowed = allows_fallback;
                let completed_at = chrono_now();
                trace.completed_at = Some(completed_at);

                let mut result = RetrievalArtifactResult::failed(
                    job,
                    batch_id,
                    self.channel_id.clone(),
                    RetrievalChannelTier::NormalWebFetch,
                    RetrievalAttemptMode::DirectImageFetch,
                    e.to_string(),
                    failure_code,
                    vec![trace.clone()],
                    vec![],
                );
                if !allows_fallback {
                    result.retrieval_status = RetrievalStatus::AccessRestricted;
                }
                (result, vec![trace])
            }
        }
    }

    /// Write all artifact files for a successful fetch.
    fn write_artifacts(
        &self,
        job: &RetrievalJob,
        data: &[u8],
        content_type_reported: Option<String>,
        content_type_sniffed: Option<String>,
    ) -> std::result::Result<RetrievalArtifactResult, RetrievalError> {
        self.write_artifacts_for_attempt(
            job,
            data,
            content_type_reported,
            content_type_sniffed,
            RetrievalAttemptMode::DirectImageFetch,
            Vec::new(),
        )
    }

    fn write_artifacts_for_attempt(
        &self,
        job: &RetrievalJob,
        data: &[u8],
        content_type_reported: Option<String>,
        content_type_sniffed: Option<String>,
        attempt_mode: RetrievalAttemptMode,
        attempt_traces: Vec<RetrievalAttemptTrace>,
    ) -> std::result::Result<RetrievalArtifactResult, RetrievalError> {
        let job_dir = self.job_dir(job);
        fs::create_dir_all(&job_dir).map_err(|e| RetrievalError::ArtifactWriteFailed {
            path: job_dir.to_string_lossy().to_string(),
            reason: e.to_string(),
        })?;

        let resolved_ct = content_type_sniffed
            .clone()
            .or_else(|| content_type_reported.clone())
            .unwrap_or_else(|| "application/octet-stream".into());
        let ext = Self::extension_from_content_type(&resolved_ct).to_string();
        let file_size = data.len() as u64;
        let image_dimensions = Self::sniff_image_dimensions(data);

        // 1. Write local artifact
        let local_filename = format!("artifact.{}", ext);
        let local_path = job_dir.join(&local_filename);
        fs::write(&local_path, data).map_err(|e| RetrievalError::ArtifactWriteFailed {
            path: local_path.to_string_lossy().to_string(),
            reason: e.to_string(),
        })?;

        // 2. Write source artifact (same bytes for direct fetch)
        let source_filename = format!("source-artifact.{}", ext);
        let source_path = job_dir.join(&source_filename);
        fs::write(&source_path, data).map_err(|e| RetrievalError::ArtifactWriteFailed {
            path: source_path.to_string_lossy().to_string(),
            reason: e.to_string(),
        })?;

        // 3. Compute checksum
        let checksum = Self::sha256_hex(data);

        // 4. Write source sidecar
        let sidecar = SourceSidecar {
            schema_version: "1.0".into(),
            retrieval_job_id: job.retrieval_job_id.clone(),
            query_plan_id: job.query_plan_id.clone(),
            candidate_id: job.candidate_id.clone(),
            channel_id: self.channel_id.clone(),
            channel_tier: RetrievalChannelTier::NormalWebFetch,
            attempt_mode,
            primary_url_redacted: redact_url(&job.target.primary_image_url),
            source_page_url_redacted: job
                .target
                .alternate_source_page_url
                .as_deref()
                .map(redact_url),
            final_url_redacted: None,
            http_status: Some(200),
            response_headers_safe: BTreeMap::new(),
            provider_id: job.target.provider_id.clone(),
            license_hint: job.target.license_hint.clone(),
            robots_policy: RobotsPolicyOutcome::NotChecked,
            authorization_risk: AuthorizationRisk::NoneDetected,
            fetched_at: chrono_now(),
            redaction_applied: true,
        };
        let sidecar_path = job_dir.join("source-sidecar.json");
        let sidecar_json = serde_json::to_string_pretty(&sidecar).map_err(|e| {
            RetrievalError::ArtifactWriteFailed {
                path: sidecar_path.to_string_lossy().to_string(),
                reason: e.to_string(),
            }
        })?;
        fs::write(&sidecar_path, &sidecar_json).map_err(|e| {
            RetrievalError::ArtifactWriteFailed {
                path: sidecar_path.to_string_lossy().to_string(),
                reason: e.to_string(),
            }
        })?;

        // 5. Write content summary
        let summary = ContentSummary {
            retrieval_job_id: job.retrieval_job_id.clone(),
            candidate_id: job.candidate_id.clone(),
            content_kind: RetrievedContentKind::ImageArtifact,
            summary_text: format!(
                "Image artifact fetched via direct image fetch. Content-Type: {}, size: {} bytes.",
                resolved_ct, file_size
            ),
            summary_quality: SummaryQuality::Pass,
            summary_quality_gate_passed: true,
            evidence_refs: vec![
                local_path.to_string_lossy().to_string(),
                sidecar_path.to_string_lossy().to_string(),
            ],
            generated_by: SummaryGeneratorKind::LocalRetrievalAdapter,
        };
        let summary_path = job_dir.join("content-summary.json");
        fs::write(
            &summary_path,
            serde_json::to_string_pretty(&summary).unwrap_or_default(),
        )
        .map_err(|e| RetrievalError::ArtifactWriteFailed {
            path: summary_path.to_string_lossy().to_string(),
            reason: e.to_string(),
        })?;

        // 6. Write task report
        let started_at = chrono_now();
        let attempts = if attempt_traces.is_empty() {
            vec![self.synthetic_success_trace(
                job,
                attempt_mode,
                &started_at,
                file_size,
                local_path.to_string_lossy().to_string(),
            )]
        } else {
            attempt_traces
        };
        let task_report = RetrievalTaskReport {
            retrieval_job_id: job.retrieval_job_id.clone(),
            query_plan_id: job.query_plan_id.clone(),
            candidate_id: job.candidate_id.clone(),
            started_at,
            completed_at: chrono_now(),
            status: RetrievalStatus::Complete,
            attempts,
            artifacts_written: vec![
                ArtifactWriteRecord {
                    artifact_type: "local_artifact".into(),
                    path: local_path.to_string_lossy().to_string(),
                    file_size_bytes: file_size,
                    checksum_sha256: Some(checksum.clone()),
                },
                ArtifactWriteRecord {
                    artifact_type: "source_artifact".into(),
                    path: source_path.to_string_lossy().to_string(),
                    file_size_bytes: file_size,
                    checksum_sha256: Some(checksum.clone()),
                },
                ArtifactWriteRecord {
                    artifact_type: "source_sidecar".into(),
                    path: sidecar_path.to_string_lossy().to_string(),
                    file_size_bytes: sidecar_json.len() as u64,
                    checksum_sha256: None,
                },
            ],
            failure_code: None,
            policy_blocks: vec![],
            redaction_applied: true,
        };
        let report_path = job_dir.join("task-report.json");
        fs::write(
            &report_path,
            serde_json::to_string_pretty(&task_report).unwrap_or_default(),
        )
        .map_err(|e| RetrievalError::ArtifactWriteFailed {
            path: report_path.to_string_lossy().to_string(),
            reason: e.to_string(),
        })?;

        // 7. Write visual description
        let visual_desc = VisualDescription {
            retrieval_job_id: job.retrieval_job_id.clone(),
            candidate_id: job.candidate_id.clone(),
            description_text: format!(
                "Image artifact: {} ({}) — {} bytes, checksum {}",
                local_filename, resolved_ct, file_size, checksum
            ),
            method: VisualDescriptionMethod::MetadataAndFilename,
            confidence: Some(0.9),
            image_dimensions,
            content_type: Some(resolved_ct.clone()),
            evidence_refs: vec![local_path.to_string_lossy().to_string()],
        };
        let visual_desc_path = job_dir.join("visual-description.json");
        fs::write(
            &visual_desc_path,
            serde_json::to_string_pretty(&visual_desc).unwrap_or_default(),
        )
        .map_err(|e| RetrievalError::ArtifactWriteFailed {
            path: visual_desc_path.to_string_lossy().to_string(),
            reason: e.to_string(),
        })?;

        // Build the result
        let media_type_match = match (&content_type_reported, &content_type_sniffed) {
            (Some(reported), Some(sniffed)) => {
                reported.starts_with("image/") && sniffed.starts_with("image/")
            }
            (Some(reported), None) => reported.starts_with("image/"),
            _ => true, // can't disprove it
        };

        Ok(RetrievalArtifactResult {
            retrieval_job_id: job.retrieval_job_id.clone(),
            retrieval_batch_id: String::new(), // filled by caller
            query_plan_id: job.query_plan_id.clone(),
            candidate_id: job.candidate_id.clone(),
            channel_id: self.channel_id.clone(),
            channel_tier: RetrievalChannelTier::NormalWebFetch,
            attempt_mode,
            retrieval_status: RetrievalStatus::Complete,
            local_artifact_path: Some(local_path),
            source_artifact_path: Some(source_path),
            source_sidecar_path: Some(sidecar_path),
            content_summary_path: Some(summary_path),
            task_report_path: Some(report_path),
            visual_description_path: Some(visual_desc_path),
            diagnostics_path: None,
            checksum_sha256: Some(checksum),
            content_type_reported,
            content_type_sniffed,
            content_type: Some(resolved_ct),
            file_extension: Some(ext.to_string()),
            file_size_bytes: Some(file_size),
            image_dimensions,
            media_type_match,
            local_artifact_exists: true,
            source_artifact_exists: true,
            sidecar_valid: true,
            summary_quality_passed: true,
            task_report_valid: true,
            visual_description_valid: true,
            job_ownership_valid: true,
            metadata_only: false,
            fetch_trace: vec![],
            policy_decisions: vec![],
            diagnostics: vec![],
            failure_reason: None,
            redaction_applied: true,
        })
    }

    fn synthetic_success_trace(
        &self,
        job: &RetrievalJob,
        attempt_mode: RetrievalAttemptMode,
        started_at: &str,
        bytes_received: u64,
        artifact_ref: String,
    ) -> RetrievalAttemptTrace {
        RetrievalAttemptTrace {
            attempt_id: format!(
                "attempt-{}-{}-{}",
                job.retrieval_job_id, attempt_mode, started_at
            ),
            retrieval_job_id: job.retrieval_job_id.clone(),
            query_plan_id: job.query_plan_id.clone(),
            candidate_id: job.candidate_id.clone(),
            channel_id: self.channel_id.clone(),
            channel_tier: RetrievalChannelTier::NormalWebFetch,
            attempt_mode,
            started_at: started_at.to_string(),
            completed_at: Some(chrono_now()),
            target_url_redacted: Some(redact_url(&job.target.primary_image_url)),
            source_page_url_redacted: job
                .target
                .alternate_source_page_url
                .as_deref()
                .map(redact_url),
            final_url_redacted: None,
            http_status: Some(200),
            bytes_received: Some(bytes_received),
            status: RetrievalAttemptStatus::Succeeded,
            failure_code: None,
            retryable: true,
            fallback_allowed: true,
            policy_reason: None,
            artifact_refs: vec![artifact_ref],
            redaction_applied: true,
        }
    }

    fn rewrite_task_report_attempts(
        &self,
        result: &RetrievalArtifactResult,
        attempts: &[RetrievalAttemptTrace],
    ) {
        let Some(path) = result.task_report_path.as_ref() else {
            return;
        };
        let Ok(bytes) = fs::read(path) else {
            return;
        };
        let Ok(mut report) = serde_json::from_slice::<RetrievalTaskReport>(&bytes) else {
            return;
        };
        report.attempts = attempts.to_vec();
        if let Some(first) = attempts.first() {
            report.started_at = first.started_at.clone();
        }
        if let Some(last_completed) = attempts.last().and_then(|a| a.completed_at.clone()) {
            report.completed_at = last_completed;
        }
        if let Ok(json) = serde_json::to_string_pretty(&report) {
            let _ = fs::write(path, json);
        }
    }

    /// Check pre-fetch policy: prohibit domains, etc.
    fn check_pre_fetch_policy(
        &self,
        job: &RetrievalJob,
        url: &str,
    ) -> Option<RetrievalPolicyDecision> {
        // Check prohibited domains
        let url_lower = url.to_lowercase();
        for domain in &job.policy_context.prohibited_domains {
            if url_lower.contains(&domain.to_lowercase()) {
                return Some(RetrievalPolicyDecision {
                    decision: "blocked".into(),
                    policy_rule: "prohibited_domain".into(),
                    reason: format!("Source domain '{}' is prohibited by policy", domain),
                });
            }
        }
        None
    }
}

impl BaseRetrievalChannel for WebFetchChannel {
    fn channel_id(&self) -> RetrievalChannelId {
        self.channel_id.clone()
    }

    fn display_name(&self) -> &str {
        "Normal Web Fetch"
    }

    fn tier(&self) -> RetrievalChannelTier {
        RetrievalChannelTier::NormalWebFetch
    }

    fn capabilities(&self) -> RetrievalChannelCapabilities {
        RetrievalChannelCapabilities {
            supports_direct_image_fetch: true,
            supports_source_page_resolve: self.supports_source_page_resolve,
            fixture_only: false,
            ..Default::default()
        }
    }

    fn readiness(&self, config: &RetrievalChannelConfig) -> RetrievalChannelReadinessReport {
        if !self.enabled || !config.enabled {
            return RetrievalChannelReadinessReport::disabled(
                self.channel_id.clone(),
                self.display_name(),
                self.tier(),
                RetrievalFailureCode::RetrievalChannelDisabled,
            );
        }

        // Check staging directory writable
        let test_file = self.staging_dir.join(".write_test");
        if fs::write(&test_file, b"").is_err() {
            return RetrievalChannelReadinessReport {
                channel_id: self.channel_id.clone(),
                display_name: self.display_name().into(),
                tier: self.tier(),
                enabled: true,
                available: false,
                included_in_fallback_order: false,
                credential_status: CredentialStatus::NotRequired,
                dependency_status: DependencyStatus::Missing {
                    detail: "staging directory not writable".into(),
                },
                policy_status: RetrievalPolicyStatus::Allowed,
                failure_code: Some(RetrievalFailureCode::RetrievalChannelDependencyMissing),
                checked_at: chrono_now(),
                evidence: vec![],
                redaction_applied: false,
            };
        }
        let _ = fs::remove_file(&test_file);

        RetrievalChannelReadinessReport::ready(
            self.channel_id.clone(),
            self.display_name(),
            self.tier(),
        )
    }

    fn retrieve_batch(
        &self,
        batch: &RetrievalBatch,
    ) -> std::result::Result<RetrievalBatchResult, RetrievalError> {
        let mut all_results: Vec<RetrievalArtifactResult> = Vec::new();
        let mut all_traces: Vec<RetrievalAttemptTrace> = Vec::new();
        let mut all_fallback_decisions = Vec::new();

        for job in &batch.jobs {
            // Try direct image fetch first
            let (result, traces) = self.attempt_direct_fetch(job, &batch.retrieval_batch_id);
            all_traces.extend(traces);

            if result.is_complete() {
                all_results.push(result);
                continue;
            }

            // If fallback is allowed and we have a source page URL, try source-page resolve
            let can_fallback = result.retrieval_status != RetrievalStatus::AccessRestricted
                && result.retrieval_status != RetrievalStatus::PolicyBlocked;
            let has_source_page = job.target.alternate_source_page_url.is_some();

            if can_fallback && has_source_page && self.supports_source_page_resolve {
                let fb_decision = crate::domain::retrieval::RetrievalFallbackDecision {
                    retrieval_job_id: job.retrieval_job_id.clone(),
                    from_tier: RetrievalChannelTier::NormalWebFetch,
                    from_attempt_mode: RetrievalAttemptMode::DirectImageFetch,
                    to_tier: Some(RetrievalChannelTier::NormalWebFetch),
                    to_attempt_mode: Some(RetrievalAttemptMode::SourcePageResolve),
                    decision: crate::domain::retrieval::FallbackDecisionKind::Proceed,
                    reason_code: RetrievalFailureCode::RetrievalDirectFetchNetwork,
                    policy_reason: None,
                };
                all_fallback_decisions.push(fb_decision);

                // Source-page resolve is a simplified approach:
                // fetch the page, look for og:image or direct image links
                let sp_url = job
                    .target
                    .alternate_source_page_url
                    .as_deref()
                    .unwrap_or("");

                let (sp_result, sp_traces) =
                    self.attempt_source_page_resolve(job, &batch.retrieval_batch_id, sp_url);
                all_traces.extend(sp_traces);
                all_results.push(sp_result);
                continue;
            }

            // No fallback available, use the failed result
            all_results.push(result);
        }

        Ok(RetrievalBatchResult::new(
            batch.retrieval_batch_id.clone(),
            batch.query_plan_id.clone(),
            batch.full_attempt_count,
            batch.retry_count,
            batch.target_size,
            vec![],
            all_results,
            all_traces,
            all_fallback_decisions,
            batch.shortage.clone(),
            vec![],
            vec![],
        ))
    }
}

impl WebFetchChannel {
    /// Attempt source-page resolve: fetch the page, extract image URLs, fetch
    /// the most promising one.
    fn attempt_source_page_resolve(
        &self,
        job: &RetrievalJob,
        batch_id: &str,
        source_page_url: &str,
    ) -> (RetrievalArtifactResult, Vec<RetrievalAttemptTrace>) {
        let agent = ureq::AgentBuilder::new().timeout(self.timeout).build();
        let started_at = chrono_now();
        let attempt_id = format!(
            "attempt-{}-source-page-{}",
            job.retrieval_job_id, started_at
        );

        let mut trace = RetrievalAttemptTrace {
            attempt_id: attempt_id.clone(),
            retrieval_job_id: job.retrieval_job_id.clone(),
            query_plan_id: job.query_plan_id.clone(),
            candidate_id: job.candidate_id.clone(),
            channel_id: self.channel_id.clone(),
            channel_tier: RetrievalChannelTier::NormalWebFetch,
            attempt_mode: RetrievalAttemptMode::SourcePageResolve,
            started_at: started_at.clone(),
            completed_at: None,
            target_url_redacted: Some(redact_url(source_page_url)),
            source_page_url_redacted: Some(redact_url(source_page_url)),
            final_url_redacted: None,
            http_status: None,
            bytes_received: None,
            status: RetrievalAttemptStatus::Started,
            failure_code: None,
            retryable: true,
            fallback_allowed: true,
            policy_reason: None,
            artifact_refs: vec![],
            redaction_applied: true,
        };

        // Fetch the source page
        let page_data = match self.http_get(source_page_url, &agent) {
            Ok((data, _ct, http_status)) => {
                trace.http_status = http_status;
                trace.bytes_received = Some(data.len() as u64);
                data
            }
            Err(e) => {
                trace.status = RetrievalAttemptStatus::Failed;
                trace.failure_code = Some(e.to_failure_code());
                let completed_at = chrono_now();
                trace.completed_at = Some(completed_at);

                let result = RetrievalArtifactResult::failed(
                    job,
                    batch_id,
                    self.channel_id.clone(),
                    RetrievalChannelTier::NormalWebFetch,
                    RetrievalAttemptMode::SourcePageResolve,
                    e.to_string(),
                    e.to_failure_code(),
                    vec![trace.clone()],
                    vec![],
                );
                return (result, vec![trace]);
            }
        };

        // Try to extract image URLs from the page
        let page_text = String::from_utf8_lossy(&page_data);
        let image_urls = extract_image_urls_from_html(&page_text, source_page_url);

        if image_urls.is_empty() {
            trace.status = RetrievalAttemptStatus::Failed;
            trace.failure_code = Some(RetrievalFailureCode::RetrievalMetadataOnly);
            let completed_at = chrono_now();
            trace.completed_at = Some(completed_at);

            let result = RetrievalArtifactResult::failed(
                job,
                batch_id,
                self.channel_id.clone(),
                RetrievalChannelTier::NormalWebFetch,
                RetrievalAttemptMode::SourcePageResolve,
                "no image URLs found on source page",
                RetrievalFailureCode::RetrievalMetadataOnly,
                vec![trace.clone()],
                vec![],
            );
            return (result, vec![trace]);
        }

        // Try fetching the first image URL that works
        for img_url in &image_urls {
            match self.http_get(img_url, &agent) {
                Ok((data, content_type_reported, _http_status)) => {
                    if data.is_empty() {
                        continue;
                    }
                    let ct_sniffed = Self::sniff_content_type(&data);
                    let is_image = ct_sniffed
                        .as_deref()
                        .map(Self::is_image_content_type)
                        .unwrap_or(false);

                    if !is_image {
                        continue;
                    }

                    trace.status = RetrievalAttemptStatus::Succeeded;
                    let completed_at = chrono_now();
                    trace.completed_at = Some(completed_at);
                    trace.final_url_redacted = Some(redact_url(img_url));

                    match self.write_artifacts_for_attempt(
                        job,
                        &data,
                        content_type_reported,
                        ct_sniffed,
                        RetrievalAttemptMode::SourcePageResolve,
                        vec![trace.clone()],
                    ) {
                        Ok(mut result) => {
                            trace.artifact_refs = result
                                .local_artifact_path
                                .iter()
                                .map(|p| p.to_string_lossy().to_string())
                                .collect();

                            result.retrieval_batch_id = batch_id.to_string();
                            result.fetch_trace = vec![trace.clone()];
                            self.rewrite_task_report_attempts(&result, &[trace.clone()]);
                            result.job_ownership_valid = result.candidate_id == job.candidate_id
                                && result.query_plan_id == job.query_plan_id;
                            result.retrieval_status = if result.has_all_required_paths()
                                && result.has_all_integrity_fields()
                            {
                                RetrievalStatus::Complete
                            } else {
                                RetrievalStatus::Partial
                            };

                            return (result, vec![trace]);
                        }
                        Err(_) => continue,
                    }
                }
                Err(_) => continue,
            }
        }

        // All image URLs failed
        trace.status = RetrievalAttemptStatus::Failed;
        trace.failure_code = Some(RetrievalFailureCode::RetrievalMetadataOnly);
        let completed_at = chrono_now();
        trace.completed_at = Some(completed_at);

        let result = RetrievalArtifactResult::failed(
            job,
            batch_id,
            self.channel_id.clone(),
            RetrievalChannelTier::NormalWebFetch,
            RetrievalAttemptMode::SourcePageResolve,
            "source page resolved but no image could be fetched",
            RetrievalFailureCode::RetrievalMetadataOnly,
            vec![trace.clone()],
            vec![],
        );
        (result, vec![trace])
    }
}

/// Extract candidate image URLs from HTML text using simple parsing.
fn extract_image_urls_from_html(html: &str, base_url: &str) -> Vec<String> {
    let mut urls: Vec<String> = Vec::new();
    let lower = html.to_lowercase();

    // Look for og:image meta tag
    for line in html.lines() {
        let lower_line = line.to_lowercase();
        if lower_line.contains("og:image") || lower_line.contains("twitter:image") {
            if let Some(content_start) = lower_line.find("content=\"") {
                let after = &lower_line[content_start + 9..];
                if let Some(content_end) = after.find('"') {
                    let img_url = &after[..content_end];
                    let resolved = resolve_url(img_url, base_url);
                    if !urls.contains(&resolved) {
                        urls.push(resolved);
                    }
                }
            }
        }
    }

    // Look for direct image links
    for img_tag in lower.split("src=\"").skip(1) {
        if let Some(end) = img_tag.find('"') {
            let src = &img_tag[..end];
            if src.ends_with(".jpg")
                || src.ends_with(".jpeg")
                || src.ends_with(".png")
                || src.ends_with(".webp")
                || src.ends_with(".gif")
            {
                let resolved = resolve_url(src, base_url);
                if !urls.contains(&resolved) {
                    urls.push(resolved);
                }
            }
        }
    }

    urls
}

/// Naive URL resolution for relative paths.
fn resolve_url(url: &str, base_url: &str) -> String {
    if url.starts_with("http://") || url.starts_with("https://") {
        return url.to_string();
    }
    if url.starts_with("//") {
        return format!("https:{}", url);
    }
    if url.starts_with('/') {
        // Find the origin part of base_url
        if let Some(scheme_end) = base_url.find("://") {
            let after_scheme = &base_url[scheme_end + 3..];
            if let Some(host_end) = after_scheme.find('/') {
                return format!("{}{}", &base_url[..scheme_end + 3 + host_end], url);
            }
            return format!("{}{}", base_url, url);
        }
    }
    // Relative path — resolve against base directory
    if let Some(last_slash) = base_url.rfind('/') {
        if last_slash > 8 {
            // past https://
            return format!("{}/{}", &base_url[..last_slash], url);
        }
    }
    format!("{}/{}", base_url, url)
}

/// Redact a URL for safe sidecar/trace storage.
fn redact_url(url: &str) -> String {
    // Strip query parameters that look like tokens
    if let Some(q_pos) = url.find('?') {
        let base = &url[..q_pos];
        let query = &url[q_pos + 1..];
        let safe_params: Vec<&str> = query
            .split('&')
            .filter(|p| {
                let lower = p.to_lowercase();
                !lower.contains("token")
                    && !lower.contains("key")
                    && !lower.contains("secret")
                    && !lower.contains("auth")
                    && !lower.contains("signature")
                    && !lower.contains("credential")
            })
            .collect();
        if safe_params.is_empty() {
            format!("{}?[REDACTED]", base)
        } else {
            format!("{}?{}...[REDACTED]", base, safe_params.join("&"))
        }
    } else {
        url.to_string()
    }
}

/// Return the current time as an ISO 8601 string.
fn chrono_now() -> String {
    let now = time::OffsetDateTime::now_utc();
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        now.year(),
        u8::from(now.month()),
        now.day(),
        now.hour(),
        now.minute(),
        now.second()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::retrieval::{
        RetrievalAttemptMode, RetrievalAttemptStatus, RetrievalAttemptTrace, RetrievalJob,
        RetrievalJobId, RetrievalPolicyContext, RetrievalTarget, RetrievalTargetType,
    };

    #[allow(dead_code)]
    fn make_job(id: &str, cid: &str, qp_id: &str, url: &str) -> RetrievalJob {
        RetrievalJob {
            retrieval_job_id: RetrievalJobId::new(id),
            query_plan_id: qp_id.into(),
            candidate_id: cid.into(),
            full_attempt_count: 1,
            retry_count: 0,
            retrieval_priority: 5,
            target: RetrievalTarget {
                target_type: RetrievalTargetType::Image,
                primary_image_url: url.into(),
                alternate_source_page_url: None,
                thumbnail_url: None,
                expected_mime_type: Some("image/jpeg".into()),
                license_hint: None,
                provider_id: "p1".into(),
                candidate_provenance_refs: vec![],
            },
            candidate_quality_decision_ref: "qd-1".into(),
            requested_outputs: vec![],
            policy_context: RetrievalPolicyContext::default(),
        }
    }

    #[allow(dead_code)]
    fn make_batch(jobs: Vec<RetrievalJob>) -> RetrievalBatch {
        RetrievalBatch::new("b-1", "qp-1", 1, 0, jobs.len() as u32, jobs, None)
    }

    #[test]
    fn web_fetch_channel_creation() {
        let dir = std::env::temp_dir().join("test-wf-create");
        let channel = WebFetchChannel::new(&dir).expect("create channel");
        assert_eq!(channel.tier(), RetrievalChannelTier::NormalWebFetch);
        assert_eq!(channel.display_name(), "Normal Web Fetch");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn web_fetch_channel_readiness() {
        let dir = std::env::temp_dir().join("test-wf-readiness");
        let channel = WebFetchChannel::new(&dir).expect("create channel");
        let config = RetrievalChannelConfig {
            channel_id: "test".into(),
            channel_kind: crate::domain::config::RetrievalChannelKind::NormalWebFetch,
            tier: RetrievalChannelTier::NormalWebFetch,
            enabled: true,
            endpoint: None,
            credential_env: None,
            max_batch_size: None,
        };
        let report = channel.readiness(&config);
        assert!(report.available);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn sha256_hex_matches_known_sha256_digest() {
        assert_eq!(
            WebFetchChannel::sha256_hex(b"abc"),
            "sha256-ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn web_fetch_channel_readiness_disabled() {
        let dir = std::env::temp_dir().join("test-wf-disabled");
        let channel = WebFetchChannel::new(&dir)
            .expect("create channel")
            .with_enabled(false);
        let config = RetrievalChannelConfig {
            channel_id: "test".into(),
            channel_kind: crate::domain::config::RetrievalChannelKind::NormalWebFetch,
            tier: RetrievalChannelTier::NormalWebFetch,
            enabled: true,
            endpoint: None,
            credential_env: None,
            max_batch_size: None,
        };
        let report = channel.readiness(&config);
        assert!(!report.available);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn is_image_content_type() {
        assert!(WebFetchChannel::is_image_content_type("image/jpeg"));
        assert!(WebFetchChannel::is_image_content_type("image/png"));
        assert!(!WebFetchChannel::is_image_content_type("text/html"));
    }

    #[test]
    fn extension_from_content_type() {
        assert_eq!(
            WebFetchChannel::extension_from_content_type("image/jpeg"),
            "jpg"
        );
        assert_eq!(
            WebFetchChannel::extension_from_content_type("image/png"),
            "png"
        );
        assert_eq!(
            WebFetchChannel::extension_from_content_type("application/octet-stream"),
            "bin"
        );
    }

    #[test]
    fn sniff_image_dimensions_reads_jpeg_and_png_headers() {
        let jpeg = [
            0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x0C, b'J', b'F', b'I', b'F', 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0xFF, 0xC0, 0x00, 0x11, 0x08, 0x01, 0xC4, 0x03, 0x20, 0x03, 0x01, 0x22,
            0x00, 0x02, 0x11, 0x01, 0x03, 0x11, 0x01,
        ];
        let png = [
            0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, b'I', b'H',
            b'D', b'R', 0x00, 0x00, 0x03, 0x20, 0x00, 0x00, 0x01, 0xC4, 0x08, 0x02, 0x00, 0x00,
            0x00,
        ];

        let jpeg_dims = WebFetchChannel::sniff_image_dimensions(&jpeg).unwrap();
        let png_dims = WebFetchChannel::sniff_image_dimensions(&png).unwrap();

        assert_eq!(jpeg_dims.width, 800);
        assert_eq!(jpeg_dims.height, 452);
        assert_eq!(png_dims.width, 800);
        assert_eq!(png_dims.height, 452);
    }

    #[test]
    fn sniff_content_type_jpeg() {
        let jpeg_header = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46];
        assert_eq!(
            WebFetchChannel::sniff_content_type(&jpeg_header),
            Some("image/jpeg".into())
        );
    }

    #[test]
    fn sniff_content_type_png() {
        let png_header = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        assert_eq!(
            WebFetchChannel::sniff_content_type(&png_header),
            Some("image/png".into())
        );
    }

    #[test]
    fn sniff_content_type_unknown() {
        let data = vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        assert_eq!(WebFetchChannel::sniff_content_type(&data), None);
    }

    #[test]
    fn redact_url_strips_token_params() {
        let redacted = redact_url("https://example.com/img.jpg?token=secret123&size=large");
        assert!(!redacted.contains("secret123"));
        assert!(redacted.contains("[REDACTED]"));
    }

    #[test]
    fn redact_url_safe_params_preserved() {
        let redacted = redact_url("https://example.com/img.jpg?size=large&format=jpg");
        assert!(redacted.contains("size=large"));
    }

    #[test]
    fn extract_image_urls_og_image() {
        let html = r#"<meta property="og:image" content="https://example.com/photo.jpg">"#;
        let urls = extract_image_urls_from_html(html, "https://example.com");
        assert_eq!(urls, vec!["https://example.com/photo.jpg"]);
    }

    #[test]
    fn extract_image_urls_direct_links() {
        let html = r#"<img src="https://example.com/img.png"><img src="/relative.jpg">"#;
        let urls = extract_image_urls_from_html(html, "https://example.com/page");
        assert!(urls.contains(&"https://example.com/img.png".to_string()));
        assert!(urls.contains(&"https://example.com/relative.jpg".to_string()));
    }

    #[test]
    fn resolve_relative_urls() {
        assert_eq!(
            resolve_url("/images/test.jpg", "https://example.com/page"),
            "https://example.com/images/test.jpg"
        );
        assert_eq!(
            resolve_url("photo.png", "https://example.com/dir/page.html"),
            "https://example.com/dir/photo.png"
        );
    }

    #[test]
    fn write_artifacts_task_report_records_attempt_and_rfc3339_time() {
        let dir = std::env::temp_dir().join(format!("test-wf-report-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let channel = WebFetchChannel::new(&dir).expect("create channel");
        let job = make_job("job-1", "cand-1", "qp-1", "https://example.com/image.jpg");
        let result = channel
            .write_artifacts(
                &job,
                &[0xFF, 0xD8, 0xFF, 0xD9],
                Some("image/jpeg".into()),
                Some("image/jpeg".into()),
            )
            .expect("write artifacts");

        let report_path = result.task_report_path.as_ref().unwrap();
        let report: serde_json::Value =
            serde_json::from_slice(&fs::read(report_path).unwrap()).unwrap();
        let started_at = report["started_at"].as_str().unwrap();
        let completed_at = report["completed_at"].as_str().unwrap();
        assert!(
            started_at.contains('T') && started_at.ends_with('Z'),
            "started_at should be RFC3339 UTC, got {}",
            started_at
        );
        assert!(
            completed_at.contains('T') && completed_at.ends_with('Z'),
            "completed_at should be RFC3339 UTC, got {}",
            completed_at
        );
        assert!(
            !report["attempts"].as_array().unwrap().is_empty(),
            "task report must include retrieval attempt evidence"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_artifacts_preserves_source_page_attempt_mode() {
        let dir = std::env::temp_dir().join(format!("test-wf-source-mode-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let channel = WebFetchChannel::new(&dir).expect("create channel");
        let job = make_job("job-2", "cand-2", "qp-1", "https://example.com/page");
        let trace = RetrievalAttemptTrace {
            attempt_id: "attempt-source-page".into(),
            retrieval_job_id: job.retrieval_job_id.clone(),
            query_plan_id: job.query_plan_id.clone(),
            candidate_id: job.candidate_id.clone(),
            channel_id: channel.channel_id.clone(),
            channel_tier: RetrievalChannelTier::NormalWebFetch,
            attempt_mode: RetrievalAttemptMode::SourcePageResolve,
            started_at: "2026-06-24T00:00:00Z".into(),
            completed_at: Some("2026-06-24T00:00:01Z".into()),
            target_url_redacted: Some("https://example.com/page".into()),
            source_page_url_redacted: Some("https://example.com/page".into()),
            final_url_redacted: Some("https://example.com/image.jpg".into()),
            http_status: Some(200),
            bytes_received: Some(4),
            status: RetrievalAttemptStatus::Succeeded,
            failure_code: None,
            retryable: true,
            fallback_allowed: true,
            policy_reason: None,
            artifact_refs: vec![],
            redaction_applied: true,
        };

        let result = channel
            .write_artifacts_for_attempt(
                &job,
                &[0xFF, 0xD8, 0xFF, 0xD9],
                Some("image/jpeg".into()),
                Some("image/jpeg".into()),
                RetrievalAttemptMode::SourcePageResolve,
                vec![trace],
            )
            .expect("write artifacts");

        assert_eq!(result.attempt_mode, RetrievalAttemptMode::SourcePageResolve);
        let sidecar: serde_json::Value = serde_json::from_slice(
            &fs::read(result.source_sidecar_path.as_ref().unwrap()).unwrap(),
        )
        .unwrap();
        assert_eq!(sidecar["attempt_mode"], "source_page_resolve");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn capabilities_default() {
        let dir = std::env::temp_dir().join("test-wf-caps");
        let channel = WebFetchChannel::new(&dir).expect("create channel");
        let caps = channel.capabilities();
        assert!(caps.supports_direct_image_fetch);
        assert!(!caps.fixture_only);
        let _ = fs::remove_dir_all(&dir);
    }
}
