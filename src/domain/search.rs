//! Search domain types.
//!
//! Covers provider registration, readiness, weighted scheduling, candidate
//! normalisation, source tracking, and shortage diagnosis.
//!
//! References: PRD §搜索与候选产品要求, HLD §Search Scheduler,
//! `docs/design/TASK-003-base-provider-search-design.md`

use crate::domain::candidate::{CandidateRecord, ProviderId};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Provider registration
// ---------------------------------------------------------------------------

/// Registration of a search provider in the provider registry.
///
/// Holds the provider's enabled/disabled state, scheduling weight, and a
/// capability declaration. Credentials are NOT stored here — they belong
/// to the provider adapter's private configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderRegistration {
    /// Stable provider identifier.
    pub provider_id: ProviderId,

    /// Human-readable display name.
    pub display_name: String,

    /// Whether the provider is enabled for scheduling.
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Scheduling weight. Must be a positive integer.
    /// Zero or negative values are diagnosed and the provider is excluded
    /// from the effective weight table.
    #[serde(default = "default_weight")]
    pub weight: i32,

    /// Optional capabilities declaration (e.g. supported image types,
    /// max results per request). This is informational for the scheduler
    /// and not used for gating.
    #[serde(default)]
    pub capabilities: ProviderCapabilities,
}

fn default_enabled() -> bool {
    true
}

fn default_weight() -> i32 {
    1
}

impl ProviderRegistration {
    pub fn new(provider_id: ProviderId, display_name: impl Into<String>) -> Self {
        Self {
            provider_id,
            display_name: display_name.into(),
            enabled: true,
            weight: 1,
            capabilities: ProviderCapabilities::default(),
        }
    }

    pub fn with_weight(mut self, weight: i32) -> Self {
        self.weight = weight;
        self
    }

    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

/// Declared capabilities of a search provider.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProviderCapabilities {
    /// Maximum number of results the provider can return per request.
    pub max_results_per_request: Option<u32>,

    /// Supported image content types (e.g. ["image/jpeg", "image/png"]).
    #[serde(default)]
    pub supported_content_types: Vec<String>,
}

// ---------------------------------------------------------------------------
// Provider readiness
// ---------------------------------------------------------------------------

/// Readiness status of a provider after inspection.
///
/// Per the design: ready, disabled, missing_credentials, misconfigured,
/// rate_limited, unavailable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderReadiness {
    /// Provider is ready to serve search requests.
    Ready,

    /// Provider is explicitly disabled in configuration.
    Disabled,

    /// Provider requires credentials that are not present.
    MissingCredentials,

    /// Provider configuration is invalid (e.g. malformed endpoint).
    Misconfigured,

    /// Provider is temporarily rate-limited.
    RateLimited,

    /// Provider is unavailable (network, timeout, unknown error).
    Unavailable,
}

impl ProviderReadiness {
    /// Returns `true` if the provider can be included in the effective
    /// weight table.
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready)
    }

    /// Returns `true` if the readiness status is terminal for this task
    /// (cannot become ready without configuration changes).
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Disabled | Self::MissingCredentials | Self::Misconfigured
        )
    }
}

impl std::fmt::Display for ProviderReadiness {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Self::Ready => "ready",
            Self::Disabled => "disabled",
            Self::MissingCredentials => "missing_credentials",
            Self::Misconfigured => "misconfigured",
            Self::RateLimited => "rate_limited",
            Self::Unavailable => "unavailable",
        };
        write!(f, "{}", label)
    }
}

// ---------------------------------------------------------------------------
// Search request
// ---------------------------------------------------------------------------

/// A search request passed to a provider.
///
/// Contains the semantic query, constraints, quality preferences, and the
/// number of candidates to request.
#[derive(Debug, Clone)]
pub struct SearchRequest {
    /// Semantic description from the validated QueryPlan.
    pub query: String,

    /// Maximum number of results to request from this provider.
    pub max_results: u32,

    /// Quality tier preference for the search.
    pub quality_tier: crate::domain::query_plan::QualityTier,

    /// Content constraints (must-include / must-avoid).
    pub content_constraints: crate::domain::query_plan::ContentConstraints,

    /// Authorization risk preference.
    pub authorization_preference: crate::domain::query_plan::AuthorizationPreference,
}

