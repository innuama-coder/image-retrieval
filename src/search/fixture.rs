//! Fixture / mock provider implementations for automated testing.
//!
//! These providers implement [`BaseSearchProvider`] and [`BaseProvider`]
//! and are suitable for scheduler tests, integration tests, and
//! self-check readiness tests.
//!
//! **Fixture-only**: fixture providers carry `fixture = true` evidence
//! and must not satisfy production readiness. Production runs with only
//! fixture providers must produce `PROVIDER_FIXTURE_NOT_PRODUCTION`.
//!
//! References:
//! - `docs/design/v1.1-TASK-002-search-provider-candidate-design.md`

use crate::domain::candidate::{
    CandidateId, CandidateProvenance, CandidateRecord, LicenseEvidence, ProviderId,
};
use crate::domain::config::SearchProviderKind;
use crate::domain::search::{
    CredentialStatus, HealthCheckStatus, ProviderConstraintSupport, ProviderEvidence,
    ProviderFailureCode, ProviderReadinessReport, ProviderReadinessStatus, QuotaStatus,
    SearchError, SearchRequest, SearchResponse, SearchResponseStatus,
};
use crate::ports::BaseSearchProvider;
use std::cell::Cell;
use std::cell::RefCell;
use std::sync::Mutex;

// ===========================================================================
// FixtureSearchProvider — v1.1 fixture for BaseSearchProvider
// ===========================================================================

/// A configurable fixture provider that implements [`BaseSearchProvider`].
///
/// Supports:
/// - Multiple response batches (consumed in order).
/// - Ready / not-ready state.
/// - Configurable provider kind (default: fixture).
/// - Weight and enabled state.
pub struct FixtureSearchProvider {
    id: ProviderId,
    name: String,
    kind: SearchProviderKind,
    ready: bool,
    credential_present: bool,
    weight: u32,
    enabled: bool,
    /// Pre-configured response batches.
    response_batches: Mutex<Vec<Vec<CandidateRecord>>>,
    call_count: Mutex<usize>,
}

impl FixtureSearchProvider {
    /// Create a ready fixture provider with default weight 1.
    pub fn new(id: &str, display_name: &str) -> Self {
        Self {
            id: ProviderId::new(id),
            name: display_name.into(),
            kind: SearchProviderKind::Fixture,
            ready: true,
            credential_present: true,
            weight: 1,
            enabled: true,
            response_batches: Mutex::new(Vec::new()),
            call_count: Mutex::new(0),
        }
    }

    /// Create a provider in not-ready state.
    pub fn not_ready(id: &str, display_name: &str) -> Self {
        let mut p = Self::new(id, display_name);
        p.ready = false;
        p
    }

    /// Set the scheduling weight.
    pub fn with_weight(mut self, weight: u32) -> Self {
        self.weight = weight;
        self
    }

    /// Set the enabled state.
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Set credential presence for readiness checks.
    pub fn with_credential(mut self, present: bool) -> Self {
        self.credential_present = present;
        self
    }

    /// Set the provider kind.
    pub fn with_kind(mut self, kind: SearchProviderKind) -> Self {
        self.kind = kind;
        self
    }

    /// Pre-load response batches.
    pub fn with_responses(self, batches: Vec<Vec<CandidateRecord>>) -> Self {
        *self.response_batches.lock().unwrap() = batches;
        self
    }

    /// How many times `search()` was called.
    pub fn call_count(&self) -> usize {
        *self.call_count.lock().unwrap()
    }
}

impl BaseSearchProvider for FixtureSearchProvider {
    fn provider_id(&self) -> ProviderId {
        self.id.clone()
    }

    fn display_name(&self) -> &str {
        &self.name
    }

    fn provider_kind(&self) -> SearchProviderKind {
        self.kind.clone()
    }

    fn supported_constraints(&self) -> ProviderConstraintSupport {
        ProviderConstraintSupport {
            max_results_per_request: Some(100),
            supported_content_types: vec!["image/jpeg".into(), "image/png".into()],
            supports_quality_filter: false,
            supports_license_filter: false,
            supports_dimension_filter: false,
        }
    }

