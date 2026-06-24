//! Production Qwen 3.5 VLM evaluation adapter.
//!
//! Implements [`OpenClawEvaluationPort`] by calling the Alibaba DashScope
//! OpenAI-compatible chat-completions endpoint with a vision-capable Qwen
//! model (e.g. `qwen-vl-plus`).
//!
//! Design notes:
//! - **Candidate-phase evaluation** ([`evaluate_candidates`]) is text-only: it
//!   compares QueryPlan semantics with the search provider's title, snippet,
//!   source, dimensions, and reference metrics. It does not send remote image
//!   URLs as visual inputs.
//! - **Image-phase evaluation** ([`evaluate_images`]) reads each retrieved
//!   image from its local path, inlines it as a base64 data URL, and asks the
//!   model whether the image satisfies the query description. DashScope's
//!   remote-URL fetch is unreliable for arbitrary image hosts, so inlining the
//!   already-downloaded bytes is the robust path.
//! - The credential is read from an environment variable (name configurable,
//!   default `QWEN_API_KEY`) and is **never** logged or serialized.

use std::cell::RefCell;
use std::time::Duration;

use crate::domain::candidate::CandidateRecord;
use crate::domain::candidate::{CandidateDecision, VlmDecisionEvidence};
use crate::domain::config::VlmEvaluatorKind;
use crate::domain::image::{ImageAcceptanceDecision, ImageRecord};
use crate::domain::query_plan::QualityTier;
use crate::error::{Error, Result};
#[allow(deprecated)]
use crate::ports::OpenClawEvaluationPort;
use crate::quality::validate_image_mechanical;

const DEFAULT_BASE_URL: &str = "https://dashscope.aliyuncs.com/compatible-mode/v1";
const DEFAULT_MODEL: &str = "qwen3-vl-plus";
const DEFAULT_TIMEOUT_SECS: u64 = 40;
const DEFAULT_CREDENTIAL_ENV: &str = "QWEN_API_KEY";
const CANDIDATE_RELEVANCE_THRESHOLD: f32 = 0.6;

#[derive(Debug, Clone)]
struct CandidateRelevanceVerdict {
    score: f32,
    rationale: Option<String>,
    raw: String,
}

/// Production Qwen VLM evaluation adapter.
pub struct QwenVlmEvaluator {
    base_url: String,
    provider_id: String,
    provider_kind: VlmEvaluatorKind,
    model: String,
    api_token: Option<String>,
    enabled: bool,
    quality_tier: QualityTier,
    timeout: Duration,
    /// Last raw model verdicts, for diagnostics (no credentials).
    last_verdicts: RefCell<Vec<String>>,
}

impl QwenVlmEvaluator {
    /// Build an evaluator, resolving the credential from the given env var name.
    pub fn new(
        base_url: impl Into<String>,
        model: impl Into<String>,
        credential_env: &str,
        quality_tier: QualityTier,
    ) -> Self {
        let api_token = std::env::var(credential_env).ok().filter(|v| !v.is_empty());
        Self {
            base_url: base_url.into(),
            provider_id: "qwen_3_5_vlm".into(),
            provider_kind: VlmEvaluatorKind::Qwen35Vlm,
            model: model.into(),
            api_token,
            enabled: true,
            quality_tier,
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
            last_verdicts: RefCell::new(Vec::new()),
        }
    }

    /// Build from a [`VlmEvaluationConfig`], applying defaults where unset.
    pub fn from_config(
        config: &crate::domain::config::VlmEvaluationConfig,
        quality_tier: QualityTier,
    ) -> Self {
        let base_url = config
            .base_url
            .clone()
            .or_else(|| config.endpoint.clone())
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
        let model = if config.model.is_empty() {
            DEFAULT_MODEL.to_string()
        } else {
            config.model.clone()
        };
        let cred = config
            .credential_env
            .clone()
            .unwrap_or_else(|| DEFAULT_CREDENTIAL_ENV.to_string());
        let mut e = Self::new(base_url, model, &cred, quality_tier);
        e.provider_id = config.provider_id.clone();
        e.provider_kind = config.provider_kind.clone();
        e.enabled = config.enabled;
        if let Some(secs) = config.timeout_seconds {
            e.timeout = Duration::from_secs(secs);
        }
        e
    }

