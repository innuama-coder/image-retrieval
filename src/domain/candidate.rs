//! Candidate domain types.
//!
//! Covers provider identity, candidate records, source tracking,
//! provenance, deduplication, and candidate-level decisions after
//! mechanical validation and VLM subjective evaluation.
//!
//! v1.1 expanded fields per LLD and
//! `docs/design/v1.1-TASK-002-search-provider-candidate-design.md`.
//!
//! References: PRD FR-004/FR-005, HLD §Candidate Quality Gate, LLD §Candidate Record

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Provider identity
// ---------------------------------------------------------------------------

/// Opaque identifier for a search provider.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub struct ProviderId(pub String);

impl ProviderId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl std::fmt::Display for ProviderId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ---------------------------------------------------------------------------
// Candidate identity
// ---------------------------------------------------------------------------

/// System-assigned candidate identifier.
///
/// Deterministic within a run:
/// `cand-{query_plan_id}-{stable_hash(provider_id, search_round, ...)}`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub struct CandidateId(pub String);

impl CandidateId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl std::fmt::Display for CandidateId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ---------------------------------------------------------------------------
// Image dimensions
// ---------------------------------------------------------------------------

/// Image dimensions in pixels.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ImageDimensions {
    pub width: u32,
    pub height: u32,
}

// ---------------------------------------------------------------------------
// License evidence
// ---------------------------------------------------------------------------

/// What evidence was found about the candidate's license.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LicenseEvidence {
    /// License explicitly identified (e.g. "CC BY 2.0").
    Identified { label: String, source: String },
    /// License hint from provider but not verified.
    Hinted { label: String },
    /// No license information available.
    Unknown,
    /// License is explicitly marked as "all rights reserved" or similar.
    Restricted { label: Option<String> },
}

impl Default for LicenseEvidence {
    fn default() -> Self {
        Self::Unknown
    }
}

// ---------------------------------------------------------------------------
// Candidate provenance
// ---------------------------------------------------------------------------

/// Provenance of a single candidate record — tracks how and when it was
/// discovered.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateProvenance {
    /// Provider's raw identifier for this result, if any.
    pub provider_raw_id: Option<String>,

    /// Provider's result page URL, if available.
    pub provider_result_url: Option<String>,

    /// Rank position within the provider's result set (1-based).
    pub provider_rank: u32,

    /// The search query text that produced this candidate.
    pub search_query: String,

    /// Which search round (1-based) within the session.
    pub search_round: u32,

    /// Full attempt count at discovery time.
    pub full_attempt_count: u8,

    /// Timestamp when the candidate was retrieved from the provider.
    pub retrieved_at: String,

    /// References to provider evidence (e.g. SerpApi position, page token).
    pub provider_evidence_refs: Vec<String>,

    /// License evidence from the provider.
    pub license_evidence: LicenseEvidence,

    /// Domain authority hint (e.g. "wikipedia.org", "unsplash.com").
    pub source_authority_hint: Option<String>,
}

