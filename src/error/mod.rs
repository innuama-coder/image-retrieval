//! Error and diagnostic model baseline.
//!
//! Error families are aligned with the LLD classification:
//! input rejection, provider failure, candidate rejection, retrieval
//! failure, image rejection, policy blocking, OpenClaw unavailability,
//! and execution blocking.
//!
//! Every error carries a user-facing diagnostic message; internal
//! implementation details (stack traces, raw service responses) are
//! never exposed.

use std::fmt;

// ---------------------------------------------------------------------------
// Top-level error type
// ---------------------------------------------------------------------------

/// Unified error type for the image-retrieval CLI.
///
/// Each variant corresponds to a product-level error family defined
/// in the LLD and PRD.
#[derive(Debug)]
pub enum Error {
    /// QueryPlan input is invalid (missing description, invalid values, etc.).
    InputRejection { reason: String },

    /// A search provider failed or returned unusable results.
    ProviderFailure { provider_id: String, reason: String },

    /// A candidate was rejected during mechanical or subjective evaluation.
    CandidateRejection {
        candidate_id: String,
        reason: String,
    },

    /// Image retrieval failed for a batch or individual candidate.
    RetrievalFailure {
        candidate_id: Option<String>,
        channel_tier: String,
        reason: String,
    },

    /// An image was rejected during acceptance checks.
    ImageRejection {
        candidate_id: String,
        reason: String,
    },

    /// A policy or guardrail blocked an action.
    PolicyBlocking { reason: String },

    /// OpenClaw production evaluation is unavailable; production tasks
    /// must enter execution-blocked state.
    OpenClawUnavailable { reason: String },

    /// A necessary production dependency is unavailable or a product
    /// policy prohibits continuing the task.
    ExecutionBlocked { reason: String },

    /// An internal error that should not normally occur (configuration,
    /// I/O, serialization). These are wrapped for fallback handling but
    /// the user-facing diagnostic is always sanitised.
    Internal { message: String },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InputRejection { reason } => {
                write!(f, "input rejected: {}", reason)
            }
            Self::ProviderFailure {
                provider_id,
                reason,
            } => {
                write!(f, "provider '{}' failed: {}", provider_id, reason)
            }
            Self::CandidateRejection {
                candidate_id,
                reason,
            } => {
                write!(f, "candidate '{}' rejected: {}", candidate_id, reason)
            }
            Self::RetrievalFailure {
                candidate_id,
                channel_tier,
                reason,
            } => {
                if let Some(cid) = candidate_id {
                    write!(
                        f,
                        "retrieval failed for '{}' via {}: {}",
                        cid, channel_tier, reason
                    )
                } else {
                    write!(f, "retrieval failed via {}: {}", channel_tier, reason)
                }
            }
            Self::ImageRejection {
                candidate_id,
                reason,
            } => {
                write!(f, "image '{}' rejected: {}", candidate_id, reason)
            }
            Self::PolicyBlocking { reason } => {
                write!(f, "policy blocked: {}", reason)
            }
            Self::OpenClawUnavailable { reason } => {
                write!(f, "OpenClaw unavailable: {}", reason)
            }
            Self::ExecutionBlocked { reason } => {
                write!(f, "execution blocked: {}", reason)
            }
            Self::Internal { message } => {
                write!(f, "internal error: {}", message)
            }
        }
    }
}

impl std::error::Error for Error {}

// ---------------------------------------------------------------------------
// Result alias
// ---------------------------------------------------------------------------

/// Standard result type for the crate.
pub type Result<T> = std::result::Result<T, Error>;

// ---------------------------------------------------------------------------
// Diagnostic model
// ---------------------------------------------------------------------------

/// User-facing diagnostic produced when a task completes (or is blocked).
///
/// Diagnostics explain *what happened* in user-understandable terms;
/// they never expose stack traces, internal paths, or credential data.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    /// The overall task status at the time the diagnostic was produced.
    pub status: String,

    /// Human-readable summary.
    pub summary: String,

    /// Ordered list of diagnostic items describing notable events,
    /// decisions, and failures.
    pub items: Vec<DiagnosticItem>,
}

impl Diagnostic {
    pub fn new(status: impl Into<String>, summary: impl Into<String>) -> Self {
        Self {
            status: status.into(),
            summary: summary.into(),
            items: Vec::new(),
        }
    }