    fn decision_evidence(
        &self,
        decision: &str,
        evidence_source: &str,
        raw_verdict: String,
    ) -> VlmDecisionEvidence {
        let mut evidence = VlmDecisionEvidence::new(
            decision,
            self.provider_id.clone(),
            self.model.clone(),
            evidence_source,
        );
        evidence.raw_verdict = Some(raw_verdict);
        evidence.rationale_summary = Some(format!("Qwen VLM returned {}.", decision));
        evidence.reason_codes = vec![format!("qwen_{}", decision)];
        evidence
    }

    fn candidate_relevance_evidence(
        &self,
        decision: &str,
        verdict: &CandidateRelevanceVerdict,
    ) -> VlmDecisionEvidence {
        let mut evidence = self.decision_evidence(
            decision,
            "qwen_candidate_text_relevance",
            verdict.raw.clone(),
        );
        evidence.confidence = Some(verdict.score);
        evidence.rationale_summary = verdict.rationale.clone().or_else(|| {
            Some(format!(
                "Qwen candidate text relevance score {:.3}.",
                verdict.score
            ))
        });
        evidence
            .reason_codes
            .push(format!("candidate_relevance_score_{:.2}", verdict.score));
        evidence
    }

    /// Ask the model a yes/no question about one local image file.
    /// Returns Ok(true) for "yes", Ok(false) for "no", Err on transport/parse.
    fn judge_image(
        &self,
        local_path: &str,
        description: &str,
        reference_metrics: &[serde_json::Value],
    ) -> Result<(bool, String)> {
        let token = self
            .api_token
            .as_deref()
            .ok_or_else(|| Error::openclaw_unavailable("QWEN credential not set"))?;

        let bytes = std::fs::read(local_path)
            .map_err(|e| Error::openclaw_unavailable(format!("read image failed: {}", e)))?;
        let b64 = base64_encode(&bytes);
        let data_url = format!("data:image/jpeg;base64,{}", b64);

        let reference_context = format_reference_metrics(reference_metrics);
        let prompt = format!(
            "You are judging whether an image satisfies a desired description. \
             Description: \"{}\". Reference metrics: {}. \
             Use the reference metrics as supporting evidence, but judge the image itself. \
             Does the image clearly satisfy this description? Answer with exactly one word: yes or no.",
            description, reference_context
        );

        let body = serde_json::json!({
            "model": self.model,
            "max_tokens": 16,
            "messages": [{
                "role": "user",
                "content": [
                    {"type": "text", "text": prompt},
                    {"type": "image_url", "image_url": {"url": data_url}}
                ]
            }]
        });

        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let body_str = serde_json::to_string(&body).map_err(|e| {
            Error::openclaw_unavailable(format!("VLM request encode failed: {}", e))
        })?;
        let agent = ureq::AgentBuilder::new().timeout(self.timeout).build();
        let resp = agent
            .post(&url)
            .set("Authorization", &format!("Bearer {}", token))
            .set("Content-Type", "application/json")
            .send_string(&body_str)
            .map_err(|e| Error::openclaw_unavailable(format!("VLM request failed: {}", e)))?;

        let resp_str = resp
            .into_string()
            .map_err(|e| Error::openclaw_unavailable(format!("VLM read failed: {}", e)))?;
        let parsed: serde_json::Value = serde_json::from_str(&resp_str)
            .map_err(|e| Error::openclaw_unavailable(format!("VLM parse failed: {}", e)))?;

        let content = parsed
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .ok_or_else(|| Error::openclaw_unavailable("VLM response missing content"))?;

        let verdict = content.trim().to_lowercase();
        self.last_verdicts.borrow_mut().push(verdict.clone());
        Ok((verdict.starts_with("yes"), verdict))
    }

