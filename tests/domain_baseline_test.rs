//! Integration-level baseline tests for v1.1 domain types.
//!
//! Verifies that the public API of the domain model is usable by
//! downstream tasks (TASK-002 through TASK-005) and that the key
//! domain invariants hold.
//!
//! Coverage:
//! - QueryPlanInput default values and serde aliases
//! - Admission / NormalizedQueryPlan derivation
//! - RuntimeConfig DTOs and defaults
//! - Policy narrowing and paid/robots gating
//! - Redaction helpers
//! - AttemptCounterState invariants
//! - Error families
//! - Config readiness reporting
//! - Serialization round-trip safety

use image_retrieval::domain::config::{
    ConfigReadiness, ConfigReadinessReport, ExecutionLimitsConfig, OutputConfig, PolicyConfig,
    RetrievalChannelKind, RobotsUnknownBehavior, RuntimeConfig, SearchProviderConfig,
    SearchProviderKind, VlmEvaluationConfig, VlmEvaluatorKind,
};
use image_retrieval::domain::policy::{
    AuthorizationRisk, EffectiveRetrievalPolicy, PolicyDecision, PolicyFact,
};
use image_retrieval::domain::query_plan::{
    admit_query_plan, effective_retrieval_policy, narrow_policy, AdmissionConfig,
    AdmissionDiagnostic, AdmissionFailureCode, AdmissionOutcome, AttemptCounterState,
    DiagnosticSeverity, NormalizedQueryPlan, QualityRequirements, QualityTier, QueryPlanId,
    QueryPlanInput, QueryProviderPolicy, QueryRetrievalPolicy,
};
use image_retrieval::error::{Diagnostic, DiagnosticLevel, Error};
use image_retrieval::policy::{
    contains_sensitive_pattern, sanitize_for_delivery, sanitize_metadata,
};

// =============================================================================
// QueryPlanInput defaults
// =============================================================================

#[test]
fn default_required_image_count_is_1() {
    let input = QueryPlanInput::default();
    assert_eq!(input.required_image_count, 1);
}

#[test]
fn default_quality_is_general() {
    let input = QueryPlanInput::default();
    assert_eq!(input.quality, QualityTier::General);
}

#[test]
fn default_retry_limit_is_3() {
    let input = QueryPlanInput::default();
    assert_eq!(input.retry_limit, 3);
}

#[test]
fn default_quality_requirements_thumbnail_only_is_false() {
    let qr = QualityRequirements::default();
    assert!(!qr.allow_thumbnail_only);
}

#[test]
fn default_query_provider_policy_allow_fixture_is_false() {
    let pp = QueryProviderPolicy::default();
    assert!(!pp.allow_fixture);
}

#[test]
fn default_query_retrieval_policy_paid_disabled() {
    let rp = QueryRetrievalPolicy::default();
    assert!(!rp.allow_paid);
    assert!(rp.respect_robots);
    assert!(!rp.allow_login);
    assert!(!rp.allow_paywalled);
}

// =============================================================================
// Serde aliases
// =============================================================================

#[test]
fn required_image_count_is_canonical() {
    let json = r#"{"description": "test", "required_image_count": 5}"#;
    let parsed: QueryPlanInput = serde_json::from_str(json).expect("deserialize");
    assert_eq!(parsed.required_image_count, 5);
}

#[test]
fn required_count_alias_accepted() {
    let json = r#"{"description": "test", "required_count": 5}"#;
    let parsed: QueryPlanInput = serde_json::from_str(json).expect("deserialize");
    assert_eq!(parsed.required_image_count, 5);
}

#[test]
fn required_image_count_serializes_as_canonical_not_alias() {
    let input = QueryPlanInput {
        description: "test".into(),
        required_image_count: 5,
        ..Default::default()
    };
    let serialized = serde_json::to_string(&input).expect("serialize");
    assert!(serialized.contains("required_image_count"));
    assert!(!serialized.contains("\"required_count\""));
}

#[test]
fn quality_field_accepts_quality_tier_alias() {
    let json = r#"{"description": "test", "quality_tier": "strict"}"#;
    let parsed: QueryPlanInput = serde_json::from_str(json).expect("deserialize");
    assert_eq!(parsed.quality, QualityTier::Strict);
}

#[test]
fn quality_field_serializes_as_quality() {
    let input = QueryPlanInput {
        description: "test".into(),
        quality: QualityTier::High,
        ..Default::default()
    };
    let serialized = serde_json::to_string(&input).expect("serialize");
    assert!(serialized.contains("\"quality\""));
    assert!(!serialized.contains("\"quality_tier\""));
}

// =============================================================================
// Admission — valid input produces NormalizedQueryPlan
// =============================================================================