impl SearchRequest {
    /// Build a SearchRequest from a TaskPlan, targeting a specific max_results
    /// for a single provider call.
    pub fn from_task_plan(plan: &crate::domain::query_plan::TaskPlan, max_results: u32) -> Self {
        Self {
            query: plan.query_plan.description.clone(),
            max_results,
            quality_tier: plan.query_plan.quality_tier,
            content_constraints: plan.query_plan.content_constraints.clone(),
            authorization_preference: plan.query_plan.authorization_preference,
        }
    }
}

// ---------------------------------------------------------------------------
// Search provider result
// ---------------------------------------------------------------------------

/// The result of a single provider search invocation.
#[derive(Debug, Clone)]
pub struct SearchProviderResult {
    /// Which provider produced this result.
    pub provider_id: ProviderId,

    /// Normalised candidate records returned by the provider.
    pub candidates: Vec<CandidateRecord>,

    /// Number of candidates remaining (if the provider signals more pages).
    /// `None` means the provider did not signal exhaustion.
    pub remaining: Option<u32>,

    /// Whether the provider is exhausted (no more results for this query).
    pub exhausted: bool,

    /// Failure category if the search did not succeed.
    pub failure: Option<SearchFailureCategory>,

    /// Human-readable failure reason (not exposed to end users with raw
    /// provider data).
    pub failure_reason: Option<String>,
}

impl SearchProviderResult {
    /// Build a successful result.
    pub fn success(provider_id: ProviderId, candidates: Vec<CandidateRecord>) -> Self {
        Self {
            provider_id,
            candidates,
            remaining: None,
            exhausted: false,
            failure: None,
            failure_reason: None,
        }
    }

    /// Build a result indicating provider exhaustion.
    pub fn exhausted_result(provider_id: ProviderId, candidates: Vec<CandidateRecord>) -> Self {
        Self {
            provider_id,
            candidates,
            remaining: Some(0),
            exhausted: true,
            failure: None,
            failure_reason: None,
        }
    }

    /// Build a failure result.
    pub fn failure(
        provider_id: ProviderId,
        category: SearchFailureCategory,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            provider_id,
            candidates: Vec::new(),
            remaining: None,
            exhausted: false,
            failure: Some(category),
            failure_reason: Some(reason.into()),
        }
    }

    /// Returns `true` if the search returned no candidates.
    pub fn is_empty(&self) -> bool {
        self.candidates.is_empty()
    }
}

/// Categories of search failures produced by a provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SearchFailureCategory {
    /// Provider returned an error response (e.g. HTTP 5xx).
    ProviderError,

    /// Provider timed out.
    Timeout,

    /// Provider is rate-limited.
    RateLimited,

    /// Provider returned a response that could not be normalised into
    /// CandidateRecords.
    UnnormalizableResponse,

    /// Provider returned an empty result set (no matches).
    EmptyResult,

    /// Provider is not available (network, configuration).
    Unavailable,

    /// Unknown / uncategorized failure.
    Other,
}

impl std::fmt::Display for SearchFailureCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Self::ProviderError => "provider_error",
            Self::Timeout => "timeout",
            Self::RateLimited => "rate_limited",
            Self::UnnormalizableResponse => "unnormalizable_response",
            Self::EmptyResult => "empty_result",
            Self::Unavailable => "unavailable",
            Self::Other => "other",
        };
        write!(f, "{}", label)
    }
}

// ---------------------------------------------------------------------------
// Weight entry — a single row in the effective weight table
// ---------------------------------------------------------------------------

/// One entry in the effective weight table used for weighted random
/// selection.
#[derive(Debug, Clone)]
pub struct WeightEntry {
    pub provider_id: ProviderId,
    pub display_name: String,
    pub weight: u32,
}

// ---------------------------------------------------------------------------
// Search usage event — per-invocation tracking
// ---------------------------------------------------------------------------

/// A record of one provider invocation during search scheduling.
///
/// Collected by the scheduler for observability (MET-002 numerator/denominator,
/// provider contribution trends).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchUsageEvent {
    /// Which provider was invoked.
    pub provider_id: ProviderId,

    /// How many candidates the provider returned (raw count).
    pub raw_candidate_count: u32,

    /// How many candidates survived deduplication.
    pub deduped_candidate_count: u32,

    /// Whether the provider reported exhaustion after this call.
    pub exhausted: bool,

    /// Failure category if the invocation failed.
    pub failure: Option<SearchFailureCategory>,

    /// Provider readiness at the time of invocation.
    pub readiness: ProviderReadiness,

    /// The effective weight used for this selection.
    pub effective_weight: u32,
}