    /// Ask the model whether provider-supplied candidate text is relevant
    /// enough to retrieve. Candidate phase is intentionally text-only; image
    /// bytes are evaluated only after retrieval.
    fn judge_candidate_relevance(
        &self,
        candidate: &CandidateRecord,
        description: &str,
        reference_metrics: &[serde_json::Value],
    ) -> Result<CandidateRelevanceVerdict> {
        let token = self
            .api_token
            .as_deref()
            .ok_or_else(|| Error::openclaw_unavailable("QWEN credential not set"))?;

        let body = candidate_relevance_request_body(
            &self.model,
            candidate,
            description,
            reference_metrics,
        );

        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let body_str = serde_json::to_string(&body).map_err(|e| {
            Error::openclaw_unavailable(format!("VLM candidate request encode failed: {}", e))
        })?;
        let agent = ureq::AgentBuilder::new().timeout(self.timeout).build();
        let resp = agent
            .post(&url)
            .set("Authorization", &format!("Bearer {}", token))
            .set("Content-Type", "application/json")
            .send_string(&body_str)
            .map_err(|e| {
                Error::openclaw_unavailable(format!("VLM candidate request failed: {}", e))
            })?;

        let resp_str = resp.into_string().map_err(|e| {
            Error::openclaw_unavailable(format!("VLM candidate read failed: {}", e))
        })?;
        let parsed: serde_json::Value = serde_json::from_str(&resp_str).map_err(|e| {
            Error::openclaw_unavailable(format!("VLM candidate parse failed: {}", e))
        })?;

        let content = parsed
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .ok_or_else(|| Error::openclaw_unavailable("VLM candidate response missing content"))?;

        let verdict = parse_candidate_relevance_score(content).map_err(|e| {
            Error::openclaw_unavailable(format!("VLM candidate relevance parse failed: {}", e))
        })?;
        self.last_verdicts
            .borrow_mut()
            .push(format!("candidate_relevance:{:.3}", verdict.score));
        Ok(verdict)
    }

    fn candidate_reference_metrics(
        request: &crate::quality::candidate::CandidateEvaluationRequest,
    ) -> Vec<serde_json::Value> {
        request
            .mechanical_evidence
            .reference_signals
            .iter()
            .filter_map(|signal| serde_json::to_value(signal).ok())
            .collect()
    }
}

// The candidate/image quality gates and the orchestrator consume the
// `OpenClawEvaluationPort` trait, so the production VLM adapter implements it.
// The trait is marked deprecated in favour of the not-yet-wired
// `VlmEvaluationPort`; suppress the lint until the gates migrate.
#[allow(deprecated)]
impl OpenClawEvaluationPort for QwenVlmEvaluator {
    fn readiness(&self) -> Result<()> {
        if !self.enabled {
            return Err(Error::openclaw_unavailable(
                "VLM evaluation is disabled in config",
            ));
        }
        if self.provider_kind != VlmEvaluatorKind::Qwen35Vlm {
            return Err(Error::openclaw_unavailable(format!(
                "Qwen VLM evaluator cannot run provider kind '{:?}'",
                self.provider_kind
            )));
        }
        if self.api_token.is_none() {
            return Err(Error::openclaw_unavailable(
                "QWEN credential not configured",
            ));
        }
        Ok(())
    }

