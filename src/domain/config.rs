//! Runtime configuration domain types.
//!
//! Provides serde-ready DTOs for provider, retrieval channel, Qwen 3.5 VLM,
//! policy, quality, output, and execution-limit configuration.
//!
//! **Security**: Configuration stores environment variable *names* only.
//! Resolved credential values must never be serialized into these DTOs.
//!
//! References: PRD FR-002/FR-004/FR-014, HLD §Config, LLD §Configuration Model,
//! `docs/design/v1.1-TASK-001-queryplan-config-policy-design.md`

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ---------------------------------------------------------------------------
// Runtime config root
// ---------------------------------------------------------------------------

/// Top-level runtime configuration loaded from TOML/JSON.
///
/// All fields default so a minimal config file is accepted. Missing
/// provider credentials, disabled channels, and unavailable VLM produce
/// readiness diagnostics rather than parse errors.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RuntimeConfig {
    /// Configured search providers.
    #[serde(default)]
    pub providers: Vec<SearchProviderConfig>,

    /// Configured retrieval channels.
    #[serde(default)]
    pub retrieval_channels: Vec<RetrievalChannelConfig>,

    /// VLM evaluation provider configuration.
    #[serde(default)]
    pub vlm_evaluation: VlmEvaluationConfig,

    /// Policy configuration.
    #[serde(default)]
    pub policy: PolicyConfig,

    /// Quality default overrides.
    #[serde(default)]
    pub quality_defaults: QualityDefaultsConfig,

    /// Output configuration.
    #[serde(default)]
    pub output: OutputConfig,

    /// Execution limits.
    #[serde(default)]
    pub limits: ExecutionLimitsConfig,
}

// ---------------------------------------------------------------------------
// Search provider config
// ---------------------------------------------------------------------------

/// Kind of search provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SearchProviderKind {
    /// SerpApi Google Images — default v1.1 broad provider.
    #[serde(rename = "serpapi_google_images")]
    SerpapiGoogleImages,

    /// A custom/other provider identified by string tag.
    #[serde(rename = "custom")]
    Custom(String),

    /// Fixture provider — test-only, must not satisfy production readiness.
    #[serde(rename = "fixture")]
    Fixture,
}

/// Configuration for a single search provider.
///
/// Credentials are referenced by environment variable name only.
/// Resolved values must never be serialized into this DTO.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchProviderConfig {
    /// Unique provider id within this configuration.
    pub provider_id: String,

    /// The kind of provider.
    pub provider_kind: SearchProviderKind,

    /// Whether this provider is enabled.
    #[serde(default)]
    pub enabled: bool,

    /// Scheduling weight (must be > 0 when enabled).
    #[serde(default = "default_weight")]
    pub weight: u32,

    /// API endpoint URL, if applicable.
    #[serde(default)]
    pub endpoint: Option<String>,

    /// Name of the environment variable holding the credential.
    /// This is configuration metadata and *can* be serialized.
    #[serde(default)]
    pub credential_env: Option<String>,

    /// Additional query parameters appended to every request.
    #[serde(default)]
    pub default_query_params: BTreeMap<String, String>,
}

fn default_weight() -> u32 {
    1
}