#[test]
fn admit_minimal_input_produces_normalized_plan() {
    let input = QueryPlanInput {
        description: "sunset over mountains".into(),
        ..Default::default()
    };
    let outcome = admit_query_plan(input, &AdmissionConfig::default());
    assert!(outcome.is_accepted());
    let plan = outcome.unwrap();
    assert_eq!(plan.required_image_count, 1);
    assert_eq!(plan.quality, QualityTier::General);
    assert_eq!(plan.retry_limit, 3);
    assert_eq!(plan.full_attempt_limit, 4);
    assert_eq!(plan.candidate_target, 20);
    assert_eq!(plan.retrieval_batch_target, 2);
    assert_eq!(plan.query_texts, vec!["sunset over mountains"]);
    assert!(!plan.query_plan_id.0.is_empty());
}

#[test]
fn admit_with_explicit_values_produces_correct_plan() {
    let input = QueryPlanInput {
        description: "cats playing".into(),
        required_image_count: 3,
        quality: QualityTier::High,
        query_texts: vec!["cats".into(), "kittens playing".into()],
        material_types: vec!["photo".into()],
        visual_requirements: vec!["high contrast".into()],
        negative_scope: vec!["dogs".into()],
        retry_limit: 2,
        ..Default::default()
    };
    let outcome = admit_query_plan(input, &AdmissionConfig::default());
    assert!(outcome.is_accepted());
    let plan = outcome.unwrap();
    assert_eq!(plan.required_image_count, 3);
    assert_eq!(plan.quality, QualityTier::High);
    assert_eq!(plan.retry_limit, 2);
    assert_eq!(plan.full_attempt_limit, 3);
    assert_eq!(plan.candidate_target, 60);
    assert_eq!(plan.retrieval_batch_target, 6);
}

// =============================================================================
// Admission — rejection paths
// =============================================================================

#[test]
fn admit_missing_description_rejected() {
    let input = QueryPlanInput {
        description: "".into(),
        ..Default::default()
    };
    let outcome = admit_query_plan(input, &AdmissionConfig::default());
    assert!(!outcome.is_accepted());
    match outcome {
        AdmissionOutcome::Rejected { diagnostics } => {
            assert_eq!(diagnostics.len(), 1);
            assert_eq!(
                diagnostics[0].code,
                AdmissionFailureCode::InputDescriptionMissing
            );
            assert_eq!(diagnostics[0].severity, DiagnosticSeverity::Error);
        }
        _ => panic!("expected rejected"),
    }
}

#[test]
fn admit_whitespace_only_description_rejected() {
    let input = QueryPlanInput {
        description: "   \n  \t  ".into(),
        ..Default::default()
    };
    let outcome = admit_query_plan(input, &AdmissionConfig::default());
    assert!(!outcome.is_accepted());
}

#[test]
fn admit_retry_limit_exceeds_max_rejected() {
    let input = QueryPlanInput {
        description: "test".into(),
        retry_limit: 10,
        ..Default::default()
    };
    let outcome = admit_query_plan(input, &AdmissionConfig::default());
    assert!(!outcome.is_accepted());
}

#[test]
fn admit_required_count_exceeds_limit_rejected() {
    let config = AdmissionConfig {
        max_required_image_count: 50,
    };
    let input = QueryPlanInput {
        description: "test".into(),
        required_image_count: 100,
        ..Default::default()
    };
    let outcome = admit_query_plan(input, &config);
    assert!(!outcome.is_accepted());
}

// =============================================================================
// Admission — non-blocking warnings
// =============================================================================

#[test]
fn admit_zero_count_defaulted_with_warning() {
    let input = QueryPlanInput {
        description: "test".into(),
        required_image_count: 0,
        ..Default::default()
    };
    let outcome = admit_query_plan(input, &AdmissionConfig::default());
    assert!(outcome.is_accepted());
    match outcome {
        AdmissionOutcome::Accepted {
            query_plan,
            warnings,
        } => {
            assert_eq!(query_plan.required_image_count, 1);
            assert!(warnings
                .iter()
                .any(|d| d.code == AdmissionFailureCode::RequiredCountZeroDefaulted));
        }
        _ => panic!("expected accepted"),
    }
}

#[test]
fn admit_query_texts_defaults_to_description() {
    let input = QueryPlanInput {
        description: "cats playing".into(),
        query_texts: vec![],
        ..Default::default()
    };
    let outcome = admit_query_plan(input, &AdmissionConfig::default());
    let plan = outcome.unwrap();
    assert_eq!(plan.query_texts, vec!["cats playing"]);
}

#[test]
fn admit_empty_query_text_entries_ignored_with_warning() {
    let input = QueryPlanInput {
        description: "dogs running".into(),
        query_texts: vec!["valid query".into(), "".into(), "  ".into()],
        ..Default::default()
    };
    let outcome = admit_query_plan(input, &AdmissionConfig::default());
    assert!(outcome.is_accepted());
    let plan = outcome.unwrap();
    assert_eq!(plan.query_texts, vec!["valid query"]);
}