impl CandidateProvenance {
    pub fn new(
        provider_rank: u32,
        search_query: impl Into<String>,
        search_round: u32,
        full_attempt_count: u8,
    ) -> Self {
        Self {
            provider_raw_id: None,
            provider_result_url: None,
            provider_rank,
            search_query: search_query.into(),
            search_round,
            full_attempt_count,
            retrieved_at: String::new(),
            provider_evidence_refs: Vec::new(),
            license_evidence: LicenseEvidence::Unknown,
            source_authority_hint: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Candidate record — canonical v1.1
// ---------------------------------------------------------------------------

/// A single search result candidate before quality gating.
///
/// v1.1 expanded fields per LLD:
/// - `image_url` is the canonical direct image URL (required).
/// - `source_page_url` is the page containing the image (preferred).
/// - `thumbnail_url` is a smaller preview.
/// - `provider_rank` is the 1-based position in the provider's result set.
/// - `dedupe_key` enables cross-provider deduplication.
/// - `origin_candidate_ids` tracks merged duplicates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateRecord {
    /// System-assigned candidate identifier.
    pub candidate_id: CandidateId,

    /// Query plan that caused this candidate to be discovered.
    pub query_plan_id: String,

    /// Which provider produced this candidate.
    pub provider_id: ProviderId,

    /// The adapter family (serpapi_google_images, fixture, etc.).
    pub provider_kind: String,

    /// Search request that produced this candidate.
    pub search_request_id: String,

    /// Which search round (1-based) within the session.
    pub search_round: u32,

    /// 1-based rank within the provider's result set.
    pub provider_rank: u32,

    /// Optional global rank hint across all providers.
    pub global_rank_hint: Option<u32>,

    /// Direct image URL. Required — candidates without this are diagnosed
    /// and excluded.
    pub image_url: String,

    /// Source page URL where the image was found.
    pub source_page_url: Option<String>,

    /// Thumbnail / preview URL.
    pub thumbnail_url: Option<String>,

    /// Title / alt text from the search result.
    pub title: Option<String>,

    /// Snippet / description from the search result.
    pub snippet: Option<String>,

    /// Image width in pixels, if reported by the provider.
    pub width: Option<u32>,

    /// Image height in pixels, if reported by the provider.
    pub height: Option<u32>,

    /// MIME type hint (e.g. "image/jpeg").
    pub mime_type: Option<String>,

    /// License hint from the provider.
    pub license_hint: Option<String>,

    /// Attribution string, if provided.
    pub attribution: Option<String>,

    /// Deduplication key for cross-provider merging.
    pub dedupe_key: String,

    /// IDs of all candidates merged into this record (includes own id).
    pub origin_candidate_ids: Vec<CandidateId>,

    /// Full provenance metadata.
    pub provenance: CandidateProvenance,

    /// Non-blocking warnings from normalization (e.g. missing dimensions).
    pub normalization_warnings: Vec<String>,
}

impl CandidateRecord {
    /// Returns `true` if the candidate has a usable image URL.
    pub fn has_image_url(&self) -> bool {
        !self.image_url.is_empty()
    }

    /// Create a minimal candidate with safe defaults for testing.
    ///
    /// Only `candidate_id`, `provider_id`, `image_url`, and `dedupe_key` must
    /// be set by the caller. All other fields get safe defaults.
    pub fn minimal(
        candidate_id: CandidateId,
        provider_id: ProviderId,
        image_url: impl Into<String>,
    ) -> Self {
        let image_url = image_url.into();
        let dedupe_key = Self::build_dedupe_key(&image_url);
        Self {
            candidate_id: candidate_id.clone(),
            query_plan_id: String::new(),
            provider_id,
            provider_kind: String::new(),
            search_request_id: String::new(),
            search_round: 1,
            provider_rank: 1,
            global_rank_hint: None,
            image_url,
            source_page_url: None,
            thumbnail_url: None,
            title: None,
            snippet: None,
            width: None,
            height: None,
            mime_type: None,
            license_hint: None,
            attribution: None,
            dedupe_key,
            origin_candidate_ids: vec![candidate_id],
            provenance: CandidateProvenance::new(1, String::new(), 1, 1),
            normalization_warnings: Vec::new(),
        }
    }

    // ------------------------------------------------------------------
    // Backward-compatible accessors (for code not yet migrated to v1.1)
    // ------------------------------------------------------------------

    /// Legacy accessor for `image_url`.
    #[deprecated(note = "use `image_url` field directly")]
    pub fn source_url(&self) -> &str {
        &self.image_url
    }

    /// Legacy accessor for `source_page_url`.
    #[deprecated(note = "use `source_page_url` field directly")]
    pub fn page_url(&self) -> Option<&str> {
        self.source_page_url.as_deref()
    }

    /// Legacy dimensions accessor.
    #[deprecated(note = "use `width` and `height` fields directly")]
    pub fn dimensions(&self) -> Option<ImageDimensions> {
        match (self.width, self.height) {
            (Some(w), Some(h)) => Some(ImageDimensions {
                width: w,
                height: h,
            }),
            _ => None,
        }
    }

    /// Build a [`CandidateId`] deterministically within a run.
    pub fn build_candidate_id(
        query_plan_id: &str,
        provider_id: &ProviderId,
        search_round: u32,
        provider_rank: u32,
        image_url: &str,
    ) -> CandidateId {
        // Simple deterministic id: hash the components
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        query_plan_id.hash(&mut hasher);
        provider_id.hash(&mut hasher);
        search_round.hash(&mut hasher);
        provider_rank.hash(&mut hasher);
        image_url.hash(&mut hasher);
        CandidateId::new(format!("cand-{}-{:016x}", query_plan_id, hasher.finish()))
    }

    /// Build a simple dedupe key from the normalized image URL.
    pub fn build_dedupe_key(image_url: &str) -> String {
        normalize_url_for_dedupe(image_url)
    }
}

/// Normalize a URL for deduplication: lowercase host, strip tracking
/// query params and fragment.
fn normalize_url_for_dedupe(url: &str) -> String {
    let lower = url.to_lowercase();
    let without_fragment = match lower.find('#') {
        Some(pos) => &lower[..pos],
        None => &lower,
    };
    // Strip common tracking parameters
    strip_tracking_params(without_fragment)
}

fn strip_tracking_params(url: &str) -> String {
    match url.find('?') {
        None => url.to_string(),
        Some(q_pos) => {
            let base = &url[..q_pos];
            let query = &url[q_pos + 1..];
            let kept_params: Vec<&str> = query
                .split('&')
                .filter(|p| {
                    let p_lower = p.to_lowercase();
                    !p_lower.starts_with("utm_")
                        && !p_lower.starts_with("fbclid=")
                        && !p_lower.starts_with("gclid=")
                        && !p_lower.starts_with("ref=")
                        && !p_lower.starts_with("source=")
                })
                .collect();
            if kept_params.is_empty() {
                base.to_string()
            } else {
                format!("{}?{}", base, kept_params.join("&"))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Deduplication types
// ---------------------------------------------------------------------------

/// Evidence used for deduplication decisions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateDedupeEvidence {
    /// The computed dedupe key.
    pub dedupe_key: String,

    /// Key derived from the normalized image URL alone.
    pub image_url_key: Option<String>,

    /// Key derived from the normalized source page URL.
    pub source_page_key: Option<String>,

    /// Key derived from the provider's raw result id.
    pub provider_raw_key: Option<String>,

    /// Weak key from dimensions (informational only, never the sole merge reason).
    pub weak_dimension_key: Option<String>,

    /// If this candidate is a duplicate, the id of the canonical record.
    pub duplicate_of: Option<CandidateId>,

    /// Why two candidates were merged.
    pub merge_reason: DedupeMergeReason,
}

/// Why two candidate records were merged into one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DedupeMergeReason {
    /// Not a duplicate — first occurrence.
    Unique,
    /// Exact normalized image URL match.
    ExactImageUrl,
    /// Same provider raw result id from the same provider.
    SameProviderRawId,
    /// Same source page URL plus same image filename.
    SameSourcePageAndFilename,
    /// Duplicate detected but merge reason not classified.
    Other,
}

impl CandidateDedupeEvidence {
    pub fn unique(dedupe_key: impl Into<String>, image_url_key: Option<String>) -> Self {
        Self {
            dedupe_key: dedupe_key.into(),
            image_url_key,
            source_page_key: None,
            provider_raw_key: None,
            weak_dimension_key: None,
            duplicate_of: None,
            merge_reason: DedupeMergeReason::Unique,
        }
    }

    pub fn duplicate(
        dedupe_key: impl Into<String>,
        duplicate_of: CandidateId,
        merge_reason: DedupeMergeReason,
    ) -> Self {
        Self {
            dedupe_key: dedupe_key.into(),
            image_url_key: None,
            source_page_key: None,
            provider_raw_key: None,
            weak_dimension_key: None,
            duplicate_of: Some(duplicate_of),
            merge_reason,
        }
    }
}

// ---------------------------------------------------------------------------
// Source tracking (legacy compat)
// ---------------------------------------------------------------------------

/// Source information for traceability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateSource {
    pub provider_id: ProviderId,
    pub search_query: String,
    pub retrieved_at: String,
}

// ---------------------------------------------------------------------------
// Candidate decision — output of the Candidate Quality Gate
// ---------------------------------------------------------------------------

/// The outcome of candidate mechanical validation + VLM subjective
/// evaluation, normalised into an executable status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CandidateDecision {
    /// Candidate passed both mechanical and subjective checks and may enter
    /// the retrievable sequence.
    Accepted {
        candidate: CandidateRecord,
        /// Priority within the retrievable sequence (higher = fetched sooner).
        priority: u32,
    },

    /// Candidate was mechanically blocked (e.g. unreachable, duplicate,
    /// clearly below quality tier). Includes the blocking reason.
    Rejected {
        candidate: CandidateRecord,
        reason: String,
    },

    /// VLM evaluation was uncertain; candidate is excluded from the
    /// retrievable sequence but is not definitively rejected.
    Uncertain {
        candidate: CandidateRecord,
        reason: String,
    },

    /// Insufficient evidence to decide (e.g. VLM was unavailable and
    /// no fallback is allowed in production).
    ExecutionBlocked {
        candidate: CandidateRecord,
        reason: String,
    },
}

impl CandidateDecision {
    /// Returns `true` iff the candidate was accepted and can enter the
    /// retrievable sequence.
    pub fn is_accepted(&self) -> bool {
        matches!(self, Self::Accepted { .. })
    }
}

// ---------------------------------------------------------------------------
// Retrievable candidate sequence
// ---------------------------------------------------------------------------

/// Ordered sequence of candidates that are eligible for retrieval.
///
/// Only candidates with `CandidateDecision::Accepted` may appear here.
/// The sequence is sorted by descending priority.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievableCandidateSequence {
    pub candidates: Vec<CandidateDecision>,
}

impl RetrievableCandidateSequence {
    pub fn empty() -> Self {
        Self {
            candidates: Vec::new(),
        }
    }