impl SearchProviderConfig {
    /// Default v1.1 SerpApi Google Images provider.
    pub fn default_serpapi() -> Self {
        Self {
            provider_id: "serpapi_google_images".into(),
            provider_kind: SearchProviderKind::SerpapiGoogleImages,
            enabled: false, // disabled until credential is confirmed
            weight: default_weight(),
            endpoint: Some("https://serpapi.com/search".into()),
            credential_env: Some("SERPAPI_API_KEY".into()),
            default_query_params: {
                let mut params = BTreeMap::new();
                params.insert("engine".into(), "google_images".into());
                params
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Retrieval channel config
// ---------------------------------------------------------------------------

/// Kind of retrieval channel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RetrievalChannelKind {
    /// Normal web fetch (direct image + source-page resolve).
    #[serde(rename = "normal_web_fetch")]
    NormalWebFetch,

    /// Self-hosted retrieval service.
    #[serde(rename = "self_hosted_service")]
    SelfHostedService,

    /// Paid online retrieval service.
    #[serde(rename = "paid_online_service")]
    PaidOnlineService,

    /// Custom channel identified by string tag.
    #[serde(rename = "custom")]
    Custom(String),

    /// Fixture channel — test-only.
    #[serde(rename = "fixture")]
    Fixture,
}

/// Configuration for a single retrieval channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalChannelConfig {
    /// Unique channel id within this configuration.
    pub channel_id: String,

    /// The kind of channel.
    pub channel_kind: RetrievalChannelKind,

    /// The tier for fallback ordering.
    pub tier: crate::domain::retrieval::RetrievalChannelTier,

    /// Whether this channel is enabled.
    #[serde(default)]
    pub enabled: bool,

    /// API endpoint URL, if applicable.
    #[serde(default)]
    pub endpoint: Option<String>,

    /// Name of the environment variable holding the credential.
    #[serde(default)]
    pub credential_env: Option<String>,

    /// Maximum batch size for this channel.
    #[serde(default)]
    pub max_batch_size: Option<u32>,
}

// ---------------------------------------------------------------------------
// VLM evaluation config
// ---------------------------------------------------------------------------

/// Kind of VLM evaluator.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum VlmEvaluatorKind {
    /// Qwen 3.5 VLM — production subjective evaluation.
    #[default]
    #[serde(rename = "qwen_3_5_vlm")]
    Qwen35Vlm,

    /// Fixture evaluator — test-only.
    #[serde(rename = "fixture")]
    Fixture,

    /// Custom evaluator identified by string tag.
    #[serde(rename = "custom")]
    Custom(String),
}

/// Configuration for the VLM evaluation provider.
///
/// Production defaults: provider_id = "qwen_3_5_vlm", kind = Qwen35Vlm,
/// model = "qwen3-vl-plus", credential_env = "QWEN_API_KEY".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VlmEvaluationConfig {
    /// Provider id.
    #[serde(default = "default_vlm_provider_id")]
    pub provider_id: String,

    /// The kind of evaluator.
    #[serde(default)]
    pub provider_kind: VlmEvaluatorKind,

    /// Model name/identifier.
    #[serde(default = "default_vlm_model")]
    pub model: String,

    /// Whether this evaluator is enabled.
    #[serde(default)]
    pub enabled: bool,

    /// Base URL for the API.
    #[serde(default)]
    pub base_url: Option<String>,

    /// Specific endpoint path.
    #[serde(default)]
    pub endpoint: Option<String>,

    /// Name of the environment variable holding the API token.
    #[serde(default = "default_vlm_credential_env")]
    pub credential_env: Option<String>,

    /// Prompt template for candidate evaluation.
    #[serde(default)]
    pub candidate_prompt_template: Option<String>,

    /// Prompt template for image evaluation.
    #[serde(default)]
    pub image_prompt_template: Option<String>,

    /// Request timeout in seconds.
    #[serde(default)]
    pub timeout_seconds: Option<u64>,

    /// Fixture mode — allowed only in tests or explicit fixture runs.
    /// Must not produce production package evidence.
    #[serde(default)]
    pub fixture_mode: bool,
}

fn default_vlm_provider_id() -> String {
    "qwen_3_5_vlm".into()
}

fn default_vlm_model() -> String {
    "qwen3-vl-plus".into()
}

fn default_vlm_credential_env() -> Option<String> {
    Some("QWEN_API_KEY".into())
}

