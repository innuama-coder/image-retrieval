//! Policy & guardrails evaluator.
//!
//! Implements the policy boundaries defined in the LLD:
//!
//! - Credential / sensitive data redaction for delivery output.
//! - Authorization risk classification (unknown / prohibited / allowed).
//! - Access-restriction detection — fallback must not bypass login walls,
//!   paywalls, access controls, or site authorisation.
//! - Paid-channel gating — paid channels default to disabled and require
//!   explicit user confirmation before use.
//! - Fallback compliance — access-restricted failures are NOT eligible for
//!   tier escalation.
//!
//! References: PRD NFR-002/NFR-003/NFR-006, HLD §Policy & Guardrails,
//! `docs/design/TASK-007-delivery-policy-observability-design.md`

use crate::domain::policy::{AuthorizationRisk, PolicyDecision, PolicyFact};
use crate::domain::retrieval::{
    ExecutionBlockingFact, FallbackEligibilityFact, RetrievalFailureCategory,
};

// ---------------------------------------------------------------------------
// Re-export domain types for convenience
// ---------------------------------------------------------------------------

pub use crate::domain::policy::{
    AuthorizationRisk as AuthRiskReexport, PolicyDecision as PolicyDecisionReexport,
    PolicyFact as PolicyFactReexport,
};

// ---------------------------------------------------------------------------
// Sensitive-information patterns (aligned with QueryPlan sensitive detection)
// ---------------------------------------------------------------------------

/// Patterns that indicate credentials, tokens, or sensitive configuration
/// that must never appear in delivery output, logs, or metrics.
const SENSITIVE_PATTERNS: &[(&str, &str)] = &[
    ("Bearer ", "Bearer token"),
    ("Authorization:", "Authorization header"),
    ("Cookie:", "Cookie header"),
    ("Set-Cookie:", "Set-Cookie header"),
    ("x-api-key:", "API key header"),
    ("api_key=", "API key param"),
    ("access_token=", "access token param"),
    ("client_secret=", "client secret param"),
    ("private_key=", "private key param"),
    ("-----BEGIN RSA PRIVATE KEY-----", "PEM private key"),
    ("-----BEGIN PRIVATE KEY-----", "PEM private key"),
    ("-----BEGIN EC PRIVATE KEY-----", "PEM EC private key"),
];

// ---------------------------------------------------------------------------
// Redaction
// ---------------------------------------------------------------------------

/// Result of sanitising a text field for delivery output.
#[derive(Debug, Clone)]
pub struct RedactionResult {
    /// The sanitised text (sensitive fragments replaced).
    pub sanitised: String,

    /// Whether any redactions were applied.
    pub redacted: bool,

    /// What kind of sensitive content was detected (without the raw value).
    pub detected_kinds: Vec<String>,
}

/// Scan `text` for known sensitive patterns and replace matching fragments
/// with `[REDACTED]`. The redacted text and a summary of what was detected
/// are returned — never the raw sensitive values.
///
/// For PEM-like patterns (containing "BEGIN"), the entire block from the
/// BEGIN marker to the matching END marker is redacted. For other patterns
/// (Bearer tokens, API keys, etc.), the pattern and the immediately
/// following token are redacted, preserving surrounding text.
pub fn sanitize_for_delivery(text: &str) -> RedactionResult {
    let mut sanitised = text.to_string();
    let mut detected_kinds: Vec<String> = Vec::new();

    // Process patterns in order; PEM patterns first so they consume the
    // block before shorter patterns (like "private_key=") try to match
    // inside them.
    let mut sorted_patterns: Vec<(&str, &str)> = SENSITIVE_PATTERNS.to_vec();
    sorted_patterns.sort_by_key(|(p, _)| if p.contains("BEGIN") { 0 } else { 1 });

    for (pattern, label) in &sorted_patterns {
        let lower_text = sanitised.to_lowercase();
        let lower_pat = pattern.to_lowercase();
        if let Some(pos) = lower_text.find(&lower_pat) {
            let is_pem = pattern.contains("BEGIN");
            let end = if is_pem {
                // Find matching END marker, or go to end of string.
                let end_marker = pattern.replace("BEGIN", "END");
                sanitised[pos..]
                    .find(&end_marker)
                    .map(|offset| pos + offset + end_marker.len())
                    .unwrap_or(sanitised.len())
            } else {
                // Replace the pattern and the immediately following token value.
                // Skip any leading whitespace between the pattern and the value,
                // then consume the value until the next whitespace or end of text.
                let after_pattern = pos + pattern.len();
                let rest = &sanitised[after_pattern..];
                // Skip leading whitespace between pattern and token value
                let leading_ws = rest.find(|c: char| !c.is_whitespace()).unwrap_or(0);
                let value_start = after_pattern + leading_ws;
                let value_rest = &sanitised[value_start..];
                let value_len = value_rest
                    .find(|c: char| c.is_whitespace())
                    .unwrap_or(value_rest.len());
                value_start + value_len
            };
            let redacted_label = format!("[REDACTED:{}]", label);
            sanitised.replace_range(pos..end, &redacted_label);
            if !detected_kinds.contains(&label.to_string()) {
                detected_kinds.push(label.to_string());
            }
        }
    }

    let redacted = !detected_kinds.is_empty();
    RedactionResult {
        sanitised,
        redacted,
        detected_kinds,
    }
}