    /// Build from a list of decisions, keeping only accepted ones sorted
    /// by descending priority.
    pub fn from_decisions(decisions: Vec<CandidateDecision>) -> Self {
        let mut accepted: Vec<CandidateDecision> =
            decisions.into_iter().filter(|d| d.is_accepted()).collect();
        accepted.sort_by_key(|d| match d {
            CandidateDecision::Accepted { priority, .. } => std::cmp::Reverse(*priority),
            _ => std::cmp::Reverse(0),
        });
        Self {
            candidates: accepted,
        }
    }

    pub fn len(&self) -> usize {
        self.candidates.len()
    }

    pub fn is_empty(&self) -> bool {
        self.candidates.is_empty()
    }
}

// ---------------------------------------------------------------------------
// v1.1 Candidate quality types per
// `docs/design/v1.1-TASK-003-quality-vlm-design.md`
// ---------------------------------------------------------------------------

/// Mechanical assessment for a single candidate (v1.1).
///
/// Uses typed [`MetricFact`] instead of string-based evidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateMechanicalAssessment {
    /// Candidate being assessed.
    pub candidate_id: CandidateId,
    /// Query plan that originated this evaluation.
    pub query_plan_id: String,
    /// Whether mechanical checks passed.
    pub passed: bool,
    /// Blocking metric facts — any non-empty means mechanical failure.
    pub blocking_metrics: Vec<crate::domain::metrics::MetricFact>,
    /// Reference metric facts — supplementary evidence only.
    pub reference_metrics: Vec<crate::domain::metrics::MetricFact>,
    /// When assessment was performed (ISO 8601).
    pub evaluated_at: String,
}