    fn readiness(
        &self,
        _config: &crate::domain::config::SearchProviderConfig,
    ) -> ProviderReadinessReport {
        if !self.enabled {
            return ProviderReadinessReport::not_ready(
                self.id.clone(),
                self.kind.clone(),
                &self.name,
                ProviderReadinessStatus::Disabled,
                ProviderFailureCode::ProviderDisabled,
                vec![ProviderEvidence {
                    code: "PROVIDER_DISABLED".into(),
                    message: format!("Fixture provider '{}' is disabled", self.id),
                    severity: "info".into(),
                }],
            );
        }

        if !self.ready {
            return ProviderReadinessReport::not_ready(
                self.id.clone(),
                self.kind.clone(),
                &self.name,
                ProviderReadinessStatus::Unavailable,
                ProviderFailureCode::ProviderUnavailable,
                vec![ProviderEvidence {
                    code: "HEALTH_FAILED".into(),
                    message: format!("Fixture provider '{}' reports not ready", self.id),
                    severity: "error".into(),
                }],
            );
        }

        if !self.credential_present {
            return ProviderReadinessReport::not_ready(
                self.id.clone(),
                self.kind.clone(),
                &self.name,
                ProviderReadinessStatus::MissingCredentials,
                ProviderFailureCode::ProviderCredentialMissing,
                vec![ProviderEvidence {
                    code: "PROVIDER_CREDENTIAL_MISSING".into(),
                    message: format!(
                        "Fixture provider '{}' has no credential configured",
                        self.id
                    ),
                    severity: "blocker".into(),
                }],
            );
        }

        if self.weight == 0 {
            return ProviderReadinessReport::not_ready(
                self.id.clone(),
                self.kind.clone(),
                &self.name,
                ProviderReadinessStatus::Misconfigured,
                ProviderFailureCode::ProviderWeightInvalid,
                vec![ProviderEvidence {
                    code: "PROVIDER_WEIGHT_INVALID".into(),
                    message: format!(
                        "Fixture provider '{}' has weight 0 and cannot be scheduled",
                        self.id
                    ),
                    severity: "error".into(),
                }],
            );
        }

        let effective_weight = if self.weight > 0 {
            Some(self.weight)
        } else {
            None
        };

        ProviderReadinessReport {
            provider_id: self.id.clone(),
            provider_kind: self.kind.clone(),
            display_name: self.name.clone(),
            status: ProviderReadinessStatus::Ready,
            available: true,
            included_in_weight_table: self.enabled && self.weight > 0,
            configured_weight: self.weight,
            effective_weight,
            credential_status: CredentialStatus::Present,
            health_check_status: HealthCheckStatus::Healthy,
            quota_status: QuotaStatus::Ok,
            constraint_support: self.supported_constraints(),
            failure_code: None,
            checked_at: String::new(),
            evidence: Vec::new(),
            redaction_applied: false,
        }
    }

    fn search(&self, request: &SearchRequest) -> std::result::Result<SearchResponse, SearchError> {
        let mut count = self.call_count.lock().unwrap();
        let idx = *count;
        *count += 1;
        drop(count);

        let mut responses = self.response_batches.lock().unwrap();
        let candidates = if idx < responses.len() {
            std::mem::take(&mut responses[idx])
        } else {
            Vec::new()
        };

        let raw_count = candidates.len() as u32;
        let exhausted = candidates.is_empty();
        let status = if candidates.is_empty() {
            SearchResponseStatus::Empty
        } else {
            SearchResponseStatus::Complete
        };

        Ok(SearchResponse {
            search_request_id: request.search_request_id.clone(),
            provider_id: self.id.clone(),
            provider_kind: self.kind.clone(),
            query_plan_id: request.query_plan_id.clone(),
            search_round: request.search_round,
            status,
            candidates,
            raw_result_count: raw_count,
            normalized_count: raw_count,
            provider_next_page_token_present: false,
            exhausted,
            diagnostics: Vec::new(),
            redaction_applied: false,
        })
    }
}