/// Return `true` if `text` contains any known sensitive pattern.
pub fn contains_sensitive_pattern(text: &str) -> bool {
    let lower = text.to_lowercase();
    SENSITIVE_PATTERNS
        .iter()
        .any(|(p, _)| lower.contains(&p.to_lowercase()))
}

// ---------------------------------------------------------------------------
// Policy evaluation
// ---------------------------------------------------------------------------

/// Evaluate a [`PolicyFact`] against the policy & guardrails rules.
///
/// # Rules (in evaluation order)
///
/// 1. **Explicitly prohibited sources** → `LocalReject` or `TaskBlock`
///    depending on whether the prohibition is scoped to the subject or
///    the entire source.
/// 2. **Access restriction detected with no workaround** → `TaskBlock`
///    if this is a task-wide restriction; `LocalReject` if per-candidate.
/// 3. **Paid channel not confirmed** → `TaskBlock` if the channel is
///    the only remaining option; `LocalReject` if alternatives exist.
/// 4. **Unknown authorization** → `Allow` with risk retention (the risk
///    is recorded in the manifest; it is not a blocking condition).
/// 5. **Known allowed authorization** → `Allow`.
pub fn evaluate_policy_fact(fact: &PolicyFact) -> PolicyDecision {
    // Rule 1: Prohibited sources
    if fact.authorization_risk == AuthorizationRisk::Prohibited {
        return PolicyDecision::LocalReject {
            reason: format!(
                "source explicitly prohibits reuse for subject '{}'",
                fact.subject_id
            ),
        };
    }

    // Rule 2: Access restriction detected
    if fact.has_access_restriction {
        return PolicyDecision::LocalReject {
            reason: format!(
                "access restriction detected for subject '{}': {}",
                fact.subject_id, fact.context
            ),
        };
    }

    // Rule 3: Paid channel not confirmed
    if fact.is_paid_channel && !fact.paid_channel_confirmed {
        return PolicyDecision::TaskBlock {
            reason: format!(
                "paid channel required for subject '{}' but explicit user confirmation is missing",
                fact.subject_id
            ),
        };
    }

    // Rule 4 & 5: Unknown or allowed authorization → allow with risk noted
    // (the risk is captured in the manifest's risk_summary, not here).
    PolicyDecision::Allow
}

/// Evaluate whether a retrieval fallback is permissible.
///
/// # Rules
///
/// - If the failure was due to access-control / authorization restrictions,
///   fallback MUST NOT be used to bypass the restriction.
/// - If the next tier is paid and not confirmed, fallback is blocked until
///   the user explicitly enables the paid channel.
/// - If the failure was due to a disabled channel, fallback is allowed.
/// - If the failure was a network/server error, fallback is allowed.
pub fn evaluate_fallback_eligibility(fact: &FallbackEligibilityFact) -> PolicyDecision {
    if fact.is_access_restricted {
        return PolicyDecision::TaskBlock {
            reason: format!(
                "fallback from {} to {:?} blocked: access restriction detected ({})",
                fact.failed_tier, fact.next_tier, fact.reason,
            ),
        };
    }

    if fact.requires_paid_confirmation {
        return PolicyDecision::TaskBlock {
            reason: format!(
                "fallback from {} to {:?} blocked: paid channel requires explicit user confirmation",
                fact.failed_tier, fact.next_tier,
            ),
        };
    }

    PolicyDecision::Allow
}