impl SearchUsageEvent {
    pub fn success_event(
        provider_id: ProviderId,
        raw: u32,
        deduped: u32,
        exhausted: bool,
        weight: u32,
    ) -> Self {
        Self {
            provider_id,
            raw_candidate_count: raw,
            deduped_candidate_count: deduped,
            exhausted,
            failure: None,
            readiness: ProviderReadiness::Ready,
            effective_weight: weight,
        }
    }

    pub fn failure_event(
        provider_id: ProviderId,
        failure: SearchFailureCategory,
        readiness: ProviderReadiness,
        weight: u32,
    ) -> Self {
        Self {
            provider_id,
            raw_candidate_count: 0,
            deduped_candidate_count: 0,
            exhausted: false,
            failure: Some(failure),
            readiness,
            effective_weight: weight,
        }
    }
}

// ---------------------------------------------------------------------------
// Search outcome — the scheduler's aggregate result
// ---------------------------------------------------------------------------

/// The aggregated result of a search scheduling session.
#[derive(Debug, Clone)]
pub struct SearchOutcome {
    /// All unique candidates collected across all provider invocations.
    pub candidates: Vec<CandidateRecord>,

    /// Per-invocation usage events for observability.
    pub usage_events: Vec<SearchUsageEvent>,

    /// Total number of provider invocations made.
    pub total_invocations: u32,

    /// The candidate target that was requested.
    pub candidate_target: u32,

    /// Whether the target was met (after dedup).
    pub target_met: bool,

    /// Shortage reason if the target was not met.
    pub shortage_reason: Option<CandidateShortageReason>,

    /// Provider readiness summary at scheduling start.
    pub readiness_summary: Vec<ProviderReadinessRecord>,
}

/// A single provider's readiness at scheduling time.
#[derive(Debug, Clone)]
pub struct ProviderReadinessRecord {
    pub provider_id: ProviderId,
    pub display_name: String,
    pub readiness: ProviderReadiness,
    pub configured_weight: i32,
    pub included_in_table: bool,
}

/// Why the candidate target was not met.
#[derive(Debug, Clone)]
pub enum CandidateShortageReason {
    /// No providers were available (all disabled, unready, or invalid weight).
    NoAvailableProviders,

    /// All providers were exhausted before reaching the target.
    AllProvidersExhausted,

    /// All providers returned results but the total (after dedup) did not
    /// reach the target.
    InsufficientUniqueCandidates {
        total_raw: u32,
        total_deduped: u32,
        duplicates_removed: u32,
    },

    /// Some providers failed and others were exhausted.
    PartialFailureWithExhaustion {
        failed_providers: Vec<ProviderId>,
        exhausted_providers: Vec<ProviderId>,
    },
}

