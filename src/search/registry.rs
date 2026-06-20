//! Provider registry.
//!
//! Holds provider registrations, evaluates readiness, and builds the
//! effective weight table for the scheduler.
//!
//! The registry does NOT own provider adapters — it only owns their
//! registration metadata. Readiness is checked via the `BaseProvider`
//! trait; the registry collects the results.

use crate::domain::candidate::ProviderId;
use crate::domain::search::{
    ProviderReadiness, ProviderReadinessRecord, ProviderRegistration, WeightEntry,
};
use std::collections::HashMap;

/// Holds registrations for all known search providers and provides
/// readiness evaluation and weight-table construction.
#[derive(Debug, Clone, Default)]
pub struct ProviderRegistry {
    /// All registered providers, keyed by provider id.
    providers: HashMap<String, ProviderRegistration>,
}

impl ProviderRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
        }
    }

    /// Register a provider.
    ///
    /// Overwrites any existing registration with the same id.
    pub fn register(&mut self, registration: ProviderRegistration) {
        self.providers
            .insert(registration.provider_id.to_string(), registration);
    }

    /// Remove a provider from the registry.
    pub fn remove(&mut self, provider_id: &ProviderId) -> Option<ProviderRegistration> {
        self.providers.remove(&provider_id.to_string())
    }

    /// Return the number of registered providers.
    pub fn len(&self) -> usize {
        self.providers.len()
    }

    /// Return `true` if the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }

    /// Look up a provider registration by id.
    pub fn get(&self, provider_id: &ProviderId) -> Option<&ProviderRegistration> {
        self.providers.get(&provider_id.to_string())
    }

    /// Return an iterator over all registered providers.
    pub fn iter(&self) -> impl Iterator<Item = &ProviderRegistration> {
        self.providers.values()
    }

    /// Build the effective weight table from enabled providers.
    ///
    /// The weight table includes only providers that are:
    /// 1. Enabled (`registration.enabled == true`).
    /// 2. Have a positive weight (`registration.weight > 0`).
    ///
    /// Providers with missing, non-numeric, zero, or negative weights are
    /// diagnosed and excluded from the table.
    ///
    /// Returns the weight table and a readiness summary for all registered
    /// providers.
    pub fn build_weight_table(&self) -> (Vec<WeightEntry>, Vec<ProviderReadinessRecord>) {
        let mut weight_table: Vec<WeightEntry> = Vec::new();
        let mut readiness_records: Vec<ProviderReadinessRecord> = Vec::new();

        for reg in self.providers.values() {
            let (readiness, included) = if !reg.enabled {
                (ProviderReadiness::Disabled, false)
            } else if reg.weight <= 0 {
                (ProviderReadiness::Misconfigured, false)
            } else {
                (ProviderReadiness::Ready, true)
            };

            readiness_records.push(ProviderReadinessRecord {
                provider_id: reg.provider_id.clone(),
                display_name: reg.display_name.clone(),
                readiness,
                configured_weight: reg.weight,
                included_in_table: included,
            });

            if included {
                weight_table.push(WeightEntry {
                    provider_id: reg.provider_id.clone(),
                    display_name: reg.display_name.clone(),
                    weight: reg.weight as u32,
                });
            }
        }

        (weight_table, readiness_records)
    }

    /// Build a weight table with explicit readiness overrides.
    ///
    /// Providers whose readiness is NOT `Ready` are excluded from the table
    /// even if their registration is enabled and weight is positive.
    /// This is the method the scheduler uses after performing actual
    /// readiness checks via `BaseProvider::readiness()`.
    pub fn build_weight_table_with_readiness(
        &self,
        readiness_map: &HashMap<String, ProviderReadiness>,
    ) -> (Vec<WeightEntry>, Vec<ProviderReadinessRecord>) {
        let mut weight_table: Vec<WeightEntry> = Vec::new();
        let mut readiness_records: Vec<ProviderReadinessRecord> = Vec::new();

        for reg in self.providers.values() {
            let effective_readiness = readiness_map
                .get(&reg.provider_id.to_string())
                .cloned()
                .unwrap_or({
                    if !reg.enabled {
                        ProviderReadiness::Disabled
                    } else if reg.weight <= 0 {
                        ProviderReadiness::Misconfigured
                    } else {
                        ProviderReadiness::Ready
                    }
                });

            let included =
                effective_readiness == ProviderReadiness::Ready && reg.enabled && reg.weight > 0;

            readiness_records.push(ProviderReadinessRecord {
                provider_id: reg.provider_id.clone(),
                display_name: reg.display_name.clone(),
                readiness: effective_readiness,
                configured_weight: reg.weight,
                included_in_table: included,
            });

            if included {
                weight_table.push(WeightEntry {
                    provider_id: reg.provider_id.clone(),
                    display_name: reg.display_name.clone(),
                    weight: reg.weight as u32,
                });
            }
        }

        (weight_table, readiness_records)
    }

    /// Compute the total weight sum of the effective weight table.
    pub fn total_weight(table: &[WeightEntry]) -> u32 {
        table.iter().map(|e| e.weight).sum()
    }

    /// Check whether the weight table is valid (has at least one entry).
    pub fn has_available_providers(table: &[WeightEntry]) -> bool {
        !table.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_reg(id: &str, name: &str, enabled: bool, weight: i32) -> ProviderRegistration {
        ProviderRegistration::new(ProviderId::new(id), name)
            .with_enabled(enabled)
            .with_weight(weight)
    }

    #[test]
    fn empty_registry_has_no_providers() {
        let registry = ProviderRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn register_and_lookup() {
        let mut registry = ProviderRegistry::new();
        registry.register(make_reg("p1", "Provider 1", true, 1));
        assert_eq!(registry.len(), 1);

        let reg = registry.get(&ProviderId::new("p1")).unwrap();
        assert_eq!(reg.display_name, "Provider 1");
    }

    #[test]
    fn register_overwrites_existing() {
        let mut registry = ProviderRegistry::new();
        registry.register(make_reg("p1", "Old", true, 1));
        registry.register(make_reg("p1", "New", false, 5));
        let reg = registry.get(&ProviderId::new("p1")).unwrap();
        assert_eq!(reg.display_name, "New");
        assert!(!reg.enabled);
    }

    #[test]
    fn remove_provider() {
        let mut registry = ProviderRegistry::new();
        registry.register(make_reg("p1", "P1", true, 1));
        let removed = registry.remove(&ProviderId::new("p1"));
        assert!(removed.is_some());
        assert!(registry.is_empty());
    }

    #[test]
    fn weight_table_only_includes_enabled_positive_weight() {
        let mut registry = ProviderRegistry::new();
        registry.register(make_reg("p1", "Good", true, 2));
        registry.register(make_reg("p2", "Disabled", false, 2));
        registry.register(make_reg("p3", "ZeroWeight", true, 0));
        registry.register(make_reg("p4", "NegativeWeight", true, -1));

        let (table, records) = registry.build_weight_table();

        // Only p1 should be in the table
        assert_eq!(table.len(), 1);
        assert_eq!(table[0].provider_id.to_string(), "p1");
        assert_eq!(table[0].weight, 2);

        // Records should cover all 4
        assert_eq!(records.len(), 4);

        // p2 should be Disabled
        let p2 = records
            .iter()
            .find(|r| r.provider_id.to_string() == "p2")
            .unwrap();
        assert_eq!(p2.readiness, ProviderReadiness::Disabled);
        assert!(!p2.included_in_table);

        // p3 and p4 should be Misconfigured
        let p3 = records
            .iter()
            .find(|r| r.provider_id.to_string() == "p3")
            .unwrap();
        assert_eq!(p3.readiness, ProviderReadiness::Misconfigured);
        assert!(!p3.included_in_table);

        let p4 = records
            .iter()
            .find(|r| r.provider_id.to_string() == "p4")
            .unwrap();
        assert_eq!(p4.readiness, ProviderReadiness::Misconfigured);
        assert!(!p4.included_in_table);
    }

    #[test]
    fn weight_table_with_all_disabled_returns_empty() {
        let mut registry = ProviderRegistry::new();
        registry.register(make_reg("p1", "P1", false, 1));
        let (table, _) = registry.build_weight_table();
        assert!(table.is_empty());
    }

    #[test]
    fn equal_weight_default_yields_equal_entries() {
        let mut registry = ProviderRegistry::new();
        // Both have default weight = 1
        registry.register(ProviderRegistration::new(ProviderId::new("a"), "A"));
        registry.register(ProviderRegistration::new(ProviderId::new("b"), "B"));

        let (table, _) = registry.build_weight_table();
        assert_eq!(table.len(), 2);
        assert_eq!(table[0].weight, 1);
        assert_eq!(table[1].weight, 1);
        assert_eq!(ProviderRegistry::total_weight(&table), 2);
    }

    #[test]
    fn total_weight_sums_correctly() {
        let entries = vec![
            WeightEntry {
                provider_id: ProviderId::new("a"),
                display_name: "A".into(),
                weight: 3,
            },
            WeightEntry {
                provider_id: ProviderId::new("b"),
                display_name: "B".into(),
                weight: 5,
            },
        ];
        assert_eq!(ProviderRegistry::total_weight(&entries), 8);
    }

    #[test]
    fn has_available_providers_detects_empty() {
        assert!(!ProviderRegistry::has_available_providers(&[]));
        assert!(ProviderRegistry::has_available_providers(&[WeightEntry {
            provider_id: ProviderId::new("a"),
            display_name: "A".into(),
            weight: 1,
        }]));
    }

    #[test]
    fn build_weight_table_with_readiness_overrides() {
        let mut registry = ProviderRegistry::new();
        registry.register(make_reg("p1", "P1", true, 1));
        registry.register(make_reg("p2", "P2", true, 1));

        // Override p1 as rate_limited, p2 as ready
        let mut readiness_map = HashMap::new();
        readiness_map.insert("p1".to_string(), ProviderReadiness::RateLimited);
        readiness_map.insert("p2".to_string(), ProviderReadiness::Ready);

        let (table, records) = registry.build_weight_table_with_readiness(&readiness_map);
        assert_eq!(table.len(), 1);
        assert_eq!(table[0].provider_id.to_string(), "p2");

        let p1 = records
            .iter()
            .find(|r| r.provider_id.to_string() == "p1")
            .unwrap();
        assert_eq!(p1.readiness, ProviderReadiness::RateLimited);
        assert!(!p1.included_in_table);
    }

    #[test]
    fn disabled_provider_excluded_with_readiness_override() {
        let mut registry = ProviderRegistry::new();
        registry.register(make_reg("p1", "P1", false, 1));

        let readiness_map = HashMap::new(); // no override
        let (table, records) = registry.build_weight_table_with_readiness(&readiness_map);
        assert!(table.is_empty());
        assert_eq!(records[0].readiness, ProviderReadiness::Disabled);
    }

    #[test]
    fn iter_yields_all_registrations() {
        let mut registry = ProviderRegistry::new();
        registry.register(make_reg("a", "A", true, 1));
        registry.register(make_reg("b", "B", true, 2));
        let all: Vec<_> = registry.iter().collect();
        assert_eq!(all.len(), 2);
    }
}
