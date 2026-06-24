#![allow(
    clippy::unnecessary_cast,
    clippy::redundant_closure,
    clippy::useless_format
)]
//! SerpApi Google Images search provider adapter.
//!
//! Implements [`BaseSearchProvider`] using the SerpApi Google Images
//! endpoint (`https://serpapi.com/search`, `engine=google_images`).
//!
//! # Credential handling
//!
//! The adapter reads the API key from the environment variable named by
//! `SearchProviderConfig.credential_env` (default: `SERPAPI_API_KEY`).
//! The resolved key is held in private adapter state and never appears in
//! DTOs, diagnostics, logs, or package files.
//!
//! # Normalization
//!
//! Maps SerpApi `image_results[]` entries into [`CandidateRecord`] values
//! with provider rank, image URL, source page URL, thumbnail URL,
//! dimensions, MIME/license hints, and provenance.
//!
//! References:
//! - `docs/design/v1.1-TASK-002-search-provider-candidate-design.md`
//! - SerpApi Google Images Results API documentation

use crate::domain::candidate::{CandidateProvenance, CandidateRecord, LicenseEvidence, ProviderId};
use crate::domain::config::SearchProviderKind;
use crate::domain::search::{
    CredentialStatus, HealthCheckStatus, ProviderConstraintSupport, ProviderEvidence,
    ProviderFailureCode, ProviderRawImageResult, ProviderReadinessReport, ProviderReadinessStatus,
    QuotaStatus, SearchDiagnostic, SearchDiagnosticCode, SearchError, SearchRequest,
    SearchResponse, SearchResponseStatus,
};
use std::collections::BTreeMap;

// ---------------------------------------------------------------------------
// SerpApi Google Images adapter
// ---------------------------------------------------------------------------

/// The SerpApi Google Images search provider adapter.
///
/// Holds a resolved API key that is never exposed outside the adapter.
pub struct SerpApiGoogleImagesAdapter {
    /// Adapter identity.
    provider_id: ProviderId,
    display_name: String,
    /// Resolved API key — PRIVATE, never serialized.
    api_key: Option<String>,
    /// Base endpoint URL.
    endpoint: String,
    /// Whether this adapter is in production mode.
    production: bool,
}

impl SerpApiGoogleImagesAdapter {
    /// Create a new adapter.
    ///
    /// `credential_env` names the environment variable holding the API key.
    /// The resolved value is stored privately and never exposed.
    pub fn new(
        provider_id: impl Into<String>,
        display_name: impl Into<String>,
        credential_env: Option<&str>,
        endpoint_override: Option<&str>,
        production: bool,
    ) -> Self {
        let api_key = credential_env.and_then(|env_var| std::env::var(env_var).ok());

        let endpoint = endpoint_override
            .unwrap_or("https://serpapi.com/search")
            .to_string();

        Self {
            provider_id: ProviderId::new(provider_id.into()),
            display_name: display_name.into(),
            api_key,
            endpoint,
            production,
        }
    }

    /// Create from a [`crate::domain::config::SearchProviderConfig`].
    pub fn from_config(
        config: &crate::domain::config::SearchProviderConfig,
        production: bool,
    ) -> Self {
        Self::new(
            &config.provider_id,
            &config.provider_id,
            config.credential_env.as_deref(),
            config.endpoint.as_deref(),
            production,
        )
    }

    /// Default v1.1 production adapter using `SERPAPI_API_KEY`.
    pub fn default_production() -> Self {
        Self::new(
            "serpapi_google_images",
            "SerpApi Google Images",
            Some("SERPAPI_API_KEY"),
            Some("https://serpapi.com/search"),
            true,
        )
    }

    /// Fixture adapter for testing (no real HTTP calls).
    pub fn fixture() -> Self {
        Self::new(
            "serpapi_google_images",
            "SerpApi Google Images (fixture)",
            None,
            Some("https://serpapi.com/search"),
            false,
        )
    }

    /// Returns true if the API key is available.
    pub fn has_credential(&self) -> bool {
        self.api_key.is_some()
    }

