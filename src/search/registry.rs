//! Provider registry.
//!
//! Holds provider registration metadata and adapter references,
//! evaluates readiness, and builds the effective weight table
//! for the scheduler.
//!
//! v1.1: built from `RuntimeConfig.providers`, produces
//! `ProviderReadinessReport` for every configured provider.
//!
//! References: PRD FR-004, LLD §Search Provider Contract,
//! `docs/design/v1.1-TASK-002-search-provider-candidate-design.md`

use crate::domain::candidate::ProviderId;
use crate::domain::config::{RuntimeConfig, SearchProviderConfig, SearchProviderKind};
use crate::domain::search::{
    CredentialStatus, HealthCheckStatus, ProviderEvidence, ProviderFailureCode,
    ProviderReadinessReport, ProviderReadinessStatus, QuotaStatus, WeightedProviderEntry,
};
use crate::ports::BaseSearchProvider;
use std::collections::BTreeMap;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Provider registration record
// ---------------------------------------------------------------------------

/// Package-safe registration metadata for a configured provider.
#[derive(Debug, Clone)]
pub struct ProviderRegistration {
    pub provider_id: ProviderId,
    pub display_name: String,
    pub provider_kind: SearchProviderKind,
    pub enabled: bool,
    pub configured_weight: u32,
    pub config_fingerprint: String,
    pub fixture_only: bool,
}

// ---------------------------------------------------------------------------
// Provider registry
// ---------------------------------------------------------------------------

/// Holds registrations and adapters for all configured search providers.
#[derive(Clone)]
pub struct ProviderRegistry {
    /// Registration metadata indexed by provider id.
    pub registrations: BTreeMap<ProviderId, ProviderRegistration>,

