//! v1.1 fixture validation tests — TASK-006.
//!
//! Tests that exercise the fixture directory at `tests/fixtures/v1_1/`,
//! covering:
//! - QueryPlan file loading and admission
//! - Config file loading and defaults
//! - Package validation with positive + negative fixture packages
//! - Secret scanning across package files
//! - Cross-package consistency checks
//!
//! All tests are local deterministic — no network, no credentials.

use image_retrieval::domain::config::{RuntimeConfig, SearchProviderKind};
use image_retrieval::domain::query_plan::{
    admit_query_plan, AdmissionConfig, AdmissionOutcome, QualityTier, QueryPlanInput,
};
use image_retrieval::policy::contains_sensitive_pattern;
use std::fs;
use std::path::{Path, PathBuf};

// =============================================================================
// Helpers
// =============================================================================

fn fixture_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("v1_1")
        .join(relative)
}

fn read_fixture_json<T: serde::de::DeserializeOwned>(relative: &str) -> T {
    let path = fixture_path(relative);
    let content = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read fixture {}: {}", path.display(), e));
    serde_json::from_str(&content)
        .unwrap_or_else(|e| panic!("failed to parse fixture {}: {}", path.display(), e))
}

fn assert_non_empty_str(value: &serde_json::Value, label: &str) {
    let text = value
        .as_str()
        .unwrap_or_else(|| panic!("{} must be a string", label));
    assert!(!text.trim().is_empty(), "{} must not be empty", label);
}

// =============================================================================
// QueryPlan fixture loading and admission
// =============================================================================

#[test]
fn fixture_query_plan_basic_loads_and_admits() {
    let input: QueryPlanInput = read_fixture_json("query-plans/query-plan-basic.json");
    assert_eq!(
        input.description,
        "sunset over mountain landscape with vibrant orange sky"
    );
    assert_eq!(input.required_image_count, 1);
    assert_eq!(input.quality, QualityTier::General);
    assert_eq!(input.query_texts.len(), 2);
    assert!(input.material_types.is_empty());
    assert_eq!(input.negative_scope, vec!["night", "black and white"]);
    assert_eq!(input.retry_limit, 3);

    let outcome = admit_query_plan(input, &AdmissionConfig::default());
    assert!(outcome.is_accepted());
    let plan = outcome.unwrap();
    assert_eq!(plan.required_image_count, 1);
    assert_eq!(plan.candidate_target, 20);
    assert_eq!(plan.retrieval_batch_target, 2);
}

#[test]
fn fixture_query_plan_high_quality_multi_loads_and_admits() {
    let input: QueryPlanInput = read_fixture_json("query-plans/query-plan-high-quality-multi.json");
    assert_eq!(input.quality, QualityTier::General);
    assert_eq!(input.required_image_count, 2);
    assert_eq!(input.source_diversity_requirement, None);
    assert_eq!(input.retry_limit, 3);

    let outcome = admit_query_plan(input, &AdmissionConfig::default());
    assert!(outcome.is_accepted());
    let plan = outcome.unwrap();
    assert_eq!(plan.candidate_target, 40);
    assert_eq!(plan.retrieval_batch_target, 4);
    assert_eq!(plan.full_attempt_limit, 4);
}

#[test]
fn fixture_query_plan_empty_description_rejected() {
    let input: QueryPlanInput =
        read_fixture_json("query-plans/query-plan-invalid-empty-description.json");
    assert!(input.description.is_empty());

    let outcome = admit_query_plan(input, &AdmissionConfig::default());
    assert!(!outcome.is_accepted());
    match outcome {
        AdmissionOutcome::Rejected { diagnostics } => {
            assert!(!diagnostics.is_empty());
        }
        _ => panic!("expected rejected"),
    }
}

// =============================================================================
// Config fixture loading
// =============================================================================

#[test]
fn fixture_config_fixture_toml_loads_providers_and_channels() {
    let path = fixture_path("configs/config-fixture.toml");
    let content = fs::read_to_string(&path).expect("read config");
    let config: RuntimeConfig = toml::from_str(&content).expect("parse config");

    assert_eq!(config.providers.len(), 1);
    assert_eq!(config.providers[0].provider_id, "fixture_search");
    assert_eq!(
        config.providers[0].provider_kind,
        SearchProviderKind::Fixture
    );
    assert!(config.providers[0].enabled);

    assert_eq!(config.retrieval_channels.len(), 1);
    assert_eq!(config.retrieval_channels[0].channel_id, "fixture_web_fetch");
    assert!(config.retrieval_channels[0].enabled);

    assert!(config.vlm_evaluation.enabled);
    assert!(config.vlm_evaluation.fixture_mode);
    assert!(!config.policy.allow_paid_channels);
    assert!(config.policy.respect_robots);
}