impl CandidateMechanicalAssessment {
    /// Create a passing mechanical assessment.
    pub fn pass(candidate_id: CandidateId, query_plan_id: impl Into<String>) -> Self {
        Self {
            candidate_id,
            query_plan_id: query_plan_id.into(),
            passed: true,
            blocking_metrics: Vec::new(),
            reference_metrics: Vec::new(),
            evaluated_at: String::new(),
        }
    }

    /// Create a blocked mechanical assessment from blocking facts.
    pub fn blocked(
        candidate_id: CandidateId,
        query_plan_id: impl Into<String>,
        blocking: Vec<crate::domain::metrics::MetricFact>,
    ) -> Self {
        Self {
            candidate_id,
            query_plan_id: query_plan_id.into(),
            passed: false,
            blocking_metrics: blocking,
            reference_metrics: Vec::new(),
            evaluated_at: String::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// VLM evaluation request (candidate phase)
// ---------------------------------------------------------------------------

/// Policy context carried into VLM evaluation requests.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct QualityPolicyContext {
    /// Whether paid retrieval is allowed.
    pub allow_paid: bool,
    /// Whether robots.txt is respected.
    pub respect_robots: bool,
    /// Whether login-required sources are allowed.
    pub allow_login: bool,
    /// Whether paywalled sources are allowed.
    pub allow_paywalled: bool,
    /// Prohibited source domains.
    pub prohibited_domains: Vec<String>,
    /// Whether redaction should be applied.
    pub redact_output: bool,
    /// Whether fixture mode is active (test only).
    pub fixture_mode: bool,
}

/// A single subject within a VLM candidate evaluation request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateEvaluationSubject {
    /// The candidate being evaluated.
    pub candidate: CandidateRecord,
    /// Mechanical assessment (must be `passed = true` to reach VLM).
    pub mechanical_assessment: CandidateMechanicalAssessment,
    /// Reference metrics for VLM context.
    pub reference_metrics: Vec<crate::domain::metrics::MetricFact>,
}

/// Request to evaluate a batch of candidates via VLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VlmCandidateEvaluationRequest {
    /// Unique request identifier.
    pub request_id: String,
    /// Query plan that originated this evaluation.
    pub query_plan_id: String,
    /// Full attempt count from the orchestrator.
    pub full_attempt_count: u8,
    /// Retry count (= full_attempt_count - 1).
    pub retry_count: u8,
    /// Semantic description of what is sought.
    pub semantic_description: String,
    /// Quality tier.
    pub quality: crate::domain::query_plan::QualityTier,
    /// Structured quality requirements.
    pub quality_requirements: crate::domain::query_plan::QualityRequirements,
    /// Visual requirements from the QueryPlan.
    pub visual_requirements: Vec<String>,
    /// Negative scope from the QueryPlan.
    pub negative_scope: Vec<String>,
    /// Candidates to evaluate (must all have passed mechanical).
    pub candidates: Vec<CandidateEvaluationSubject>,
    /// Policy context for the evaluator.
    pub policy_context: QualityPolicyContext,
    /// Model identifier (e.g. "qwen-3.5").
    pub model: String,
    /// Evaluator provider identifier (e.g. "qwen_3_5_vlm").
    pub evaluator_provider_id: String,
    /// Whether fixture mode is active.
    pub fixture_mode: bool,
}