#[test]
fn admit_source_diversity_exceeds_required_warns() {
    let input = QueryPlanInput {
        description: "test".into(),
        required_image_count: 2,
        source_diversity_requirement: Some(5),
        ..Default::default()
    };
    let outcome = admit_query_plan(input, &AdmissionConfig::default());
    assert!(outcome.is_accepted());
    match outcome {
        AdmissionOutcome::Accepted { warnings, .. } => {
            assert!(warnings
                .iter()
                .any(|d| d.code == AdmissionFailureCode::SourceDiversityExceedsRequired));
        }
        _ => panic!("expected accepted"),
    }
}

#[test]
fn admit_large_count_warns() {
    let input = QueryPlanInput {
        description: "test".into(),
        required_image_count: 200,
        ..Default::default()
    };
    let outcome = admit_query_plan(input, &AdmissionConfig::default());
    assert!(outcome.is_accepted());
    match outcome {
        AdmissionOutcome::Accepted { warnings, .. } => {
            assert!(warnings
                .iter()
                .any(|d| d.remediation.is_none() || d.message.contains("Large request")));
        }
        _ => panic!("expected accepted"),
    }
}

// =============================================================================
// Target derivation invariants
// =============================================================================

#[test]
fn candidate_target_is_20n() {
    for n in &[1, 2, 3, 5, 10] {
        let input = QueryPlanInput {
            description: "test".into(),
            required_image_count: *n,
            ..Default::default()
        };
        let outcome = admit_query_plan(input, &AdmissionConfig::default());
        let plan = outcome.unwrap();
        assert_eq!(
            plan.candidate_target,
            n * 20,
            "candidate_target for n={}",
            n
        );
    }
}

#[test]
fn retrieval_batch_target_is_2n() {
    for n in &[1, 2, 3, 5, 10] {
        let input = QueryPlanInput {
            description: "test".into(),
            required_image_count: *n,
            ..Default::default()
        };
        let outcome = admit_query_plan(input, &AdmissionConfig::default());
        let plan = outcome.unwrap();
        assert_eq!(
            plan.retrieval_batch_target,
            n * 2,
            "retrieval_batch_target for n={}",
            n
        );
    }
}

#[test]
fn full_attempt_limit_is_one_plus_retry_limit() {
    for retry in &[0, 1, 2, 3] {
        let input = QueryPlanInput {
            description: "test".into(),
            retry_limit: *retry,
            ..Default::default()
        };
        let outcome = admit_query_plan(input, &AdmissionConfig::default());
        let plan = outcome.unwrap();
        assert_eq!(
            plan.full_attempt_limit,
            retry + 1,
            "full_attempt_limit for retry={}",
            retry
        );
    }
}

// =============================================================================
// AttemptCounterState invariants
// =============================================================================

#[test]
fn attempt_counter_initial_state() {
    let state = AttemptCounterState::initial(3);
    assert_eq!(state.full_attempt_count, 1);
    assert_eq!(state.retry_count, 0);
    assert_eq!(state.full_attempt_limit, 4);
    assert_eq!(state.retry_limit, 3);
    assert!(state.invariant_holds());
}

#[test]
fn attempt_counter_advance() {
    let mut state = AttemptCounterState::initial(3);
    assert!(state.advance().is_some());
    assert_eq!(state.full_attempt_count, 2);
    assert_eq!(state.retry_count, 1);
    assert!(state.invariant_holds());
}

#[test]
fn attempt_counter_exhausted_after_limit() {
    let mut state = AttemptCounterState::initial(1); // limit = 2
    assert!(state.advance().is_some()); // count=2, retry=1
    assert!(state.advance().is_none()); // exhausted
    assert_eq!(state.full_attempt_count, 2);
}

#[test]
fn attempt_counter_invariant_fails_on_mismatch() {
    let state = AttemptCounterState {
        full_attempt_count: 2,
        retry_count: 0, // should be 1
        full_attempt_limit: 4,
        retry_limit: 3,
    };
    assert!(!state.invariant_holds());
}

#[test]
fn attempt_counter_retry_count_equals_full_attempt_minus_one() {
    let mut state = AttemptCounterState::initial(3);
    for _ in 0..3 {
        if state.advance().is_some() {
            assert_eq!(state.retry_count, state.full_attempt_count - 1);
        }
    }
}

// =============================================================================
// Policy narrowing — paid disabled by default
// =============================================================================

#[test]
fn policy_paid_disabled_by_default() {
    let config = EffectiveRetrievalPolicy::from_config(false, true, false, false);
    let query = QueryRetrievalPolicy::default();
    let result = config.narrow(&query);
    assert!(!result.paid_allowed);
    assert!(!result.effective.allow_paid);
    assert!(result.diagnostics.is_empty()); // query doesn't ask for it
}