#[test]
fn fixture_config_minimal_toml_has_empty_providers() {
    let path = fixture_path("configs/config-minimal.toml");
    let content = fs::read_to_string(&path).expect("read config");
    let config: RuntimeConfig = toml::from_str(&content).expect("parse config");

    assert!(config.providers.is_empty());
    assert!(config.retrieval_channels.is_empty());
    assert!(!config.vlm_evaluation.enabled);
    assert!(!config.vlm_evaluation.fixture_mode);
}

#[test]
fn fixture_config_production_like_has_env_var_names_not_values() {
    let path = fixture_path("configs/config-production-like.toml");
    let content = fs::read_to_string(&path).expect("read config");
    let config: RuntimeConfig = toml::from_str(&content).expect("parse config");

    assert_eq!(config.providers.len(), 1);
    let provider = &config.providers[0];
    assert_eq!(provider.provider_id, "serpapi_google_images");
    assert_eq!(
        provider.provider_kind,
        SearchProviderKind::SerpapiGoogleImages
    );
    assert_eq!(provider.credential_env, Some("SERPAPI_API_KEY".into()));
    assert_eq!(provider.endpoint, Some("https://serpapi.com/search".into()));

    assert_eq!(config.vlm_evaluation.provider_id, "qwen_3_5_vlm");
    assert_eq!(config.vlm_evaluation.model, "qwen3-vl-plus");
    assert_eq!(
        config.vlm_evaluation.credential_env,
        Some("QWEN_API_KEY".into())
    );
    assert_eq!(
        config.vlm_evaluation.base_url,
        Some("https://dashscope.aliyuncs.com/compatible-mode/v1".into())
    );
    assert!(!config.vlm_evaluation.fixture_mode);

    // Verify no secret values in the config content (only env var names)
    let lower = content.to_lowercase();
    assert!(lower.contains("serpapi_api_key"));
    assert!(lower.contains("qwen_api_key"));
    // No resolved secrets
    assert!(!lower.contains("sk-"));
    assert!(!lower.contains("eyj"));
    assert!(!lower.contains("bearer "));
}

// =============================================================================
// Package validation — positive fixture
// =============================================================================

#[test]
fn fixture_package_passed_minimal_has_all_canonical_files() {
    let pkg_dir = fixture_path("packages/passed_minimal");

    let required_files = [
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

    for file in &required_files {
        let path = pkg_dir.join(file);
        assert!(path.exists(), "required file {} missing", file);
    }

    // Subdirectories exist
    assert!(pkg_dir.join("images").exists() || pkg_dir.join("images").is_dir());
    assert!(pkg_dir.join("evidence").exists() || pkg_dir.join("evidence").is_dir());
    assert!(pkg_dir.join("diagnostics").exists() || pkg_dir.join("diagnostics").is_dir());
}

#[test]
fn fixture_package_passed_minimal_all_json_valid() {
    let pkg_dir = fixture_path("packages/passed_minimal");

    for entry in fs::read_dir(&pkg_dir).expect("read dir") {
        let entry = entry.expect("entry");
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "json") {
            let content = fs::read_to_string(&path).expect("read file");
            let _: serde_json::Value = serde_json::from_str(&content)
                .unwrap_or_else(|_| panic!("invalid JSON: {}", path.display()));
        }
    }
}

#[test]
fn fixture_package_passed_minimal_preserves_image_reference_metrics() {
    let retrieved: serde_json::Value =
        read_fixture_json("packages/passed_minimal/retrieved-images.json");
    let delivery: serde_json::Value =
        read_fixture_json("packages/passed_minimal/delivery-report.json");

    let image_metrics = retrieved["image_acceptance_decisions"][0]["reference_metrics"]
        .as_array()
        .expect("image acceptance reference metrics");
    let delivery_metrics = delivery["items"][0]["reference_metrics"]
        .as_array()
        .expect("delivery reference metrics");

    for metrics in [image_metrics, delivery_metrics] {
        assert!(metrics
            .iter()
            .any(|metric| metric["kind"] == "retrieval_channel"));
        assert!(metrics
            .iter()
            .any(|metric| metric["kind"] == "source_sidecar_path"));
        assert!(metrics
            .iter()
            .any(|metric| metric["kind"] == "mechanical_reference"));
    }
}