// ---------------------------------------------------------------------------
// VLM evaluation response (shared across phases)
// ---------------------------------------------------------------------------

/// Kind of VLM evaluator that produced the response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VlmEvaluatorKind {
    /// Production Qwen 3.5 VLM.
    #[serde(rename = "qwen_3_5_vlm")]
    Qwen35Vlm,
    /// Fixture evaluator — test only.
    #[serde(rename = "fixture")]
    Fixture,
}

/// Overall status of a VLM evaluation response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VlmResponseStatus {
    /// All submitted subjects received a decision.
    Complete,
    /// Some decisions are missing (cardinality mismatch).
    Incomplete,
    /// The evaluator returned an unexpected error.
    Error,
}

/// Kind of decision a VLM can make about a subject.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VlmSubjectDecisionKind {
    /// VLM explicitly approves the subject.
    #[serde(rename = "approve")]
    Approve,
    /// VLM explicitly rejects the subject.
    #[serde(rename = "reject")]
    Reject,
    /// VLM cannot decide — insufficient information.
    #[serde(rename = "uncertain")]
    Uncertain,
    /// VLM could not evaluate — execution dependency failure.
    #[serde(rename = "unexecutable")]
    Unexecutable,
}

/// Decision for a single subject within a VLM evaluation response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VlmSubjectDecision {
    /// Subject identifier (matches the request subject id).
    pub subject_id: String,
    /// The VLM's decision.
    pub decision: VlmSubjectDecisionKind,
    /// Confidence score from the VLM, if provided (0.0 .. 1.0).
    pub confidence: Option<f32>,
    /// Machine-readable reason codes from the VLM.
    pub reason_codes: Vec<String>,
    /// Human-readable rationale summary (redacted).
    pub rationale_summary: String,
    /// References to evidence (redacted paths only).
    pub evidence_refs: Vec<String>,
}

/// Response from a VLM evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VlmEvaluationResponse {
    /// Echoes the request id.
    pub request_id: String,
    /// Which evaluator produced this response.
    pub evaluator_id: String,
    /// The kind of evaluator.
    pub evaluator_kind: VlmEvaluatorKind,
    /// Overall response status.
    pub status: VlmResponseStatus,
    /// Per-subject decisions (must have exactly one per submitted subject).
    pub decisions: Vec<VlmSubjectDecision>,
    /// Diagnostics from the evaluation.
    pub diagnostics: Vec<crate::domain::metrics::QualityDiagnostic>,
    /// Optional audit reference (package-safe path only).
    pub audit_ref: Option<String>,
    /// Whether redaction was applied to any response text.
    pub redaction_applied: bool,
}

impl VlmEvaluationResponse {
    /// Validate that the response contains exactly one decision per expected subject.
    pub fn validate_cardinality(&self, expected_subject_count: usize) -> Result<(), String> {
        if self.decisions.len() != expected_subject_count {
            return Err(format!(
                "VLM response cardinality mismatch: expected {} decisions, got {}",
                expected_subject_count,
                self.decisions.len()
            ));
        }
        Ok(())
    }

    /// Check if any decision is missing (incomplete status).
    pub fn has_missing_decisions(&self) -> bool {
        self.status == VlmResponseStatus::Incomplete
    }

    /// Check if the evaluation is from a fixture evaluator.
    pub fn is_fixture(&self) -> bool {
        self.evaluator_kind == VlmEvaluatorKind::Fixture
    }
}

// ---------------------------------------------------------------------------
// Candidate quality decision and outcomes
// ---------------------------------------------------------------------------

/// Final status for a candidate after mechanical + VLM evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CandidateQualityStatus {
    /// Candidate passed both mechanical and VLM gates — eligible for retrieval.
    #[serde(rename = "retrievable")]
    Retrievable,
    /// Candidate was mechanically blocked before VLM.
    #[serde(rename = "mechanically_rejected")]
    MechanicallyRejected,
    /// Candidate was rejected by VLM subjective evaluation.
    #[serde(rename = "subjectively_rejected")]
    SubjectivelyRejected,
    /// VLM was uncertain about the candidate.
    #[serde(rename = "subjectively_uncertain")]
    SubjectivelyUncertain,
    /// VLM was unavailable — execution blocked for this subject.
    #[serde(rename = "execution_blocked")]
    ExecutionBlocked,
}