    /// Normalize a SerpApi `image_results[]` entry into a [`CandidateRecord`].
    ///
    /// This is public so integration tests can validate normalization
    /// independently of HTTP calls.
    pub fn normalize_image_result(
        &self,
        raw: &ProviderRawImageResult,
        query_plan_id: &str,
        search_request_id: &str,
        search_round: u32,
        full_attempt_count: u8,
        query_text: &str,
    ) -> Option<CandidateRecord> {
        let image_url = raw.image_url.as_deref()?;
        if image_url.is_empty() {
            return None;
        }

        let candidate_id = CandidateRecord::build_candidate_id(
            query_plan_id,
            &self.provider_id,
            search_round,
            raw.provider_rank,
            image_url,
        );

        let dedupe_key = CandidateRecord::build_dedupe_key(image_url);

        let mut normalization_warnings = Vec::new();
        if raw.width.is_none() && raw.height.is_none() {
            normalization_warnings.push("dimensions not reported by provider".to_string());
        }
        if raw.mime_type.is_none() {
            normalization_warnings.push("MIME type not reported by provider".to_string());
        }
        if raw.license_hint.is_none() {
            // This is info-level, not a failure — many providers don't report licenses
        }

        let license_evidence = match raw.license_hint.as_deref() {
            Some(label) if !label.is_empty() => LicenseEvidence::Hinted {
                label: label.to_string(),
            },
            _ => LicenseEvidence::Unknown,
        };

        let source_authority_hint = raw
            .source_page_url
            .as_deref()
            .and_then(|url| extract_domain_hint(url));

        let provenance = CandidateProvenance {
            provider_raw_id: raw.provider_raw_id.clone(),
            provider_result_url: None,
            provider_rank: raw.provider_rank,
            search_query: query_text.to_string(),
            search_round,
            full_attempt_count,
            retrieved_at: chrono_now_iso(),
            provider_evidence_refs: Vec::new(),
            license_evidence,
            source_authority_hint,
        };

        Some(CandidateRecord {
            candidate_id: candidate_id.clone(),
            query_plan_id: query_plan_id.to_string(),
            provider_id: self.provider_id.clone(),
            provider_kind: "serpapi_google_images".to_string(),
            search_request_id: search_request_id.to_string(),
            search_round,
            provider_rank: raw.provider_rank,
            global_rank_hint: None,
            image_url: image_url.to_string(),
            source_page_url: raw.source_page_url.clone(),
            thumbnail_url: raw.thumbnail_url.clone(),
            title: raw.title.clone(),
            snippet: raw.snippet.clone(),
            width: raw.width,
            height: raw.height,
            mime_type: raw.mime_type.clone(),
            license_hint: raw.license_hint.clone(),
            attribution: raw.attribution.clone(),
            dedupe_key,
            origin_candidate_ids: vec![candidate_id],
            provenance,
            normalization_warnings,
        })
    }