/// Evaluate whether an execution-blocking fact from the retrieval layer
/// should be treated as a policy block.
pub fn evaluate_execution_block(fact: &ExecutionBlockingFact) -> PolicyDecision {
    if fact.is_access_restricted {
        return PolicyDecision::TaskBlock {
            reason: format!("execution blocked by access restriction: {}", fact.reason),
        };
    }

    if fact.is_paid_unconfirmed {
        return PolicyDecision::TaskBlock {
            reason: format!(
                "execution blocked by unconfirmed paid channel: {}",
                fact.reason
            ),
        };
    }

    // The execution block is due to a non-policy reason (missing dependency,
    // network failure, etc.) — it's a TaskBlock from a policy perspective
    // because it blocks the task, but the reason is infrastructure, not
    // policy violation.
    PolicyDecision::TaskBlock {
        reason: fact.reason.clone(),
    }
}

// ---------------------------------------------------------------------------
// Convenience: bulk sanitisation
// ---------------------------------------------------------------------------

/// Sanitise a collection of metadata key-value pairs for delivery output.
/// Any value matching a sensitive pattern is replaced with `[REDACTED]`.
/// The key is preserved unless it too matches a sensitive pattern.
pub fn sanitize_metadata(metadata: &[(String, String)]) -> Vec<(String, String)> {
    metadata
        .iter()
        .map(|(k, v)| {
            let sanitised_key = if contains_sensitive_pattern(k) {
                "[REDACTED]".to_string()
            } else {
                k.clone()
            };
            let sanitised_val = if contains_sensitive_pattern(v) {
                "[REDACTED]".to_string()
            } else {
                v.clone()
            };
            (sanitised_key, sanitised_val)
        })
        .collect()
}

/// Check whether a retrieval failure category is access-restriction related.
pub fn is_access_restriction_failure(category: &RetrievalFailureCategory) -> bool {
    matches!(category, RetrievalFailureCategory::AccessRestricted)
}