#[test]
fn policy_paid_blocked_when_config_disables_but_query_requests() {
    let config = EffectiveRetrievalPolicy::from_config(false, true, false, false);
    let query = QueryRetrievalPolicy {
        allow_paid: true,
        ..Default::default()
    };
    let result = config.narrow(&query);
    assert!(!result.paid_allowed);
    assert!(!result.effective.allow_paid);
    assert!(result
        .diagnostics
        .iter()
        .any(|d| d.code == AdmissionFailureCode::PolicyPaidBlockedByConfig));
}

#[test]
fn policy_paid_allowed_when_both_enable() {
    let config = EffectiveRetrievalPolicy::from_config(true, true, false, false);
    let query = QueryRetrievalPolicy {
        allow_paid: true,
        ..Default::default()
    };
    let result = config.narrow(&query);
    assert!(result.paid_allowed);
    assert!(result.effective.allow_paid);
    assert!(result.diagnostics.is_empty());
}

#[test]
fn policy_cannot_silently_broaden() {
    // Config is very restrictive; query tries to open everything
    let config = EffectiveRetrievalPolicy::from_config(false, true, false, false);
    let query = QueryRetrievalPolicy {
        allow_paid: true,
        respect_robots: false,
        allow_login: true,
        allow_paywalled: true,
    };
    let result = config.narrow(&query);
    // All broadening attempts are rejected
    assert!(!result.effective.allow_paid);
    assert!(result.effective.respect_robots); // config-enforced
    assert!(!result.effective.allow_login);
    assert!(!result.effective.allow_paywalled);
    assert!(!result.diagnostics.is_empty());
}

#[test]
fn policy_query_can_narrow() {
    // Config is permissive; query narrows it
    let config = EffectiveRetrievalPolicy::from_config(true, true, true, true);
    let query = QueryRetrievalPolicy {
        allow_paid: false,
        respect_robots: true,
        allow_login: false,
        allow_paywalled: false,
    };
    let result = config.narrow(&query);
    assert!(!result.effective.allow_paid);
    assert!(result.effective.respect_robots);
    assert!(!result.effective.allow_login);
    assert!(!result.effective.allow_paywalled);
    // Narrowing silently is fine — no diagnostics needed
    assert!(result.diagnostics.is_empty());
}

#[test]
fn policy_robots_enforced_by_config() {
    let config = EffectiveRetrievalPolicy::from_config(false, true, false, false);
    let query = QueryRetrievalPolicy {
        respect_robots: false,
        ..Default::default()
    };
    let result = config.narrow(&query);
    assert!(result.effective.respect_robots);
}

#[test]
fn policy_login_blocked_by_config() {
    let config = EffectiveRetrievalPolicy::from_config(false, true, false, false);
    let query = QueryRetrievalPolicy {
        allow_login: true,
        ..Default::default()
    };
    let result = config.narrow(&query);
    assert!(!result.effective.allow_login);
}

#[test]
fn policy_paywalled_blocked_by_config() {
    let config = EffectiveRetrievalPolicy::from_config(false, true, false, false);
    let query = QueryRetrievalPolicy {
        allow_paywalled: true,
        ..Default::default()
    };
    let result = config.narrow(&query);
    assert!(!result.effective.allow_paywalled);
}

// =============================================================================
// Policy narrowing free functions
// =============================================================================

#[test]
fn narrow_policy_fn_detects_broadening() {
    let query = QueryRetrievalPolicy {
        allow_paid: true,
        respect_robots: false,
        allow_login: true,
        allow_paywalled: true,
    };
    let diags = narrow_policy(&query, false, true, false, false);
    assert!(diags
        .iter()
        .any(|d| d.code == AdmissionFailureCode::PolicyPaidBlockedByConfig));
    assert!(diags
        .iter()
        .any(|d| d.code == AdmissionFailureCode::PolicyBroadeningBlocked));
    // Should have 4 diagnostics (one for each blocked broadening)
    assert!(diags.len() >= 4);
}

#[test]
fn narrow_policy_fn_quiet_when_query_complies() {
    let query = QueryRetrievalPolicy::default(); // all defaults are restrictive
    let diags = narrow_policy(&query, false, true, false, false);
    assert!(diags.is_empty());
}

#[test]
fn effective_retrieval_policy_fn_narrows_correctly() {
    let query = QueryRetrievalPolicy {
        allow_paid: true,
        respect_robots: false,
        allow_login: true,
        allow_paywalled: true,
    };
    let effective = effective_retrieval_policy(&query, true, true, false, false);
    // paid: query=true AND config=true → true
    assert!(effective.allow_paid);
    // robots: query=false OR config=true → true (config overrides)
    assert!(effective.respect_robots);
    // login: query=true AND config=false → false
    assert!(!effective.allow_login);
    // paywalled: query=true AND config=false → false
    assert!(!effective.allow_paywalled);
}