    fn evaluate_candidates(
        &self,
        candidates: &[CandidateRecord],
        description: &str,
    ) -> Result<Vec<CandidateDecision>> {
        self.readiness()?;

        candidates
            .iter()
            .map(
                |c| match self.judge_candidate_relevance(c, description, &[]) {
                    Ok(verdict) if candidate_relevance_passes(verdict.score) => {
                        Ok(CandidateDecision::Accepted {
                            candidate: c.clone(),
                            priority: u32::MAX.saturating_sub(c.provider_rank),
                            vlm_evidence: Some(
                                self.candidate_relevance_evidence("approve", &verdict),
                            ),
                        })
                    }
                    Ok(verdict) => Ok(CandidateDecision::Rejected {
                        candidate: c.clone(),
                        reason: format!(
                            "Qwen VLM candidate relevance {:.3} did not exceed threshold {:.1}",
                            verdict.score, CANDIDATE_RELEVANCE_THRESHOLD
                        ),
                    }),
                    Err(e) => Err(e),
                },
            )
            .collect()
    }

    fn evaluate_candidate_requests(
        &self,
        requests: &[crate::quality::candidate::CandidateEvaluationRequest],
    ) -> Result<Vec<CandidateDecision>> {
        self.readiness()?;

        requests
            .iter()
            .map(|request| {
                let reference_metrics = Self::candidate_reference_metrics(request);
                match self.judge_candidate_relevance(
                    &request.candidate,
                    &request.query_description,
                    &reference_metrics,
                ) {
                    Ok(verdict) if candidate_relevance_passes(verdict.score) => {
                        Ok(CandidateDecision::Accepted {
                            candidate: request.candidate.clone(),
                            priority: u32::MAX.saturating_sub(request.candidate.provider_rank),
                            vlm_evidence: Some({
                                let mut evidence =
                                    self.candidate_relevance_evidence("approve", &verdict);
                                if !reference_metrics.is_empty() {
                                    evidence
                                        .reason_codes
                                        .push("reference_metrics_provided".into());
                                }
                                evidence
                            }),
                        })
                    }
                    Ok(verdict) => Ok(CandidateDecision::Rejected {
                        candidate: request.candidate.clone(),
                        reason: format!(
                            "Qwen VLM candidate relevance {:.3} did not exceed threshold {:.1}",
                            verdict.score, CANDIDATE_RELEVANCE_THRESHOLD
                        ),
                    }),
                    Err(e) => Err(e),
                }
            })
            .collect()
    }

    fn evaluate_images(
        &self,
        images: &[ImageRecord],
        description: &str,
    ) -> Result<Vec<ImageAcceptanceDecision>> {
        self.readiness()?;

        let mut decisions: Vec<ImageAcceptanceDecision> = Vec::new();

        for img in images {
            let evidence = validate_image_mechanical(img, self.quality_tier);
            if !evidence.passed_mechanical() {
                // Mechanically blocked images are filtered before VLM; skip.
                continue;
            }
            let decision =
                match self.judge_image(&img.local_path, description, &img.reference_metrics) {
                    Ok((true, verdict)) => ImageAcceptanceDecision::Accepted {
                        image: img.clone(),
                        notes: "Qwen VLM approved".into(),
                        vlm_evidence: Some({
                            let mut evidence =
                                self.decision_evidence("approve", "qwen_image_evaluation", verdict);
                            evidence
                                .reason_codes
                                .push("reference_metrics_provided".into());
                            evidence
                        }),
                    },
                    Ok((false, _verdict)) => ImageAcceptanceDecision::SubjectivelyRejected {
                        image: img.clone(),
                        mechanical_evidence: evidence,
                        reason: "Qwen VLM judged image does not match description".into(),
                    },
                    Err(e) => ImageAcceptanceDecision::ExecutionBlocked {
                        reason: format!("Qwen VLM evaluation failed: {}", e),
                    },
                };
            decisions.push(decision);
        }

        Ok(decisions)
    }
}

fn format_reference_metrics(metrics: &[serde_json::Value]) -> String {
    if metrics.is_empty() {
        return "[]".into();
    }
    serde_json::to_string(metrics).unwrap_or_else(|_| "[]".into())
}