impl Default for VlmEvaluationConfig {
    fn default() -> Self {
        Self {
            provider_id: default_vlm_provider_id(),
            provider_kind: VlmEvaluatorKind::Qwen35Vlm,
            model: default_vlm_model(),
            enabled: false,
            base_url: None,
            endpoint: None,
            credential_env: default_vlm_credential_env(),
            candidate_prompt_template: None,
            image_prompt_template: None,
            timeout_seconds: None,
            fixture_mode: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Policy config
// ---------------------------------------------------------------------------

/// Behaviour when robots/site-rule posture is unknown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RobotsUnknownBehavior {
    /// Warn but allow the request.
    #[default]
    #[serde(rename = "warn")]
    Warn,

    /// Block the request.
    #[serde(rename = "block")]
    Block,
}

/// Configuration for output redaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedactionConfig {
    /// Additional patterns to redact beyond the built-in set.
    #[serde(default)]
    pub extra_patterns: Vec<String>,

    /// Whether to redact query text fields in diagnostics.
    #[serde(default = "default_true")]
    pub redact_query_texts: bool,

    /// Whether to redact provider evidence fields.
    #[serde(default = "default_true")]
    pub redact_provider_evidence: bool,
}

fn default_true() -> bool {
    true
}

impl Default for RedactionConfig {
    fn default() -> Self {
        Self {
            extra_patterns: Vec::new(),
            redact_query_texts: true,
            redact_provider_evidence: true,
        }
    }
}

/// Runtime policy configuration.
///
/// QueryPlan policy can narrow but never broaden these settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyConfig {
    /// Whether paid retrieval channels are allowed at all.
    #[serde(default)]
    pub allow_paid_channels: bool,

    /// Optional budget limit for paid channels (decimal string).
    #[serde(default)]
    pub paid_budget_limit: Option<String>,

    /// Whether to respect robots.txt and site rules.
    #[serde(default = "default_true")]
    pub respect_robots: bool,

    /// Behaviour when robots/site-rule posture cannot be determined.
    #[serde(default)]
    pub robots_unknown_behavior: RobotsUnknownBehavior,

    /// Whether login-required sources can be accessed.
    #[serde(default)]
    pub allow_login_required_sources: bool,

    /// Whether paywalled sources can be accessed.
    #[serde(default)]
    pub allow_paywalled_sources: bool,

    /// Domains explicitly prohibited.
    #[serde(default)]
    pub prohibited_domains: Vec<String>,

    /// Redaction configuration.
    #[serde(default)]
    pub sensitive_redaction: RedactionConfig,
}

impl Default for PolicyConfig {
    fn default() -> Self {
        Self {
            allow_paid_channels: false,
            paid_budget_limit: None,
            respect_robots: true,
            robots_unknown_behavior: RobotsUnknownBehavior::default(),
            allow_login_required_sources: false,
            allow_paywalled_sources: false,
            prohibited_domains: Vec::new(),
            sensitive_redaction: RedactionConfig::default(),
        }
    }
}

// ---------------------------------------------------------------------------
// Quality defaults config
// ---------------------------------------------------------------------------

/// Configurable quality defaults that override tier-derived values.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct QualityDefaultsConfig {
    /// Default minimum width for `high` tier.
    #[serde(default)]
    pub high_minimum_width: Option<u32>,

    /// Default minimum height for `high` tier.
    #[serde(default)]
    pub high_minimum_height: Option<u32>,

    /// Default minimum width for `strict` tier.
    #[serde(default)]
    pub strict_minimum_width: Option<u32>,

    /// Default minimum height for `strict` tier.
    #[serde(default)]
    pub strict_minimum_height: Option<u32>,

    /// Default minimum visual relevance score for `high` tier.
    #[serde(default)]
    pub high_min_relevance_score: Option<f32>,

    /// Default minimum visual relevance score for `strict` tier.
    #[serde(default)]
    pub strict_min_relevance_score: Option<f32>,
}

// ---------------------------------------------------------------------------
// Output config
// ---------------------------------------------------------------------------

/// Output delivery configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputConfig {
    /// Default output directory for packages.
    #[serde(default)]
    pub default_output_dir: Option<String>,

    /// Whether to write diagnostics to a separate file.
    #[serde(default = "default_true")]
    pub write_diagnostics: bool,

    /// Maximum package size in bytes (soft limit).
    #[serde(default)]
    pub max_package_size_bytes: Option<u64>,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            default_output_dir: None,
            write_diagnostics: true,
            max_package_size_bytes: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Execution limits config