    /// Adapter references indexed by provider id.
    pub adapters: BTreeMap<ProviderId, Arc<dyn BaseSearchProvider>>,
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ProviderRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            registrations: BTreeMap::new(),
            adapters: BTreeMap::new(),
        }
    }

    /// Register an adapter by provider id.
    pub fn register_adapter(
        &mut self,
        provider_id: ProviderId,
        adapter: Arc<dyn BaseSearchProvider>,
    ) {
        self.adapters.insert(provider_id.clone(), adapter);
        // Ensure a registration entry exists
        self.registrations
            .entry(provider_id.clone())
            .or_insert_with(|| ProviderRegistration {
                provider_id,
                display_name: String::new(),
                provider_kind: SearchProviderKind::Custom("unknown".into()),
                enabled: true,
                configured_weight: 1,
                config_fingerprint: String::new(),
                fixture_only: false,
            });
    }

    /// Build a registry from `RuntimeConfig.providers` and an adapter lookup function.
    ///
    /// The `adapter_for` function maps a `provider_kind` string to an optional
    /// adapter. In production, this is driven by the compiled adapter map.
    pub fn from_config<F>(config: &RuntimeConfig, adapter_for: F) -> Self
    where
        F: Fn(&SearchProviderKind) -> Option<Arc<dyn BaseSearchProvider>>,
    {
        let mut registry = Self::new();

        for provider_config in &config.providers {
            let provider_id = ProviderId::new(&provider_config.provider_id);
            let fixture_only = provider_config.provider_kind == SearchProviderKind::Fixture;

            let registration = ProviderRegistration {
                provider_id: provider_id.clone(),
                display_name: provider_config.provider_id.clone(),
                provider_kind: provider_config.provider_kind.clone(),
                enabled: provider_config.enabled,
                configured_weight: provider_config.weight,
                config_fingerprint: String::new(),
                fixture_only,
            };

            registry
                .registrations
                .insert(provider_id.clone(), registration);

            // Look up adapter
            if let Some(adapter) = adapter_for(&provider_config.provider_kind) {
                registry.adapters.insert(provider_id, adapter);
            }
            // If no adapter, the readiness report will show PROVIDER_ADAPTER_MISSING
        }

        registry
    }

    /// Evaluate readiness for all registered providers.
    ///
    /// Returns a readiness report for each configured provider.
    /// This includes providers that have no adapter, are disabled, etc.
    pub fn evaluate_readiness(&self) -> Vec<ProviderReadinessReport> {
        let mut reports = Vec::new();

        for (provider_id, registration) in &self.registrations {
            let report = if let Some(adapter) = self.adapters.get(provider_id) {
                // Build a minimal SearchProviderConfig for readiness check
                let config = SearchProviderConfig {
                    provider_id: provider_id.to_string(),
                    provider_kind: registration.provider_kind.clone(),
                    enabled: registration.enabled,
                    weight: registration.configured_weight,
                    endpoint: None,
                    credential_env: None,
                    default_query_params: BTreeMap::new(),
                };
                let mut report = adapter.readiness(&config);
                // Override with registration metadata
                report.provider_id = provider_id.clone();
                report.display_name = registration.display_name.clone();
                report.configured_weight = registration.configured_weight;

                // Enforce disabled / fixture / weight config rules
                if !registration.enabled {
                    report.status = ProviderReadinessStatus::Disabled;
                    report.available = false;
                    report.included_in_weight_table = false;
                    report.effective_weight = None;
                    report.failure_code = Some(ProviderFailureCode::ProviderDisabled);
                    report.evidence.push(ProviderEvidence {
                        code: "PROVIDER_DISABLED".into(),
                        message: format!("Provider '{}' is disabled in configuration", provider_id),
                        severity: "info".into(),
                    });
                } else if registration.configured_weight == 0 {
                    report.status = ProviderReadinessStatus::Misconfigured;
                    report.available = false;
                    report.included_in_weight_table = false;
                    report.effective_weight = None;
                    report.failure_code = Some(ProviderFailureCode::ProviderWeightInvalid);
                    report.evidence.push(ProviderEvidence {
                        code: "PROVIDER_WEIGHT_INVALID".into(),
                        message: format!(
                            "Provider '{}' has weight 0 and cannot be scheduled",
                            provider_id
                        ),
                        severity: "error".into(),
                    });
                } else if registration.fixture_only && !is_fixture_mode() {
                    report.status = ProviderReadinessStatus::FixtureOnly;
                    report.available = false;
                    report.included_in_weight_table = false;
                    report.effective_weight = None;
                    report.failure_code = Some(ProviderFailureCode::ProviderFixtureNotProduction);
                    report.evidence.push(ProviderEvidence {
                        code: "PROVIDER_FIXTURE_NOT_PRODUCTION".into(),
                        message: format!(
                            "Fixture provider '{}' cannot be used in production mode",
                            provider_id
                        ),
                        severity: "blocker".into(),
                    });
                } else if report.status.is_ready()
                    && registration.enabled
                    && registration.configured_weight > 0
                {
                    report.included_in_weight_table = true;
                    report.effective_weight = Some(registration.configured_weight);
                }

                report
            } else {
                // No adapter registered for this provider kind
                ProviderReadinessReport {
                    provider_id: provider_id.clone(),
                    provider_kind: registration.provider_kind.clone(),
                    display_name: registration.display_name.clone(),
                    status: ProviderReadinessStatus::Unavailable,
                    available: false,
                    included_in_weight_table: false,
                    configured_weight: if registration.enabled {
                        registration.configured_weight
                    } else {
                        0
                    },
                    effective_weight: None,
                    credential_status: CredentialStatus::NotRequired,
                    health_check_status: HealthCheckStatus::NotChecked,
                    quota_status: QuotaStatus::Unknown,
                    constraint_support: Default::default(),
                    failure_code: Some(ProviderFailureCode::ProviderAdapterMissing),
                    checked_at: String::new(),
                    evidence: vec![ProviderEvidence {
                        code: "PROVIDER_ADAPTER_MISSING".into(),
                        message: format!(
                            "No adapter registered for provider kind {:?} (provider '{}')",
                            registration.provider_kind, provider_id
                        ),
                        severity: "error".into(),
                    }],
                    redaction_applied: false,
                }
            };

            reports.push(report);
        }

        reports
    }

    /// Build the effective weight table from readiness reports.
    ///
    /// Only includes providers that are:
    /// 1. Status is Ready
    /// 2. Included in weight table
    /// 3. Have a positive effective weight
    pub fn build_weight_table(
        readiness_reports: &[ProviderReadinessReport],
    ) -> Vec<WeightedProviderEntry> {
        readiness_reports
            .iter()
            .filter(|r| r.included_in_weight_table && r.status.is_ready())
            .filter_map(|r| {
                r.effective_weight.and_then(|w| {
                    if w > 0 {
                        Some(WeightedProviderEntry {
                            provider_id: r.provider_id.clone(),
                            display_name: r.display_name.clone(),
                            effective_weight: w,
                            capabilities: r.constraint_support.clone(),
                        })
                    } else {
                        None
                    }
                })
            })
            .collect()
    }

    /// Compute the total weight of the effective weight table.
    pub fn total_weight(table: &[WeightedProviderEntry]) -> u32 {
        table.iter().map(|e| e.effective_weight).sum()
    }

    /// Return true if the weight table has at least one entry.
    pub fn has_available_providers(table: &[WeightedProviderEntry]) -> bool {
        !table.is_empty()
    }

    /// Look up an adapter by provider id.
    pub fn get_adapter(&self, provider_id: &ProviderId) -> Option<&Arc<dyn BaseSearchProvider>> {
        self.adapters.get(provider_id)
    }

    /// Return the number of registered providers.
    pub fn len(&self) -> usize {
        self.registrations.len()
    }

    /// Return true if no providers are registered.
    pub fn is_empty(&self) -> bool {
        self.registrations.is_empty()
    }

    // -----------------------------------------------------------------------
    // Legacy backward-compatible methods (for code not yet migrated)
    // -----------------------------------------------------------------------

    /// Legacy: register a provider using the old [`ProviderRegistration`] type.
    #[deprecated(note = "use `register_adapter` with BaseSearchProvider")]
    pub fn register(&mut self, registration: crate::domain::search::ProviderRegistration) {
        self.registrations.insert(
            registration.provider_id.clone(),
            ProviderRegistration {
                provider_id: registration.provider_id.clone(),
                display_name: registration.display_name.clone(),
                provider_kind: SearchProviderKind::Custom("legacy".into()),
                enabled: registration.enabled,
                configured_weight: registration.weight.max(0) as u32,
                config_fingerprint: String::new(),
                fixture_only: false,
            },
        );
    }

    /// Legacy: build weight table from enabled registrations (no adapters required).
    #[allow(deprecated)]
    #[deprecated(note = "use `evaluate_readiness` then `build_weight_table`")]
    pub fn build_weight_table_legacy(
        &self,
    ) -> (
        Vec<crate::domain::search::WeightEntry>,
        Vec<crate::domain::search::ProviderReadinessRecord>,
    ) {
        let mut weight_table: Vec<crate::domain::search::WeightEntry> = Vec::new();
        let mut readiness_records: Vec<crate::domain::search::ProviderReadinessRecord> = Vec::new();

        for reg in self.registrations.values() {
            let (readiness, included) = if !reg.enabled {
                (crate::domain::search::ProviderReadiness::Disabled, false)
            } else if reg.configured_weight == 0 {
                (
                    crate::domain::search::ProviderReadiness::Misconfigured,
                    false,
                )
            } else {
                (crate::domain::search::ProviderReadiness::Ready, true)
            };

            readiness_records.push(crate::domain::search::ProviderReadinessRecord {
                provider_id: reg.provider_id.clone(),
                display_name: reg.display_name.clone(),
                readiness,
                configured_weight: reg.configured_weight as i32,
                included_in_table: included,
            });

            if included {
                weight_table.push(crate::domain::search::WeightEntry {
                    provider_id: reg.provider_id.clone(),
                    display_name: reg.display_name.clone(),
                    weight: reg.configured_weight,
                });
            }
        }

        (weight_table, readiness_records)
    }

    /// Legacy: build weight table with readiness overrides.
    #[deprecated(note = "use `evaluate_readiness` then `build_weight_table`")]
    pub fn build_weight_table_with_readiness(
        &self,
        readiness_map: &std::collections::HashMap<String, crate::domain::search::ProviderReadiness>,
    ) -> (
        Vec<crate::domain::search::WeightEntry>,
        Vec<crate::domain::search::ProviderReadinessRecord>,
    ) {
        let mut weight_table: Vec<crate::domain::search::WeightEntry> = Vec::new();
        let mut readiness_records: Vec<crate::domain::search::ProviderReadinessRecord> = Vec::new();

        for reg in self.registrations.values() {
            let effective_readiness = readiness_map
                .get(&reg.provider_id.to_string())
                .cloned()
                .unwrap_or({
                    if !reg.enabled {
                        crate::domain::search::ProviderReadiness::Disabled
                    } else if reg.configured_weight == 0 {
                        crate::domain::search::ProviderReadiness::Misconfigured
                    } else {
                        crate::domain::search::ProviderReadiness::Ready
                    }
                });

            let included =
                effective_readiness.is_ready() && reg.enabled && reg.configured_weight > 0;

            readiness_records.push(crate::domain::search::ProviderReadinessRecord {
                provider_id: reg.provider_id.clone(),
                display_name: reg.display_name.clone(),
                readiness: effective_readiness,
                configured_weight: reg.configured_weight as i32,
                included_in_table: included,
            });

            if included {
                weight_table.push(crate::domain::search::WeightEntry {
                    provider_id: reg.provider_id.clone(),
                    display_name: reg.display_name.clone(),
                    weight: reg.configured_weight,
                });
            }
        }

        (weight_table, readiness_records)
    }

    /// Legacy: total weight sum.
    #[deprecated(note = "use `total_weight` with WeightedProviderEntry")]
    pub fn total_weight_legacy(table: &[crate::domain::search::WeightEntry]) -> u32 {
        table.iter().map(|e| e.weight).sum()
    }
}