impl std::fmt::Display for CandidateShortageReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoAvailableProviders => {
                write!(f, "no providers available for scheduling")
            }
            Self::AllProvidersExhausted => {
                write!(
                    f,
                    "all providers exhausted before reaching candidate target"
                )
            }
            Self::InsufficientUniqueCandidates {
                total_raw,
                total_deduped,
                duplicates_removed,
            } => {
                write!(
                    f,
                    "insufficient unique candidates: {} raw, {} after dedup ({} duplicates removed)",
                    total_raw, total_deduped, duplicates_removed
                )
            }
            Self::PartialFailureWithExhaustion {
                failed_providers,
                exhausted_providers,
            } => {
                write!(
                    f,
                    "partial failure: {} provider(s) failed, {} provider(s) exhausted",
                    failed_providers.len(),
                    exhausted_providers.len()
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::candidate::CandidateId;

    #[test]
    fn provider_registration_defaults() {
        let reg = ProviderRegistration::new(ProviderId::new("p1"), "Provider 1");
        assert!(reg.enabled);
        assert_eq!(reg.weight, 1);
    }

    #[test]
    fn provider_registration_builder() {
        let reg = ProviderRegistration::new(ProviderId::new("p1"), "P1")
            .with_weight(3)
            .with_enabled(false);
        assert_eq!(reg.weight, 3);
        assert!(!reg.enabled);
    }

    #[test]
    fn provider_readiness_is_ready() {
        assert!(ProviderReadiness::Ready.is_ready());
        assert!(!ProviderReadiness::Disabled.is_ready());
        assert!(!ProviderReadiness::MissingCredentials.is_ready());
        assert!(!ProviderReadiness::Misconfigured.is_ready());
        assert!(!ProviderReadiness::RateLimited.is_ready());
        assert!(!ProviderReadiness::Unavailable.is_ready());
    }

    #[test]
    fn provider_readiness_terminal() {
        assert!(ProviderReadiness::Disabled.is_terminal());
        assert!(ProviderReadiness::MissingCredentials.is_terminal());
        assert!(ProviderReadiness::Misconfigured.is_terminal());
        assert!(!ProviderReadiness::Ready.is_terminal());
        assert!(!ProviderReadiness::RateLimited.is_terminal());
        assert!(!ProviderReadiness::Unavailable.is_terminal());
    }

    #[test]
    fn provider_readiness_display() {
        assert_eq!(ProviderReadiness::Ready.to_string(), "ready");
        assert_eq!(ProviderReadiness::Disabled.to_string(), "disabled");
        assert_eq!(
            ProviderReadiness::MissingCredentials.to_string(),
            "missing_credentials"
        );
        assert_eq!(
            ProviderReadiness::Misconfigured.to_string(),
            "misconfigured"
        );
        assert_eq!(ProviderReadiness::RateLimited.to_string(), "rate_limited");
        assert_eq!(ProviderReadiness::Unavailable.to_string(), "unavailable");
    }

    #[test]
    fn search_provider_result_success() {
        let c = CandidateRecord {
            id: CandidateId::new("c1"),
            provider_id: ProviderId::new("p1"),
            source_url: "https://example.com/1.jpg".into(),
            thumbnail_url: None,
            title: None,
            page_url: None,
            dimensions: None,
        };
        let result = SearchProviderResult::success(ProviderId::new("p1"), vec![c]);
        assert!(!result.is_empty());
        assert!(result.failure.is_none());
    }

    #[test]
    fn search_provider_result_failure() {
        let result = SearchProviderResult::failure(
            ProviderId::new("p1"),
            SearchFailureCategory::RateLimited,
            "too many requests",
        );
        assert!(result.is_empty());
        assert_eq!(result.failure, Some(SearchFailureCategory::RateLimited));
    }

    #[test]
    fn search_failure_category_display() {
        assert_eq!(
            SearchFailureCategory::ProviderError.to_string(),
            "provider_error"
        );
        assert_eq!(SearchFailureCategory::Timeout.to_string(), "timeout");
        assert_eq!(
            SearchFailureCategory::RateLimited.to_string(),
            "rate_limited"
        );
        assert_eq!(
            SearchFailureCategory::UnnormalizableResponse.to_string(),
            "unnormalizable_response"
        );
        assert_eq!(
            SearchFailureCategory::EmptyResult.to_string(),
            "empty_result"
        );
        assert_eq!(
            SearchFailureCategory::Unavailable.to_string(),
            "unavailable"
        );
    }

    #[test]
    fn search_usage_event_builders() {
        let success = SearchUsageEvent::success_event(ProviderId::new("p1"), 10, 8, false, 2);
        assert_eq!(success.raw_candidate_count, 10);
        assert_eq!(success.deduped_candidate_count, 8);
        assert!(success.failure.is_none());

        let failure = SearchUsageEvent::failure_event(
            ProviderId::new("p2"),
            SearchFailureCategory::Timeout,
            ProviderReadiness::Unavailable,
            1,
        );
        assert_eq!(failure.raw_candidate_count, 0);
        assert!(failure.failure.is_some());
    }

    #[test]
    fn candidate_shortage_reason_display() {
        let r = CandidateShortageReason::NoAvailableProviders;
        assert!(r.to_string().contains("no providers available"));

        let r = CandidateShortageReason::AllProvidersExhausted;
        assert!(r.to_string().contains("exhausted"));

        let r = CandidateShortageReason::InsufficientUniqueCandidates {
            total_raw: 50,
            total_deduped: 30,
            duplicates_removed: 20,
        };
        assert!(r.to_string().contains("50"));
        assert!(r.to_string().contains("30"));
        assert!(r.to_string().contains("20"));
    }

    #[test]
    fn weight_entry_construction() {
        let entry = WeightEntry {
            provider_id: ProviderId::new("p1"),
            display_name: "Provider 1".into(),
            weight: 3,
        };
        assert_eq!(entry.weight, 3);
    }
}