/// Check whether a retrieval failure category allows fallback.
pub fn allows_fallback(category: &RetrievalFailureCategory) -> bool {
    !matches!(
        category,
        RetrievalFailureCategory::AccessRestricted
            | RetrievalFailureCategory::ChannelDisabled
            | RetrievalFailureCategory::PaidNotConfirmed
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::retrieval::RetrievalChannelTier;

    // -----------------------------------------------------------------------
    // Sanitisation
    // -----------------------------------------------------------------------

    #[test]
    fn sanitize_clean_text_passes_through() {
        let result = sanitize_for_delivery("a beautiful sunset over mountains");
        assert_eq!(result.sanitised, "a beautiful sunset over mountains");
        assert!(!result.redacted);
        assert!(result.detected_kinds.is_empty());
    }

    #[test]
    fn sanitize_bearer_token_redacted() {
        let result = sanitize_for_delivery("Bearer eyJhbGciOiJIUzI1NiJ9.test description");
        assert!(result.redacted);
        assert!(result.sanitised.contains("[REDACTED:"));
        assert!(!result.sanitised.contains("eyJhbGci"));
        // The token (including ".test") is redacted; "description" survives.
        assert!(result.sanitised.contains("description"));
    }

    #[test]
    fn sanitize_api_key_redacted() {
        let result = sanitize_for_delivery("use x-api-key: abc123secret");
        assert!(result.redacted);
        assert!(!result.sanitised.contains("abc123secret"));
        assert!(result
            .detected_kinds
            .contains(&"API key header".to_string()));
    }

    #[test]
    fn sanitize_authorization_header_redacted() {
        let result = sanitize_for_delivery("Authorization: Bearer tokendata");
        assert!(result.redacted);
        assert!(!result.sanitised.contains("tokendata"));
    }

    #[test]
    fn sanitize_pem_private_key_redacted() {
        let result = sanitize_for_delivery(
            "key: -----BEGIN RSA PRIVATE KEY-----\nMIIEpAIBAAKCAQEA...\n-----END RSA PRIVATE KEY-----",
        );
        assert!(result.redacted);
        assert!(!result.sanitised.contains("MIIEpAIBAA"));
    }

    #[test]
    fn contains_sensitive_detects_bearer() {
        assert!(contains_sensitive_pattern("Bearer xyz"));
        // "not a bearer token" contains the word "bearer" (case-insensitive
        // match of "Bearer "), so it IS flagged — this is an acceptable
        // false-positive for a simple pattern-based detector.
        assert!(contains_sensitive_pattern("not a bearer token"));
    }

    #[test]
    fn contains_sensitive_detects_api_key() {
        assert!(contains_sensitive_pattern("api_key=secret"));
        assert!(!contains_sensitive_pattern("api version 2"));
    }

    // -----------------------------------------------------------------------
    // Metadata sanitisation
    // -----------------------------------------------------------------------

    #[test]
    fn sanitize_metadata_redacts_values() {
        let meta = vec![
            ("provider".into(), "fixture".into()),
            ("auth".into(), "Bearer secret123".into()),
            ("url".into(), "https://example.com".into()),
        ];
        let sanitised = sanitize_metadata(&meta);
        assert_eq!(sanitised.len(), 3);
        assert_eq!(sanitised[0].1, "fixture"); // clean
        assert_eq!(sanitised[1].1, "[REDACTED]"); // redacted
        assert_eq!(sanitised[2].1, "https://example.com"); // clean
    }

    #[test]
    fn sanitize_metadata_redacts_keys() {
        let meta = vec![("api_key=secret".into(), "value".into())];
        let sanitised = sanitize_metadata(&meta);
        assert_eq!(sanitised[0].0, "[REDACTED]");
    }

    // -----------------------------------------------------------------------
    // Policy evaluation
    // -----------------------------------------------------------------------

    #[test]
    fn prohibited_source_local_reject() {
        let fact = PolicyFact {
            subject_id: "img-1".into(),
            authorization_risk: AuthorizationRisk::Prohibited,
            has_access_restriction: false,
            is_paid_channel: false,
            paid_channel_confirmed: false,
            context: "site policy prohibits reuse".into(),
        };
        let decision = evaluate_policy_fact(&fact);
        assert!(matches!(decision, PolicyDecision::LocalReject { .. }));
        if let PolicyDecision::LocalReject { reason } = decision {
            assert!(reason.contains("prohibits"));
            assert!(reason.contains("img-1"));
        }
    }

    #[test]
    fn access_restriction_local_reject() {
        let fact = PolicyFact {
            subject_id: "img-2".into(),
            authorization_risk: AuthorizationRisk::Unknown,
            has_access_restriction: true,
            is_paid_channel: false,
            paid_channel_confirmed: false,
            context: "login wall detected".into(),
        };
        let decision = evaluate_policy_fact(&fact);
        assert!(matches!(decision, PolicyDecision::LocalReject { .. }));
        if let PolicyDecision::LocalReject { reason } = decision {
            assert!(reason.contains("access restriction"));
        }
    }

    #[test]
    fn paid_unconfirmed_task_block() {
        let fact = PolicyFact {
            subject_id: "img-3".into(),
            authorization_risk: AuthorizationRisk::Unknown,
            has_access_restriction: false,
            is_paid_channel: true,
            paid_channel_confirmed: false,
            context: "paid channel needed".into(),
        };
        let decision = evaluate_policy_fact(&fact);
        assert!(matches!(decision, PolicyDecision::TaskBlock { .. }));
    }

    #[test]
    fn unknown_authorization_allowed_with_risk() {
        let fact = PolicyFact {
            subject_id: "img-4".into(),
            authorization_risk: AuthorizationRisk::Unknown,
            has_access_restriction: false,
            is_paid_channel: false,
            paid_channel_confirmed: false,
            context: "unknown license".into(),
        };
        let decision = evaluate_policy_fact(&fact);
        assert!(matches!(decision, PolicyDecision::Allow));
    }

    #[test]
    fn allowed_authorization_is_allowed() {
        let fact = PolicyFact {
            subject_id: "img-5".into(),
            authorization_risk: AuthorizationRisk::Allowed,
            has_access_restriction: false,
            is_paid_channel: false,
            paid_channel_confirmed: false,
            context: "CC BY 2.0".into(),
        };
        let decision = evaluate_policy_fact(&fact);
        assert!(matches!(decision, PolicyDecision::Allow));
    }

    #[test]
    fn paid_confirmed_allowed() {
        let fact = PolicyFact {
            subject_id: "img-6".into(),
            authorization_risk: AuthorizationRisk::Unknown,
            has_access_restriction: false,
            is_paid_channel: true,
            paid_channel_confirmed: true,
            context: "user approved paid channel".into(),
        };
        let decision = evaluate_policy_fact(&fact);
        assert!(matches!(decision, PolicyDecision::Allow));
    }

    // -----------------------------------------------------------------------
    // Fallback eligibility
    // -----------------------------------------------------------------------

    #[test]
    fn fallback_allowed_for_network_error() {
        let fact =
            FallbackEligibilityFact::new(RetrievalChannelTier::WebFetch, "network timeout", false);
        let decision = evaluate_fallback_eligibility(&fact);
        assert!(matches!(decision, PolicyDecision::Allow));
    }

    #[test]
    fn fallback_blocked_for_access_restriction() {
        let fact = FallbackEligibilityFact::new(
            RetrievalChannelTier::WebFetch,
            "HTTP 403 Forbidden",
            true,
        );
        let decision = evaluate_fallback_eligibility(&fact);
        assert!(matches!(decision, PolicyDecision::TaskBlock { .. }));
        if let PolicyDecision::TaskBlock { reason } = decision {
            assert!(reason.contains("access restriction"));
        }
    }

    #[test]
    fn fallback_blocked_for_paid_unconfirmed() {
        let fact = FallbackEligibilityFact::new(
            RetrievalChannelTier::SelfHosted,
            "service unavailable",
            false,
        );
        // next tier from SelfHosted is Paid → requires_paid_confirmation = true
        assert!(fact.requires_paid_confirmation);
        let decision = evaluate_fallback_eligibility(&fact);
        assert!(matches!(decision, PolicyDecision::TaskBlock { .. }));
    }

    // -----------------------------------------------------------------------
    // Execution block evaluation
    // -----------------------------------------------------------------------

    #[test]
    fn execution_block_by_access_restriction_is_policy_block() {
        let fact = ExecutionBlockingFact {
            reason: "all channels blocked by access restriction".into(),
            source_tier: Some(RetrievalChannelTier::WebFetch),
            is_access_restricted: true,
            is_paid_unconfirmed: false,
        };
        let decision = evaluate_execution_block(&fact);
        assert!(matches!(decision, PolicyDecision::TaskBlock { .. }));
    }

    #[test]
    fn execution_block_by_paid_unconfirmed_is_policy_block() {
        let fact = ExecutionBlockingFact {
            reason: "paid channel not confirmed".into(),
            source_tier: Some(RetrievalChannelTier::Paid),
            is_access_restricted: false,
            is_paid_unconfirmed: true,
        };
        let decision = evaluate_execution_block(&fact);
        assert!(matches!(decision, PolicyDecision::TaskBlock { .. }));
    }

    // -----------------------------------------------------------------------
    // Retrieval failure category helpers
    // -----------------------------------------------------------------------

    #[test]
    fn access_restricted_category_is_detected() {
        assert!(is_access_restriction_failure(
            &RetrievalFailureCategory::AccessRestricted
        ));
        assert!(!is_access_restriction_failure(
            &RetrievalFailureCategory::Network
        ));
    }

    #[test]
    fn fallback_allowed_for_non_restrictive_categories() {
        assert!(allows_fallback(&RetrievalFailureCategory::Network));
        assert!(allows_fallback(&RetrievalFailureCategory::HttpStatus));
        assert!(allows_fallback(&RetrievalFailureCategory::InvalidContent));
        assert!(allows_fallback(&RetrievalFailureCategory::UnsupportedUrl));
        assert!(allows_fallback(&RetrievalFailureCategory::Other));

        assert!(!allows_fallback(
            &RetrievalFailureCategory::AccessRestricted
        ));
        assert!(!allows_fallback(&RetrievalFailureCategory::ChannelDisabled));
        assert!(!allows_fallback(
            &RetrievalFailureCategory::PaidNotConfirmed
        ));
    }
}