#[test]
fn fixture_package_passed_minimal_preserves_vlm_rationales() {
    let retrieved: serde_json::Value =
        read_fixture_json("packages/passed_minimal/retrieved-images.json");
    let candidate_quality: serde_json::Value =
        read_fixture_json("packages/passed_minimal/evidence/candidate-quality/cand-001.json");

    let image_vlm = &retrieved["image_acceptance_decisions"][0]["vlm_decision"];
    assert_non_empty_str(&image_vlm["rationale_summary"], "image VLM rationale");
    assert_non_empty_str(&image_vlm["raw_verdict"], "image VLM raw verdict");

    let candidate_vlm = &candidate_quality["vlm_decision"];
    assert_non_empty_str(
        &candidate_vlm["rationale_summary"],
        "candidate VLM rationale",
    );
    assert_non_empty_str(&candidate_vlm["raw_verdict"], "candidate VLM raw verdict");
}

#[test]
fn fixture_package_passed_minimal_manifest_links_are_consistent() {
    let pkg_dir = fixture_path("packages/passed_minimal");

    let manifest_path = pkg_dir.join("retrieval-manifest.json");
    let manifest_content = fs::read_to_string(&manifest_path).expect("read manifest");
    let manifest: serde_json::Value =
        serde_json::from_str(&manifest_content).expect("parse manifest");

    let image_recalls_path = pkg_dir.join("image-recalls.json");
    let image_recalls_content = fs::read_to_string(&image_recalls_path).expect("read recalls");
    let image_recalls: serde_json::Value =
        serde_json::from_str(&image_recalls_content).expect("parse recalls");

    // Query plan IDs match across files
    assert_eq!(
        manifest["query_plan_id"].as_str().unwrap(),
        image_recalls["query_plan_id"].as_str().unwrap()
    );

    // Delivery status matches
    assert_eq!(
        manifest["query_plan_id"].as_str().unwrap(),
        "qp-fixture-passed-001"
    );

    // Candidate IDs in manifest match those in image-recalls
    let manifest_candidates = manifest["candidates"].as_object().unwrap();
    if let Some(accepted_images) = image_recalls["accepted_images"].as_array() {
        for img in accepted_images {
            let cid = img["candidate_id"].as_str().unwrap();
            assert!(
                manifest_candidates.contains_key(cid),
                "candidate {} in image-recalls not found in manifest",
                cid
            );
        }
    }
}

#[test]
fn fixture_package_passed_minimal_coverage_counts_consistent() {
    let coverage: serde_json::Value =
        read_fixture_json("packages/passed_minimal/coverage-report.json");
    let summary: serde_json::Value =
        read_fixture_json("packages/passed_minimal/package-summary.json");

    // Package summary and coverage report agree
    let cov_accepted = coverage["accepted_count"].as_u64().unwrap();
    let sum_accepted = summary["accepted_count"].as_u64().unwrap();
    assert_eq!(cov_accepted, sum_accepted);

    // Coverage gap is zero for passed packages
    assert_eq!(coverage["coverage_gap"].as_u64().unwrap(), 0);
}

// =============================================================================
// Package validation — negative fixtures
// =============================================================================

#[test]
fn fixture_package_missing_canonical_file_has_issue() {
    let validation: serde_json::Value =
        read_fixture_json("packages/missing-canonical-file/validation.json");

    assert!(!validation["validation_passed"].as_bool().unwrap());
    let issues = validation["issues"].as_array().unwrap();
    assert!(!issues.is_empty());

    let first_issue = &issues[0];
    assert_eq!(
        first_issue["code"].as_str().unwrap(),
        "PKG_REQUIRED_FILE_MISSING"
    );
}

#[test]
fn fixture_package_invalid_json_is_invalid() {
    let path = fixture_path("packages/invalid-json/image-recalls.json");
    let content = fs::read_to_string(&path).expect("read file");

    let result: Result<serde_json::Value, _> = serde_json::from_str(&content);
    assert!(result.is_err(), "expected invalid JSON to fail parsing");
}