// =============================================================================
// Redaction — removes credential-like values
// =============================================================================

#[test]
fn redact_bearer_token() {
    let result = sanitize_for_delivery("Bearer eyJhbGciOiJIUzI1NiJ9.test description");
    assert!(result.redacted);
    assert!(result.sanitised.contains("[REDACTED:"));
    assert!(!result.sanitised.contains("eyJhbGci"));
}

#[test]
fn redact_api_key() {
    let result = sanitize_for_delivery("use x-api-key: abc123secret");
    assert!(result.redacted);
    assert!(!result.sanitised.contains("abc123secret"));
}

#[test]
fn redact_authorization_header() {
    let result = sanitize_for_delivery("Authorization: Bearer tokendata");
    assert!(result.redacted);
    assert!(!result.sanitised.contains("tokendata"));
}

#[test]
fn redact_pem_private_key() {
    let result = sanitize_for_delivery(
        "key: -----BEGIN RSA PRIVATE KEY-----\nMIIEpAIBAAKCAQEA...\n-----END RSA PRIVATE KEY-----",
    );
    assert!(result.redacted);
    assert!(!result.sanitised.contains("MIIEpAIBAA"));
}

#[test]
fn clean_text_passes_through() {
    let result = sanitize_for_delivery("a beautiful sunset over mountains");
    assert!(!result.redacted);
    assert_eq!(result.sanitised, "a beautiful sunset over mountains");
}

#[test]
fn contains_sensitive_detects_credentials() {
    assert!(contains_sensitive_pattern("Bearer xyz"));
    assert!(contains_sensitive_pattern("api_key=secret"));
    assert!(!contains_sensitive_pattern("api version 2"));
    assert!(!contains_sensitive_pattern("normal description"));
}

#[test]
fn redact_metadata_preserves_clean_values() {
    let meta = vec![
        ("provider".into(), "fixture".into()),
        ("auth".into(), "Bearer secret123".into()),
        ("url".into(), "https://example.com".into()),
    ];
    let sanitised = sanitize_metadata(&meta);
    assert_eq!(sanitised.len(), 3);
    assert_eq!(sanitised[0].1, "fixture");
    assert_eq!(sanitised[1].1, "[REDACTED]");
    assert_eq!(sanitised[2].1, "https://example.com");
}

#[test]
fn redact_metadata_redacts_sensitive_keys() {
    let meta = vec![("api_key=secret".into(), "value".into())];
    let sanitised = sanitize_metadata(&meta);
    assert_eq!(sanitised[0].0, "[REDACTED]");
}

// =============================================================================
// Sensitive input detection in admission
// =============================================================================

#[test]
fn admission_redacts_bearer_token_in_description() {
    let input = QueryPlanInput {
        description: "Bearer eyJhbGciOiJIUzI1NiJ9.test description".into(),
        ..Default::default()
    };
    let outcome = admit_query_plan(input, &AdmissionConfig::default());
    assert!(outcome.is_accepted());
    match outcome {
        AdmissionOutcome::Accepted { warnings, .. } => {
            let sens = warnings
                .iter()
                .find(|d| d.code == AdmissionFailureCode::SensitiveInputRedacted)
                .expect("should have sensitive content warning");
            assert!(sens.redacted);
            assert!(!sens.message.contains("eyJhbGci"));
        }
        _ => panic!("expected accepted"),
    }
}

#[test]
fn admission_redacts_api_key_in_description() {
    let input = QueryPlanInput {
        description: "use x-api-key: abc123secret for access".into(),
        ..Default::default()
    };
    let outcome = admit_query_plan(input, &AdmissionConfig::default());
    assert!(outcome.is_accepted());
    match outcome {
        AdmissionOutcome::Accepted { warnings, .. } => {
            let sens = warnings
                .iter()
                .find(|d| d.code == AdmissionFailureCode::SensitiveInputRedacted)
                .expect("should have API key warning");
            assert!(!sens.message.contains("abc123secret"));
        }
        _ => panic!("expected accepted"),
    }
}

#[test]
fn clean_description_no_sensitive_warning() {
    let input = QueryPlanInput {
        description: "a beautiful sunset over mountains with orange sky".into(),
        ..Default::default()
    };
    let outcome = admit_query_plan(input, &AdmissionConfig::default());
    assert!(outcome.is_accepted());
    match outcome {
        AdmissionOutcome::Accepted { warnings, .. } => {
            let has_sensitive = warnings
                .iter()
                .any(|d| d.code == AdmissionFailureCode::SensitiveInputRedacted);
            assert!(
                !has_sensitive,
                "clean description should not trigger sensitive warning"
            );
        }
        _ => panic!("expected accepted"),
    }
}