    /// Parse SerpApi JSON response into raw image results.
    pub fn parse_image_results(
        &self,
        body: &str,
    ) -> Result<Vec<ProviderRawImageResult>, SearchError> {
        let parsed: serde_json::Value =
            serde_json::from_str(body).map_err(|e| SearchError::parse(e.to_string()))?;

        // Check for SerpApi error
        if let Some(error_msg) = parsed.get("error").and_then(|v| v.as_str()) {
            return Err(SearchError::unavailable(error_msg));
        }

        // Extract search_metadata for diagnostics
        let _metadata = parsed.get("search_metadata");
        let _search_params = parsed.get("search_parameters");

        // Extract the image results array. The live SerpApi Google Images API
        // returns `images_results` (plural); older fixtures use `image_results`.
        // Accept either so the adapter works against the real service and tests.
        let image_results = match parsed
            .get("images_results")
            .or_else(|| parsed.get("image_results"))
        {
            Some(serde_json::Value::Array(arr)) => arr,
            Some(_) => {
                return Err(SearchError::parse("images_results is not an array"));
            }
            None => {
                // No results field present — treat as an empty search.
                return Ok(Vec::new());
            }
        };

        let mut results = Vec::with_capacity(image_results.len());

        for (index, item) in image_results.iter().enumerate() {
            let rank = index as u32 + 1; // 1-based rank

            let raw = ProviderRawImageResult {
                provider_raw_id: item
                    .get("position")
                    .and_then(|v| v.as_u64())
                    .map(|p| p.to_string()),
                provider_rank: rank,
                image_url: item
                    .get("original")
                    .or_else(|| item.get("thumbnail"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                source_page_url: item
                    .get("link")
                    .or_else(|| item.get("source"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                thumbnail_url: item
                    .get("thumbnail")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                title: item
                    .get("title")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                snippet: item
                    .get("snippet")
                    .or_else(|| item.get("alt"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                width: item
                    .get("original_width")
                    .or_else(|| item.get("image_width"))
                    .and_then(|v| v.as_u64())
                    .map(|w| w as u32),
                height: item
                    .get("original_height")
                    .or_else(|| item.get("image_height"))
                    .and_then(|v| v.as_u64())
                    .map(|h| h as u32),
                mime_type: None, // SerpApi doesn't provide MIME type directly
                license_hint: item
                    .get("license")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                attribution: item
                    .get("source")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                provider_extra_safe: BTreeMap::new(),
            };

            results.push(raw);
        }

        Ok(results)
    }
}

impl crate::ports::BaseSearchProvider for SerpApiGoogleImagesAdapter {
    fn provider_id(&self) -> ProviderId {
        self.provider_id.clone()
    }

    fn display_name(&self) -> &str {
        &self.display_name
    }

    fn provider_kind(&self) -> SearchProviderKind {
        SearchProviderKind::SerpapiGoogleImages
    }

    fn supported_constraints(&self) -> ProviderConstraintSupport {
        ProviderConstraintSupport {
            max_results_per_request: Some(100),
            supported_content_types: vec![
                "image/jpeg".into(),
                "image/png".into(),
                "image/gif".into(),
                "image/webp".into(),
                "image/svg+xml".into(),
            ],
            supports_quality_filter: false,
            supports_license_filter: true,
            supports_dimension_filter: true,
        }
    }

    fn readiness(
        &self,
        config: &crate::domain::config::SearchProviderConfig,
    ) -> ProviderReadinessReport {
        let mut evidence = Vec::new();
        let mut status = ProviderReadinessStatus::Ready;
        let mut available = true;
        let mut failure_code: Option<ProviderFailureCode> = None;
        let credential_status;

        // Check credential
        if let Some(env_var) = &config.credential_env {
            if self.api_key.is_some() {
                credential_status = CredentialStatus::Present;
            } else {
                credential_status = CredentialStatus::Missing {
                    env_var: env_var.clone(),
                };
                status = ProviderReadinessStatus::MissingCredentials;
                available = false;
                failure_code = Some(ProviderFailureCode::ProviderCredentialMissing);
                evidence.push(ProviderEvidence {
                    code: "PROVIDER_CREDENTIAL_MISSING".into(),
                    message: format!(
                        "Environment variable '{}' is not set. SerpApi requires an API key.",
                        env_var
                    ),
                    severity: "blocker".into(),
                });
            }
        } else {
            credential_status = CredentialStatus::Missing {
                env_var: "SERPAPI_API_KEY".into(),
            };
            status = ProviderReadinessStatus::MissingCredentials;
            available = false;
            failure_code = Some(ProviderFailureCode::ProviderCredentialMissing);
            evidence.push(ProviderEvidence {
                code: "PROVIDER_CREDENTIAL_MISSING".into(),
                message: "No credential_env configured for SerpApi. Set SERPAPI_API_KEY or configure a credential_env.".into(),
                severity: "blocker".into(),
            });
        }

        // Check endpoint
        if config.endpoint.is_some() || !self.endpoint.is_empty() {
            // Endpoint is parseable
        }

        // Check production vs fixture
        if !self.production {
            status = ProviderReadinessStatus::FixtureOnly;
            available = false;
            failure_code = Some(ProviderFailureCode::ProviderFixtureNotProduction);
            evidence.push(ProviderEvidence {
                code: "PROVIDER_FIXTURE_NOT_PRODUCTION".into(),
                message: "SerpApi adapter is in fixture/non-production mode.".into(),
                severity: "info".into(),
            });
        }

        let effective_weight = if available && config.weight > 0 {
            Some(config.weight)
        } else {
            None
        };

        ProviderReadinessReport {
            provider_id: self.provider_id.clone(),
            provider_kind: SearchProviderKind::SerpapiGoogleImages,
            display_name: self.display_name.clone(),
            status,
            available,
            included_in_weight_table: available && config.enabled && config.weight > 0,
            configured_weight: config.weight,
            effective_weight,
            credential_status,
            health_check_status: HealthCheckStatus::NotChecked,
            quota_status: QuotaStatus::Unknown,
            constraint_support: self.supported_constraints(),
            failure_code,
            checked_at: chrono_now_iso(),
            evidence,
            redaction_applied: false,
        }
    }

    fn search(&self, request: &SearchRequest) -> Result<SearchResponse, SearchError> {
        let api_key = self
            .api_key
            .as_deref()
            .ok_or_else(|| SearchError::credential_missing("SERPAPI_API_KEY"))?;

        let url = &self.endpoint;

        // Build query parameters — api_key is NEVER logged or serialized
        let query_params = build_serpapi_query_params(request, api_key);

        // Build query string manually to avoid logging api_key
        let query_string = build_query_string(&query_params);

        // Make HTTP request
        let full_url = format!("{}?{}", url, query_string);

        // The api_key is in the query string, but we never log the full URL.
        // We log only the endpoint + engine + q for diagnostics.
        let response = ureq::get(&full_url)
            .set("User-Agent", "image-retrieval/0.1.0")
            .call()
            .map_err(|e| {
                let err_str = e.to_string();
                if err_str.contains("timeout") || err_str.contains("timed out") {
                    SearchError::timeout(err_str)
                } else if err_str.contains("Connect") || err_str.contains("resolve") {
                    SearchError::unavailable(format!("network error: {}", err_str))
                } else {
                    SearchError::http(None, err_str)
                }
            })?;

        let status_code = response.status();
        let body = response
            .into_string()
            .map_err(|e| SearchError::parse(format!("failed to read response: {}", e)))?;

        match status_code {
            200 => {
                let raw_results = limit_raw_results(
                    self.parse_image_results(&body)?,
                    request.max_results as usize,
                );

                if raw_results.is_empty() {
                    return Ok(SearchResponse::empty(
                        request,
                        self.provider_id.clone(),
                        SearchProviderKind::SerpapiGoogleImages,
                    ));
                }

                let raw_count = raw_results.len() as u32;
                let mut candidates = Vec::with_capacity(raw_results.len());
                let mut diagnostics = Vec::new();

                for raw in &raw_results {
                    match self.normalize_image_result(
                        raw,
                        &request.query_plan_id.to_string(),
                        &request.search_request_id,
                        request.search_round,
                        request.full_attempt_count,
                        &request.query_text,
                    ) {
                        Some(candidate) => {
                            // Add diagnostic for missing optional fields
                            if candidate.width.is_none() && candidate.height.is_none() {
                                diagnostics.push(
                                    SearchDiagnostic::info(
                                        SearchDiagnosticCode::CandidateDimensionsMissing,
                                        format!(
                                            "Candidate '{}' has no dimensions from provider",
                                            candidate.candidate_id
                                        ),
                                    )
                                    .with_candidate(candidate.candidate_id.clone()),
                                );
                            }
                            if candidate.source_page_url.is_none() {
                                diagnostics.push(
                                    SearchDiagnostic::info(
                                        SearchDiagnosticCode::CandidateSourceUrlMissing,
                                        format!(
                                            "Candidate '{}' has no source page URL",
                                            candidate.candidate_id
                                        ),
                                    )
                                    .with_candidate(candidate.candidate_id.clone()),
                                );
                            }
                            if candidate.license_hint.is_none() {
                                diagnostics.push(
                                    SearchDiagnostic::info(
                                        SearchDiagnosticCode::CandidateLicenseUnknown,
                                        format!(
                                            "Candidate '{}' has no license information",
                                            candidate.candidate_id
                                        ),
                                    )
                                    .with_candidate(candidate.candidate_id.clone()),
                                );
                            }
                            candidates.push(candidate);
                        }
                        None => {
                            diagnostics.push(
                                SearchDiagnostic::warning(
                                    SearchDiagnosticCode::CandidateImageUrlMissing,
                                    "Provider result has no image URL; skipped.",
                                )
                                .with_provider(self.provider_id.clone()),
                            );
                        }
                    }
                }

                let normalized_count = candidates.len() as u32;
                let exhausted = raw_count == 0;

                Ok(SearchResponse {
                    search_request_id: request.search_request_id.clone(),
                    provider_id: self.provider_id.clone(),
                    provider_kind: SearchProviderKind::SerpapiGoogleImages,
                    query_plan_id: request.query_plan_id.clone(),
                    search_round: request.search_round,
                    status: if normalized_count > 0 {
                        SearchResponseStatus::Complete
                    } else {
                        SearchResponseStatus::Empty
                    },
                    candidates,
                    raw_result_count: raw_count,
                    normalized_count,
                    provider_next_page_token_present: false,
                    exhausted,
                    diagnostics,
                    redaction_applied: true,
                })
            }
            429 => Err(SearchError::rate_limited(format!(
                "SerpApi returned HTTP 429 Too Many Requests"
            ))),
            401 | 403 => Err(SearchError::misconfigured(format!(
                "SerpApi returned HTTP {} — check API key",
                status_code
            ))),
            _ => Err(SearchError::http(
                Some(status_code),
                format!("SerpApi returned unexpected HTTP {}", status_code),
            )),
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn build_serpapi_query_params(request: &SearchRequest, api_key: &str) -> Vec<(String, String)> {
    let mut query_params: Vec<(String, String)> = vec![
        ("engine".into(), "google_images".into()),
        ("q".into(), request.query_text.clone()),
        ("num".into(), request.max_results.max(1).to_string()),
        ("api_key".into(), api_key.to_string()),
    ];

    // Add safe filtering params
    if let Some(license) = request.request_tags.iter().find_map(|(k, v)| {
        if k == "license_type" {
            Some(v.clone())
        } else {
            None
        }
    }) {
        query_params.push(("license_type".into(), license));
    }

    query_params
}

fn limit_raw_results(
    raw_results: Vec<ProviderRawImageResult>,
    max_results: usize,
) -> Vec<ProviderRawImageResult> {
    if max_results == 0 {
        return Vec::new();
    }
    raw_results.into_iter().take(max_results).collect()
}

/// Build a URL query string from key-value pairs.
fn build_query_string(params: &[(String, String)]) -> String {
    params
        .iter()
        .map(|(k, v)| format!("{}={}", url_encode(k), url_encode(v)))
        .collect::<Vec<_>>()
        .join("&")
}

/// Simple URL encoding for query parameters.
fn url_encode(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                result.push(byte as char)
            }
            b' ' => result.push('+'),
            other => {
                result.push_str(&format!("%{:02X}", other));
            }
        }
    }
    result
}

/// Extract a domain hint from a URL for source authority tracking.
fn extract_domain_hint(url: &str) -> Option<String> {
    let without_scheme = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);
    let domain = without_scheme.split('/').next().unwrap_or(without_scheme);
    let domain = domain.split(':').next().unwrap_or(domain);
    if domain.is_empty() {
        None
    } else {
        Some(domain.to_string())
    }
}

/// Return current time as ISO 8601 string (for evidence timestamps).
fn chrono_now_iso() -> String {
    // Simple ISO 8601 without chrono dependency
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();
    // Basic ISO 8601 format
    let days_since_epoch = secs / 86400;
    let remaining_secs = secs % 86400;
    let hours = remaining_secs / 3600;
    let minutes = (remaining_secs % 3600) / 60;
    let seconds = remaining_secs % 60;

    // Simple date calculation (approximate for evidence timestamps)
    let year = 1970 + (days_since_epoch / 365) as u64; // approximate
    let day_of_year = (days_since_epoch % 365) as u64;
    let month = 1 + (day_of_year * 12 / 365) as u64; // approximate
    let day = 1 + (day_of_year % 30) as u64; // approximate

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hours, minutes, seconds
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::BaseSearchProvider;

    /// Sample SerpApi image_results JSON fixture.
    const SERPAPI_FIXTURE: &str = r#"{
        "search_metadata": {
            "id": "test-123",
            "status": "Success"
        },
        "search_parameters": {
            "engine": "google_images",
            "q": "cats playing"
        },
        "image_results": [
            {
                "position": 1,
                "title": "Cats playing with yarn",
                "link": "https://example.com/cats-playing",
                "original": "https://example.com/images/cats1.jpg",
                "thumbnail": "https://example.com/thumbs/cats1_t.jpg",
                "original_width": 1920,
                "original_height": 1080,
                "source": "example.com",
                "license": "creative commons"
            },
            {
                "position": 2,
                "title": "Kittens in a box",
                "link": "https://photos.example.com/kittens-box",
                "original": "https://photos.example.com/images/kittens.jpg",
                "thumbnail": "https://photos.example.com/thumbs/kittens_t.jpg",
                "original_width": 800,
                "original_height": 600,
                "source": "photos.example.com"
            },
            {
                "position": 3,
                "title": "Missing image URL entry",
                "link": "https://bad.example.com/no-image",
                "source": "bad.example.com"
            }
        ]
    }"#;

    #[test]
    fn serpapi_parse_image_results_success() {
        let adapter = SerpApiGoogleImagesAdapter::fixture();
        let results = adapter.parse_image_results(SERPAPI_FIXTURE).unwrap();
        assert_eq!(results.len(), 3);

        // First result
        assert_eq!(results[0].provider_rank, 1);
        assert_eq!(
            results[0].image_url.as_deref(),
            Some("https://example.com/images/cats1.jpg")
        );
        assert_eq!(
            results[0].source_page_url.as_deref(),
            Some("https://example.com/cats-playing")
        );
        assert_eq!(results[0].width, Some(1920));
        assert_eq!(results[0].height, Some(1080));
        assert_eq!(results[0].license_hint.as_deref(), Some("creative commons"));

        // Second result
        assert_eq!(results[1].provider_rank, 2);
        assert_eq!(results[1].width, Some(800));
        assert_eq!(results[1].height, Some(600));

        // Third result — missing image URL
        assert_eq!(results[2].provider_rank, 3);
        assert!(results[2].image_url.is_none());
    }

    #[test]
    fn serpapi_parse_live_images_results_field() {
        // The live SerpApi Google Images API returns `images_results` (plural),
        // not `image_results`. Regression guard for the field-name mismatch that
        // caused zero candidates against the real service.
        let adapter = SerpApiGoogleImagesAdapter::fixture();
        let body = r#"{
            "search_metadata": {"status": "Success"},
            "images_results": [
                {"position": 1, "original": "https://example.com/a.jpg",
                 "link": "https://example.com/a", "title": "A",
                 "original_width": 1024, "original_height": 768}
            ]
        }"#;
        let results = adapter.parse_image_results(body).unwrap();
        assert_eq!(results.len(), 1, "live images_results field must be parsed");
        assert_eq!(
            results[0].image_url.as_deref(),
            Some("https://example.com/a.jpg")
        );
    }

    #[test]
    fn serpapi_query_params_include_request_max_results() {
        let request = SearchRequest::new(
            crate::domain::query_plan::QueryPlanId::new("qp-1"),
            ProviderId::new("serpapi_google_images"),
            "sunset",
            20,
            1,
            1,
        );
        let params = build_serpapi_query_params(&request, "secret-key");

        assert!(params.contains(&("engine".to_string(), "google_images".to_string())));
        assert!(params.contains(&("q".to_string(), "sunset".to_string())));
        assert!(params.contains(&("num".to_string(), "20".to_string())));
        assert!(params.contains(&("api_key".to_string(), "secret-key".to_string())));
    }

    #[test]
    fn serpapi_limits_raw_results_to_request_max_results() {
        let raw = vec![
            ProviderRawImageResult {
                provider_raw_id: Some("1".into()),
                provider_rank: 1,
                image_url: Some("https://example.com/1.jpg".into()),
                source_page_url: None,
                thumbnail_url: None,
                title: None,
                snippet: None,
                width: None,
                height: None,
                mime_type: None,
                license_hint: None,
                attribution: None,
                provider_extra_safe: BTreeMap::new(),
            },
            ProviderRawImageResult {
                provider_raw_id: Some("2".into()),
                provider_rank: 2,
                image_url: Some("https://example.com/2.jpg".into()),
                source_page_url: None,
                thumbnail_url: None,
                title: None,
                snippet: None,
                width: None,
                height: None,
                mime_type: None,
                license_hint: None,
                attribution: None,
                provider_extra_safe: BTreeMap::new(),
            },
            ProviderRawImageResult {
                provider_raw_id: Some("3".into()),
                provider_rank: 3,
                image_url: Some("https://example.com/3.jpg".into()),
                source_page_url: None,
                thumbnail_url: None,
                title: None,
                snippet: None,
                width: None,
                height: None,
                mime_type: None,
                license_hint: None,
                attribution: None,
                provider_extra_safe: BTreeMap::new(),
            },
        ];

        let limited = limit_raw_results(raw, 2);
        assert_eq!(limited.len(), 2);
        assert_eq!(limited[0].provider_raw_id.as_deref(), Some("1"));
        assert_eq!(limited[1].provider_raw_id.as_deref(), Some("2"));
    }

    #[test]
    fn serpapi_normalize_image_result() {
        let adapter = SerpApiGoogleImagesAdapter::fixture();
        let raw = ProviderRawImageResult {
            provider_raw_id: Some("1".into()),
            provider_rank: 1,
            image_url: Some("https://example.com/images/cats1.jpg".into()),
            source_page_url: Some("https://example.com/cats-playing".into()),
            thumbnail_url: Some("https://example.com/thumbs/cats1_t.jpg".into()),
            title: Some("Cats playing".into()),
            snippet: Some("kittens with yarn".into()),
            width: Some(1920),
            height: Some(1080),
            mime_type: None,
            license_hint: Some("creative commons".into()),
            attribution: Some("example.com".into()),
            provider_extra_safe: BTreeMap::new(),
        };

        let candidate = adapter
            .normalize_image_result(&raw, "qp-test", "sr-test", 1, 1, "cats")
            .unwrap();

        assert_eq!(candidate.image_url, "https://example.com/images/cats1.jpg");
        assert_eq!(
            candidate.source_page_url,
            Some("https://example.com/cats-playing".into())
        );
        assert_eq!(
            candidate.thumbnail_url,
            Some("https://example.com/thumbs/cats1_t.jpg".into())
        );
        assert_eq!(candidate.width, Some(1920));
        assert_eq!(candidate.height, Some(1080));
        assert_eq!(candidate.license_hint, Some("creative commons".into()));
        assert_eq!(candidate.provider_rank, 1);
        assert_eq!(candidate.search_round, 1);
        assert_eq!(candidate.provider_id.to_string(), "serpapi_google_images");
        assert_eq!(candidate.provider_kind, "serpapi_google_images");
        assert!(!candidate.candidate_id.0.is_empty());
        assert!(!candidate.dedupe_key.is_empty());
        assert!(!candidate.origin_candidate_ids.is_empty());

        // Provenance
        assert_eq!(candidate.provenance.provider_rank, 1);
        assert_eq!(candidate.provenance.search_round, 1);
        assert!(!candidate.provenance.retrieved_at.is_empty());
        assert_eq!(
            candidate.provenance.source_authority_hint,
            Some("example.com".into())
        );

        // MIME type missing → no blocking diagnostic, just info
        assert!(candidate
            .normalization_warnings
            .iter()
            .any(|w| w.contains("MIME type")));
    }

    #[test]
    fn serpapi_normalize_missing_image_url_returns_none() {
        let adapter = SerpApiGoogleImagesAdapter::fixture();
        let raw = ProviderRawImageResult {
            provider_raw_id: None,
            provider_rank: 1,
            image_url: None,
            source_page_url: None,
            thumbnail_url: None,
            title: None,
            snippet: None,
            width: None,
            height: None,
            mime_type: None,
            license_hint: None,
            attribution: None,
            provider_extra_safe: BTreeMap::new(),
        };

        let result = adapter.normalize_image_result(&raw, "qp-test", "sr-test", 1, 1, "test");
        assert!(result.is_none(), "missing image_url should return None");
    }

    #[test]
    fn serpapi_normalize_empty_image_url_returns_none() {
        let adapter = SerpApiGoogleImagesAdapter::fixture();
        let raw = ProviderRawImageResult {
            provider_raw_id: None,
            provider_rank: 1,
            image_url: Some("".into()),
            source_page_url: None,
            thumbnail_url: None,
            title: None,
            snippet: None,
            width: None,
            height: None,
            mime_type: None,
            license_hint: None,
            attribution: None,
            provider_extra_safe: BTreeMap::new(),
        };

        let result = adapter.normalize_image_result(&raw, "qp-test", "sr-test", 1, 1, "test");
        assert!(result.is_none(), "empty image_url should return None");
    }

    #[test]
    fn serpapi_candidate_has_dedupe_key() {
        let adapter = SerpApiGoogleImagesAdapter::fixture();
        let raw = ProviderRawImageResult {
            provider_raw_id: Some("42".into()),
            provider_rank: 5,
            image_url: Some("https://EXAMPLE.com/Path/Image.jpg?utm_source=x#frag".into()),
            source_page_url: Some("https://example.com/page".into()),
            thumbnail_url: None,
            title: Some("Test".into()),
            snippet: None,
            width: Some(100),
            height: Some(100),
            mime_type: None,
            license_hint: None,
            attribution: None,
            provider_extra_safe: BTreeMap::new(),
        };

        let candidate = adapter
            .normalize_image_result(&raw, "qp-1", "sr-1", 1, 1, "test")
            .unwrap();

        // Dedupe key should be normalized (lowercase, no tracking params, no fragment)
        assert!(!candidate.dedupe_key.contains("utm_source"));
        assert!(!candidate.dedupe_key.contains("#frag"));
        assert!(candidate.dedupe_key.contains("example.com/path/image.jpg"));

        let dedupe_key1 = candidate.dedupe_key.clone();

        // Dedupe keys for the same URL with different tracking should match
        let raw2 = ProviderRawImageResult {
            provider_raw_id: Some("43".into()),
            provider_rank: 6,
            image_url: Some("https://example.com/path/image.jpg".into()),
            source_page_url: None,
            thumbnail_url: None,
            title: None,
            snippet: None,
            width: None,
            height: None,
            mime_type: None,
            license_hint: None,
            attribution: None,
            provider_extra_safe: BTreeMap::new(),
        };

        let candidate2 = adapter
            .normalize_image_result(&raw2, "qp-1", "sr-2", 2, 1, "test")
            .unwrap();

        assert_eq!(dedupe_key1, candidate2.dedupe_key);
    }

    #[test]
    fn serpapi_candidate_ids_differ_by_url() {
        let adapter = SerpApiGoogleImagesAdapter::fixture();
        let raw1 = ProviderRawImageResult {
            provider_raw_id: None,
            provider_rank: 1,
            image_url: Some("https://example.com/1.jpg".into()),
            source_page_url: None,
            thumbnail_url: None,
            title: None,
            snippet: None,
            width: None,
            height: None,
            mime_type: None,
            license_hint: None,
            attribution: None,
            provider_extra_safe: BTreeMap::new(),
        };
        let raw2 = ProviderRawImageResult {
            provider_raw_id: None,
            provider_rank: 1,
            image_url: Some("https://example.com/2.jpg".into()),
            source_page_url: None,
            thumbnail_url: None,
            title: None,
            snippet: None,
            width: None,
            height: None,
            mime_type: None,
            license_hint: None,
            attribution: None,
            provider_extra_safe: BTreeMap::new(),
        };

        let c1 = adapter
            .normalize_image_result(&raw1, "qp-1", "sr-1", 1, 1, "test")
            .unwrap();
        let c2 = adapter
            .normalize_image_result(&raw2, "qp-1", "sr-1", 1, 1, "test")
            .unwrap();

        assert_ne!(c1.candidate_id, c2.candidate_id);
    }

    #[test]
    fn serpapi_parse_empty_results() {
        let adapter = SerpApiGoogleImagesAdapter::fixture();
        let json = r#"{"search_metadata": {"status": "Success"}, "image_results": []}"#;
        let results = adapter.parse_image_results(json).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn serpapi_parse_error_response() {
        let adapter = SerpApiGoogleImagesAdapter::fixture();
        let json = r#"{"error": "Invalid API key"}"#;
        let result = adapter.parse_image_results(json);
        assert!(result.is_err());
    }

    #[test]
    fn serpapi_readiness_missing_credential() {
        // Use production mode but without a real API key env var set
        let adapter = SerpApiGoogleImagesAdapter::new(
            "serpapi",
            "SerpApi",
            Some("SERPAPI_API_KEY"),
            Some("https://serpapi.com/search"),
            true, // production mode
        );
        // The adapter has no real API key because SERPAPI_API_KEY is likely not set
        let config = crate::domain::config::SearchProviderConfig {
            provider_id: "serpapi".into(),
            provider_kind: SearchProviderKind::SerpapiGoogleImages,
            enabled: true,
            weight: 100,
            endpoint: Some("https://serpapi.com/search".into()),
            credential_env: Some("SERPAPI_API_KEY".into()),
            default_query_params: BTreeMap::new(),
        };

        let report = adapter.readiness(&config);
        // If SERPAPI_API_KEY is not set in test env, readiness shows MissingCredentials
        // If it IS set (e.g. in CI), the adapter would be Ready
        // We test that the report is valid regardless
        assert!(!report.available || report.status == ProviderReadinessStatus::Ready);
        if !report.available {
            assert_eq!(
                report.failure_code,
                Some(ProviderFailureCode::ProviderCredentialMissing)
            );
        }
    }

    #[test]
    fn serpapi_readiness_fixture_mode() {
        let adapter = SerpApiGoogleImagesAdapter::fixture();
        // Even with credential present, fixture mode should show FixtureOnly
        let config = crate::domain::config::SearchProviderConfig {
            provider_id: "serpapi".into(),
            provider_kind: SearchProviderKind::SerpapiGoogleImages,
            enabled: true,
            weight: 100,
            endpoint: Some("https://serpapi.com/search".into()),
            credential_env: None,
            default_query_params: BTreeMap::new(),
        };

        let report = adapter.readiness(&config);
        // The adapter checks both credential and fixture mode
        // fixture mode → FixtureOnly status
        assert!(!report.available);
    }

    #[test]
    fn serpapi_no_credentials_in_diagnostics() {
        let adapter = SerpApiGoogleImagesAdapter::fixture();
        let config = crate::domain::config::SearchProviderConfig {
            provider_id: "serpapi".into(),
            provider_kind: SearchProviderKind::SerpapiGoogleImages,
            enabled: true,
            weight: 100,
            endpoint: Some("https://serpapi.com/search".into()),
            credential_env: Some("SERPAPI_API_KEY".into()),
            default_query_params: BTreeMap::new(),
        };

        let report = adapter.readiness(&config);
        let json = serde_json::to_string(&report).unwrap_or_default();
        let lower = json.to_lowercase();

        // The env var NAME is fine; the VALUE must not appear
        assert!(!lower.contains("api_key="));
        assert!(!lower.contains("apikey="));
        // credential env NAME may appear
        assert!(lower.contains("serpapi_api_key"));
    }

    #[test]
    fn extract_domain_hint_from_url() {
        assert_eq!(
            extract_domain_hint("https://example.com/path/image.jpg"),
            Some("example.com".into())
        );
        assert_eq!(
            extract_domain_hint("http://photos.example.com:8080/path"),
            Some("photos.example.com".into())
        );
        assert_eq!(extract_domain_hint(""), None);
    }

    #[test]
    fn url_encode_basic() {
        let encoded = url_encode("cats playing");
        assert_eq!(encoded, "cats+playing");
        let encoded = url_encode("hello world!");
        assert!(encoded.contains("hello"));
    }
}