#[test]
fn fixture_package_metadata_only_delivered_detected() {
    let validation: serde_json::Value =
        read_fixture_json("packages/metadata-only-delivered/validation.json");

    assert!(!validation["validation_passed"].as_bool().unwrap());
    let issues = validation["issues"].as_array().unwrap();
    assert!(issues
        .iter()
        .any(|i| { i["code"].as_str().unwrap() == "PKG_DELIVERED_IMAGE_METADATA_ONLY" }));

    // Verify the delivered image indeed has no local artifact path
    let delivered: serde_json::Value =
        read_fixture_json("packages/metadata-only-delivered/retrieved-images.json");
    let images = delivered["delivered_images"].as_array().unwrap();
    let first = &images[0];
    assert!(first["local_artifact_path"].is_null());
}

#[test]
fn fixture_package_checksum_missing_detected() {
    let validation: serde_json::Value =
        read_fixture_json("packages/checksum-missing/validation.json");

    assert!(!validation["validation_passed"].as_bool().unwrap());
    let issues = validation["issues"].as_array().unwrap();
    assert!(issues
        .iter()
        .any(|i| { i["code"].as_str().unwrap() == "PKG_CHECKSUM_MISSING" }));
}

#[test]
fn fixture_package_coverage_count_mismatch_detected() {
    let validation: serde_json::Value =
        read_fixture_json("packages/coverage-count-mismatch/validation.json");

    assert!(!validation["validation_passed"].as_bool().unwrap());
    let issues = validation["issues"].as_array().unwrap();
    assert!(issues
        .iter()
        .any(|i| { i["code"].as_str().unwrap() == "PKG_COVERAGE_COUNT_MISMATCH" }));
}

#[test]
fn fixture_package_retry_counter_invalid_detected() {
    let validation: serde_json::Value =
        read_fixture_json("packages/retry-counter-invalid/validation.json");

    assert!(!validation["validation_passed"].as_bool().unwrap());
    let issues = validation["issues"].as_array().unwrap();
    assert!(issues
        .iter()
        .any(|i| { i["code"].as_str().unwrap() == "PKG_RETRY_COUNTER_INVALID" }));

    // Verify the counter violation
    let manifest: serde_json::Value =
        read_fixture_json("packages/retry-counter-invalid/retrieval-manifest.json");
    let fac = manifest["full_attempt_count"].as_u64().unwrap();
    let rc = manifest["retry_count"].as_u64().unwrap();
    assert!(
        rc != fac - 1,
        "retry_count should not equal full_attempt_count - 1"
    );
}

#[test]
fn fixture_package_broken_manifest_link_detected() {
    let validation: serde_json::Value =
        read_fixture_json("packages/broken-manifest-link/validation.json");

    assert!(!validation["validation_passed"].as_bool().unwrap());
    let issues = validation["issues"].as_array().unwrap();
    assert!(issues
        .iter()
        .any(|i| { i["code"].as_str().unwrap() == "PKG_MANIFEST_LINK_BROKEN" }));
}

#[test]
fn fixture_package_secret_leak_detected() {
    let validation: serde_json::Value = read_fixture_json("packages/secret-leak/validation.json");

    assert!(!validation["validation_passed"].as_bool().unwrap());
    let issues = validation["issues"].as_array().unwrap();
    assert!(issues
        .iter()
        .any(|i| { i["code"].as_str().unwrap() == "PKG_SECRET_LEAK" }));
}

// =============================================================================
// Secret scanning across packages
// =============================================================================

#[test]
fn no_fixture_package_file_contains_real_credentials() {
    // Scan all JSON files in all fixtures for credential-like patterns
    let v1_1_dir = fixture_path("");

    let sensitive_patterns = [
        "Bearer ",
        "Authorization:",
        "x-api-key:",
        "api_key=",
        "access_token=",
        "client_secret=",
        "-----BEGIN RSA PRIVATE KEY-----",
        "-----BEGIN EC PRIVATE KEY-----",
        "password=",
    ];

    let mut files_scanned = 0;
    scan_dir_for_sensitive(&v1_1_dir, &sensitive_patterns, &mut files_scanned);
    assert!(files_scanned > 0, "should have scanned some fixture files");
}

fn scan_dir_for_sensitive(dir: &Path, patterns: &[&str], count: &mut usize) {
    for entry in fs::read_dir(dir).expect("read dir") {
        let entry = entry.expect("entry");
        let path = entry.path();

        if path.is_dir() {
            scan_dir_for_sensitive(&path, patterns, count);
        } else if path.extension().is_some_and(|e| e == "json" || e == "toml") {
            *count += 1;
            let content = fs::read_to_string(&path).expect("read file");

            for pattern in patterns {
                if content.contains(pattern) {
                    // Some fixture files intentionally contain secrets for testing
                    if path.to_string_lossy().contains("secret-leak") {
                        continue; // expected in secret-leak fixtures
                    }
                    panic!(
                        "fixture file {} contains sensitive pattern '{}'",
                        path.display(),
                        pattern
                    );
                }
            }
        }
    }
}

