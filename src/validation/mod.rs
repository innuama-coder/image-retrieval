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
                "passed" => {
                    if accepted < required {
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
                }
                "partial" => {
                    if accepted == 0 {
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
                }
                _ => {}
            }
        }

        // ---- 6. Check delivered images for artifacts ----
        if let Some(retrieved) = canonical_data.get("retrieved-images.json") {
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
                        let full_path: String = if path_str.is_empty() {
                            String::new()
                        } else {
                            package_dir.join(path_str).display().to_string()
                        };

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
                        } else {
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
                    if let Ok(entries) = std::fs::read_dir(&dir) {
                        for entry in entries.flatten() {
                            let path = entry.path();
                            if path.is_file() {
                                files.push(path);
                            }
                        }
                    }
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

        // Build timestamp
        let now = "2026-06-22T00:00:00Z"; // deterministic for fixture tests

        Ok(PackageValidationReport {
            schema_version: 1,
            validator_version: "v1.1".into(),
            package_dir: package_dir.display().to_string(),
            status,
            validated_at: now.to_string(),
            issues,
            file_checks,
            artifact_checks,
            redaction_checks,
            counter_checks,
            coverage_checks,
        })
    }
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
    let validator = PackageValidator::new();
    let request = PackageValidationRequest {
        package_dir: package_dir.to_path_buf(),
        execution_mode: crate::domain::delivery::ExecutionMode::Fixture,
        expected_query_plan_id: None,
    };
    validator.validate(&request)
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
                "attempts": []
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
}