// =============================================================================
// RuntimeConfig defaults
// =============================================================================

#[test]
fn runtime_config_all_defaults() {
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
fn serpapi_default_provider_has_correct_values() {
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
    assert!(!config.allow_login_required_sources);
    assert!(!config.allow_paywalled_sources);
    assert!(config.prohibited_domains.is_empty());
}

#[test]
fn policy_config_robots_warn_default() {
    let config = PolicyConfig::default();
    assert_eq!(config.robots_unknown_behavior, RobotsUnknownBehavior::Warn);
}

#[test]
fn execution_limits_defaults() {
    let limits = ExecutionLimitsConfig::default();
    assert_eq!(limits.max_required_image_count, 1000);
    assert_eq!(limits.max_retry_limit, 3);
}

#[test]
fn output_config_write_diagnostics_default_true() {
    let config = OutputConfig::default();
    assert!(config.write_diagnostics);
}

// =============================================================================
// Config readiness
// =============================================================================

#[test]
fn readiness_ready_component() {
    let r = ConfigReadiness::ready("serpapi");
    assert!(r.ready);
    assert!(r.reason.is_none());
    assert!(r.failure_code.is_none());
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

// =============================================================================
// Config serialization round-trip
// =============================================================================

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
    assert!(!config.policy.allow_paid_channels);
}

#[test]
fn config_stores_env_var_names_not_values() {
    let provider = SearchProviderConfig {
        provider_id: "test".into(),
        provider_kind: SearchProviderKind::Custom("test".into()),
        enabled: true,
        weight: 1,
        endpoint: None,
        credential_env: Some("MY_SECRET_KEY".into()),
        default_query_params: std::collections::BTreeMap::new(),
    };
    let json = serde_json::to_string(&provider).unwrap();
    // Env var name is present; actual secret must never be there
    assert!(json.contains("MY_SECRET_KEY"));
    assert!(!json.contains("actual-secret-value"));
}

// =============================================================================
// QueryPlanInput serde round-trip
// =============================================================================

#[test]
fn query_plan_input_round_trip() {
    let json = r#"{
        "description": "a cat on a sofa",
        "required_image_count": 3,
        "quality": "high",
        "content_constraints": {
            "must_include": ["cat"],
            "must_avoid": ["dog"]
        },
        "authorization_preference": "default",
        "output_preference": "human",
        "retry_limit": 3
    }"#;

    let parsed: QueryPlanInput = serde_json::from_str(json).expect("deserialize QueryPlanInput");
    assert_eq!(parsed.description, "a cat on a sofa");
    assert_eq!(parsed.required_image_count, 3);
    assert_eq!(parsed.quality, QualityTier::High);

    let round_tripped = serde_json::to_string_pretty(&parsed).expect("serialize");
    let parsed_again: QueryPlanInput =
        serde_json::from_str(&round_tripped).expect("deserialize again");
    assert_eq!(parsed_again.description, parsed.description);
    assert_eq!(
        parsed_again.required_image_count,
        parsed.required_image_count
    );
}

// =============================================================================
// Error families
// =============================================================================

#[test]
fn error_families_are_distinguishable() {
    let input_err = Error::input_rejection("missing description");
    let provider_err = Error::provider_failure("brave", "timeout");
    let exec_err = Error::execution_blocked("Qwen unavailable");
    let admission_err = Error::admission_blocked("INPUT_DESCRIPTION_MISSING", "empty description");
    let config_err = Error::config_error("serpapi", "key not set");
    let policy_err = Error::policy_violation("POLICY_PAID_UNCONFIRMED", "paid not confirmed");

    assert!(matches!(input_err, Error::InputRejection { .. }));
    assert!(matches!(provider_err, Error::ProviderFailure { .. }));
    assert!(matches!(exec_err, Error::ExecutionBlocked { .. }));
    assert!(matches!(admission_err, Error::AdmissionBlocked { .. }));
    assert!(matches!(config_err, Error::ConfigError { .. }));
    assert!(matches!(policy_err, Error::PolicyViolation { .. }));

    // All errors implement std::error::Error
    let _: &dyn std::error::Error = &input_err;
}

#[test]
fn admission_blocked_display() {
    let err = Error::admission_blocked("RETRY_LIMIT_EXCEEDED", "retry limit too high");
    let s = err.to_string();
    assert!(s.contains("admission blocked"));
    assert!(s.contains("RETRY_LIMIT_EXCEEDED"));
}

#[test]
fn config_error_display() {
    let err = Error::config_error("serpapi", "SERPAPI_API_KEY not set");
    let s = err.to_string();
    assert!(s.contains("config error"));
    assert!(s.contains("serpapi"));
}