fn candidate_relevance_request_body(
    model: &str,
    candidate: &CandidateRecord,
    description: &str,
    reference_metrics: &[serde_json::Value],
) -> serde_json::Value {
    let prompt = candidate_relevance_prompt(candidate, description, reference_metrics);
    serde_json::json!({
        "model": model,
        "max_tokens": 128,
        "messages": [{
            "role": "user",
            "content": [
                {"type": "text", "text": prompt}
            ]
        }]
    })
}

fn candidate_relevance_prompt(
    candidate: &CandidateRecord,
    description: &str,
    reference_metrics: &[serde_json::Value],
) -> String {
    let source_authority = candidate
        .provenance
        .source_authority_hint
        .as_deref()
        .unwrap_or("unknown");
    let source_page = candidate.source_page_url.as_deref().unwrap_or("unknown");
    let dimensions = match (candidate.width, candidate.height) {
        (Some(w), Some(h)) => format!("{}x{}", w, h),
        _ => "unknown".into(),
    };
    let title = candidate.title.as_deref().unwrap_or("");
    let snippet = candidate.snippet.as_deref().unwrap_or("");
    let license = candidate.license_hint.as_deref().unwrap_or("unknown");
    let reference_context = format_reference_metrics(reference_metrics);

    format!(
        "You are evaluating a search-provider image candidate before retrieval. \
         Do not inspect or fetch the image URL. Compare only the provider text and metadata with the user need. \
         User need: \"{}\". \
         Candidate title: \"{}\". Candidate description/snippet: \"{}\". \
         Source page: {}. Source authority hint: {}. Dimensions: {}. License hint: {}. Provider rank: {}. \
         Reference metrics: {}. \
         Return strict JSON only with fields: relevance_score (number from 0 to 1) and rationale (short string). \
         Score semantic relevance of the text/metadata to the user need; 1 means direct match, 0 means unrelated.",
        description,
        title,
        snippet,
        source_page,
        source_authority,
        dimensions,
        license,
        candidate.provider_rank,
        reference_context
    )
}

fn parse_candidate_relevance_score(
    content: &str,
) -> std::result::Result<CandidateRelevanceVerdict, String> {
    let trimmed = content.trim();
    let json_text = extract_json_object(trimmed).unwrap_or(trimmed);
    let parsed: serde_json::Value = serde_json::from_str(json_text)
        .map_err(|e| format!("expected JSON relevance response: {}", e))?;
    let score = parsed
        .get("relevance_score")
        .or_else(|| parsed.get("score"))
        .and_then(|v| v.as_f64())
        .ok_or_else(|| "missing numeric relevance_score".to_string())?;
    if !(0.0..=1.0).contains(&score) {
        return Err(format!("relevance_score {} outside 0..1", score));
    }
    let rationale = parsed
        .get("rationale")
        .and_then(|v| v.as_str())
        .map(ToOwned::to_owned);
    Ok(CandidateRelevanceVerdict {
        score: score as f32,
        rationale,
        raw: trimmed.to_string(),
    })
}

fn extract_json_object(text: &str) -> Option<&str> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    if end <= start {
        return None;
    }
    Some(&text[start..=end])
}

fn candidate_relevance_passes(score: f32) -> bool {
    score > CANDIDATE_RELEVANCE_THRESHOLD
}

