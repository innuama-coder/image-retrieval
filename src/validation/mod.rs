//! Package validator — deterministic v1.1 canonical package validation.
//!
//! Implements the package validation contract defined in
//! `docs/design/v1.1-TASK-005-orchestrator-package-validation-cli-design.md`
//! and LLD §Validation Checks.
//!
//! The validator checks:
//! - Required canonical file existence and JSON validity
//! - Artifact existence, checksums, and completeness
//! - Retry counter invariants (retry_count == full_attempt_count - 1)
//! - Coverage invariants (gap_count matches delivered images)
//! - Manifest link resolution
//! - Sensitive data (credential) scanning
//! - Fixture-in-production rejection
//!
//! References: PRD FR-013/AC-013, TASK-005 design §Package Validation

use crate::domain::delivery::{
    ArtifactCheck, CounterCheck, CoverageCheck, FileCheck, PackageValidationIssue,
    PackageValidationReport, RedactionCheck, ValidationStatus,
};
use crate::error::{Error, Result};
use crate::policy::contains_sensitive_pattern;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Validation failure codes
// ---------------------------------------------------------------------------

/// Canonical validation issue codes per the TASK-005 design.
pub mod codes {
    pub const REQUIRED_FILE_MISSING: &str = "PKG_REQUIRED_FILE_MISSING";
    pub const JSON_INVALID: &str = "PKG_JSON_INVALID";
    pub const QUERY_PLAN_MISMATCH: &str = "PKG_QUERY_PLAN_MISMATCH";
    pub const DELIVERED_IMAGE_ARTIFACT_MISSING: &str = "PKG_DELIVERED_IMAGE_ARTIFACT_MISSING";
    pub const DELIVERED_IMAGE_METADATA_ONLY: &str = "PKG_DELIVERED_IMAGE_METADATA_ONLY";
    pub const CHECKSUM_MISSING: &str = "PKG_CHECKSUM_MISSING";
    pub const MEDIA_TYPE_MISMATCH: &str = "PKG_MEDIA_TYPE_MISMATCH";
    pub const OWNERSHIP_MISMATCH: &str = "PKG_OWNERSHIP_MISMATCH";
    pub const VLM_EVALUATION_DECISION_MISSING: &str = "PKG_VLM_EVALUATION_DECISION_MISSING";
    pub const COVERAGE_COUNT_MISMATCH: &str = "PKG_COVERAGE_COUNT_MISMATCH";
    pub const RETRY_COUNTER_INVALID: &str = "PKG_RETRY_COUNTER_INVALID";
    pub const MANIFEST_LINK_BROKEN: &str = "PKG_MANIFEST_LINK_BROKEN";
    pub const PROVIDER_READINESS_MISSING: &str = "PKG_PROVIDER_READINESS_MISSING";
    pub const RETRIEVAL_TRACE_MISSING: &str = "PKG_RETRIEVAL_TRACE_MISSING";
    pub const SECRET_LEAK: &str = "PKG_SECRET_LEAK";
    pub const FIXTURE_USED_IN_PRODUCTION_PASS: &str = "PKG_FIXTURE_USED_IN_PRODUCTION_PASS";
}

// ---------------------------------------------------------------------------
// Canonical package file names
// ---------------------------------------------------------------------------

/// The complete set of canonical v1.1 package files that must exist.
pub const CANONICAL_FILES: &[&str] = &[
    "image-recalls.json",
    "retrieved-images.json",
    "coverage-report.json",
    "retrieval-manifest.json",
    "package-summary.json",
    "delivery-report.json",
    "validation.json",
    "review.json",
    "handoff-report.json",
];

/// Subdirectories that must exist in the package root.
pub const CANONICAL_DIRS: &[&str] = &["images", "evidence", "diagnostics"];

/// Required fields in a DeliveredImageRecord that reference artifact files.
pub const REQUIRED_ARTIFACT_FIELDS: &[(&str, &str)] = &[
    ("local_artifact_path", "local artifact"),
    ("source_artifact_path", "source artifact"),
    ("source_sidecar_path", "source sidecar"),
    ("content_summary_path", "content summary"),
    ("task_report_path", "task report"),
    ("visual_description_path", "visual description"),
];

// ---------------------------------------------------------------------------
// Package validator
// ---------------------------------------------------------------------------

/// Configuration for a package validation run.
#[derive(Debug, Clone)]
pub struct PackageValidationRequest {
    pub package_dir: PathBuf,
    pub execution_mode: crate::domain::delivery::ExecutionMode,
    pub expected_query_plan_id: Option<String>,
}

/// Deterministic package validator.
pub struct PackageValidator;

impl PackageValidator {
    /// Create a new validator.
    pub fn new() -> Self {
        Self
    }

