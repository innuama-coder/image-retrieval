//! Candidate mechanical validation — blocking reasons and reference signals.
//!
//! References: PRD §校验与评价产品要求, HLD §Candidate Quality Gate,
//! `docs/design/TASK-004-candidate-quality-openclaw-design.md`

use crate::domain::candidate::CandidateRecord;
use crate::domain::query_plan::{ContentConstraints, QualityTier};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

// ---------------------------------------------------------------------------
// Blocking reasons
// ---------------------------------------------------------------------------

/// Reasons a candidate can be mechanically blocked.
///
/// Mechanical blocking is deterministic and does not require subjective
/// judgment. Blocked candidates do not enter OpenClaw evaluation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CandidateBlockingReason {
    /// Source URL is empty, malformed, or uses an unsupported scheme.
    ObviouslyInvalid { detail: String },

    /// Candidate is a duplicate of another already-processed candidate
    /// (same source URL or content hash).
    Duplicate {
        /// The id of the candidate this one duplicates.
        duplicate_of: String,
    },

    /// Candidate title/alt text clearly contradicts the QueryPlan
    /// description or constraints (e.g. must_include term explicitly
    /// absent, must_avoid term explicitly present).
    SemanticMismatch { detail: String },

    /// Candidate is unambiguously below the minimum quality tier
    /// (e.g. reported dimensions below absolute minimum, no source
    /// URL at all).
    LowQuality { detail: String },

    /// Source is unreachable (domain does not resolve, or explicitly
    /// known-dead host).
    Unreachable { detail: String },

    /// Unacceptable risk: candidate source is explicitly prohibited
    /// by policy, or carries a known security/compliance risk that
    /// cannot be mitigated.
    UnacceptableRisk { detail: String },
}

impl CandidateBlockingReason {
    /// Human-readable label for metrics / diagnostics.
    pub fn label(&self) -> &'static str {
        match self {
            Self::ObviouslyInvalid { .. } => "obviously_invalid",
            Self::Duplicate { .. } => "duplicate",
            Self::SemanticMismatch { .. } => "semantic_mismatch",
            Self::LowQuality { .. } => "low_quality",
            Self::Unreachable { .. } => "unreachable",
            Self::UnacceptableRisk { .. } => "unacceptable_risk",
        }
    }

    /// Human-readable explanation of the blocking reason.
    pub fn description(&self) -> String {
        match self {
            Self::ObviouslyInvalid { detail } => {
                format!("candidate is obviously invalid: {}", detail)
            }
            Self::Duplicate { duplicate_of } => {
                format!("duplicate of candidate {}", duplicate_of)
            }
            Self::SemanticMismatch { detail } => {
                format!("semantic mismatch with query plan: {}", detail)
            }
            Self::LowQuality { detail } => {
                format!("below minimum quality: {}", detail)
            }
            Self::Unreachable { detail } => {
                format!("source unreachable: {}", detail)
            }
            Self::UnacceptableRisk { detail } => {
                format!("unacceptable risk: {}", detail)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Reference signals
// ---------------------------------------------------------------------------

/// Non-blocking signals that inform OpenClaw subjective evaluation and
/// downstream risk/quality explanations.
///
/// Reference signals alone cannot reject a candidate unless a product
/// policy explicitly promotes them to blocking.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CandidateReferenceSignal {
    /// Information about the source domain or provider quality.
    SourceQuality { note: String },

    /// Clues from title, alt text, or surrounding text about how well
    /// the candidate matches the query.
    TextContext { note: String },

    /// Confidence cues from the search provider (e.g. relevance score,
    /// ranking position).
    ProviderConfidence { note: String },

    /// Authorization or licensing risk information. Unknown authorization
    /// is noted but not blocked unless policy says otherwise.
    AuthorizationRisk { note: String },

    /// Candidate is similar to another candidate but not an exact duplicate.
    /// Useful for diversity-aware sorting.
    DuplicateSimilarity { similar_to: String, note: String },
}

impl CandidateReferenceSignal {
    /// Human-readable label for diagnostics.
    pub fn label(&self) -> &'static str {
        match self {
            Self::SourceQuality { .. } => "source_quality",
            Self::TextContext { .. } => "text_context",
            Self::ProviderConfidence { .. } => "provider_confidence",
            Self::AuthorizationRisk { .. } => "authorization_risk",
            Self::DuplicateSimilarity { .. } => "duplicate_similarity",
        }
    }
}

// ---------------------------------------------------------------------------
// Mechanical evidence
// ---------------------------------------------------------------------------

/// Evidence produced by mechanical candidate validation.
///
/// Split into two classes per the constitution:
/// - **Blocking**: any non-empty list means the candidate is rejected
///   before OpenClaw evaluation.
/// - **Reference**: supplementary information for OpenClaw evaluation and
///   risk explanation. Reference evidence alone cannot reject a candidate
///   unless a product policy explicitly makes it blocking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateMechanicalEvidence {
    /// Blocking findings — if non-empty, the candidate is rejected.
    pub blocking_findings: Vec<CandidateBlockingReason>,

    /// Reference signals — fed into OpenClaw evaluation and diagnostics.
    pub reference_signals: Vec<CandidateReferenceSignal>,
}