// ===========================================================================
// FixtureProvider — legacy BaseProvider (deprecated compat)
// ===========================================================================

/// Legacy fixture provider implementing [`BaseProvider`].
///
/// **Deprecated for v1.1**: prefer [`FixtureSearchProvider`].
pub struct FixtureProvider {
    id: ProviderId,
    name: String,
    ready: bool,
    weight: i32,
    enabled: bool,
    response_batches: RefCell<Vec<Vec<CandidateRecord>>>,
    call_count: Cell<u32>,
}

impl FixtureProvider {
    pub fn new(id: &str, display_name: &str) -> Self {
        Self {
            id: ProviderId::new(id),
            name: display_name.into(),
            ready: true,
            weight: 1,
            enabled: true,
            response_batches: RefCell::new(Vec::new()),
            call_count: Cell::new(0),
        }
    }

    pub fn not_ready(id: &str, display_name: &str) -> Self {
        let mut p = Self::new(id, display_name);
        p.ready = false;
        p
    }

    pub fn with_weight(mut self, weight: i32) -> Self {
        self.weight = weight;
        self
    }

    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn with_responses(self, batches: Vec<Vec<CandidateRecord>>) -> Self {
        *self.response_batches.borrow_mut() = batches;
        self
    }

    pub fn call_count(&self) -> u32 {
        self.call_count.get()
    }
}

impl crate::ports::BaseProvider for FixtureProvider {
    fn provider_id(&self) -> ProviderId {
        self.id.clone()
    }

    fn display_name(&self) -> &str {
        &self.name
    }

    fn readiness(&self) -> crate::error::Result<()> {
        if self.ready {
            Ok(())
        } else {
            Err(crate::error::Error::provider_failure(
                self.id.to_string(),
                "fixture: provider not ready",
            ))
        }
    }

    fn weight(&self) -> i32 {
        self.weight
    }

    fn search(
        &self,
        _query: &str,
        _max_results: u32,
    ) -> crate::error::Result<Vec<CandidateRecord>> {
        let count = self.call_count.get();
        self.call_count.set(count + 1);

        let mut batches = self.response_batches.borrow_mut();
        if (count as usize) < batches.len() {
            Ok(std::mem::take(&mut batches[count as usize]))
        } else {
            Ok(Vec::new())
        }
    }
}

// ===========================================================================
// Candidate factory helpers
// ===========================================================================

/// Create a minimal v1.1 candidate record for testing.
pub fn make_fixture_candidate(index: u32, provider_id: &str, base_url: &str) -> CandidateRecord {
    let image_url = format!("{}/image{}.jpg", base_url, index);
    let candidate_id = CandidateId::new(format!("fixture-{}-{}", provider_id, index));
    CandidateRecord {
        candidate_id: candidate_id.clone(),
        query_plan_id: "qp-fixture".into(),
        provider_id: ProviderId::new(provider_id),
        provider_kind: "fixture".into(),
        search_request_id: "sr-fixture".into(),
        search_round: 1,
        provider_rank: index + 1,
        global_rank_hint: None,
        image_url: image_url.clone(),
        source_page_url: Some(format!("{}/page{}.html", base_url, index)),
        thumbnail_url: Some(format!("{}/thumb{}.jpg", base_url, index)),
        title: Some(format!("Fixture Image {} from {}", index, provider_id)),
        snippet: None,
        width: Some(800),
        height: Some(600),
        mime_type: Some("image/jpeg".into()),
        license_hint: None,
        attribution: None,
        dedupe_key: CandidateRecord::build_dedupe_key(&image_url),
        origin_candidate_ids: vec![candidate_id],
        provenance: CandidateProvenance {
            provider_raw_id: None,
            provider_result_url: None,
            provider_rank: index + 1,
            search_query: "fixture query".into(),
            search_round: 1,
            full_attempt_count: 1,
            retrieved_at: String::new(),
            provider_evidence_refs: Vec::new(),
            license_evidence: LicenseEvidence::Unknown,
            source_authority_hint: None,
        },
        normalization_warnings: Vec::new(),
    }
}