/// Minimal base64 encoder (standard alphabet, with padding). Avoids adding a
/// dependency just for this adapter.
fn base64_encode(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((n >> 18) & 63) as usize] as char);
        out.push(TABLE[((n >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[((n >> 6) & 63) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(n & 63) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

#[cfg(test)]
#[allow(deprecated)]
mod tests {
    use super::*;

    #[test]
    fn base64_encode_matches_known_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn readiness_fails_without_credential() {
        let e = QwenVlmEvaluator::new(
            DEFAULT_BASE_URL,
            DEFAULT_MODEL,
            "DEFINITELY_UNSET_ENV_VAR_XYZ",
            QualityTier::General,
        );
        assert!(e.readiness().is_err());
    }

    #[test]
    fn candidate_evaluation_fails_closed_without_credential() {
        let e = QwenVlmEvaluator::new(
            DEFAULT_BASE_URL,
            DEFAULT_MODEL,
            "DEFINITELY_UNSET_ENV_VAR_XYZ",
            QualityTier::General,
        );
        let candidate = CandidateRecord::minimal(
            crate::domain::candidate::CandidateId::new("cand-1"),
            crate::domain::candidate::ProviderId::new("provider-1"),
            "https://example.com/image.jpg",
        );

        let result = e.evaluate_candidates(&[candidate], "anything");

        assert!(result.is_err());
    }

    #[test]
    fn from_config_disabled_fails_readiness_even_with_credential() {
        let env_var = "IMAGE_RETRIEVAL_QWEN_DISABLED_TEST_TOKEN";
        std::env::set_var(env_var, "dummy-token");
        let config = crate::domain::config::VlmEvaluationConfig {
            enabled: false,
            credential_env: Some(env_var.into()),
            ..Default::default()
        };
        let e = QwenVlmEvaluator::from_config(&config, QualityTier::General);

        let readiness = e.readiness();

        std::env::remove_var(env_var);
        assert!(readiness.is_err());
        assert!(readiness
            .unwrap_err()
            .to_string()
            .contains("VLM evaluation is disabled"));
    }

    #[test]
    fn from_config_empty_model_uses_v11_default_model() {
        let env_var = "IMAGE_RETRIEVAL_QWEN_MODEL_DEFAULT_TEST_TOKEN";
        std::env::set_var(env_var, "dummy-token");
        let config = crate::domain::config::VlmEvaluationConfig {
            enabled: true,
            model: String::new(),
            credential_env: Some(env_var.into()),
            ..Default::default()
        };
        let e = QwenVlmEvaluator::from_config(&config, QualityTier::General);

        let evidence = e.decision_evidence("approve", "unit_test", "yes".into());

        std::env::remove_var(env_var);
        assert_eq!(evidence.model.as_deref(), Some("qwen3-vl-plus"));
    }

    #[test]
    fn candidate_relevance_request_is_text_only_and_contains_provider_description() {
        let candidate = CandidateRecord {
            title: Some("Sunset Mountain Landscape by Vii-photo".into()),
            snippet: Some("Warm orange sunset over mountain ridge".into()),
            source_page_url: Some("https://example.com/sunset-mountain".into()),
            width: Some(900),
            height: Some(596),
            ..CandidateRecord::minimal(
                crate::domain::candidate::CandidateId::new("cand-1"),
                crate::domain::candidate::ProviderId::new("serpapi_google_images"),
                "https://example.com/image.jpg",
            )
        };

        let body = candidate_relevance_request_body(
            "qwen3-vl-plus",
            &candidate,
            "sunset over mountain landscape with vibrant orange sky",
            &[],
        );
        let content = body["messages"][0]["content"].as_array().unwrap();

        assert_eq!(
            content.len(),
            1,
            "candidate relevance request must be text-only"
        );
        assert_eq!(content[0]["type"], "text");
        let text = content[0]["text"].as_str().unwrap();
        assert!(text.contains("Sunset Mountain Landscape"));
        assert!(text.contains("Warm orange sunset"));
        assert!(text.contains("relevance_score"));
        assert!(!text.contains("image_url"));
    }

    #[test]
    fn parse_candidate_relevance_score_accepts_json_and_threshold_is_strict() {
        let high = parse_candidate_relevance_score(
            r#"{"relevance_score":0.72,"rationale":"title and snippet match"}"#,
        )
        .unwrap();
        let boundary =
            parse_candidate_relevance_score(r#"{"relevance_score":0.6,"rationale":"borderline"}"#)
                .unwrap();

        assert!(candidate_relevance_passes(high.score));
        assert!(!candidate_relevance_passes(boundary.score));
        assert_eq!(high.rationale.as_deref(), Some("title and snippet match"));
    }
}