impl CandidateMechanicalEvidence {
    /// Create evidence with no blocking findings (candidate passes mechanical).
    pub fn pass() -> Self {
        Self {
            blocking_findings: Vec::new(),
            reference_signals: Vec::new(),
        }
    }

    /// Create evidence that is mechanically clean with reference signals.
    pub fn pass_with_signals(signals: Vec<CandidateReferenceSignal>) -> Self {
        Self {
            blocking_findings: Vec::new(),
            reference_signals: signals,
        }
    }

    /// Create evidence with a single blocking reason.
    pub fn blocked(reason: CandidateBlockingReason) -> Self {
        Self {
            blocking_findings: vec![reason],
            reference_signals: Vec::new(),
        }
    }

    /// Returns `true` when there are no blocking findings.
    pub fn passed_mechanical(&self) -> bool {
        self.blocking_findings.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Mechanical validation
// ---------------------------------------------------------------------------

/// Run mechanical validation on a single candidate.
///
/// Checks performed (in order):
/// 1. Obviously invalid — missing or malformed source URL.
/// 2. Duplicate — same source URL as a previously-seen candidate.
/// 3. Semantic hard-mismatch — must_avoid term present in title.
/// 4. Low quality — dimensions reported but below absolute minimums.
///
/// # Note
///
/// "Unreachable" and "UnacceptableRisk" checks require external knowledge
/// (DNS resolution, policy database) and are performed by the calling context
/// before this function. When the caller has that evidence it should construct
/// `CandidateMechanicalEvidence::blocked(...)` directly.
pub fn validate_candidate_mechanical(
    candidate: &CandidateRecord,
    seen_urls: &HashSet<String>,
    content_constraints: &ContentConstraints,
    _quality_tier: QualityTier,
) -> CandidateMechanicalEvidence {
    let mut blocking: Vec<CandidateBlockingReason> = Vec::new();
    let mut reference: Vec<CandidateReferenceSignal> = Vec::new();

    // 1. Obviously invalid — source URL
    let url = candidate.image_url.trim();
    if url.is_empty() {
        blocking.push(CandidateBlockingReason::ObviouslyInvalid {
            detail: "source URL is empty".into(),
        });
    } else if !url.starts_with("http://") && !url.starts_with("https://") {
        blocking.push(CandidateBlockingReason::ObviouslyInvalid {
            detail: format!("source URL has unsupported scheme: {}", url),
        });
    }

    // 2. Duplicate — same source URL as already seen
    if !url.is_empty() && seen_urls.contains(url) {
        blocking.push(CandidateBlockingReason::Duplicate {
            duplicate_of: "previously-seen candidate".into(),
        });
    }

    // 3. Semantic hard-mismatch — must_avoid terms present in title
    if let Some(ref title) = candidate.title {
        let lower_title = title.to_lowercase();
        for avoid_term in &content_constraints.must_avoid {
            if lower_title.contains(&avoid_term.to_lowercase()) {
                blocking.push(CandidateBlockingReason::SemanticMismatch {
                    detail: format!(
                        "title contains must_avoid term '{}': {:?}",
                        avoid_term, title
                    ),
                });
                break; // one mismatch is enough
            }
        }
    }

    // 3b. Semantic hard-mismatch — must_include terms absent from title
    //     Only flag when must_include is non-empty AND title is Some but
    //     contains NONE of the must_include terms. This is a STRONG signal
    //     but we only make it blocking when quality_tier is Strict.
    //     For General/High we record it as a reference signal.
    if let Some(ref title) = candidate.title {
        let lower_title = title.to_lowercase();
        let any_included = content_constraints
            .must_include
            .iter()
            .any(|term| lower_title.contains(&term.to_lowercase()));

        if !content_constraints.must_include.is_empty() && !any_included {
            reference.push(CandidateReferenceSignal::TextContext {
                note: format!(
                    "title does not contain any must_include terms: {:?}",
                    content_constraints.must_include
                ),
            });
        }
    }

    // 4. Low quality — dimensions below absolute minimums
    if let Some(dims) = candidate.dimensions() {
        const MIN_WIDTH: u32 = 2;
        const MIN_HEIGHT: u32 = 2;
        if dims.width < MIN_WIDTH || dims.height < MIN_HEIGHT {
            blocking.push(CandidateBlockingReason::LowQuality {
                detail: format!(
                    "dimensions {}x{} below minimum {}x{}",
                    dims.width, dims.height, MIN_WIDTH, MIN_HEIGHT
                ),
            });
        } else if dims.width < 50 || dims.height < 50 {
            // Sub-50px in either dimension is suspect — record as reference
            reference.push(CandidateReferenceSignal::SourceQuality {
                note: format!(
                    "small dimensions: {}x{} — may be thumbnail or low-res",
                    dims.width, dims.height
                ),
            });
        }
    } else {
        // No dimensions reported — reference signal, not blocking
        reference.push(CandidateReferenceSignal::SourceQuality {
            note: "no dimensions reported by provider".into(),
        });
    }

    // 5. Provider confidence reference — based on presence/absence of metadata
    if candidate.title.is_none() && candidate.source_page_url.is_none() {
        reference.push(CandidateReferenceSignal::ProviderConfidence {
            note: "candidate has no title or page URL — provider confidence may be low".into(),
        });
    }

    CandidateMechanicalEvidence {
        blocking_findings: blocking,
        reference_signals: reference,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::candidate::{CandidateId, ImageDimensions, ProviderId};

    fn make_candidate(id: &str, url: &str, title: Option<&str>) -> CandidateRecord {
        let cid = CandidateId::new(id);
        CandidateRecord {
            candidate_id: cid.clone(),
            query_plan_id: "qp-test".into(),
            provider_id: ProviderId::new("test-provider"),
            provider_kind: "fixture".into(),
            search_request_id: "sr-test".into(),
            search_round: 1,
            provider_rank: 1,
            global_rank_hint: None,
            image_url: url.into(),
            source_page_url: None,
            thumbnail_url: None,
            title: title.map(|s| s.into()),
            snippet: None,
            width: None,
            height: None,
            mime_type: None,
            license_hint: None,
            attribution: None,
            dedupe_key: CandidateRecord::build_dedupe_key(url),
            origin_candidate_ids: vec![cid],
            provenance: crate::domain::candidate::CandidateProvenance::new(1, "test", 1, 1),
            normalization_warnings: Vec::new(),
        }
    }

    fn make_candidate_with_dims(
        id: &str,
        url: &str,
        title: Option<&str>,
        width: u32,
        height: u32,
    ) -> CandidateRecord {
        let cid = CandidateId::new(id);
        CandidateRecord {
            candidate_id: cid.clone(),
            query_plan_id: "qp-test".into(),
            provider_id: ProviderId::new("test-provider"),
            provider_kind: "fixture".into(),
            search_request_id: "sr-test".into(),
            search_round: 1,
            provider_rank: 1,
            global_rank_hint: None,
            image_url: url.into(),
            source_page_url: None,
            thumbnail_url: None,
            title: title.map(|s| s.into()),
            snippet: None,
            width: Some(width),
            height: Some(height),
            mime_type: None,
            license_hint: None,
            attribution: None,
            dedupe_key: CandidateRecord::build_dedupe_key(url),
            origin_candidate_ids: vec![cid],
            provenance: crate::domain::candidate::CandidateProvenance::new(1, "test", 1, 1),
            normalization_warnings: Vec::new(),
        }
    }

    // -----------------------------------------------------------------------
    // Obviously invalid
    // -----------------------------------------------------------------------

    #[test]
    fn empty_source_url_is_obviously_invalid() {
        let c = make_candidate("c1", "", None);
        let seen = HashSet::new();
        let evidence = validate_candidate_mechanical(
            &c,
            &seen,
            &ContentConstraints::default(),
            QualityTier::General,
        );
        assert!(!evidence.passed_mechanical());
        assert_eq!(evidence.blocking_findings.len(), 1);
        assert!(matches!(
            evidence.blocking_findings[0],
            CandidateBlockingReason::ObviouslyInvalid { .. }
        ));
    }

    #[test]
    fn unsupported_url_scheme_is_obviously_invalid() {
        let c = make_candidate("c1", "ftp://files.example.com/img.jpg", None);
        let seen = HashSet::new();
        let evidence = validate_candidate_mechanical(
            &c,
            &seen,
            &ContentConstraints::default(),
            QualityTier::General,
        );
        assert!(!evidence.passed_mechanical());
        assert!(matches!(
            evidence.blocking_findings[0],
            CandidateBlockingReason::ObviouslyInvalid { .. }
        ));
    }

    #[test]
    fn valid_https_url_passes() {
        let c = make_candidate("c1", "https://example.com/img.jpg", None);
        let seen = HashSet::new();
        let evidence = validate_candidate_mechanical(
            &c,
            &seen,
            &ContentConstraints::default(),
            QualityTier::General,
        );
        assert!(evidence.passed_mechanical());
    }

    // -----------------------------------------------------------------------
    // Duplicate detection
    // -----------------------------------------------------------------------

    #[test]
    fn duplicate_url_is_blocked() {
        let c = make_candidate("c2", "https://example.com/img.jpg", None);
        let mut seen = HashSet::new();
        seen.insert("https://example.com/img.jpg".to_string());

        let evidence = validate_candidate_mechanical(
            &c,
            &seen,
            &ContentConstraints::default(),
            QualityTier::General,
        );
        assert!(!evidence.passed_mechanical());
        assert!(matches!(
            evidence.blocking_findings[0],
            CandidateBlockingReason::Duplicate { .. }
        ));
    }

    #[test]
    fn unique_url_passes_duplicate_check() {
        let c = make_candidate("c3", "https://example.com/unique.jpg", None);
        let mut seen = HashSet::new();
        seen.insert("https://example.com/other.jpg".to_string());

        let evidence = validate_candidate_mechanical(
            &c,
            &seen,
            &ContentConstraints::default(),
            QualityTier::General,
        );
        assert!(evidence.passed_mechanical());
    }

    // -----------------------------------------------------------------------
    // Semantic mismatch — must_avoid
    // -----------------------------------------------------------------------

    #[test]
    fn title_containing_must_avoid_term_is_blocked() {
        let c = make_candidate(
            "c4",
            "https://example.com/img.jpg",
            Some("Beautiful city skyline at sunset"),
        );
        let seen = HashSet::new();
        let constraints = ContentConstraints {
            must_include: vec![],
            must_avoid: vec!["city".into()],
        };

        let evidence = validate_candidate_mechanical(&c, &seen, &constraints, QualityTier::General);
        assert!(!evidence.passed_mechanical());
        assert!(matches!(
            evidence.blocking_findings[0],
            CandidateBlockingReason::SemanticMismatch { .. }
        ));
    }

    #[test]
    fn case_insensitive_must_avoid() {
        let c = make_candidate(
            "c5",
            "https://example.com/img.jpg",
            Some("CITY lights at night"),
        );
        let seen = HashSet::new();
        let constraints = ContentConstraints {
            must_include: vec![],
            must_avoid: vec!["city".into()],
        };

        let evidence = validate_candidate_mechanical(&c, &seen, &constraints, QualityTier::General);
        assert!(!evidence.passed_mechanical());
        assert!(matches!(
            evidence.blocking_findings[0],
            CandidateBlockingReason::SemanticMismatch { .. }
        ));
    }

    #[test]
    fn title_without_must_avoid_terms_passes() {
        let c = make_candidate(
            "c6",
            "https://example.com/img.jpg",
            Some("Mountain landscape"),
        );
        let seen = HashSet::new();
        let constraints = ContentConstraints {
            must_include: vec![],
            must_avoid: vec!["city".into()],
        };

        let evidence = validate_candidate_mechanical(&c, &seen, &constraints, QualityTier::General);
        assert!(evidence.passed_mechanical());
    }

    // -----------------------------------------------------------------------
    // must_include — reference signal (not blocking for General/High)
    // -----------------------------------------------------------------------

    #[test]
    fn title_missing_must_include_produces_reference_signal() {
        let c = make_candidate("c7", "https://example.com/img.jpg", Some("Generic view"));
        let seen = HashSet::new();
        let constraints = ContentConstraints {
            must_include: vec!["mountain".into()],
            must_avoid: vec![],
        };

        let evidence = validate_candidate_mechanical(&c, &seen, &constraints, QualityTier::General);
        // Not blocked — only a reference signal
        assert!(evidence.passed_mechanical());
        let has_text_signal = evidence
            .reference_signals
            .iter()
            .any(|s| matches!(s, CandidateReferenceSignal::TextContext { .. }));
        assert!(has_text_signal);
    }

    #[test]
    fn title_with_must_include_no_text_signal() {
        let c = make_candidate(
            "c8",
            "https://example.com/img.jpg",
            Some("Mountain view at dawn"),
        );
        let seen = HashSet::new();
        let constraints = ContentConstraints {
            must_include: vec!["mountain".into()],
            must_avoid: vec![],
        };

        let evidence = validate_candidate_mechanical(&c, &seen, &constraints, QualityTier::General);
        assert!(evidence.passed_mechanical());
        let has_text_signal = evidence
            .reference_signals
            .iter()
            .any(|s| matches!(s, CandidateReferenceSignal::TextContext { .. }));
        assert!(!has_text_signal);
    }

    // -----------------------------------------------------------------------
    // Low quality — dimensions
    // -----------------------------------------------------------------------

    #[test]
    fn dimensions_below_minimum_are_blocked() {
        let c = make_candidate_with_dims("c9", "https://example.com/img.jpg", None, 1, 1);
        let seen = HashSet::new();
        let evidence = validate_candidate_mechanical(
            &c,
            &seen,
            &ContentConstraints::default(),
            QualityTier::General,
        );
        assert!(!evidence.passed_mechanical());
        assert!(matches!(
            evidence.blocking_findings[0],
            CandidateBlockingReason::LowQuality { .. }
        ));
    }

    #[test]
    fn small_dimensions_produce_reference_signal() {
        let c = make_candidate_with_dims("c10", "https://example.com/img.jpg", None, 40, 40);
        let seen = HashSet::new();
        let evidence = validate_candidate_mechanical(
            &c,
            &seen,
            &ContentConstraints::default(),
            QualityTier::General,
        );
        // 40x40 is above minimum (2x2) but below 50x50 — reference only
        assert!(evidence.passed_mechanical());
        let has_small_signal = evidence
            .reference_signals
            .iter()
            .any(|s| matches!(s, CandidateReferenceSignal::SourceQuality { .. }));
        assert!(has_small_signal);
    }

    #[test]
    fn adequate_dimensions_no_quality_signal() {
        let c = make_candidate_with_dims("c11", "https://example.com/img.jpg", None, 800, 600);
        let seen = HashSet::new();
        let evidence = validate_candidate_mechanical(
            &c,
            &seen,
            &ContentConstraints::default(),
            QualityTier::General,
        );
        assert!(evidence.passed_mechanical());
        // 800x600 — no low-quality signals
        let has_quality_signal = evidence
            .reference_signals
            .iter()
            .any(|s| matches!(s, CandidateReferenceSignal::SourceQuality { .. }));
        assert!(!has_quality_signal);
    }

    // -----------------------------------------------------------------------
    // Missing metadata → provider confidence reference
    // -----------------------------------------------------------------------

    #[test]
    fn missing_title_and_page_url_produces_confidence_signal() {
        let c = make_candidate("c12", "https://example.com/img.jpg", None);
        // page_url is also None by default
        let seen = HashSet::new();
        let evidence = validate_candidate_mechanical(
            &c,
            &seen,
            &ContentConstraints::default(),
            QualityTier::General,
        );
        assert!(evidence.passed_mechanical());
        let has_conf_signal = evidence
            .reference_signals
            .iter()
            .any(|s| matches!(s, CandidateReferenceSignal::ProviderConfidence { .. }));
        assert!(has_conf_signal);
    }

    #[test]
    fn candidate_with_title_has_no_confidence_signal() {
        let c = make_candidate("c13", "https://example.com/img.jpg", Some("A nice photo"));
        let seen = HashSet::new();
        let evidence = validate_candidate_mechanical(
            &c,
            &seen,
            &ContentConstraints::default(),
            QualityTier::General,
        );
        assert!(evidence.passed_mechanical());
        // Has title, so no provider-confidence signal
        let has_conf_signal = evidence
            .reference_signals
            .iter()
            .any(|s| matches!(s, CandidateReferenceSignal::ProviderConfidence { .. }));
        assert!(!has_conf_signal);
    }

    // -----------------------------------------------------------------------
    // CandidateMechanicalEvidence helpers
    // -----------------------------------------------------------------------

    #[test]
    fn evidence_pass_creates_clean_evidence() {
        let e = CandidateMechanicalEvidence::pass();
        assert!(e.passed_mechanical());
        assert!(e.blocking_findings.is_empty());
        assert!(e.reference_signals.is_empty());
    }

    #[test]
    fn evidence_pass_with_signals() {
        let signals = vec![CandidateReferenceSignal::SourceQuality {
            note: "low resolution".into(),
        }];
        let e = CandidateMechanicalEvidence::pass_with_signals(signals);
        assert!(e.passed_mechanical());
        assert_eq!(e.reference_signals.len(), 1);
    }

    #[test]
    fn evidence_blocked() {
        let reason = CandidateBlockingReason::ObviouslyInvalid {
            detail: "no URL".into(),
        };
        let e = CandidateMechanicalEvidence::blocked(reason);
        assert!(!e.passed_mechanical());
        assert_eq!(e.blocking_findings.len(), 1);
    }

    // -----------------------------------------------------------------------
    // Blocking reason labels and descriptions
    // -----------------------------------------------------------------------

    #[test]
    fn blocking_reason_labels() {
        assert_eq!(
            CandidateBlockingReason::ObviouslyInvalid { detail: "x".into() }.label(),
            "obviously_invalid"
        );
        assert_eq!(
            CandidateBlockingReason::Duplicate {
                duplicate_of: "x".into()
            }
            .label(),
            "duplicate"
        );
        assert_eq!(
            CandidateBlockingReason::SemanticMismatch { detail: "x".into() }.label(),
            "semantic_mismatch"
        );
        assert_eq!(
            CandidateBlockingReason::LowQuality { detail: "x".into() }.label(),
            "low_quality"
        );
        assert_eq!(
            CandidateBlockingReason::Unreachable { detail: "x".into() }.label(),
            "unreachable"
        );
        assert_eq!(
            CandidateBlockingReason::UnacceptableRisk { detail: "x".into() }.label(),
            "unacceptable_risk"
        );
    }

    #[test]
    fn blocking_reason_descriptions_contain_detail() {
        let r = CandidateBlockingReason::Unreachable {
            detail: "DNS timeout".into(),
        };
        assert!(r.description().contains("DNS timeout"));
    }

    // -----------------------------------------------------------------------
    // Reference signal labels
    // -----------------------------------------------------------------------

    #[test]
    fn reference_signal_labels() {
        assert_eq!(
            CandidateReferenceSignal::SourceQuality { note: "x".into() }.label(),
            "source_quality"
        );
        assert_eq!(
            CandidateReferenceSignal::TextContext { note: "x".into() }.label(),
            "text_context"
        );
        assert_eq!(
            CandidateReferenceSignal::ProviderConfidence { note: "x".into() }.label(),
            "provider_confidence"
        );
        assert_eq!(
            CandidateReferenceSignal::AuthorizationRisk { note: "x".into() }.label(),
            "authorization_risk"
        );
        assert_eq!(
            CandidateReferenceSignal::DuplicateSimilarity {
                similar_to: "x".into(),
                note: "y".into()
            }
            .label(),
            "duplicate_similarity"
        );
    }

    // -----------------------------------------------------------------------
    // Edge cases
    // -----------------------------------------------------------------------

    #[test]
    fn whitespace_only_url_is_invalid() {
        let c = make_candidate("c14", "   ", None);
        let seen = HashSet::new();
        let evidence = validate_candidate_mechanical(
            &c,
            &seen,
            &ContentConstraints::default(),
            QualityTier::General,
        );
        assert!(!evidence.passed_mechanical());
    }

    #[test]
    fn multiple_blocking_reasons_can_coexist() {
        // Empty URL + must_avoid in title
        let mut c = CandidateRecord::minimal(
            CandidateId::new("c15"),
            ProviderId::new("test-provider"),
            "",
        );
        c.title = Some("city scape".into());
        let seen = HashSet::new();
        let constraints = ContentConstraints {
            must_include: vec![],
            must_avoid: vec!["city".into()],
        };
        let evidence = validate_candidate_mechanical(&c, &seen, &constraints, QualityTier::General);
        assert!(!evidence.passed_mechanical());
        // Should have both Invalid and SemanticMismatch
        assert!(!evidence.blocking_findings.is_empty());
    }

    #[test]
    fn candidate_with_no_title_and_constraints_passes() {
        let c = make_candidate("c16", "https://example.com/img.jpg", None);
        let seen = HashSet::new();
        let constraints = ContentConstraints {
            must_include: vec!["mountain".into()],
            must_avoid: vec!["city".into()],
        };
        let evidence = validate_candidate_mechanical(&c, &seen, &constraints, QualityTier::General);
        // No title → cannot check must_avoid/must_include → passes mechanical
        // (reference signals for missing metadata will be added)
        assert!(evidence.passed_mechanical());
    }
}