#[test]
fn policy_violation_display() {
    let err = Error::policy_violation("POLICY_PAID_UNCONFIRMED", "paid channel not confirmed");
    let s = err.to_string();
    assert!(s.contains("policy violation"));
    assert!(s.contains("POLICY_PAID_UNCONFIRMED"));
}

// =============================================================================
// Diagnostic model
// =============================================================================

#[test]
fn diagnostic_accumulates_items() {
    let diag = Diagnostic::new("limited_delivery", "Shortfall of 2 images.")
        .with_item(image_retrieval::error::DiagnosticItem {
            level: DiagnosticLevel::Error,
            category: "candidate shortage".into(),
            message: "searched all providers, got only 10 of 60 target".into(),
        })
        .with_item(image_retrieval::error::DiagnosticItem {
            level: DiagnosticLevel::Warning,
            category: "channel fallback".into(),
            message: "web_fetch failed, used self_hosted".into(),
        })
        .with_item(image_retrieval::error::DiagnosticItem {
            level: DiagnosticLevel::Info,
            category: "provider usage".into(),
            message: "used providers: fixture-a (weight 1)".into(),
        });

    assert_eq!(diag.items.len(), 3);
    assert_eq!(diag.items[0].level, DiagnosticLevel::Error);
    assert_eq!(diag.items[2].level, DiagnosticLevel::Info);
}

// =============================================================================
// AdmissionDiagnostic constructors
// =============================================================================

#[test]
fn admission_diagnostic_error() {
    let d = AdmissionDiagnostic::error(
        AdmissionFailureCode::InputDescriptionMissing,
        "description",
        "missing",
        Some("fix it"),
    );
    assert_eq!(d.severity, DiagnosticSeverity::Error);
    assert_eq!(d.code, AdmissionFailureCode::InputDescriptionMissing);
    assert!(d.remediation.is_some());
}

#[test]
fn admission_diagnostic_warning() {
    let d = AdmissionDiagnostic::warning(
        AdmissionFailureCode::RequiredCountZeroDefaulted,
        "required_image_count",
        "was zero",
    );
    assert_eq!(d.severity, DiagnosticSeverity::Warning);
    assert_eq!(d.default_applied, None);
}

#[test]
fn admission_diagnostic_info() {
    let d = AdmissionDiagnostic::info(
        AdmissionFailureCode::RequiredCountZeroDefaulted,
        "field",
        "applied default",
        "1",
    );
    assert_eq!(d.severity, DiagnosticSeverity::Info);
    assert_eq!(d.default_applied, Some("1".to_string()));
}

#[test]
fn admission_diagnostic_blocker() {
    let d = AdmissionDiagnostic::blocker(
        AdmissionFailureCode::VlmEvaluationUnavailable,
        "vlm_evaluation",
        "Qwen not available",
    );
    assert_eq!(d.severity, DiagnosticSeverity::Blocker);
}

#[test]
fn admission_diagnostic_with_redacted() {
    let d = AdmissionDiagnostic::warning(
        AdmissionFailureCode::SensitiveInputRedacted,
        "description",
        "sensitive content",
    )
    .with_redacted();
    assert!(d.redacted);
}

// =============================================================================
// AdmissionOutcome
// =============================================================================

#[test]
fn outcome_diagnostics_returns_warnings_for_accepted() {
    let input = QueryPlanInput {
        description: "test".into(),
        required_image_count: 0,
        ..Default::default()
    };
    let outcome = admit_query_plan(input, &AdmissionConfig::default());
    assert!(outcome.is_accepted());
    let diags = outcome.diagnostics();
    assert!(!diags.is_empty());
}

#[test]
fn outcome_diagnostics_returns_errors_for_rejected() {
    let input = QueryPlanInput {
        description: "".into(),
        ..Default::default()
    };
    let outcome = admit_query_plan(input, &AdmissionConfig::default());
    assert!(!outcome.is_accepted());
    let diags = outcome.diagnostics();
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].severity, DiagnosticSeverity::Error);
}

#[test]
#[should_panic]
fn outcome_unwrap_panics_on_rejected() {
    let input = QueryPlanInput {
        description: "".into(),
        ..Default::default()
    };
    let outcome = admit_query_plan(input, &AdmissionConfig::default());
    outcome.unwrap();
}

// =============================================================================
// QueryPlanId
// =============================================================================

#[test]
fn query_plan_id_generates_unique() {
    let a = QueryPlanId::generate();
    let b = QueryPlanId::generate();
    assert_ne!(a, b);
    assert!(!a.0.is_empty());
}

#[test]
fn query_plan_id_new_from_string() {
    let id = QueryPlanId::new("custom-id-123");
    assert_eq!(id.0, "custom-id-123");
    assert_eq!(id.to_string(), "custom-id-123");
}

// =============================================================================
// QualityTier serde
// =============================================================================