#[test]
fn secret_leak_fixture_contains_seeded_secret() {
    // Verify the secret-leak fixture actually contains the seeded secret
    let content = fs::read_to_string(fixture_path("packages/secret-leak/image-recalls.json"))
        .expect("read file");
    assert!(
        content.contains("Bearer fake-secret-token-for-testing-purposes-leak-detection"),
        "secret-leak fixture should contain the seeded secret for negative testing"
    );
    assert!(
        contains_sensitive_pattern(&content),
        "sensitive pattern function should detect the seeded secret"
    );
}

// =============================================================================
// Golden output consistency
// =============================================================================

#[test]
fn golden_self_check_fixture_ready_has_expected_fields() {
    let golden: serde_json::Value = read_fixture_json("golden/self-check-fixture-ready.json");

    assert_eq!(golden["status"].as_str().unwrap(), "ready");
    assert!(golden["blocks"].as_array().unwrap().is_empty());
    assert_eq!(golden["execution_mode"].as_str().unwrap(), "fixture");
    assert!(golden["vlm_readiness"]["fixture_mode"].as_bool().unwrap());
}

#[test]
fn golden_self_check_blocked_no_providers_has_blockers() {
    let golden: serde_json::Value =
        read_fixture_json("golden/self-check-blocked-no-providers.json");

    assert_eq!(golden["status"].as_str().unwrap(), "blocked");
    assert!(!golden["blockers"].as_array().unwrap().is_empty());
    assert_eq!(golden["provider_summary"]["ready"].as_u64().unwrap(), 0);
}

#[test]
fn golden_validate_package_passed_minimal_matches_actual() {
    let golden: serde_json::Value =
        read_fixture_json("golden/validate-package-passed-minimal.json");
    let actual: serde_json::Value = read_fixture_json("packages/passed_minimal/validation.json");

    // Golden and actual fixture should agree on key fields
    assert_eq!(
        golden["validation_passed"].as_bool().unwrap(),
        actual["validation_passed"].as_bool().unwrap()
    );
    assert_eq!(
        golden["query_plan_id"].as_str().unwrap(),
        actual["query_plan_id"].as_str().unwrap()
    );
}

// =============================================================================
// SerpApi fixture loading
// =============================================================================

#[test]
fn fixture_serpapi_response_loads_and_has_image_results() {
    let content = fs::read_to_string(fixture_path(
        "provider-responses/serpapi-google-images-success.json",
    ))
    .expect("read fixture");

    let response: serde_json::Value = serde_json::from_str(&content).expect("parse JSON");

    assert_eq!(
        response["search_metadata"]["status"].as_str().unwrap(),
        "Success"
    );
    assert_eq!(
        response["search_parameters"]["engine"].as_str().unwrap(),
        "google_images"
    );

    let results = response["image_results"].as_array().unwrap();
    assert_eq!(results.len(), 3);

    // First result has all required fields
    let first = &results[0];
    assert_eq!(first["position"].as_u64().unwrap(), 1);
    assert!(!first["original"].as_str().unwrap().is_empty());
    assert!(!first["link"].as_str().unwrap().is_empty());
    assert!(first["original_width"].as_u64().unwrap() > 0);
    assert!(first["original_height"].as_u64().unwrap() > 0);

    // License field present when available
    assert!(first["license"].as_str().is_some());
}

// =============================================================================
// Cross-fixture consistency
// =============================================================================

#[test]
fn all_negative_fixture_packages_have_validation_json() {
    let packages_dir = fixture_path("packages");
    let expected_negatives = [
        "missing-canonical-file",
        "metadata-only-delivered",
        "checksum-missing",
        "coverage-count-mismatch",
        "retry-counter-invalid",
        "broken-manifest-link",
        "secret-leak",
    ];

    for name in &expected_negatives {
        let validation_path = packages_dir.join(name).join("validation.json");
        assert!(
            validation_path.exists(),
            "negative fixture {} should have validation.json",
            name
        );
    }
}