    /// Run the full deterministic validation suite against a package.
    ///
    /// Returns a `PackageValidationReport` with status and all check results.
    /// Does not modify the package.
    pub fn validate(&self, request: &PackageValidationRequest) -> Result<PackageValidationReport> {
        let package_dir = &request.package_dir;

        // Guard: package dir must exist
        if !package_dir.exists() || !package_dir.is_dir() {
            return Err(Error::InputRejection {
                reason: format!(
                    "package directory does not exist: {}",
                    package_dir.display()
                ),
            });
        }

        let mut issues: Vec<PackageValidationIssue> = Vec::new();
        let mut file_checks: Vec<FileCheck> = Vec::new();
        let mut artifact_checks: Vec<ArtifactCheck> = Vec::new();
        let mut redaction_checks: Vec<RedactionCheck> = Vec::new();
        let mut counter_checks: Vec<CounterCheck> = Vec::new();
        let mut coverage_checks: Vec<CoverageCheck> = Vec::new();

        let mut issue_counter: u32 = 0;
        let mut next_issue_id = || {
            issue_counter += 1;
            format!("pvi-{:04}", issue_counter)
        };

        // ---- 1. Check required package directories ----
        for dir_name in CANONICAL_DIRS {
            let dir_path = package_dir.join(dir_name);
            file_checks.push(FileCheck {
                file_name: format!("{}/", dir_name),
                exists: dir_path.exists() && dir_path.is_dir(),
                valid_json: None,
                message: if dir_path.exists() {
                    format!("directory '{}' exists", dir_name)
                } else {
                    format!("directory '{}' is missing", dir_name)
                },
            });
        }

        // ---- 2. Check required canonical files exist and are valid JSON ----
        let mut canonical_data: std::collections::HashMap<String, serde_json::Value> =
            std::collections::HashMap::new();

        for file_name in CANONICAL_FILES {
            let file_path = package_dir.join(file_name);
            let exists = file_path.exists() && file_path.is_file();
            let mut valid_json = None;

            if exists {
                match std::fs::read_to_string(&file_path) {
                    Ok(content) => match serde_json::from_str::<serde_json::Value>(&content) {
                        Ok(value) => {
                            valid_json = Some(true);
                            canonical_data.insert((*file_name).to_string(), value);
                        }
                        Err(e) => {
                            valid_json = Some(false);
                            issues.push(PackageValidationIssue {
                                issue_id: next_issue_id(),
                                code: codes::JSON_INVALID.into(),
                                severity: "error".into(),
                                subject: file_name.to_string(),
                                message: format!("{} is invalid JSON: {}", file_name, e),
                                artifact_path: Some(file_path.display().to_string()),
                                expected: None,
                                actual: None,
                            });
                        }
                    },
                    Err(e) => {
                        valid_json = Some(false);
                        issues.push(PackageValidationIssue {
                            issue_id: next_issue_id(),
                            code: codes::JSON_INVALID.into(),
                            severity: "error".into(),
                            subject: file_name.to_string(),
                            message: format!("cannot read {}: {}", file_name, e),
                            artifact_path: Some(file_path.display().to_string()),
                            expected: None,
                            actual: None,
                        });
                    }
                }
            } else {
                issues.push(PackageValidationIssue {
                    issue_id: next_issue_id(),
                    code: codes::REQUIRED_FILE_MISSING.into(),
                    severity: "error".into(),
                    subject: file_name.to_string(),
                    message: format!("required canonical file '{}' is missing", file_name),
                    artifact_path: Some(file_path.display().to_string()),
                    expected: None,
                    actual: None,
                });
            }

            file_checks.push(FileCheck {
                file_name: (*file_name).to_string(),
                exists,
                valid_json,
                message: if exists && valid_json == Some(true) {
                    format!("file '{}' exists and is valid JSON", file_name)
                } else if exists {
                    format!("file '{}' exists but is NOT valid JSON", file_name)
                } else {
                    format!("file '{}' is MISSING", file_name)
                },
            });
        }

        // ---- 3. Verify query_plan_id consistency ----
        let query_plan_ids: Vec<Option<&str>> = [
            "image-recalls.json",
            "retrieved-images.json",
            "coverage-report.json",
            "retrieval-manifest.json",
            "package-summary.json",
        ]
        .iter()
        .filter_map(|f| canonical_data.get(*f))
        .map(|v| v.get("query_plan_id").and_then(|id| id.as_str()))
        .collect();

        if query_plan_ids.len() >= 2 {
            let first = query_plan_ids[0];
            for (_i, id) in query_plan_ids.iter().enumerate().skip(1) {
                if *id != first {
                    issues.push(PackageValidationIssue {
                        issue_id: next_issue_id(),
                        code: codes::QUERY_PLAN_MISMATCH.into(),
                        severity: "error".into(),
                        subject: "query_plan_id".into(),
                        message: format!(
                            "package files disagree on query_plan_id: expected '{}', found '{}'",
                            first.unwrap_or("null"),
                            id.unwrap_or("null")
                        ),
                        artifact_path: None,
                        expected: first.map(|s| s.to_string()),
                        actual: id.map(|s| s.to_string()),
                    });
                }
            }
        }

        if let Some(ref expected) = request.expected_query_plan_id {
            if let Some(actual) = query_plan_ids.first().and_then(|id| *id) {
                if actual != expected.as_str() {
                    issues.push(PackageValidationIssue {
                        issue_id: next_issue_id(),
                        code: codes::QUERY_PLAN_MISMATCH.into(),
                        severity: "error".into(),
                        subject: "query_plan_id".into(),
                        message: format!(
                            "package query_plan_id '{}' does not match expected '{}'",
                            actual, expected
                        ),
                        artifact_path: None,
                        expected: Some(expected.clone()),
                        actual: Some(actual.to_string()),
                    });
                }
            }
        }

        // ---- 4. Check retry counter invariants from coverage-report.json ----
        if let Some(coverage) = canonical_data.get("coverage-report.json") {
            let full_attempt_count = coverage
                .get("full_attempt_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;
            let retry_count = coverage
                .get("retry_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;

            let expected_retry = if full_attempt_count > 0 {
                full_attempt_count - 1
            } else {
                0
            };

            if retry_count != expected_retry {
                issues.push(PackageValidationIssue {
                    issue_id: next_issue_id(),
                    code: codes::RETRY_COUNTER_INVALID.into(),
                    severity: "error".into(),
                    subject: "retry_count".into(),
                    message: format!(
                        "retry_count ({}) != full_attempt_count - 1 ({})",
                        retry_count, expected_retry
                    ),
                    artifact_path: None,
                    expected: Some(expected_retry.to_string()),
                    actual: Some(retry_count.to_string()),
                });
                counter_checks.push(CounterCheck {
                    invariant: "retry_count = full_attempt_count - 1".into(),
                    passed: false,
                    expected: expected_retry.to_string(),
                    actual: retry_count.to_string(),
                    message: "retry counter invariant violated".into(),
                });
            } else {
                counter_checks.push(CounterCheck {
                    invariant: "retry_count = full_attempt_count - 1".into(),
                    passed: true,
                    expected: expected_retry.to_string(),
                    actual: retry_count.to_string(),
                    message: "retry counter invariant holds".into(),
                });
            }

            if full_attempt_count > 4 {
                issues.push(PackageValidationIssue {
                    issue_id: next_issue_id(),
                    code: codes::RETRY_COUNTER_INVALID.into(),
                    severity: "error".into(),
                    subject: "full_attempt_count".into(),
                    message: format!(
                        "full_attempt_count ({}) exceeds default limit (4)",
                        full_attempt_count
                    ),
                    artifact_path: None,
                    expected: Some("<= 4".into()),
                    actual: Some(full_attempt_count.to_string()),
                });
            }
        }

        // ---- 5. Check coverage invariants ----
        if let Some(coverage) = canonical_data.get("coverage-report.json") {
            let required = coverage
                .get("required_image_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;
            let accepted = coverage
                .get("accepted_image_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;
            let gap_count = coverage
                .get("gap_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;

            let expected_gap = required.saturating_sub(accepted);
            if gap_count != expected_gap {
                issues.push(PackageValidationIssue {
                    issue_id: next_issue_id(),
                    code: codes::COVERAGE_COUNT_MISMATCH.into(),
                    severity: "error".into(),
                    subject: "gap_count".into(),
                    message: format!(
                        "gap_count ({}) != required_image_count - accepted_image_count ({})",
                        gap_count, expected_gap
                    ),
                    artifact_path: None,
                    expected: Some(expected_gap.to_string()),
                    actual: Some(gap_count.to_string()),
                });
                coverage_checks.push(CoverageCheck {
                    check: "gap_count = required - accepted".into(),
                    passed: false,
                    message: "coverage count mismatch".into(),
                });
            } else {
                coverage_checks.push(CoverageCheck {
                    check: "gap_count = required - accepted".into(),
                    passed: true,
                    message: "coverage counts are consistent".into(),
                });
            }

            // Check status consistency
            let status = coverage
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            match status {
                "passed" if accepted < required => {
                    issues.push(PackageValidationIssue {
                        issue_id: next_issue_id(),
                        code: codes::COVERAGE_COUNT_MISMATCH.into(),
                        severity: "error".into(),
                        subject: "status".into(),
                        message: "package is 'passed' but accepted < required".into(),
                        artifact_path: None,
                        expected: Some("accepted >= required".into()),
                        actual: Some(format!("accepted={}, required={}", accepted, required)),
                    });
                }
                "partial" if accepted == 0 => {
                    issues.push(PackageValidationIssue {
                        issue_id: next_issue_id(),
                        code: codes::COVERAGE_COUNT_MISMATCH.into(),
                        severity: "error".into(),
                        subject: "status".into(),
                        message: "package is 'partial' but has zero accepted images".into(),
                        artifact_path: None,
                        expected: None,
                        actual: None,
                    });
                }
                _ => {}
            }
        }

        // ---- 6. Check delivered images for artifacts ----
        if let Some(retrieved) = canonical_data.get("retrieved-images.json") {
            let retrieval_results = retrieved
                .get("retrieval_results")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            let image_acceptance_decisions = retrieved
                .get("image_acceptance_decisions")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();

            if let Some(delivered) = retrieved.get("delivered_images").and_then(|v| v.as_array()) {
                for img in delivered {
                    let candidate_id = img
                        .get("candidate_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");

                    // Check retrieval_job_id
                    match img.get("retrieval_job_id") {
                        Some(v) if v.is_string() && !v.as_str().unwrap_or("").is_empty() => {}
                        _ => {
                            issues.push(PackageValidationIssue {
                                issue_id: next_issue_id(),
                                code: codes::DELIVERED_IMAGE_ARTIFACT_MISSING.into(),
                                severity: "error".into(),
                                subject: candidate_id.to_string(),
                                message: format!(
                                    "delivered image '{}' lacks retrieval_job_id",
                                    candidate_id
                                ),
                                artifact_path: None,
                                expected: None,
                                actual: None,
                            });
                        }
                    }

                    // Check checksum
                    match img.get("checksum_sha256") {
                        Some(v) if v.is_string() && !v.as_str().unwrap_or("").is_empty() => {}
                        _ => {
                            issues.push(PackageValidationIssue {
                                issue_id: next_issue_id(),
                                code: codes::CHECKSUM_MISSING.into(),
                                severity: "error".into(),
                                subject: candidate_id.to_string(),
                                message: format!(
                                    "delivered image '{}' lacks checksum_sha256",
                                    candidate_id
                                ),
                                artifact_path: None,
                                expected: None,
                                actual: None,
                            });
                        }
                    }

                    // Check required artifact paths
                    for (field, label) in REQUIRED_ARTIFACT_FIELDS {
                        let path_str = img.get(*field).and_then(|v| v.as_str()).unwrap_or("");
                        if path_str.is_empty() {
                            issues.push(PackageValidationIssue {
                                issue_id: next_issue_id(),
                                code: codes::DELIVERED_IMAGE_ARTIFACT_MISSING.into(),
                                severity: "error".into(),
                                subject: candidate_id.to_string(),
                                message: format!(
                                    "delivered image '{}' is missing {} ({})",
                                    candidate_id, label, field
                                ),
                                artifact_path: None,
                                expected: None,
                                actual: None,
                            });
                        } else if let Err(reason) =
                            resolve_package_artifact_path(package_dir, path_str)
                        {
                            issues.push(PackageValidationIssue {
                                issue_id: next_issue_id(),
                                code: codes::DELIVERED_IMAGE_ARTIFACT_MISSING.into(),
                                severity: "error".into(),
                                subject: candidate_id.to_string(),
                                message: format!(
                                    "required artifact '{}' path '{}' is outside package root: {}",
                                    label, path_str, reason
                                ),
                                artifact_path: Some(path_str.to_string()),
                                expected: Some("package-relative path inside package root".into()),
                                actual: Some(path_str.to_string()),
                            });
                        } else {
                            let resolved_path =
                                resolve_package_artifact_path(package_dir, path_str).unwrap();
                            let full_path = resolved_path.display().to_string();
                            let exists = Path::new(&full_path).exists();
                            let non_empty = if exists {
                                std::fs::metadata(&full_path)
                                    .map(|m| m.len() > 0)
                                    .unwrap_or(false)
                            } else {
                                false
                            };

                            artifact_checks.push(ArtifactCheck {
                                candidate_id: candidate_id.to_string(),
                                artifact_type: (*label).to_string(),
                                path: full_path.clone(),
                                exists,
                                non_empty,
                                message: if exists && non_empty {
                                    format!(
                                        "artifact '{}' for '{}' exists and is non-empty",
                                        label, candidate_id
                                    )
                                } else if exists {
                                    format!(
                                        "artifact '{}' for '{}' exists but is EMPTY",
                                        label, candidate_id
                                    )
                                } else {
                                    format!(
                                        "artifact '{}' for '{}' is MISSING",
                                        label, candidate_id
                                    )
                                },
                            });

                            if !exists {
                                issues.push(PackageValidationIssue {
                                    issue_id: next_issue_id(),
                                    code: codes::DELIVERED_IMAGE_ARTIFACT_MISSING.into(),
                                    severity: "error".into(),
                                    subject: candidate_id.to_string(),
                                    message: format!(
                                        "required artifact '{}' at '{}' does not exist for delivered image '{}'",
                                        label, full_path, candidate_id
                                    ),
                                    artifact_path: Some(full_path),
                                    expected: None,
                                    actual: None,
                                });
                            }
                        }
                    }

                    // Check content type
                    let ct = img
                        .get("content_type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    if ct.is_empty() || ct == "unknown" {
                        issues.push(PackageValidationIssue {
                            issue_id: next_issue_id(),
                            code: codes::MEDIA_TYPE_MISMATCH.into(),
                            severity: "warning".into(),
                            subject: candidate_id.to_string(),
                            message: format!(
                                "delivered image '{}' has missing or unknown content_type",
                                candidate_id
                            ),
                            artifact_path: None,
                            expected: None,
                            actual: None,
                        });
                    }

                    let retrieval_job_id = img
                        .get("retrieval_job_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let matching_retrieval_result = retrieval_results.iter().find(|result| {
                        result.get("candidate_id").and_then(|v| v.as_str()) == Some(candidate_id)
                            && result.get("retrieval_job_id").and_then(|v| v.as_str())
                                == Some(retrieval_job_id)
                            && result.get("retrieval_status").and_then(|v| v.as_str())
                                == Some("complete")
                    });
                    if matching_retrieval_result.is_none() {
                        issues.push(PackageValidationIssue {
                            issue_id: next_issue_id(),
                            code: codes::RETRIEVAL_TRACE_MISSING.into(),
                            severity: "error".into(),
                            subject: candidate_id.to_string(),
                            message: format!(
                                "delivered image '{}' is missing retrieval decision evidence",
                                candidate_id
                            ),
                            artifact_path: None,
                            expected: Some(
                                "matching retrieved-images.json retrieval_results entry".into(),
                            ),
                            actual: None,
                        });
                    } else if let Some(result) = matching_retrieval_result {
                        let channel_id = result
                            .get("channel_id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        let channel_tier = result
                            .get("channel_tier")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        let fetch_trace_len = result
                            .get("fetch_trace")
                            .and_then(|v| v.as_array())
                            .map(|v| v.len())
                            .unwrap_or(0);
                        if channel_id.is_empty()
                            || channel_id == "unknown"
                            || channel_tier.is_empty()
                            || channel_tier == "unknown"
                            || fetch_trace_len == 0
                        {
                            issues.push(PackageValidationIssue {
                                issue_id: next_issue_id(),
                                code: codes::RETRIEVAL_TRACE_MISSING.into(),
                                severity: "error".into(),
                                subject: candidate_id.to_string(),
                                message: format!(
                                    "delivered image '{}' has incomplete retrieval trace evidence: channel_id='{}', channel_tier='{}', fetch_trace entries={}",
                                    candidate_id, channel_id, channel_tier, fetch_trace_len
                                ),
                                artifact_path: Some("retrieved-images.json".into()),
                                expected: Some(
                                    "non-unknown channel_id/channel_tier and non-empty fetch_trace"
                                        .into(),
                                ),
                                actual: Some(format!(
                                    "channel_id='{}', channel_tier='{}', fetch_trace={}",
                                    channel_id, channel_tier, fetch_trace_len
                                )),
                            });
                        }
                        if result.get("media_type_match").and_then(|v| v.as_bool()) != Some(true) {
                            issues.push(PackageValidationIssue {
                                issue_id: next_issue_id(),
                                code: codes::MEDIA_TYPE_MISMATCH.into(),
                                severity: "error".into(),
                                subject: candidate_id.to_string(),
                                message: format!(
                                    "delivered image '{}' retrieval evidence does not confirm media_type_match",
                                    candidate_id
                                ),
                                artifact_path: Some("retrieved-images.json".into()),
                                expected: Some("media_type_match=true".into()),
                                actual: result
                                    .get("media_type_match")
                                    .map(|v| v.to_string()),
                            });
                        }
                    }

                    let matching_image_decision =
                        image_acceptance_decisions.iter().find(|decision| {
                            decision.get("candidate_id").and_then(|v| v.as_str())
                                == Some(candidate_id)
                                && decision.get("retrieval_job_id").and_then(|v| v.as_str())
                                    == Some(retrieval_job_id)
                                && decision.get("final_status").and_then(|v| v.as_str())
                                    == Some("accepted")
                        });
                    if matching_image_decision.is_none() {
                        issues.push(PackageValidationIssue {
                            issue_id: next_issue_id(),
                            code: codes::VLM_EVALUATION_DECISION_MISSING.into(),
                            severity: "error".into(),
                            subject: candidate_id.to_string(),
                            message: format!(
                                "delivered image '{}' is missing image decision evidence",
                                candidate_id
                            ),
                            artifact_path: None,
                            expected: Some(
                                "matching retrieved-images.json image_acceptance_decisions entry"
                                    .into(),
                            ),
                            actual: None,
                        });
                    } else if let Some(decision) = matching_image_decision {
                        let decision_ok = decision
                            .get("mechanical_passed")
                            .and_then(|v| v.as_bool())
                            == Some(true)
                            && decision.get("vlm_passed").and_then(|v| v.as_bool()) == Some(true)
                            && decision.get("artifact_complete").and_then(|v| v.as_bool())
                                == Some(true);
                        let vlm_decision = decision.get("vlm_decision");
                        let provider_id = vlm_decision
                            .and_then(|v| v.as_object())
                            .and_then(|obj| obj.get("provider_id"))
                            .and_then(|v| v.as_str());
                        let vlm_ok = vlm_decision
                            .and_then(|v| v.as_object())
                            .and_then(|obj| obj.get("decision"))
                            .and_then(|v| v.as_str())
                            == Some("approve")
                            && provider_id.is_some_and(|provider| {
                                !provider.is_empty() && provider != "openclaw_legacy"
                            });
                        if !decision_ok || !vlm_ok {
                            issues.push(PackageValidationIssue {
                                issue_id: next_issue_id(),
                                code: codes::VLM_EVALUATION_DECISION_MISSING.into(),
                                severity: "error".into(),
                                subject: candidate_id.to_string(),
                                message: format!(
                                    "delivered image '{}' image acceptance evidence lacks truthful pass flags or vlm_decision",
                                    candidate_id
                                ),
                                artifact_path: Some("retrieved-images.json".into()),
                                expected: Some(
                                    "mechanical_passed=true, vlm_passed=true, artifact_complete=true, vlm_decision.decision=approve"
                                        .into(),
                                ),
                                actual: Some(decision.to_string()),
                            });
                        }
                        for (path, reason) in invalid_reference_metric_paths(package_dir, decision)
                        {
                            issues.push(PackageValidationIssue {
                                issue_id: next_issue_id(),
                                code: codes::DELIVERED_IMAGE_ARTIFACT_MISSING.into(),
                                severity: "error".into(),
                                subject: candidate_id.to_string(),
                                message: format!(
                                    "delivered image '{}' has reference metric path outside package root or missing: {} ({})",
                                    candidate_id, path, reason
                                ),
                                artifact_path: Some("retrieved-images.json".into()),
                                expected: Some(
                                    "reference metric path is package-relative, inside package root, and exists"
                                        .into(),
                                ),
                                actual: Some(path),
                            });
                        }
                    }

                    for (field, label) in [
                        (
                            "candidate_quality_decision_ref",
                            "candidate decision evidence",
                        ),
                        ("image_acceptance_decision_ref", "image decision evidence"),
                    ] {
                        let ref_str = img.get(field).and_then(|v| v.as_str()).unwrap_or("");
                        if ref_str.is_empty()
                            || !package_reference_exists(package_dir, &canonical_data, ref_str)
                        {
                            issues.push(PackageValidationIssue {
                                issue_id: next_issue_id(),
                                code: codes::VLM_EVALUATION_DECISION_MISSING.into(),
                                severity: "error".into(),
                                subject: candidate_id.to_string(),
                                message: format!(
                                    "delivered image '{}' is missing {} ({})",
                                    candidate_id, label, field
                                ),
                                artifact_path: if ref_str.is_empty() {
                                    None
                                } else {
                                    Some(ref_str.to_string())
                                },
                                expected: Some("resolvable package evidence reference".into()),
                                actual: if ref_str.is_empty() {
                                    None
                                } else {
                                    Some(ref_str.to_string())
                                },
                            });
                        } else if field == "candidate_quality_decision_ref" {
                            if let Some(decision) = resolve_package_reference_value(
                                package_dir,
                                &canonical_data,
                                ref_str,
                            ) {
                                let candidate_decision_ok = decision
                                    .get("candidate_id")
                                    .and_then(|v| v.as_str())
                                    == Some(candidate_id)
                                    && decision.get("mechanical_passed").and_then(|v| v.as_bool())
                                        == Some(true)
                                    && decision.get("vlm_passed").and_then(|v| v.as_bool())
                                        == Some(true)
                                    && decision.get("final_status").and_then(|v| v.as_str())
                                        == Some("retrievable");
                                let vlm_decision = decision.get("vlm_decision");
                                let provider_id = vlm_decision
                                    .and_then(|v| v.as_object())
                                    .and_then(|obj| obj.get("provider_id"))
                                    .and_then(|v| v.as_str());
                                let vlm_ok = vlm_decision
                                    .and_then(|v| v.as_object())
                                    .and_then(|obj| obj.get("decision"))
                                    .and_then(|v| v.as_str())
                                    == Some("approve")
                                    && provider_id.is_some_and(|provider| {
                                        !provider.is_empty() && provider != "openclaw_legacy"
                                    });
                                if !candidate_decision_ok || !vlm_ok {
                                    issues.push(PackageValidationIssue {
                                        issue_id: next_issue_id(),
                                        code: codes::VLM_EVALUATION_DECISION_MISSING.into(),
                                        severity: "error".into(),
                                        subject: candidate_id.to_string(),
                                        message: format!(
                                            "delivered image '{}' candidate quality evidence lacks truthful pass flags or vlm_decision",
                                            candidate_id
                                        ),
                                        artifact_path: Some(ref_str.to_string()),
                                        expected: Some(
                                            "candidate quality mechanical_passed=true, vlm_passed=true, final_status=retrievable, vlm_decision.decision=approve"
                                                .into(),
                                        ),
                                        actual: Some(decision.to_string()),
                                    });
                                }
                                for (path, reason) in
                                    invalid_reference_metric_paths(package_dir, &decision)
                                {
                                    issues.push(PackageValidationIssue {
                                        issue_id: next_issue_id(),
                                        code: codes::DELIVERED_IMAGE_ARTIFACT_MISSING.into(),
                                        severity: "error".into(),
                                        subject: candidate_id.to_string(),
                                        message: format!(
                                            "delivered image '{}' candidate quality reference metric path outside package root or missing: {} ({})",
                                            candidate_id, path, reason
                                        ),
                                        artifact_path: Some(ref_str.to_string()),
                                        expected: Some(
                                            "candidate quality reference metric path is package-relative, inside package root, and exists"
                                                .into(),
                                        ),
                                        actual: Some(path),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        if let Some(delivery) = canonical_data.get("delivery-report.json") {
            if let Some(items) = delivery.get("items").and_then(|v| v.as_array()) {
                for item in items {
                    let candidate_id = item
                        .get("candidate_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("delivery-item");
                    for (path, reason) in invalid_reference_metric_paths(package_dir, item) {
                        issues.push(PackageValidationIssue {
                            issue_id: next_issue_id(),
                            code: codes::DELIVERED_IMAGE_ARTIFACT_MISSING.into(),
                            severity: "error".into(),
                            subject: candidate_id.to_string(),
                            message: format!(
                                "delivery item '{}' has reference metric path outside package root or missing: {} ({})",
                                candidate_id, path, reason
                            ),
                            artifact_path: Some("delivery-report.json".into()),
                            expected: Some(
                                "reference metric path is package-relative, inside package root, and exists"
                                    .into(),
                            ),
                            actual: Some(path),
                        });
                    }
                }
            }
        }

        if let Some(manifest) = canonical_data.get("retrieval-manifest.json") {
            if let Some(entries) = manifest.get("entries").and_then(|v| v.as_array()) {
                for entry in entries {
                    let subject = entry
                        .get("candidate_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("manifest-entry");
                    for field in [
                        "search_ref",
                        "candidate_quality_ref",
                        "retrieval_result_ref",
                        "image_acceptance_ref",
                        "delivery_ref",
                    ] {
                        let ref_str = entry.get(field).and_then(|v| v.as_str()).unwrap_or("");
                        if ref_str.is_empty()
                            || !package_reference_exists(package_dir, &canonical_data, ref_str)
                        {
                            issues.push(PackageValidationIssue {
                                issue_id: next_issue_id(),
                                code: codes::MANIFEST_LINK_BROKEN.into(),
                                severity: "error".into(),
                                subject: subject.to_string(),
                                message: format!(
                                    "manifest field '{}' points to missing package reference '{}'",
                                    field, ref_str
                                ),
                                artifact_path: if ref_str.is_empty() {
                                    None
                                } else {
                                    Some(ref_str.to_string())
                                },
                                expected: Some("resolvable package reference".into()),
                                actual: if ref_str.is_empty() {
                                    None
                                } else {
                                    Some(ref_str.to_string())
                                },
                            });
                        }
                    }
                }
            }
        }

        // ---- 7. Sensitive data scan ----
        let scanned_files: Vec<PathBuf> = {
            let mut files = canonical_data
                .keys()
                .map(|f| package_dir.join(f))
                .collect::<Vec<_>>();
            // Also scan evidence and diagnostics subdirectories
            for subdir in &["evidence", "diagnostics"] {
                let dir = package_dir.join(subdir);
                if dir.exists() {
                    collect_files_recursive(&dir, &mut files);
                }
            }
            files
        };

        for file_path in &scanned_files {
            if let Ok(content) = std::fs::read_to_string(file_path) {
                if contains_sensitive_pattern(&content) {
                    let rel_path = file_path
                        .strip_prefix(package_dir)
                        .unwrap_or(file_path)
                        .display()
                        .to_string();
                    issues.push(PackageValidationIssue {
                        issue_id: next_issue_id(),
                        code: codes::SECRET_LEAK.into(),
                        severity: "error".into(),
                        subject: rel_path.clone(),
                        message: format!(
                            "sensitive data (credentials/tokens) detected in '{}'",
                            rel_path
                        ),
                        artifact_path: Some(file_path.display().to_string()),
                        expected: None,
                        actual: None,
                    });
                    redaction_checks.push(RedactionCheck {
                        file: rel_path,
                        passed: false,
                        found_patterns: vec!["credential/token pattern".into()],
                        message: "sensitive data detected".into(),
                    });
                } else {
                    let rel_path = file_path
                        .strip_prefix(package_dir)
                        .unwrap_or(file_path)
                        .display()
                        .to_string();
                    redaction_checks.push(RedactionCheck {
                        file: rel_path,
                        passed: true,
                        found_patterns: vec![],
                        message: "no sensitive patterns found".into(),
                    });
                }
            }
        }

        // ---- 8. Fixture-in-production check ----
        if request.execution_mode == crate::domain::delivery::ExecutionMode::Production {
            // Check if any package data indicates fixture sources
            if let Some(summary) = canonical_data.get("package-summary.json") {
                let status = summary.get("status").and_then(|v| v.as_str()).unwrap_or("");
                if status == "passed" {
                    // Scan for fixture indicators
                    let fixture_indicators = ["fixture", "mock", "test-only", "non-production"];
                    for (file_name, value) in &canonical_data {
                        let text = value.to_string().to_lowercase();
                        for indicator in &fixture_indicators {
                            if text.contains(indicator) {
                                issues.push(PackageValidationIssue {
                                    issue_id: next_issue_id(),
                                    code: codes::FIXTURE_USED_IN_PRODUCTION_PASS.into(),
                                    severity: "error".into(),
                                    subject: file_name.clone(),
                                    message: format!(
                                        "production package 'passed' but contains fixture indicator '{}' in '{}'",
                                        indicator, file_name
                                    ),
                                    artifact_path: None,
                                    expected: None,
                                    actual: None,
                                });
                                break;
                            }
                        }
                    }
                }
            }
        }

        // ---- Determine final validation status ----
        let has_errors = issues.iter().any(|i| i.severity == "error");
        let has_blockers = file_checks
            .iter()
            .any(|fc| fc.file_name.ends_with(".json") && !fc.exists);

        let status = if has_errors || has_blockers {
            ValidationStatus::Fail
        } else {
            ValidationStatus::Pass
        };

        Ok(PackageValidationReport {
            schema_version: 1,
            validator_version: "v1.1".into(),
            package_dir: package_dir.display().to_string(),
            status,
            validated_at: utc_now_rfc3339_seconds(),
            issues,
            file_checks,
            artifact_checks,
            redaction_checks,
            counter_checks,
            coverage_checks,
        })
    }
}

fn utc_now_rfc3339_seconds() -> String {
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

impl Default for PackageValidator {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Convenience: validate a package directory with default settings
// ---------------------------------------------------------------------------

/// Validate a package directory with fixture mode. Returns the report or an
/// error if the directory cannot be read.
pub fn validate_package_dir(package_dir: &Path) -> Result<PackageValidationReport> {
    validate_package_dir_with_mode(package_dir, crate::domain::delivery::ExecutionMode::Fixture)
}

/// Validate a package directory with an explicit execution mode.
pub fn validate_package_dir_with_mode(
    package_dir: &Path,
    execution_mode: crate::domain::delivery::ExecutionMode,
) -> Result<PackageValidationReport> {
    let validator = PackageValidator::new();
    let request = PackageValidationRequest {
        package_dir: package_dir.to_path_buf(),
        execution_mode,
        expected_query_plan_id: None,
    };
    validator.validate(&request)
}

fn resolve_package_artifact_path(
    package_dir: &Path,
    path_str: &str,
) -> std::result::Result<PathBuf, String> {
    let path = Path::new(path_str);
    if path.is_absolute() {
        return Err("absolute paths are not allowed".into());
    }
    if path
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err("parent-directory components are not allowed".into());
    }
    Ok(package_dir.join(path))
}

fn package_reference_exists(
    package_dir: &Path,
    canonical_data: &std::collections::HashMap<String, serde_json::Value>,
    ref_str: &str,
) -> bool {
    if ref_str.is_empty() {
        return false;
    }
    if let Some((file, pointer)) = ref_str.split_once("#") {
        let Some(value) = canonical_data.get(file) else {
            return false;
        };
        if pointer.is_empty() {
            return true;
        }
        return pointer
            .strip_prefix('/')
            .map(|p| value.pointer(&format!("/{}", p)).is_some())
            .unwrap_or(false);
    }
    resolve_package_artifact_path(package_dir, ref_str)
        .map(|path| path.exists() && path.is_file())
        .unwrap_or(false)
}

fn resolve_package_reference_value(
    package_dir: &Path,
    canonical_data: &std::collections::HashMap<String, serde_json::Value>,
    ref_str: &str,
) -> Option<serde_json::Value> {
    if ref_str.is_empty() {
        return None;
    }

    if let Some((file, pointer)) = ref_str.split_once("#") {
        let value = canonical_data
            .get(file)
            .cloned()
            .or_else(|| read_package_json(package_dir, file))?;
        if pointer.is_empty() {
            return Some(value);
        }
        let pointer = if pointer.starts_with('/') {
            pointer.to_string()
        } else {
            format!("/{}", pointer)
        };
        return value.pointer(&pointer).cloned();
    }

    read_package_json(package_dir, ref_str)
}

fn read_package_json(package_dir: &Path, path_str: &str) -> Option<serde_json::Value> {
    let path = resolve_package_artifact_path(package_dir, path_str).ok()?;
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn invalid_reference_metric_paths(
    package_dir: &Path,
    value: &serde_json::Value,
) -> Vec<(String, String)> {
    let mut invalid = Vec::new();
    if let Some(metrics) = value.get("reference_metrics").and_then(|v| v.as_array()) {
        for metric in metrics {
            collect_invalid_reference_paths(package_dir, metric, &mut invalid);
        }
    }
    invalid
}

fn collect_invalid_reference_paths(
    package_dir: &Path,
    value: &serde_json::Value,
    invalid: &mut Vec<(String, String)>,
) {
    match value {
        serde_json::Value::Array(values) => {
            for value in values {
                collect_invalid_reference_paths(package_dir, value, invalid);
            }
        }
        serde_json::Value::Object(map) => {
            for (key, value) in map {
                if key == "path" {
                    if let Some(path_str) = value.as_str() {
                        match resolve_package_artifact_path(package_dir, path_str) {
                            Ok(path) if path.exists() && path.is_file() => {}
                            Ok(_) => invalid.push((
                                path_str.to_string(),
                                "path does not exist inside package".into(),
                            )),
                            Err(reason) => invalid.push((path_str.to_string(), reason)),
                        }
                    }
                }
                collect_invalid_reference_paths(package_dir, value, invalid);
            }
        }
        _ => {}
    }
}

fn collect_files_recursive(dir: &Path, files: &mut Vec<PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_files_recursive(&path, files);
            } else if path.is_file() {
                files.push(path);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir(prefix: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("val-{}-{}-{}", prefix, std::process::id(), n));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_json(dir: &Path, name: &str, value: &serde_json::Value) {
        let content = serde_json::to_string_pretty(value).unwrap();
        fs::write(dir.join(name), content).unwrap();
    }

    fn make_minimal_package(dir: &Path, status: &str) {
        // Create subdirectories
        for sub in CANONICAL_DIRS {
            fs::create_dir_all(dir.join(sub)).ok();
        }

        // Write canonical files
        let run_id = "run-test-1";
        let qp_id = "qp-test-1";

        write_json(
            dir,
            "image-recalls.json",
            &serde_json::json!({
                "schema_version": 1,
                "run_id": run_id,
                "query_plan_id": qp_id,
                "candidate_target": 20,
                "attempts": [{
                    "full_attempt_count": 1,
                    "retry_count": 0,
                    "candidate_count": 1,
                    "target_met": false,
                    "candidates": [{
                        "candidate_id": "cand-1",
                        "query_plan_id": qp_id,
                        "provider_id": "fixture_search",
                        "candidate_quality_ref": "evidence/candidate-quality/cand-1.json"
                    }]
                }]
            }),
        );

        write_json(
            dir,
            "retrieved-images.json",
            &serde_json::json!({
                "schema_version": 1,
                "run_id": run_id,
                "query_plan_id": qp_id,
                "retrieval_batch_target": 2,
                "retrieval_results": [],
                "image_acceptance_decisions": [],
                "delivered_images": []
            }),
        );

        write_json(
            dir,
            "coverage-report.json",
            &serde_json::json!({
                "schema_version": 1,
                "run_id": run_id,
                "query_plan_id": qp_id,
                "required_image_count": 1,
                "accepted_image_count": 1,
                "gap_count": 0,
                "full_attempt_count": 1,
                "retry_count": 0,
                "status": status,
                "gaps": []
            }),
        );

        write_json(
            dir,
            "retrieval-manifest.json",
            &serde_json::json!({
                "schema_version": 1,
                "run_id": run_id,
                "query_plan_id": qp_id,
                "entries": []
            }),
        );

        write_json(
            dir,
            "package-summary.json",
            &serde_json::json!({
                "schema_version": 1,
                "run_id": run_id,
                "query_plan_id": qp_id,
                "status": status,
                "required_image_count": 1,
                "accepted_image_count": 1,
                "gap_count": 0,
                "full_attempt_count": 1,
                "retry_count": 0
            }),
        );

        write_json(
            dir,
            "delivery-report.json",
            &serde_json::json!({
                "schema_version": 1,
                "items": [],
                "rejected_items": [],
                "policy_notes": []
            }),
        );

        write_json(
            dir,
            "validation.json",
            &serde_json::json!({
                "schema_version": 1,
                "validator_version": "v1.1",
                "status": "pass"
            }),
        );

        write_json(
            dir,
            "review.json",
            &serde_json::json!({
                "schema_version": 1,
                "review_status": "pass",
                "findings": []
            }),
        );

        write_json(
            dir,
            "handoff-report.json",
            &serde_json::json!({
                "schema_version": 1,
                "handoff_status": "ready",
                "package_status": status,
                "delivered_image_count": 1,
                "required_image_count": 1
            }),
        );
    }

    fn write_required_artifacts(dir: &Path) {
        for path in [
            "images/image.jpg",
            "evidence/source.html",
            "evidence/sidecar.json",
            "evidence/summary.txt",
            "evidence/task-report.json",
            "evidence/visual-description.txt",
        ] {
            let full = dir.join(path);
            if let Some(parent) = full.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(full, "not empty").unwrap();
        }
        fs::create_dir_all(dir.join("evidence/candidate-quality")).unwrap();
        write_json(
            dir,
            "evidence/candidate-quality/cand-1.json",
            &serde_json::json!({
                "schema_version": 1,
                "candidate_id": "cand-1",
                "query_plan_id": "qp-test-1",
                "mechanical_passed": true,
                "vlm_passed": true,
                "final_status": "retrievable",
                "priority": 5,
                "blocking_metrics": [],
                "reference_metrics": [{"kind": "provider_confidence", "value": "fixture"}],
                "vlm_decision": {"decision": "approve", "provider_id": "fixture_vlm"},
                "redaction_applied": true
            }),
        );
    }

    fn write_package_with_single_delivered_image(
        dir: &Path,
        retrieval_result: serde_json::Value,
        image_decision: serde_json::Value,
    ) {
        write_required_artifacts(dir);
        write_json(
            dir,
            "retrieved-images.json",
            &serde_json::json!({
                "schema_version": 1,
                "run_id": "run-test-1",
                "query_plan_id": "qp-test-1",
                "retrieval_batch_target": 2,
                "retrieval_results": [retrieval_result],
                "image_acceptance_decisions": [image_decision],
                "delivered_images": [{
                    "delivered_image_id": "delivered-1",
                    "query_plan_id": "qp-test-1",
                    "candidate_id": "cand-1",
                    "retrieval_job_id": "job-1",
                    "package_image_path": "images/image.jpg",
                    "local_artifact_path": "images/image.jpg",
                    "source_artifact_path": "evidence/source.html",
                    "source_sidecar_path": "evidence/sidecar.json",
                    "content_summary_path": "evidence/summary.txt",
                    "task_report_path": "evidence/task-report.json",
                    "visual_description_path": "evidence/visual-description.txt",
                    "checksum_sha256": "sha256-ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
                    "content_type": "image/jpeg",
                    "file_size_bytes": 9,
                    "width": 1,
                    "height": 1,
                    "candidate_quality_decision_ref": "evidence/candidate-quality/cand-1.json",
                    "image_acceptance_decision_ref": "retrieved-images.json#/image_acceptance_decisions/0",
                    "manifest_entry_ref": "manifest-0001"
                }]
            }),
        );
        write_json(
            dir,
            "retrieval-manifest.json",
            &serde_json::json!({
                "schema_version": 1,
                "run_id": "run-test-1",
                "query_plan_id": "qp-test-1",
                "entries": [{
                    "manifest_entry_id": "manifest-0001",
                    "candidate_id": "cand-1",
                    "provider_id": "fixture_search",
                    "candidate_status": "delivered",
                    "retrieval_job_id": "job-1",
                    "search_ref": "image-recalls.json#/attempts/0/candidates/0",
                    "candidate_quality_ref": "evidence/candidate-quality/cand-1.json",
                    "retrieval_result_ref": "retrieved-images.json#/retrieval_results/0",
                    "image_acceptance_ref": "retrieved-images.json#/image_acceptance_decisions/0",
                    "delivery_ref": "delivery-report.json#/items/0",
                    "artifact_refs": [
                        "images/image.jpg",
                        "evidence/source.html",
                        "evidence/sidecar.json",
                        "evidence/summary.txt",
                        "evidence/task-report.json",
                        "evidence/visual-description.txt"
                    ]
                }]
            }),
        );
        write_json(
            dir,
            "delivery-report.json",
            &serde_json::json!({
                "schema_version": 1,
                "items": [{
                    "candidate_id": "cand-1",
                    "retrieval_job_id": "job-1",
                    "delivery_status": "delivered",
                    "mechanical_passed": true,
                    "vlm_passed": true,
                    "artifact_complete": true,
                    "blocking_reasons": [],
                    "reference_metrics": [{"kind": "file_size", "value": 9}],
                    "package_image_path": "images/image.jpg"
                }],
                "rejected_items": [],
                "policy_notes": []
            }),
        );
    }

    #[test]
    fn validator_missing_package_dir() {
        let validator = PackageValidator::new();
        let request = PackageValidationRequest {
            package_dir: PathBuf::from("/nonexistent/path/package"),
            execution_mode: crate::domain::delivery::ExecutionMode::Fixture,
            expected_query_plan_id: None,
        };
        let result = validator.validate(&request);
        assert!(result.is_err());
    }

    #[test]
    fn validator_pass_for_complete_package() {
        let dir = temp_dir("pass");
        make_minimal_package(&dir, "passed");

        let validator = PackageValidator::new();
        let request = PackageValidationRequest {
            package_dir: dir.clone(),
            execution_mode: crate::domain::delivery::ExecutionMode::Fixture,
            expected_query_plan_id: None,
        };
        let report = validator.validate(&request).unwrap();
        assert_eq!(report.status, ValidationStatus::Pass);
        assert_ne!(report.validated_at, "2026-06-22T00:00:00Z");
        assert!(report.validated_at.contains('T'));
        assert!(report.validated_at.ends_with('Z'));
        assert!(report.issues.is_empty());
        assert_eq!(report.file_checks.len(), 12); // 9 files + 3 dirs
        assert!(report.file_checks.iter().all(|fc| fc.exists));
        assert!(report.redaction_checks.iter().all(|rc| rc.passed));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn validator_fails_missing_required_file() {
        let dir = temp_dir("missing-file");
        make_minimal_package(&dir, "passed");
        // Delete one required file
        fs::remove_file(dir.join("image-recalls.json")).unwrap();

        let validator = PackageValidator::new();
        let request = PackageValidationRequest {
            package_dir: dir.clone(),
            execution_mode: crate::domain::delivery::ExecutionMode::Fixture,
            expected_query_plan_id: None,
        };
        let report = validator.validate(&request).unwrap();
        assert_eq!(report.status, ValidationStatus::Fail);
        assert!(report
            .issues
            .iter()
            .any(|i| i.code == codes::REQUIRED_FILE_MISSING));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn validator_fails_invalid_json() {
        let dir = temp_dir("bad-json");
        make_minimal_package(&dir, "passed");
        // Corrupt a JSON file
        fs::write(dir.join("image-recalls.json"), "not valid json {{{").unwrap();

        let validator = PackageValidator::new();
        let request = PackageValidationRequest {
            package_dir: dir.clone(),
            execution_mode: crate::domain::delivery::ExecutionMode::Fixture,
            expected_query_plan_id: None,
        };
        let report = validator.validate(&request).unwrap();
        assert_eq!(report.status, ValidationStatus::Fail);
        assert!(report.issues.iter().any(|i| i.code == codes::JSON_INVALID));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn validator_fails_retry_counter_mismatch() {
        let dir = temp_dir("bad-retry");
        make_minimal_package(&dir, "passed");
        // Write bad coverage report with counter mismatch
        write_json(
            &dir,
            "coverage-report.json",
            &serde_json::json!({
                "schema_version": 1,
                "run_id": "run-1",
                "query_plan_id": "qp-1",
                "required_image_count": 1,
                "accepted_image_count": 1,
                "gap_count": 0,
                "full_attempt_count": 3,
                "retry_count": 1,
                "status": "passed"
            }),
        );

        let validator = PackageValidator::new();
        let request = PackageValidationRequest {
            package_dir: dir.clone(),
            execution_mode: crate::domain::delivery::ExecutionMode::Fixture,
            expected_query_plan_id: None,
        };
        let report = validator.validate(&request).unwrap();
        assert_eq!(report.status, ValidationStatus::Fail);
        assert!(report
            .issues
            .iter()
            .any(|i| i.code == codes::RETRY_COUNTER_INVALID));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn validator_detects_secret_in_package() {
        let dir = temp_dir("secret-leak");
        make_minimal_package(&dir, "passed");
        // Inject a credential-like value
        fs::write(
            dir.join("image-recalls.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "schema_version": 1,
                "run_id": "run-1",
                "query_plan_id": "qp-1",
                "candidate_target": 20,
                "attempts": [],
                "Authorization": "Bearer sk-secret-token-abc123"
            }))
            .unwrap(),
        )
        .unwrap();

        let validator = PackageValidator::new();
        let request = PackageValidationRequest {
            package_dir: dir.clone(),
            execution_mode: crate::domain::delivery::ExecutionMode::Fixture,
            expected_query_plan_id: None,
        };
        let report = validator.validate(&request).unwrap();
        assert!(report.issues.iter().any(|i| i.code == codes::SECRET_LEAK));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn validator_detects_secret_in_nested_evidence_file() {
        let dir = temp_dir("nested-secret-leak");
        make_minimal_package(&dir, "passed");
        let nested = dir.join("evidence/candidate-quality");
        fs::create_dir_all(&nested).unwrap();
        fs::write(
            nested.join("cand-1.json"),
            r#"{"Authorization":"Bearer sk-nested-secret-token"}"#,
        )
        .unwrap();

        let validator = PackageValidator::new();
        let request = PackageValidationRequest {
            package_dir: dir.clone(),
            execution_mode: crate::domain::delivery::ExecutionMode::Fixture,
            expected_query_plan_id: None,
        };
        let report = validator.validate(&request).unwrap();
        assert!(report.issues.iter().any(|i| i.code == codes::SECRET_LEAK));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn validator_detects_query_plan_id_mismatch() {
        let dir = temp_dir("qp-mismatch");
        make_minimal_package(&dir, "passed");

        let validator = PackageValidator::new();
        let request = PackageValidationRequest {
            package_dir: dir.clone(),
            execution_mode: crate::domain::delivery::ExecutionMode::Fixture,
            expected_query_plan_id: Some("different-qp-id".into()),
        };
        let report = validator.validate(&request).unwrap();
        assert!(report
            .issues
            .iter()
            .any(|i| i.code == codes::QUERY_PLAN_MISMATCH));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn validator_rejects_delivered_artifact_paths_outside_package_root() {
        let dir = temp_dir("outside-artifact");
        make_minimal_package(&dir, "passed");

        let external = temp_dir("outside-artifact-source");
        for name in [
            "image.jpg",
            "source.html",
            "sidecar.json",
            "summary.txt",
            "task-report.json",
            "visual-description.txt",
        ] {
            fs::write(external.join(name), "not empty").unwrap();
        }

        write_json(
            &dir,
            "retrieved-images.json",
            &serde_json::json!({
                "schema_version": 1,
                "run_id": "run-test-1",
                "query_plan_id": "qp-test-1",
                "retrieval_batch_target": 2,
                "retrieval_results": [],
                "image_acceptance_decisions": [],
                "delivered_images": [{
                    "delivered_image_id": "delivered-1",
                    "query_plan_id": "qp-test-1",
                    "candidate_id": "cand-1",
                    "retrieval_job_id": "job-1",
                    "package_image_path": external.join("image.jpg").display().to_string(),
                    "local_artifact_path": external.join("image.jpg").display().to_string(),
                    "source_artifact_path": external.join("source.html").display().to_string(),
                    "source_sidecar_path": external.join("sidecar.json").display().to_string(),
                    "content_summary_path": external.join("summary.txt").display().to_string(),
                    "task_report_path": external.join("task-report.json").display().to_string(),
                    "visual_description_path": external.join("visual-description.txt").display().to_string(),
                    "checksum_sha256": "sha256-ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
                    "content_type": "image/jpeg",
                    "file_size_bytes": 9,
                    "width": 1,
                    "height": 1,
                    "candidate_quality_decision_ref": "candidate-quality-decision-cand-1",
                    "image_acceptance_decision_ref": "accepted",
                    "manifest_entry_ref": "manifest-0001"
                }]
            }),
        );

        let validator = PackageValidator::new();
        let request = PackageValidationRequest {
            package_dir: dir.clone(),
            execution_mode: crate::domain::delivery::ExecutionMode::Fixture,
            expected_query_plan_id: None,
        };
        let report = validator.validate(&request).unwrap();

        assert_eq!(report.status, ValidationStatus::Fail);
        assert!(report
            .issues
            .iter()
            .any(|i| i.code == codes::DELIVERED_IMAGE_ARTIFACT_MISSING
                && i.message.contains("outside package root")));

        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&external);
    }

    #[test]
    fn validator_rejects_reference_metric_paths_outside_package_root() {
        let dir = temp_dir("outside-reference-metric");
        make_minimal_package(&dir, "passed");
        let external = temp_dir("outside-reference-metric-source");
        fs::write(external.join("sidecar.json"), "not empty").unwrap();

        write_package_with_single_delivered_image(
            &dir,
            serde_json::json!({
                "retrieval_job_id": "job-1",
                "candidate_id": "cand-1",
                "query_plan_id": "qp-test-1",
                "channel_id": "fixture_channel",
                "channel_tier": "normal_web_fetch",
                "retrieval_status": "complete",
                "local_artifact_path": "images/image.jpg",
                "source_artifact_path": "evidence/source.html",
                "source_sidecar_path": "evidence/sidecar.json",
                "content_summary_path": "evidence/summary.txt",
                "task_report_path": "evidence/task-report.json",
                "visual_description_path": "evidence/visual-description.txt",
                "checksum_sha256": "sha256-ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
                "content_type": "image/jpeg",
                "file_size_bytes": 9,
                "image_dimensions": {"width": 1, "height": 1},
                "media_type_match": true,
                "fetch_trace": [{"url": "https://example.com/image.jpg", "status": "complete"}],
                "failure_reason": null
            }),
            serde_json::json!({
                "decision_id": "retrieved-images.json#/image_acceptance_decisions/0",
                "candidate_id": "cand-1",
                "retrieval_job_id": "job-1",
                "query_plan_id": "qp-test-1",
                "mechanical_passed": true,
                "vlm_passed": true,
                "artifact_complete": true,
                "final_status": "accepted",
                "blocking_reasons": [],
                "reference_metrics": [{
                    "kind": "source_sidecar_path",
                    "path": external.join("sidecar.json").display().to_string()
                }],
                "vlm_decision": {"decision": "approve", "provider_id": "fixture_vlm"}
            }),
        );

        let validator = PackageValidator::new();
        let request = PackageValidationRequest {
            package_dir: dir.clone(),
            execution_mode: crate::domain::delivery::ExecutionMode::Fixture,
            expected_query_plan_id: None,
        };
        let report = validator.validate(&request).unwrap();

        assert_eq!(report.status, ValidationStatus::Fail);
        assert!(report
            .issues
            .iter()
            .any(|i| i.code == codes::DELIVERED_IMAGE_ARTIFACT_MISSING
                && i.message.contains("reference metric path")));

        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&external);
    }

    #[test]
    fn validator_rejects_candidate_quality_reference_metric_paths_outside_package_root() {
        let dir = temp_dir("outside-candidate-quality-reference-metric");
        make_minimal_package(&dir, "passed");
        let external = temp_dir("outside-candidate-quality-reference-metric-source");
        fs::write(external.join("candidate-sidecar.json"), "not empty").unwrap();

        write_package_with_single_delivered_image(
            &dir,
            serde_json::json!({
                "retrieval_job_id": "job-1",
                "candidate_id": "cand-1",
                "query_plan_id": "qp-test-1",
                "channel_id": "fixture_channel",
                "channel_tier": "normal_web_fetch",
                "retrieval_status": "complete",
                "local_artifact_path": "images/image.jpg",
                "source_artifact_path": "evidence/source.html",
                "source_sidecar_path": "evidence/sidecar.json",
                "content_summary_path": "evidence/summary.txt",
                "task_report_path": "evidence/task-report.json",
                "visual_description_path": "evidence/visual-description.txt",
                "checksum_sha256": "sha256-ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
                "content_type": "image/jpeg",
                "file_size_bytes": 9,
                "image_dimensions": {"width": 1, "height": 1},
                "media_type_match": true,
                "fetch_trace": [{"url": "https://example.com/image.jpg", "status": "complete"}],
                "failure_reason": null
            }),
            serde_json::json!({
                "decision_id": "retrieved-images.json#/image_acceptance_decisions/0",
                "candidate_id": "cand-1",
                "retrieval_job_id": "job-1",
                "query_plan_id": "qp-test-1",
                "mechanical_passed": true,
                "vlm_passed": true,
                "artifact_complete": true,
                "final_status": "accepted",
                "blocking_reasons": [],
                "reference_metrics": [{"kind": "file_size", "value": 9}],
                "vlm_decision": {"decision": "approve", "provider_id": "fixture_vlm"}
            }),
        );
        write_json(
            &dir,
            "evidence/candidate-quality/cand-1.json",
            &serde_json::json!({
                "schema_version": 1,
                "candidate_id": "cand-1",
                "query_plan_id": "qp-test-1",
                "mechanical_passed": true,
                "vlm_passed": true,
                "final_status": "retrievable",
                "priority": 5,
                "blocking_metrics": [],
                "reference_metrics": [{
                    "kind": "source_sidecar_path",
                    "path": external.join("candidate-sidecar.json").display().to_string()
                }],
                "vlm_decision": {"decision": "approve", "provider_id": "fixture_vlm"},
                "redaction_applied": true
            }),
        );

        let validator = PackageValidator::new();
        let request = PackageValidationRequest {
            package_dir: dir.clone(),
            execution_mode: crate::domain::delivery::ExecutionMode::Fixture,
            expected_query_plan_id: None,
        };
        let report = validator.validate(&request).unwrap();

        assert_eq!(report.status, ValidationStatus::Fail);
        assert!(report
            .issues
            .iter()
            .any(|i| i.code == codes::DELIVERED_IMAGE_ARTIFACT_MISSING
                && i.message
                    .contains("candidate quality reference metric path")));

        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&external);
    }

    #[test]
    fn validator_rejects_delivered_image_without_decision_evidence() {
        let dir = temp_dir("missing-decision-evidence");
        make_minimal_package(&dir, "passed");

        for path in [
            "images/image.jpg",
            "evidence/source.html",
            "evidence/sidecar.json",
            "evidence/summary.txt",
            "evidence/task-report.json",
            "evidence/visual-description.txt",
        ] {
            fs::write(dir.join(path), "not empty").unwrap();
        }

        write_json(
            &dir,
            "retrieved-images.json",
            &serde_json::json!({
                "schema_version": 1,
                "run_id": "run-test-1",
                "query_plan_id": "qp-test-1",
                "retrieval_batch_target": 2,
                "retrieval_results": [],
                "image_acceptance_decisions": [],
                "delivered_images": [{
                    "delivered_image_id": "delivered-1",
                    "query_plan_id": "qp-test-1",
                    "candidate_id": "cand-1",
                    "retrieval_job_id": "job-1",
                    "package_image_path": "images/image.jpg",
                    "local_artifact_path": "images/image.jpg",
                    "source_artifact_path": "evidence/source.html",
                    "source_sidecar_path": "evidence/sidecar.json",
                    "content_summary_path": "evidence/summary.txt",
                    "task_report_path": "evidence/task-report.json",
                    "visual_description_path": "evidence/visual-description.txt",
                    "checksum_sha256": "sha256-ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
                    "content_type": "image/jpeg",
                    "file_size_bytes": 9,
                    "width": 1,
                    "height": 1,
                    "candidate_quality_decision_ref": "evidence/candidate-quality/cand-1.json",
                    "image_acceptance_decision_ref": "retrieved-images.json#/image_acceptance_decisions/0",
                    "manifest_entry_ref": "manifest-0001"
                }]
            }),
        );
        write_json(
            &dir,
            "retrieval-manifest.json",
            &serde_json::json!({
                "schema_version": 1,
                "run_id": "run-test-1",
                "query_plan_id": "qp-test-1",
                "entries": [{
                    "manifest_entry_id": "manifest-0001",
                    "candidate_id": "cand-1",
                    "retrieval_job_id": "job-1",
                    "retrieval_result_ref": "retrieved-images.json#/retrieval_results/0",
                    "image_acceptance_ref": "retrieved-images.json#/image_acceptance_decisions/0",
                    "candidate_quality_ref": "evidence/candidate-quality/cand-1.json",
                    "artifact_refs": [
                        "images/image.jpg",
                        "evidence/source.html",
                        "evidence/sidecar.json",
                        "evidence/summary.txt",
                        "evidence/task-report.json",
                        "evidence/visual-description.txt"
                    ]
                }]
            }),
        );

        let validator = PackageValidator::new();
        let request = PackageValidationRequest {
            package_dir: dir.clone(),
            execution_mode: crate::domain::delivery::ExecutionMode::Fixture,
            expected_query_plan_id: None,
        };
        let report = validator.validate(&request).unwrap();

        assert_eq!(report.status, ValidationStatus::Fail);
        assert!(report
            .issues
            .iter()
            .any(|i| i.code == codes::VLM_EVALUATION_DECISION_MISSING
                && i.message.contains("decision evidence")));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn validator_rejects_unknown_retrieval_channel_and_empty_fetch_trace() {
        let dir = temp_dir("unknown-retrieval-evidence");
        make_minimal_package(&dir, "passed");
        write_package_with_single_delivered_image(
            &dir,
            serde_json::json!({
                "retrieval_job_id": "job-1",
                "candidate_id": "cand-1",
                "query_plan_id": "qp-test-1",
                "channel_id": "unknown",
                "channel_tier": "unknown",
                "retrieval_status": "complete",
                "local_artifact_path": "images/image.jpg",
                "source_artifact_path": "evidence/source.html",
                "source_sidecar_path": "evidence/sidecar.json",
                "content_summary_path": "evidence/summary.txt",
                "task_report_path": "evidence/task-report.json",
                "visual_description_path": "evidence/visual-description.txt",
                "checksum_sha256": "sha256-ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
                "content_type": "image/jpeg",
                "file_size_bytes": 9,
                "image_dimensions": {"width": 1, "height": 1},
                "media_type_match": true,
                "fetch_trace": [],
                "failure_reason": null
            }),
            serde_json::json!({
                "decision_id": "retrieved-images.json#/image_acceptance_decisions/0",
                "candidate_id": "cand-1",
                "retrieval_job_id": "job-1",
                "query_plan_id": "qp-test-1",
                "mechanical_passed": true,
                "vlm_passed": true,
                "artifact_complete": true,
                "final_status": "accepted",
                "blocking_reasons": [],
                "reference_metrics": [{"kind": "file_size", "value": 9}],
                "vlm_decision": {"decision": "approve", "provider_id": "fixture_vlm"}
            }),
        );

        let validator = PackageValidator::new();
        let request = PackageValidationRequest {
            package_dir: dir.clone(),
            execution_mode: crate::domain::delivery::ExecutionMode::Fixture,
            expected_query_plan_id: None,
        };
        let report = validator.validate(&request).unwrap();

        assert_eq!(report.status, ValidationStatus::Fail);
        assert!(
            report
                .issues
                .iter()
                .any(|i| i.code == codes::RETRIEVAL_TRACE_MISSING
                    && i.message.contains("fetch_trace"))
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn validator_rejects_broken_manifest_search_ref() {
        let dir = temp_dir("broken-search-ref");
        make_minimal_package(&dir, "passed");
        write_package_with_single_delivered_image(
            &dir,
            serde_json::json!({
                "retrieval_job_id": "job-1",
                "candidate_id": "cand-1",
                "query_plan_id": "qp-test-1",
                "channel_id": "fixture_channel",
                "channel_tier": "normal_web_fetch",
                "retrieval_status": "complete",
                "local_artifact_path": "images/image.jpg",
                "source_artifact_path": "evidence/source.html",
                "source_sidecar_path": "evidence/sidecar.json",
                "content_summary_path": "evidence/summary.txt",
                "task_report_path": "evidence/task-report.json",
                "visual_description_path": "evidence/visual-description.txt",
                "checksum_sha256": "sha256-ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
                "content_type": "image/jpeg",
                "file_size_bytes": 9,
                "image_dimensions": {"width": 1, "height": 1},
                "media_type_match": true,
                "fetch_trace": [{"url": "https://example.com/image.jpg", "status": "complete"}],
                "failure_reason": null
            }),
            serde_json::json!({
                "decision_id": "retrieved-images.json#/image_acceptance_decisions/0",
                "candidate_id": "cand-1",
                "retrieval_job_id": "job-1",
                "query_plan_id": "qp-test-1",
                "mechanical_passed": true,
                "vlm_passed": true,
                "artifact_complete": true,
                "final_status": "accepted",
                "blocking_reasons": [],
                "reference_metrics": [{"kind": "file_size", "value": 9}],
                "vlm_decision": {"decision": "approve", "provider_id": "fixture_vlm"}
            }),
        );

        let mut manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(dir.join("retrieval-manifest.json")).unwrap())
                .unwrap();
        manifest["entries"][0]["search_ref"] =
            serde_json::Value::String("image-recalls.json#/attempts/0/candidates/999".into());
        write_json(&dir, "retrieval-manifest.json", &manifest);

        let report = PackageValidator::new()
            .validate(&PackageValidationRequest {
                package_dir: dir.clone(),
                execution_mode: crate::domain::delivery::ExecutionMode::Fixture,
                expected_query_plan_id: None,
            })
            .unwrap();

        assert_eq!(report.status, ValidationStatus::Fail);
        assert!(report.issues.iter().any(|issue| {
            issue.code == codes::MANIFEST_LINK_BROKEN
                && issue.message.contains("search_ref")
                && issue.message.contains("candidates/999")
        }));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn validator_rejects_synthetic_image_acceptance_without_vlm_decision() {
        let dir = temp_dir("synthetic-image-decision");
        make_minimal_package(&dir, "passed");
        write_package_with_single_delivered_image(
            &dir,
            serde_json::json!({
                "retrieval_job_id": "job-1",
                "candidate_id": "cand-1",
                "query_plan_id": "qp-test-1",
                "channel_id": "fixture_channel",
                "channel_tier": "normal_web_fetch",
                "retrieval_status": "complete",
                "local_artifact_path": "images/image.jpg",
                "source_artifact_path": "evidence/source.html",
                "source_sidecar_path": "evidence/sidecar.json",
                "content_summary_path": "evidence/summary.txt",
                "task_report_path": "evidence/task-report.json",
                "visual_description_path": "evidence/visual-description.txt",
                "checksum_sha256": "sha256-ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
                "content_type": "image/jpeg",
                "file_size_bytes": 9,
                "image_dimensions": {"width": 1, "height": 1},
                "media_type_match": true,
                "fetch_trace": [{"url": "https://example.com/image.jpg", "status": "complete"}],
                "failure_reason": null
            }),
            serde_json::json!({
                "decision_id": "retrieved-images.json#/image_acceptance_decisions/0",
                "candidate_id": "cand-1",
                "retrieval_job_id": "job-1",
                "query_plan_id": "qp-test-1",
                "mechanical_passed": true,
                "vlm_passed": true,
                "artifact_complete": true,
                "final_status": "accepted",
                "blocking_reasons": [],
                "reference_metrics": [],
                "vlm_decision": null
            }),
        );

        let validator = PackageValidator::new();
        let request = PackageValidationRequest {
            package_dir: dir.clone(),
            execution_mode: crate::domain::delivery::ExecutionMode::Fixture,
            expected_query_plan_id: None,
        };
        let report = validator.validate(&request).unwrap();

        assert_eq!(report.status, ValidationStatus::Fail);
        assert!(report
            .issues
            .iter()
            .any(|i| i.code == codes::VLM_EVALUATION_DECISION_MISSING
                && i.message.contains("vlm_decision")));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn validator_rejects_synthetic_candidate_quality_without_vlm_decision() {
        let dir = temp_dir("synthetic-candidate-decision");
        make_minimal_package(&dir, "passed");
        write_package_with_single_delivered_image(
            &dir,
            serde_json::json!({
                "retrieval_job_id": "job-1",
                "candidate_id": "cand-1",
                "query_plan_id": "qp-test-1",
                "channel_id": "fixture_channel",
                "channel_tier": "normal_web_fetch",
                "retrieval_status": "complete",
                "local_artifact_path": "images/image.jpg",
                "source_artifact_path": "evidence/source.html",
                "source_sidecar_path": "evidence/sidecar.json",
                "content_summary_path": "evidence/summary.txt",
                "task_report_path": "evidence/task-report.json",
                "visual_description_path": "evidence/visual-description.txt",
                "checksum_sha256": "sha256-ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
                "content_type": "image/jpeg",
                "file_size_bytes": 9,
                "image_dimensions": {"width": 1, "height": 1},
                "media_type_match": true,
                "fetch_trace": [{"url": "https://example.com/image.jpg", "status": "complete"}],
                "failure_reason": null
            }),
            serde_json::json!({
                "decision_id": "retrieved-images.json#/image_acceptance_decisions/0",
                "candidate_id": "cand-1",
                "retrieval_job_id": "job-1",
                "query_plan_id": "qp-test-1",
                "mechanical_passed": true,
                "vlm_passed": true,
                "artifact_complete": true,
                "final_status": "accepted",
                "blocking_reasons": [],
                "reference_metrics": [{"kind": "file_size", "value": 9}],
                "vlm_decision": {"decision": "approve", "provider_id": "fixture_vlm"}
            }),
        );
        write_json(
            &dir,
            "evidence/candidate-quality/cand-1.json",
            &serde_json::json!({
                "schema_version": 1,
                "candidate_id": "cand-1",
                "query_plan_id": "qp-test-1",
                "mechanical_passed": true,
                "vlm_passed": true,
                "final_status": "retrievable",
                "priority": 5,
                "blocking_metrics": [],
                "reference_metrics": [],
                "vlm_decision": null,
                "redaction_applied": true
            }),
        );

        let validator = PackageValidator::new();
        let request = PackageValidationRequest {
            package_dir: dir.clone(),
            execution_mode: crate::domain::delivery::ExecutionMode::Fixture,
            expected_query_plan_id: None,
        };
        let report = validator.validate(&request).unwrap();

        assert_eq!(report.status, ValidationStatus::Fail);
        assert!(report
            .issues
            .iter()
            .any(|i| i.code == codes::VLM_EVALUATION_DECISION_MISSING
                && i.message.contains("candidate quality")));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn validator_rejects_legacy_openclaw_vlm_evidence() {
        let dir = temp_dir("legacy-openclaw-vlm");
        make_minimal_package(&dir, "passed");
        write_package_with_single_delivered_image(
            &dir,
            serde_json::json!({
                "retrieval_job_id": "job-1",
                "candidate_id": "cand-1",
                "query_plan_id": "qp-test-1",
                "channel_id": "fixture_channel",
                "channel_tier": "normal_web_fetch",
                "retrieval_status": "complete",
                "local_artifact_path": "images/image.jpg",
                "source_artifact_path": "evidence/source.html",
                "source_sidecar_path": "evidence/sidecar.json",
                "content_summary_path": "evidence/summary.txt",
                "task_report_path": "evidence/task-report.json",
                "visual_description_path": "evidence/visual-description.txt",
                "checksum_sha256": "sha256-ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
                "content_type": "image/jpeg",
                "file_size_bytes": 9,
                "image_dimensions": {"width": 1, "height": 1},
                "media_type_match": true,
                "fetch_trace": [{"url": "https://example.com/image.jpg", "status": "complete"}],
                "failure_reason": null
            }),
            serde_json::json!({
                "decision_id": "retrieved-images.json#/image_acceptance_decisions/0",
                "candidate_id": "cand-1",
                "retrieval_job_id": "job-1",
                "query_plan_id": "qp-test-1",
                "mechanical_passed": true,
                "vlm_passed": true,
                "artifact_complete": true,
                "final_status": "accepted",
                "blocking_reasons": [],
                "reference_metrics": [{"kind": "file_size", "value": 9}],
                "vlm_decision": {"decision": "approve", "provider_id": "openclaw_legacy"}
            }),
        );
        write_json(
            &dir,
            "evidence/candidate-quality/cand-1.json",
            &serde_json::json!({
                "schema_version": 1,
                "candidate_id": "cand-1",
                "query_plan_id": "qp-test-1",
                "mechanical_passed": true,
                "vlm_passed": true,
                "final_status": "retrievable",
                "priority": 5,
                "blocking_metrics": [],
                "reference_metrics": [],
                "vlm_decision": {"decision": "approve", "provider_id": "openclaw_legacy"},
                "redaction_applied": true
            }),
        );

        let validator = PackageValidator::new();
        let request = PackageValidationRequest {
            package_dir: dir.clone(),
            execution_mode: crate::domain::delivery::ExecutionMode::Fixture,
            expected_query_plan_id: None,
        };
        let report = validator.validate(&request).unwrap();

        assert_eq!(report.status, ValidationStatus::Fail);
        assert!(report
            .issues
            .iter()
            .any(|i| i.code == codes::VLM_EVALUATION_DECISION_MISSING));

        let _ = fs::remove_dir_all(&dir);
    }
}