    pub fn with_item(mut self, item: DiagnosticItem) -> Self {
        self.items.push(item);
        self
    }
}

/// A single diagnostic entry.
#[derive(Debug, Clone)]
pub struct DiagnosticItem {
    /// Severity level of this item.
    pub level: DiagnosticLevel,

    /// Short category label (e.g. "candidate rejection", "channel fallback").
    pub category: String,

    /// Human-readable message.
    pub message: String,
}

/// Severity levels for diagnostic items.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DiagnosticLevel {
    /// Informational — normal operation.
    Info,

    /// Warning — something worth attention but not blocking.
    Warning,

    /// Error — something was blocked or failed.
    Error,
}

// ---------------------------------------------------------------------------
// Convenience constructors for each error family
// ---------------------------------------------------------------------------

impl Error {
    pub fn input_rejection(reason: impl Into<String>) -> Self {
        Self::InputRejection {
            reason: reason.into(),
        }
    }

    pub fn provider_failure(provider_id: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::ProviderFailure {
            provider_id: provider_id.into(),
            reason: reason.into(),
        }
    }

    pub fn candidate_rejection(candidate_id: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::CandidateRejection {
            candidate_id: candidate_id.into(),
            reason: reason.into(),
        }
    }

    pub fn retrieval_failure(
        candidate_id: Option<impl Into<String>>,
        channel_tier: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self::RetrievalFailure {
            candidate_id: candidate_id.map(|s| s.into()),
            channel_tier: channel_tier.into(),
            reason: reason.into(),
        }
    }

    pub fn image_rejection(candidate_id: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::ImageRejection {
            candidate_id: candidate_id.into(),
            reason: reason.into(),
        }
    }

    pub fn policy_blocking(reason: impl Into<String>) -> Self {
        Self::PolicyBlocking {
            reason: reason.into(),
        }
    }

    pub fn openclaw_unavailable(reason: impl Into<String>) -> Self {
        Self::OpenClawUnavailable {
            reason: reason.into(),
        }
    }

    pub fn execution_blocked(reason: impl Into<String>) -> Self {
        Self::ExecutionBlocked {
            reason: reason.into(),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal {
            message: message.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_rejection_display() {
        let err = Error::input_rejection("description is empty");
        assert!(err.to_string().contains("input rejected"));
        assert!(err.to_string().contains("description is empty"));
    }

    #[test]
    fn provider_failure_display() {
        let err = Error::provider_failure("brave", "401 Unauthorized");
        assert!(err.to_string().contains("provider 'brave' failed"));
        assert!(err.to_string().contains("401 Unauthorized"));
    }

    #[test]
    fn candidate_rejection_display() {
        let err = Error::candidate_rejection("c-1", "below quality threshold");
        assert!(err.to_string().contains("candidate 'c-1' rejected"));
    }

    #[test]
    fn retrieval_failure_with_candidate_display() {
        let err = Error::retrieval_failure(Some("c-2"), "web_fetch", "connection timeout");
        assert!(err.to_string().contains("retrieval failed for 'c-2'"));
    }

    #[test]
    fn openclaw_unavailable_display() {
        let err = Error::openclaw_unavailable("no production endpoint configured");
        assert!(err.to_string().contains("OpenClaw unavailable"));
    }

    #[test]
    fn execution_blocked_display() {
        let err = Error::execution_blocked("OpenClaw missing");
        assert!(err.to_string().contains("execution blocked"));
    }

    #[test]
    fn internal_error_display() {
        let err = Error::internal("config file not found");
        assert!(err.to_string().contains("internal error"));
    }

    #[test]
    fn diagnostic_builder() {
        let diag = Diagnostic::new("limited_delivery", "Only 1 of 3 delivered.")
            .with_item(DiagnosticItem {
                level: DiagnosticLevel::Error,
                category: "candidate shortage".into(),
                message: "only 5 candidates found, target was 60".into(),
            })
            .with_item(DiagnosticItem {
                level: DiagnosticLevel::Warning,
                category: "channel fallback".into(),
                message: "fell back from web_fetch to self_hosted".into(),
            });

        assert_eq!(diag.status, "limited_delivery");
        assert_eq!(diag.items.len(), 2);
        assert_eq!(diag.items[0].level, DiagnosticLevel::Error);
    }
}