#[test]
fn no_fixture_marked_as_production_acceptance() {
    // All fixture packages with validation.json should have fixture_evidence: true
    // or not claim production acceptance
    let packages_dir = fixture_path("packages");

    for entry in fs::read_dir(&packages_dir).expect("read dir") {
        let entry = entry.expect("entry");
        let pkg_path = entry.path();
        if !pkg_path.is_dir() {
            continue;
        }

        let validation_path = pkg_path.join("validation.json");
        if validation_path.exists() {
            let content = fs::read_to_string(&validation_path).unwrap_or_default();
            let lower = content.to_lowercase();
            // Should not claim production acceptance
            assert!(
                !lower.contains("\"execution_mode\": \"production\"")
                    || lower.contains("\"validation_passed\": false"),
                "fixture package {} should not claim production acceptance",
                pkg_path.display()
            );
        }
    }
}

// =============================================================================
// ProviderResponse normalization contract
// =============================================================================

#[test]
fn serpapi_image_results_preserve_required_fields() {
    use image_retrieval::search::SerpApiGoogleImagesAdapter;

    let content = fs::read_to_string(fixture_path(
        "provider-responses/serpapi-google-images-success.json",
    ))
    .expect("read fixture");

    let adapter = SerpApiGoogleImagesAdapter::fixture();
    let raw_results = adapter
        .parse_image_results(&content)
        .expect("parse results");
    assert_eq!(raw_results.len(), 3);

    // Normalize each
    let candidates: Vec<_> = raw_results
        .iter()
        .filter_map(|raw| {
            adapter.normalize_image_result(
                raw,
                "qp-test",
                "sr-test",
                1,
                1,
                "sunset mountain landscape",
            )
        })
        .collect();

    assert_eq!(candidates.len(), 3);

    for c in &candidates {
        // Required fields
        assert!(
            !c.candidate_id.0.is_empty(),
            "candidate_id must not be empty"
        );
        assert!(!c.image_url.is_empty(), "image_url must not be empty");
        assert!(!c.dedupe_key.is_empty(), "dedupe_key must not be empty");
        assert!(
            !c.origin_candidate_ids.is_empty(),
            "origin_candidate_ids must not be empty"
        );

        // Provider provenance
        assert_eq!(c.provider_id.to_string(), "serpapi_google_images");
        assert_eq!(c.search_round, 1);
        assert!(c.provider_rank >= 1);

        // No secrets in output
        let json = serde_json::to_string(c).unwrap_or_default();
        assert!(!json.contains("api_key"));
        assert!(!json.contains("token"));
        assert!(!json.contains("secret"));
    }
}

// =============================================================================
// Config readiness contract
// =============================================================================

#[test]
fn fixture_config_providers_are_explicitly_fixture() {
    let path = fixture_path("configs/config-fixture.toml");
    let content = fs::read_to_string(&path).expect("read config");
    let config: RuntimeConfig = toml::from_str(&content).expect("parse config");

    for provider in &config.providers {
        assert_eq!(
            provider.provider_kind,
            SearchProviderKind::Fixture,
            "fixture config providers must be fixture kind"
        );
    }
}

#[test]
fn production_like_config_has_no_fixture_providers() {
    let path = fixture_path("configs/config-production-like.toml");
    let content = fs::read_to_string(&path).expect("read config");
    let config: RuntimeConfig = toml::from_str(&content).expect("parse config");

    for provider in &config.providers {
        assert_ne!(
            provider.provider_kind,
            SearchProviderKind::Fixture,
            "production-like config should not have fixture providers"
        );
    }

    assert!(
        !config.vlm_evaluation.fixture_mode,
        "production-like config should not use fixture VLM"
    );
}

// =============================================================================
// Retrieved images and coverage consistency
// =============================================================================

#[test]
fn fixture_package_delivered_images_match_image_recalls() {
    let recalls: serde_json::Value =
        read_fixture_json("packages/passed_minimal/image-recalls.json");
    let retrieved: serde_json::Value =
        read_fixture_json("packages/passed_minimal/retrieved-images.json");

    let accepted = recalls["accepted_images"].as_array().unwrap();
    let delivered = retrieved["delivered_images"].as_array().unwrap();

    // Every accepted image that has all evidence should also be in delivered
    for img in accepted {
        let cid = img["candidate_id"].as_str().unwrap();
        let has_artifact = img
            .get("local_artifact_path")
            .and_then(|v| v.as_str())
            .is_some();
        let has_checksum = img
            .get("checksum_sha256")
            .and_then(|v| v.as_str())
            .is_some();

        if has_artifact && has_checksum {
            let found = delivered
                .iter()
                .any(|d| d["candidate_id"].as_str().unwrap() == cid);
            assert!(
                found,
                "accepted image {} should be in delivered_images",
                cid
            );
        }
    }
}