#[test]
fn quality_tier_serde_round_trip() {
    for tier in &[QualityTier::General, QualityTier::High, QualityTier::Strict] {
        let json = serde_json::to_string(tier).expect("serialize");
        let round: QualityTier = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(round, *tier);
    }
}

// =============================================================================
// NormalizedQueryPlan contains package-safe diagnostics
// =============================================================================

#[test]
fn normalized_plan_carries_admission_diagnostics() {
    let input = QueryPlanInput {
        description: "test".into(),
        required_image_count: 0,
        ..Default::default()
    };
    let outcome = admit_query_plan(input, &AdmissionConfig::default());
    let plan = outcome.unwrap();
    assert!(!plan.admission_diagnostics.is_empty());
}

#[test]
fn normalized_plan_is_serde_round_trip_safe() {
    let input = QueryPlanInput {
        description: "test image".into(),
        required_image_count: 2,
        quality: QualityTier::High,
        ..Default::default()
    };
    let outcome = admit_query_plan(input, &AdmissionConfig::default());
    let plan = outcome.unwrap();
    let json = serde_json::to_string_pretty(&plan).expect("serialize NormalizedQueryPlan");
    let plan2: NormalizedQueryPlan = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(plan.query_plan_id, plan2.query_plan_id);
    assert_eq!(plan.required_image_count, plan2.required_image_count);
    assert_eq!(plan.candidate_target, plan2.candidate_target);
}

// =============================================================================
// Config enums serde
// =============================================================================

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
    let warn: RobotsUnknownBehavior = serde_json::from_str("\"warn\"").unwrap();
    assert_eq!(warn, RobotsUnknownBehavior::Warn);
    let block: RobotsUnknownBehavior = serde_json::from_str("\"block\"").unwrap();
    assert_eq!(block, RobotsUnknownBehavior::Block);
}

#[test]
fn search_provider_kind_serde() {
    assert_eq!(
        serde_json::to_string(&SearchProviderKind::SerpapiGoogleImages).unwrap(),
        "\"serpapi_google_images\""
    );
    assert_eq!(
        serde_json::to_string(&SearchProviderKind::Fixture).unwrap(),
        "\"fixture\""
    );
    // Custom variant serializes as an object with the custom tag
    let custom = SearchProviderKind::Custom("mcp".into());
    let json = serde_json::to_string(&custom).unwrap();
    let round: SearchProviderKind = serde_json::from_str(&json).unwrap();
    assert_eq!(round, custom);
}

#[test]
fn vlm_evaluator_kind_serde() {
    assert_eq!(
        serde_json::to_string(&VlmEvaluatorKind::Qwen35Vlm).unwrap(),
        "\"qwen_3_5_vlm\""
    );
    assert_eq!(
        serde_json::to_string(&VlmEvaluatorKind::Fixture).unwrap(),
        "\"fixture\""
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
    assert_eq!(
        serde_json::to_string(&RetrievalChannelKind::SelfHostedService).unwrap(),
        "\"self_hosted_service\""
    );
}

// =============================================================================
// PolicyFact invariants
// =============================================================================

#[test]
fn policy_fact_prohibited_source() {
    let fact = PolicyFact {
        subject_id: "cand-6".into(),
        authorization_risk: AuthorizationRisk::Prohibited,
        has_access_restriction: true,
        is_paid_channel: false,
        paid_channel_confirmed: false,
        context: "explicitly disallowed by source".into(),
    };
    assert_eq!(fact.authorization_risk, AuthorizationRisk::Prohibited);
    assert!(fact.has_access_restriction);
}

#[test]
fn policy_fact_unknown_authorization_is_not_blocked() {
    let fact = PolicyFact {
        subject_id: "cand-5".into(),
        authorization_risk: AuthorizationRisk::Unknown,
        has_access_restriction: false,
        is_paid_channel: false,
        paid_channel_confirmed: false,
        context: "source has no clear license".into(),
    };
    assert_eq!(fact.authorization_risk, AuthorizationRisk::Unknown);
    assert!(!fact.has_access_restriction);
}

// =============================================================================
// PolicyDecision
// =============================================================================

#[test]
fn policy_decision_allow_is_not_task_block() {
    assert!(!PolicyDecision::Allow.is_task_block());
}

#[test]
fn policy_decision_local_reject_is_not_task_block() {
    assert!(!PolicyDecision::LocalReject {
        reason: "duplicate".into()
    }
    .is_task_block());
}

#[test]
fn policy_decision_task_block() {
    assert!(PolicyDecision::TaskBlock {
        reason: "paid not confirmed".into()
    }
    .is_task_block());
}

// =============================================================================
// Execution limits sanity
// =============================================================================

#[test]
fn execution_limits_max_retry_respects_constitution() {
    let limits = ExecutionLimitsConfig::default();
    assert!(
        limits.max_retry_limit <= 3,
        "constitution allows at most 3 retries"
    );
}