/// Create a batch of fixture candidates.
pub fn make_fixture_batch(provider_id: &str, count: u32, start_index: u32) -> Vec<CandidateRecord> {
    (start_index..start_index + count)
        .map(|i| {
            make_fixture_candidate(
                i,
                provider_id,
                &format!("https://{}.example.com", provider_id),
            )
        })
        .collect()
}

// ===========================================================================
// Ready-set fixture helpers
// ===========================================================================

/// Create a ready v1.1 fixture provider with a single batch.
pub fn ready_fixture_with_candidates(
    id: &str,
    name: &str,
    weight: u32,
    candidate_count: u32,
) -> FixtureSearchProvider {
    FixtureSearchProvider::new(id, name)
        .with_weight(weight)
        .with_responses(vec![make_fixture_batch(id, candidate_count, 0)])
}

/// Create a ready v1.1 fixture provider with multiple batches.
pub fn ready_fixture_with_batches(
    id: &str,
    name: &str,
    weight: u32,
    batch_sizes: &[u32],
) -> FixtureSearchProvider {
    let mut offset = 0;
    let batches: Vec<Vec<CandidateRecord>> = batch_sizes
        .iter()
        .map(|&size| {
            let batch = make_fixture_batch(id, size, offset);
            offset += size;
            batch
        })
        .collect();
    FixtureSearchProvider::new(id, name)
        .with_weight(weight)
        .with_responses(batches)
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_search_provider_basics() {
        let p = FixtureSearchProvider::new("test", "Test Provider");
        assert_eq!(p.provider_id().to_string(), "test");
        assert_eq!(p.display_name(), "Test Provider");
        assert!(matches!(p.provider_kind(), SearchProviderKind::Fixture));
    }

    #[test]
    fn fixture_search_provider_not_ready() {
        let p = FixtureSearchProvider::not_ready("test", "Test");
        let config = crate::domain::config::SearchProviderConfig {
            provider_id: "test".into(),
            provider_kind: SearchProviderKind::Fixture,
            enabled: true,
            weight: 1,
            endpoint: None,
            credential_env: None,
            default_query_params: std::collections::BTreeMap::new(),
        };
        let report = p.readiness(&config);
        assert!(!report.available);
        assert_eq!(report.status, ProviderReadinessStatus::Unavailable);
    }

    #[test]
    fn fixture_search_provider_returns_batches() {
        let batch1 = make_fixture_batch("p1", 5, 0);
        let batch2 = make_fixture_batch("p1", 3, 5);
        let p = FixtureSearchProvider::new("p1", "P1")
            .with_responses(vec![batch1.clone(), batch2.clone()]);

        let req = SearchRequest::new(
            crate::domain::query_plan::QueryPlanId::new("qp-t"),
            ProviderId::new("p1"),
            "test",
            10,
            1,
            1,
        );

        let r1 = p.search(&req).unwrap();
        assert_eq!(r1.candidates.len(), 5);
        assert_eq!(p.call_count(), 1);

        let r2 = p.search(&req).unwrap();
        assert_eq!(r2.candidates.len(), 3);
        assert_eq!(p.call_count(), 2);

        // Exhausted
        let r3 = p.search(&req).unwrap();
        assert!(r3.candidates.is_empty());
        assert_eq!(p.call_count(), 3);
    }

    #[test]
    fn fixture_search_provider_custom_weight() {
        let p = FixtureSearchProvider::new("p", "P").with_weight(5);
        let config = crate::domain::config::SearchProviderConfig {
            provider_id: "p".into(),
            provider_kind: SearchProviderKind::Fixture,
            enabled: true,
            weight: 5,
            endpoint: None,
            credential_env: None,
            default_query_params: std::collections::BTreeMap::new(),
        };
        let report = p.readiness(&config);
        assert_eq!(report.effective_weight, Some(5));
    }

    #[test]
    fn make_fixture_candidate_has_all_fields() {
        let c = make_fixture_candidate(42, "brave", "https://brave.example.com");
        assert_eq!(c.provider_id.to_string(), "brave");
        assert_eq!(c.provider_kind, "fixture");
        assert_eq!(c.image_url, "https://brave.example.com/image42.jpg");
        assert!(c.source_page_url.is_some());
        assert!(c.thumbnail_url.is_some());
        assert!(c.title.is_some());
        assert_eq!(c.width, Some(800));
        assert_eq!(c.height, Some(600));
        assert_eq!(c.mime_type, Some("image/jpeg".into()));
        assert!(!c.dedupe_key.is_empty());
        assert!(!c.origin_candidate_ids.is_empty());
    }

    #[test]
    fn make_fixture_batch_creates_unique_ids() {
        let batch = make_fixture_batch("p1", 10, 100);
        assert_eq!(batch.len(), 10);
        let ids: std::collections::HashSet<String> =
            batch.iter().map(|c| c.candidate_id.to_string()).collect();
        assert_eq!(ids.len(), 10);
    }

    #[test]
    fn ready_fixture_with_candidates_works() {
        let p = ready_fixture_with_candidates("p1", "P1", 2, 25);
        let req = SearchRequest::new(
            crate::domain::query_plan::QueryPlanId::new("qp-t"),
            ProviderId::new("p1"),
            "test",
            30,
            1,
            1,
        );
        let results = p.search(&req).unwrap();
        assert_eq!(results.candidates.len(), 25);
    }

    #[test]
    fn ready_fixture_with_batches_works() {
        let p = ready_fixture_with_batches("p1", "P1", 1, &[10, 10, 5]);
        let req = SearchRequest::new(
            crate::domain::query_plan::QueryPlanId::new("qp-t"),
            ProviderId::new("p1"),
            "test",
            20,
            1,
            1,
        );
        assert_eq!(p.search(&req).unwrap().candidates.len(), 10);
        assert_eq!(p.search(&req).unwrap().candidates.len(), 10);
        assert_eq!(p.search(&req).unwrap().candidates.len(), 5);
        assert!(p.search(&req).unwrap().candidates.is_empty());
    }

    #[test]
    fn fixture_provider_no_credentials_leak() {
        let p = ready_fixture_with_candidates("p1", "P1", 1, 5);
        let req = SearchRequest::new(
            crate::domain::query_plan::QueryPlanId::new("qp-t"),
            ProviderId::new("p1"),
            "test",
            10,
            1,
            1,
        );
        let results = p.search(&req).unwrap();
        let json = serde_json::to_string(&results.candidates).unwrap();
        assert!(!json.to_lowercase().contains("api_key"));
        assert!(!json.to_lowercase().contains("token"));
        assert!(!json.to_lowercase().contains("secret"));
    }

    // Legacy fixture tests
    #[test]
    fn legacy_fixture_provider_basics() {
        use crate::ports::BaseProvider;
        let p = FixtureProvider::new("test", "Test Provider");
        assert_eq!(p.provider_id().to_string(), "test");
        assert_eq!(p.display_name(), "Test Provider");
        assert_eq!(p.weight(), 1);
        assert!(p.readiness().is_ok());
    }

    #[test]
    fn legacy_fixture_provider_not_ready() {
        use crate::ports::BaseProvider;
        let p = FixtureProvider::not_ready("test", "Test");
        assert!(p.readiness().is_err());
    }

    #[test]
    fn legacy_fixture_provider_returns_batches() {
        use crate::ports::BaseProvider;
        let batch1 = make_fixture_batch("p1", 5, 0);
        let batch2 = make_fixture_batch("p1", 3, 5);
        let p =
            FixtureProvider::new("p1", "P1").with_responses(vec![batch1.clone(), batch2.clone()]);

        let r1 = p.search("q", 10).unwrap();
        assert_eq!(r1.len(), 5);
        let r2 = p.search("q", 10).unwrap();
        assert_eq!(r2.len(), 3);
        let r3 = p.search("q", 10).unwrap();
        assert!(r3.is_empty());
    }
}