// ---------------------------------------------------------------------------

/// Hard execution limits.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionLimitsConfig {
    /// Maximum allowed `required_image_count`.
    #[serde(default = "default_max_required_image_count")]
    pub max_required_image_count: u32,

    /// Maximum allowed retry limit (must be ≤ 3 per constitution).
    #[serde(default = "default_max_retry_limit")]
    pub max_retry_limit: u8,

    /// Maximum candidate target (soft bound).
    #[serde(default)]
    pub max_candidate_target: Option<u32>,

    /// Maximum retrieval batch size.
    #[serde(default)]
    pub max_retrieval_batch_size: Option<u32>,

    /// Search request timeout in seconds.
    #[serde(default)]
    pub search_timeout_seconds: Option<u64>,

    /// Retrieval request timeout in seconds.
    #[serde(default)]
    pub retrieval_timeout_seconds: Option<u64>,

    /// VLM request timeout in seconds.
    #[serde(default)]
    pub vlm_timeout_seconds: Option<u64>,
}

fn default_max_required_image_count() -> u32 {
    1000
}

fn default_max_retry_limit() -> u8 {
    3
}

impl Default for ExecutionLimitsConfig {
    fn default() -> Self {
        Self {
            max_required_image_count: default_max_required_image_count(),
            max_retry_limit: default_max_retry_limit(),
            max_candidate_target: None,
            max_retrieval_batch_size: None,
            search_timeout_seconds: None,
            retrieval_timeout_seconds: None,
            vlm_timeout_seconds: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Config readiness helpers
// ---------------------------------------------------------------------------

/// Readiness check result for a provider, channel, or evaluator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigReadiness {
    /// The component identifier.
    pub component_id: String,

    /// Whether the component is ready.
    pub ready: bool,

    /// Reason for non-readiness, if applicable.
    pub reason: Option<String>,

    /// Failure code for machine consumption.
    pub failure_code: Option<String>,
}

impl ConfigReadiness {
    pub fn ready(component_id: impl Into<String>) -> Self {
        Self {
            component_id: component_id.into(),
            ready: true,
            reason: None,
            failure_code: None,
        }
    }

    pub fn not_ready(
        component_id: impl Into<String>,
        reason: impl Into<String>,
        failure_code: impl Into<String>,
    ) -> Self {
        Self {
            component_id: component_id.into(),
            ready: false,
            reason: Some(reason.into()),
            failure_code: Some(failure_code.into()),
        }
    }
}

/// Aggregate readiness report for all configured components.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigReadinessReport {
    /// Individual component readiness results.
    pub items: Vec<ConfigReadiness>,

    /// Whether all components are ready.
    pub all_ready: bool,

    /// Blocking readiness failures.
    pub blockers: Vec<ConfigReadiness>,
}

impl ConfigReadinessReport {
    pub fn new(items: Vec<ConfigReadiness>) -> Self {
        let blockers: Vec<ConfigReadiness> = items.iter().filter(|r| !r.ready).cloned().collect();
        let all_ready = blockers.is_empty();
        Self {
            items,
            all_ready,
            blockers,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // Default value tests
    // =========================================================================

    #[test]
    fn runtime_config_defaults() {
        let config = RuntimeConfig::default();
        assert!(config.providers.is_empty());
        assert!(config.retrieval_channels.is_empty());
        assert!(!config.vlm_evaluation.enabled);
        assert!(!config.policy.allow_paid_channels);
        assert!(config.policy.respect_robots);
        assert_eq!(
            config.policy.robots_unknown_behavior,
            RobotsUnknownBehavior::Warn
        );
        assert_eq!(config.limits.max_required_image_count, 1000);
        assert_eq!(config.limits.max_retry_limit, 3);
    }

    #[test]
    fn search_provider_default_weight_is_1() {
        assert_eq!(default_weight(), 1);
    }

    #[test]
    fn serpapi_default_has_correct_values() {
        let provider = SearchProviderConfig::default_serpapi();
        assert_eq!(provider.provider_id, "serpapi_google_images");
        assert_eq!(
            provider.provider_kind,
            SearchProviderKind::SerpapiGoogleImages
        );
        assert!(!provider.enabled);
        assert_eq!(provider.credential_env, Some("SERPAPI_API_KEY".into()));
        assert_eq!(provider.endpoint, Some("https://serpapi.com/search".into()));
        assert_eq!(
            provider.default_query_params.get("engine"),
            Some(&"google_images".to_string())
        );
    }

    #[test]
    fn vlm_evaluation_defaults() {
        let config = VlmEvaluationConfig::default();
        assert_eq!(config.provider_id, "qwen_3_5_vlm");
        assert_eq!(config.provider_kind, VlmEvaluatorKind::Qwen35Vlm);
        assert_eq!(config.model, "qwen3-vl-plus");
        assert_eq!(config.credential_env, Some("QWEN_API_KEY".into()));
        assert!(!config.enabled);
        assert!(!config.fixture_mode);
    }

    #[test]
    fn policy_config_paid_disabled_by_default() {
        let config = PolicyConfig::default();
        assert!(!config.allow_paid_channels);
        assert!(config.paid_budget_limit.is_none());
    }

    #[test]
    fn policy_config_robots_warn_by_default() {
        let config = PolicyConfig::default();
        assert_eq!(config.robots_unknown_behavior, RobotsUnknownBehavior::Warn);
    }

    #[test]
    fn policy_config_login_paywall_disabled_by_default() {
        let config = PolicyConfig::default();
        assert!(!config.allow_login_required_sources);
        assert!(!config.allow_paywalled_sources);
    }

    #[test]
    fn quality_defaults_are_all_none() {
        let config = QualityDefaultsConfig::default();
        assert!(config.high_minimum_width.is_none());
        assert!(config.high_minimum_height.is_none());
        assert!(config.strict_minimum_width.is_none());
        assert!(config.strict_minimum_height.is_none());
    }

    #[test]
    fn output_config_write_diagnostics_default_true() {
        let config = OutputConfig::default();
        assert!(config.write_diagnostics);
    }

    #[test]
    fn execution_limits_max_required_is_1000() {
        assert_eq!(default_max_required_image_count(), 1000);
    }

    #[test]
    fn execution_limits_max_retry_is_3() {
        assert_eq!(default_max_retry_limit(), 3);
    }

    // =========================================================================
    // Config readiness tests
    // =========================================================================

    #[test]
    fn readiness_ready_component() {
        let r = ConfigReadiness::ready("serpapi");
        assert!(r.ready);
        assert!(r.reason.is_none());
    }

    #[test]
    fn readiness_not_ready_component() {
        let r = ConfigReadiness::not_ready(
            "serpapi",
            "SERPAPI_API_KEY not set",
            "CONFIG_CREDENTIAL_ENV_MISSING",
        );
        assert!(!r.ready);
        assert_eq!(r.reason, Some("SERPAPI_API_KEY not set".into()));
        assert_eq!(r.failure_code, Some("CONFIG_CREDENTIAL_ENV_MISSING".into()));
    }

    #[test]
    fn readiness_report_all_ready() {
        let items = vec![
            ConfigReadiness::ready("serpapi"),
            ConfigReadiness::ready("normal_web_fetch"),
        ];
        let report = ConfigReadinessReport::new(items);
        assert!(report.all_ready);
        assert!(report.blockers.is_empty());
    }

    #[test]
    fn readiness_report_with_blockers() {
        let items = vec![
            ConfigReadiness::ready("serpapi"),
            ConfigReadiness::not_ready("qwen", "QWEN_API_KEY not set", "MISSING"),
        ];
        let report = ConfigReadinessReport::new(items);
        assert!(!report.all_ready);
        assert_eq!(report.blockers.len(), 1);
    }

    // =========================================================================
    // Serde round-trip tests
    // =========================================================================

    #[test]
    fn search_provider_kind_serde() {
        let kind = SearchProviderKind::SerpapiGoogleImages;
        let json = serde_json::to_string(&kind).unwrap();
        assert_eq!(json, "\"serpapi_google_images\"");
        let round: SearchProviderKind = serde_json::from_str(&json).unwrap();
        assert_eq!(round, kind);
    }

    #[test]
    fn vlm_evaluator_kind_serde() {
        let kind = VlmEvaluatorKind::Qwen35Vlm;
        let json = serde_json::to_string(&kind).unwrap();
        assert_eq!(json, "\"qwen_3_5_vlm\"");
        let round: VlmEvaluatorKind = serde_json::from_str(&json).unwrap();
        assert_eq!(round, kind);
    }

    #[test]
    fn robots_unknown_behavior_serde() {
        assert_eq!(
            serde_json::to_string(&RobotsUnknownBehavior::Warn).unwrap(),
            "\"warn\""
        );
        assert_eq!(
            serde_json::to_string(&RobotsUnknownBehavior::Block).unwrap(),
            "\"block\""
        );
    }

    #[test]
    fn retrieval_channel_kind_serde() {
        assert_eq!(
            serde_json::to_string(&RetrievalChannelKind::NormalWebFetch).unwrap(),
            "\"normal_web_fetch\""
        );
        assert_eq!(
            serde_json::to_string(&RetrievalChannelKind::PaidOnlineService).unwrap(),
            "\"paid_online_service\""
        );
    }

    // =========================================================================
    // Config builder / integration tests
    // =========================================================================

    #[test]
    fn full_runtime_config_from_json() {
        let json = r#"{
            "providers": [
                {
                    "provider_id": "serpapi",
                    "provider_kind": "serpapi_google_images",
                    "enabled": true,
                    "weight": 2,
                    "credential_env": "SERPAPI_API_KEY"
                }
            ],
            "retrieval_channels": [
                {
                    "channel_id": "web_fetch",
                    "channel_kind": "normal_web_fetch",
                    "tier": "web_fetch",
                    "enabled": true
                }
            ],
            "vlm_evaluation": {
                "enabled": true,
                "base_url": "https://api.example.com/v1",
                "credential_env": "QWEN_API_KEY"
            },
            "policy": {
                "allow_paid_channels": false,
                "respect_robots": true
            },
            "limits": {
                "max_required_image_count": 50,
                "max_retry_limit": 3
            }
        }"#;

        let config: RuntimeConfig = serde_json::from_str(json).expect("deserialize RuntimeConfig");
        assert_eq!(config.providers.len(), 1);
        assert_eq!(config.providers[0].provider_id, "serpapi");
        assert!(config.providers[0].enabled);
        assert_eq!(config.providers[0].weight, 2);
        assert_eq!(config.retrieval_channels.len(), 1);
        assert!(config.vlm_evaluation.enabled);
        assert_eq!(config.limits.max_required_image_count, 50);
    }

    #[test]
    fn minimal_config_accepted() {
        let json = "{}";
        let config: RuntimeConfig = serde_json::from_str(json).expect("deserialize empty");
        assert!(config.providers.is_empty());
        assert!(config.retrieval_channels.is_empty());
        assert!(!config.vlm_evaluation.enabled);
    }

    #[test]
    fn config_credential_env_is_stored_not_resolved() {
        // The config stores the env var NAME, never the value.
        let provider = SearchProviderConfig {
            provider_id: "test".into(),
            provider_kind: SearchProviderKind::Custom("test".into()),
            enabled: true,
            weight: 1,
            endpoint: None,
            credential_env: Some("MY_SECRET_KEY".into()),
            default_query_params: BTreeMap::new(),
        };
        let json = serde_json::to_string(&provider).unwrap();
        // The env var name is present; no resolved secret value exists
        assert!(json.contains("MY_SECRET_KEY"));
        assert!(!json.contains("actual-secret-value"));
    }
}