/// Decision for a single candidate after full quality evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateQualityDecision {
    /// Candidate identifier.
    pub candidate_id: CandidateId,
    /// Query plan that originated this evaluation.
    pub query_plan_id: String,
    /// Whether mechanical checks passed.
    pub mechanical_passed: bool,
    /// Whether VLM approved.
    pub vlm_passed: bool,
    /// Final status.
    pub final_status: CandidateQualityStatus,
    /// Priority for retrieval ordering (higher = sooner).
    pub priority: u32,
    /// Blocking metric facts from mechanical evaluation.
    pub blocking_metrics: Vec<crate::domain::metrics::MetricFact>,
    /// Reference metric facts from mechanical evaluation.
    pub reference_metrics: Vec<crate::domain::metrics::MetricFact>,
    /// VLM decision, if VLM was reached.
    pub vlm_decision: Option<VlmSubjectDecision>,
    /// Diagnostics from the evaluation.
    pub diagnostics: Vec<crate::domain::metrics::QualityDiagnostic>,
}

impl CandidateQualityDecision {
    /// Returns `true` iff this candidate is eligible for retrieval.
    pub fn is_retrievable(&self) -> bool {
        self.final_status == CandidateQualityStatus::Retrievable
    }

    /// Build a decision for a mechanically rejected candidate.
    pub fn mechanically_rejected(
        candidate_id: CandidateId,
        query_plan_id: impl Into<String>,
        blocking: Vec<crate::domain::metrics::MetricFact>,
    ) -> Self {
        Self {
            candidate_id,
            query_plan_id: query_plan_id.into(),
            mechanical_passed: false,
            vlm_passed: false,
            final_status: CandidateQualityStatus::MechanicallyRejected,
            priority: 0,
            blocking_metrics: blocking,
            reference_metrics: Vec::new(),
            vlm_decision: None,
            diagnostics: Vec::new(),
        }
    }

    /// Build a decision from merged mechanical + VLM results.
    pub fn merged(
        candidate_id: CandidateId,
        query_plan_id: impl Into<String>,
        mechanical: &CandidateMechanicalAssessment,
        vlm: Option<&VlmSubjectDecision>,
    ) -> Self {
        let vlm_passed = vlm
            .map(|d| d.decision == VlmSubjectDecisionKind::Approve)
            .unwrap_or(false);
        let final_status = match vlm {
            None => CandidateQualityStatus::ExecutionBlocked,
            Some(d) => match d.decision {
                VlmSubjectDecisionKind::Approve => CandidateQualityStatus::Retrievable,
                VlmSubjectDecisionKind::Reject => CandidateQualityStatus::SubjectivelyRejected,
                VlmSubjectDecisionKind::Uncertain => CandidateQualityStatus::SubjectivelyUncertain,
                VlmSubjectDecisionKind::Unexecutable => CandidateQualityStatus::ExecutionBlocked,
            },
        };
        Self {
            candidate_id,
            query_plan_id: query_plan_id.into(),
            mechanical_passed: mechanical.passed,
            vlm_passed,
            final_status,
            priority: if vlm_passed { 5 } else { 0 },
            blocking_metrics: mechanical.blocking_metrics.clone(),
            reference_metrics: mechanical.reference_metrics.clone(),
            vlm_decision: vlm.cloned(),
            diagnostics: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Retrievable candidate — handoff to TASK-004
// ---------------------------------------------------------------------------

/// A single candidate approved for retrieval.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievableCandidate {
    /// The candidate record.
    pub candidate: CandidateRecord,
    /// Full quality decision.
    pub candidate_quality_decision: CandidateQualityDecision,
    /// Retrieval priority (higher = sooner).
    pub retrieval_priority: u32,
    /// Primary image URL for retrieval.
    pub primary_image_url: String,
    /// Source page URL, if available.
    pub source_page_url: Option<String>,
    /// Thumbnail URL, if available.
    pub thumbnail_url: Option<String>,
    /// Expected MIME type, if known.
    pub expected_mime_type: Option<String>,
    /// License or authorization hint, if available.
    pub license_hint: Option<String>,
    /// Provenance references for traceability.
    pub provenance_refs: Vec<String>,
}

/// Batch of retrievable candidates for TASK-004.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievableCandidateBatch {
    /// Query plan that originated the batch.
    pub query_plan_id: String,
    /// Full attempt count from the orchestrator.
    pub full_attempt_count: u8,
    /// Retry count.
    pub retry_count: u8,
    /// Target batch size for retrieval.
    pub retrieval_batch_target: u32,
    /// Candidates approved for retrieval, sorted by priority.
    pub candidates: Vec<RetrievableCandidate>,
    /// Rejected decisions for coverage and manifest.
    pub rejected_decisions: Vec<CandidateQualityDecision>,
    /// Execution blocking facts if any.
    pub execution_blocking_facts: Vec<crate::domain::metrics::QualityExecutionBlock>,
}

impl RetrievableCandidateBatch {
    /// Create an empty batch.
    pub fn empty(
        query_plan_id: impl Into<String>,
        full_attempt_count: u8,
        retry_count: u8,
        retrieval_batch_target: u32,
    ) -> Self {
        Self {
            query_plan_id: query_plan_id.into(),
            full_attempt_count,
            retry_count,
            retrieval_batch_target,
            candidates: Vec::new(),
            rejected_decisions: Vec::new(),
            execution_blocking_facts: Vec::new(),
        }
    }