/// Check whether we are running in fixture/test mode.
///
/// In production, fixture providers must be marked as
/// PROVIDER_FIXTURE_NOT_PRODUCTION and excluded from the weight table.
fn is_fixture_mode() -> bool {
    // Fixture mode is active when IMAGE_RETRIEVAL_FIXTURE_MODE=1
    // or when running under `cargo test`.
    // For now, detect test mode via the standard env var.
    std::env::var("IMAGE_RETRIEVAL_FIXTURE_MODE")
        .map(|v| v == "1")
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::config::RuntimeConfig;
    use crate::domain::search::ProviderConstraintSupport;

    /// A minimal stub adapter for registry tests.
    struct StubAdapter {
        id: ProviderId,
        name: String,
        kind: SearchProviderKind,
        ready: bool,
        credential_present: bool,
    }

    impl BaseSearchProvider for StubAdapter {
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
            ProviderConstraintSupport::default()
        }

        fn readiness(&self, _config: &SearchProviderConfig) -> ProviderReadinessReport {
            if self.ready {
                let mut report =
                    ProviderReadinessReport::ready(self.id.clone(), self.kind.clone(), &self.name);
                if !self.credential_present {
                    report.credential_status = CredentialStatus::Missing {
                        env_var: "SERPAPI_API_KEY".into(),
                    };
                    report.status = ProviderReadinessStatus::MissingCredentials;
                    report.available = false;
                    report.included_in_weight_table = false;
                    report.effective_weight = None;
                    report.failure_code = Some(ProviderFailureCode::ProviderCredentialMissing);
                    report.evidence.push(ProviderEvidence {
                        code: "PROVIDER_CREDENTIAL_MISSING".into(),
                        message: "SERPAPI_API_KEY is not set".into(),
                        severity: "blocker".into(),
                    });
                }
                report
            } else {
                ProviderReadinessReport::not_ready(
                    self.id.clone(),
                    self.kind.clone(),
                    &self.name,
                    ProviderReadinessStatus::Unavailable,
                    ProviderFailureCode::ProviderUnavailable,
                    vec![ProviderEvidence {
                        code: "HEALTH_FAILED".into(),
                        message: "stub health check failed".into(),
                        severity: "error".into(),
                    }],
                )
            }
        }

        fn search(
            &self,
            _request: &crate::domain::search::SearchRequest,
        ) -> std::result::Result<
            crate::domain::search::SearchResponse,
            crate::domain::search::SearchError,
        > {
            unimplemented!("stub adapter does not implement search")
        }
    }

    fn make_config() -> RuntimeConfig {
        let json = r#"{
            "providers": [
                {
                    "provider_id": "serpapi",
                    "provider_kind": "serpapi_google_images",
                    "enabled": true,
                    "weight": 100,
                    "credential_env": "SERPAPI_API_KEY"
                },
                {
                    "provider_id": "fixture_p",
                    "provider_kind": "fixture",
                    "enabled": true,
                    "weight": 1
                },
                {
                    "provider_id": "disabled_p",
                    "provider_kind": "serpapi_google_images",
                    "enabled": false,
                    "weight": 1
                }
            ]
        }"#;
        serde_json::from_str(json).expect("deserialize test config")
    }

    #[test]
    fn registry_empty_by_default() {
        let registry = ProviderRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn registry_builds_from_config() {
        let config = make_config();
        let adapter = Arc::new(StubAdapter {
            id: ProviderId::new("serpapi"),
            name: "SerpApi".into(),
            kind: SearchProviderKind::SerpapiGoogleImages,
            ready: true,
            credential_present: true,
        });

        let registry = ProviderRegistry::from_config(&config, |kind| match kind {
            SearchProviderKind::SerpapiGoogleImages => Some(adapter.clone()),
            _ => None,
        });

        assert_eq!(registry.len(), 3);
        // serpapi should have an adapter
        assert!(registry.get_adapter(&ProviderId::new("serpapi")).is_some());
        // fixture_p should not have an adapter
        assert!(registry
            .get_adapter(&ProviderId::new("fixture_p"))
            .is_none());
    }

    #[test]
    fn readiness_reports_for_all_providers() {
        let config = make_config();
        let adapter = Arc::new(StubAdapter {
            id: ProviderId::new("serpapi"),
            name: "SerpApi".into(),
            kind: SearchProviderKind::SerpapiGoogleImages,
            ready: true,
            credential_present: true,
        });

        let registry = ProviderRegistry::from_config(&config, |kind| match kind {
            SearchProviderKind::SerpapiGoogleImages => Some(adapter.clone()),
            _ => None,
        });

        let reports = registry.evaluate_readiness();
        assert_eq!(reports.len(), 3);

        // serpapi: ready
        let serpapi_report = reports
            .iter()
            .find(|r| r.provider_id.to_string() == "serpapi")
            .unwrap();
        assert_eq!(serpapi_report.status, ProviderReadinessStatus::Ready);
        assert!(serpapi_report.available);
        assert!(serpapi_report.included_in_weight_table);

        // fixture_p: fixture only, no adapter → adapter missing
        let fixture_report = reports
            .iter()
            .find(|r| r.provider_id.to_string() == "fixture_p")
            .unwrap();
        assert!(!fixture_report.available);
        assert!(!fixture_report.included_in_weight_table);

        // disabled_p: disabled
        let disabled_report = reports
            .iter()
            .find(|r| r.provider_id.to_string() == "disabled_p")
            .unwrap();
        assert!(!disabled_report.available);
    }

    #[test]
    fn readiness_credential_missing() {
        let config = make_config();
        let adapter = Arc::new(StubAdapter {
            id: ProviderId::new("serpapi"),
            name: "SerpApi".into(),
            kind: SearchProviderKind::SerpapiGoogleImages,
            ready: true,
            credential_present: false, // credential missing!
        });

        let registry = ProviderRegistry::from_config(&config, |kind| match kind {
            SearchProviderKind::SerpapiGoogleImages => Some(adapter.clone()),
            _ => None,
        });

        let reports = registry.evaluate_readiness();
        let serpapi_report = reports
            .iter()
            .find(|r| r.provider_id.to_string() == "serpapi")
            .unwrap();

        assert_eq!(
            serpapi_report.status,
            ProviderReadinessStatus::MissingCredentials
        );
        assert!(!serpapi_report.available);
        assert!(!serpapi_report.included_in_weight_table);
        assert_eq!(
            serpapi_report.failure_code,
            Some(ProviderFailureCode::ProviderCredentialMissing)
        );
    }

    #[test]
    fn weight_table_only_includes_ready_providers() {
        let config = make_config();
        let serpapi_adapter = Arc::new(StubAdapter {
            id: ProviderId::new("serpapi"),
            name: "SerpApi".into(),
            kind: SearchProviderKind::SerpapiGoogleImages,
            ready: true,
            credential_present: true,
        });
        let disabled_adapter = Arc::new(StubAdapter {
            id: ProviderId::new("disabled_p"),
            name: "Disabled".into(),
            kind: SearchProviderKind::SerpapiGoogleImages,
            ready: true,
            credential_present: true,
        });

        let registry = ProviderRegistry::from_config(&config, |kind| match kind {
            SearchProviderKind::SerpapiGoogleImages => Some(serpapi_adapter.clone()),
            _ => None,
        });
        // Also register adapter for disabled_p
        let mut registry = registry;
        registry.register_adapter(ProviderId::new("disabled_p"), disabled_adapter);

        let reports = registry.evaluate_readiness();
        let table = ProviderRegistry::build_weight_table(&reports);

        // Only serpapi should be in the table (enabled, ready, positive weight)
        assert_eq!(table.len(), 1);
        assert_eq!(table[0].provider_id.to_string(), "serpapi");
        assert_eq!(table[0].effective_weight, 100);
    }

    #[test]
    fn weight_table_empty_when_no_ready_providers() {
        let registry = ProviderRegistry::new();
        let reports = registry.evaluate_readiness();
        let table = ProviderRegistry::build_weight_table(&reports);
        assert!(table.is_empty());
        assert!(!ProviderRegistry::has_available_providers(&table));
    }

    #[test]
    fn total_weight_sums_correctly() {
        let entries = vec![
            WeightedProviderEntry {
                provider_id: ProviderId::new("a"),
                display_name: "A".into(),
                effective_weight: 30,
                capabilities: ProviderConstraintSupport::default(),
            },
            WeightedProviderEntry {
                provider_id: ProviderId::new("b"),
                display_name: "B".into(),
                effective_weight: 70,
                capabilities: ProviderConstraintSupport::default(),
            },
        ];
        assert_eq!(ProviderRegistry::total_weight(&entries), 100);
    }

    #[test]
    fn register_adapter_creates_registration() {
        let mut registry = ProviderRegistry::new();
        let adapter = Arc::new(StubAdapter {
            id: ProviderId::new("p1"),
            name: "P1".into(),
            kind: SearchProviderKind::SerpapiGoogleImages,
            ready: true,
            credential_present: true,
        });
        registry.register_adapter(ProviderId::new("p1"), adapter);
        assert_eq!(registry.len(), 1);
        assert!(registry.get_adapter(&ProviderId::new("p1")).is_some());
    }
}