    /// Number of retrievable candidates in the batch.
    pub fn len(&self) -> usize {
        self.candidates.len()
    }

    /// Whether the batch is empty.
    pub fn is_empty(&self) -> bool {
        self.candidates.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Candidate quality outcome — handoff to TASK-005
// ---------------------------------------------------------------------------

/// Full outcome of the candidate quality phase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateQualityOutcome {
    /// Query plan that was evaluated.
    pub query_plan_id: String,
    /// Full attempt count from the orchestrator.
    pub full_attempt_count: u8,
    /// Retry count.
    pub retry_count: u8,
    /// Per-candidate quality decisions.
    pub decisions: Vec<CandidateQualityDecision>,
    /// Batch of retrievable candidates for TASK-004.
    pub retrievable_batch: RetrievableCandidateBatch,
    /// Diagnostics from the evaluation.
    pub diagnostics: Vec<crate::domain::metrics::QualityDiagnostic>,
    /// Aggregate summary.
    pub summary: crate::domain::metrics::QualitySummary,
}

impl CandidateQualityOutcome {
    /// Build an outcome from decisions and a retrievable batch.
    pub fn new(
        query_plan_id: impl Into<String>,
        full_attempt_count: u8,
        retry_count: u8,
        decisions: Vec<CandidateQualityDecision>,
        retrievable_batch: RetrievableCandidateBatch,
        diagnostics: Vec<crate::domain::metrics::QualityDiagnostic>,
        summary: crate::domain::metrics::QualitySummary,
    ) -> Self {
        Self {
            query_plan_id: query_plan_id.into(),
            full_attempt_count,
            retry_count,
            decisions,
            retrievable_batch,
            diagnostics,
            summary,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_candidate(id: &str, provider: &str, image_url: &str) -> CandidateRecord {
        CandidateRecord {
            candidate_id: CandidateId::new(id),
            query_plan_id: "qp-test".into(),
            provider_id: ProviderId::new(provider),
            provider_kind: "fixture".into(),
            search_request_id: "sr-test".into(),
            search_round: 1,
            provider_rank: 1,
            global_rank_hint: None,
            image_url: image_url.into(),
            source_page_url: None,
            thumbnail_url: None,
            title: None,
            snippet: None,
            width: None,
            height: None,
            mime_type: None,
            license_hint: None,
            attribution: None,
            dedupe_key: CandidateRecord::build_dedupe_key(image_url),
            origin_candidate_ids: vec![CandidateId::new(id)],
            provenance: CandidateProvenance::new(1, "test query", 1, 1),
            normalization_warnings: Vec::new(),
        }
    }

    #[test]
    fn candidate_decision_accepted_is_accepted() {
        let c = make_candidate("img-1", "test-provider", "https://example.com/1.jpg");
        let d = CandidateDecision::Accepted {
            candidate: c,
            priority: 5,
        };
        assert!(d.is_accepted());
    }

    #[test]
    fn candidate_decision_rejected_is_not_accepted() {
        let c = make_candidate("img-2", "test-provider", "https://example.com/2.jpg");
        let d = CandidateDecision::Rejected {
            candidate: c,
            reason: "duplicate".into(),
        };
        assert!(!d.is_accepted());
    }

    #[test]
    fn candidate_decision_uncertain_is_not_accepted() {
        let c = make_candidate("img-3", "test-provider", "https://example.com/3.jpg");
        let d = CandidateDecision::Uncertain {
            candidate: c,
            reason: "ambiguous match".into(),
        };
        assert!(!d.is_accepted());
    }

    #[test]
    fn retrievable_sequence_only_includes_accepted() {
        let c1 = make_candidate("img-a", "p1", "https://ex.com/a.jpg");
        let c2 = make_candidate("img-b", "p1", "https://ex.com/b.jpg");
        let c3 = make_candidate("img-c", "p1", "https://ex.com/c.jpg");

        let decisions = vec![
            CandidateDecision::Accepted {
                candidate: c1,
                priority: 3,
            },
            CandidateDecision::Rejected {
                candidate: c2,
                reason: "low quality".into(),
            },
            CandidateDecision::Accepted {
                candidate: c3,
                priority: 7,
            },
        ];

        let seq = RetrievableCandidateSequence::from_decisions(decisions);
        assert_eq!(seq.len(), 2);
        // Higher priority first
        match &seq.candidates[0] {
            CandidateDecision::Accepted { priority, .. } => assert_eq!(*priority, 7),
            _ => panic!("expected accepted"),
        }
    }

    #[test]
    fn empty_sequence_is_empty() {
        let seq = RetrievableCandidateSequence::empty();
        assert!(seq.is_empty());
        assert_eq!(seq.len(), 0);
    }

    #[test]
    fn provider_id_display() {
        let id = ProviderId::new("brave");
        assert_eq!(id.to_string(), "brave");
    }

    #[test]
    fn candidate_id_display() {
        let id = CandidateId::new("cand-001");
        assert_eq!(id.to_string(), "cand-001");
    }

    #[test]
    fn candidate_build_id_is_deterministic() {
        let a = CandidateRecord::build_candidate_id(
            "qp-1",
            &ProviderId::new("serpapi"),
            1,
            1,
            "https://example.com/1.jpg",
        );
        let b = CandidateRecord::build_candidate_id(
            "qp-1",
            &ProviderId::new("serpapi"),
            1,
            1,
            "https://example.com/1.jpg",
        );
        assert_eq!(a, b);
    }

    #[test]
    fn candidate_build_id_differs_on_url() {
        let a = CandidateRecord::build_candidate_id(
            "qp-1",
            &ProviderId::new("serpapi"),
            1,
            1,
            "https://example.com/1.jpg",
        );
        let b = CandidateRecord::build_candidate_id(
            "qp-1",
            &ProviderId::new("serpapi"),
            1,
            1,
            "https://example.com/2.jpg",
        );
        assert_ne!(a, b);
    }

    #[test]
    fn dedupe_key_normalizes_url() {
        let key1 = CandidateRecord::build_dedupe_key("https://EXAMPLE.com/path?utm_source=x");
        let key2 = CandidateRecord::build_dedupe_key("https://example.com/path");
        assert_eq!(key1, key2, "dedupe keys should match after normalization");
    }

    #[test]
    fn dedupe_key_strips_fragment() {
        let key1 = CandidateRecord::build_dedupe_key("https://example.com/image.jpg#fragment");
        let key2 = CandidateRecord::build_dedupe_key("https://example.com/image.jpg");
        assert_eq!(key1, key2);
    }

    #[test]
    fn dedupe_evidence_unique() {
        let evidence =
            CandidateDedupeEvidence::unique("key-1", Some("https://example.com/1.jpg".into()));
        assert!(evidence.duplicate_of.is_none());
        assert_eq!(evidence.merge_reason, DedupeMergeReason::Unique);
    }

    #[test]
    fn dedupe_evidence_duplicate() {
        let evidence = CandidateDedupeEvidence::duplicate(
            "key-1",
            CandidateId::new("canonical-1"),
            DedupeMergeReason::ExactImageUrl,
        );
        assert!(evidence.duplicate_of.is_some());
        assert_eq!(evidence.merge_reason, DedupeMergeReason::ExactImageUrl);
    }

    #[test]
    fn candidate_has_image_url() {
        let c = make_candidate("img-1", "p1", "https://example.com/1.jpg");
        assert!(c.has_image_url());
    }

    #[test]
    fn candidate_empty_image_url_detected() {
        let mut c = make_candidate("img-1", "p1", "");
        c.image_url = "".into();
        assert!(!c.has_image_url());
    }

    #[test]
    fn provenance_builder() {
        let p = CandidateProvenance::new(3, "cats playing", 2, 1);
        assert_eq!(p.provider_rank, 3);
        assert_eq!(p.search_query, "cats playing");
        assert_eq!(p.search_round, 2);
    }

    #[test]
    fn license_evidence_default_is_unknown() {
        let le = LicenseEvidence::default();
        assert!(matches!(le, LicenseEvidence::Unknown));
    }

    #[test]
    fn candidate_record_has_all_required_fields() {
        let c = make_candidate("c1", "p1", "https://example.com/1.jpg");
        assert!(!c.candidate_id.0.is_empty());
        assert!(!c.provider_id.0.is_empty());
        assert!(!c.image_url.is_empty());
        assert!(!c.dedupe_key.is_empty());
        assert!(!c.origin_candidate_ids.is_empty());
        assert_eq!(c.origin_candidate_ids[0], c.candidate_id);
    }
}
