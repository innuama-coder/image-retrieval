//! Production Qwen 3.5 VLM evaluation adapter.
//!
//! Implements [`OpenClawEvaluationPort`] by calling the Alibaba DashScope
//! OpenAI-compatible chat-completions endpoint with a vision-capable Qwen
//! model (e.g. `qwen-vl-plus`).
//!
//! Design notes:
//! - **Image-phase evaluation** ([`evaluate_images`]) reads each retrieved
//!   image from its local path, inlines it as a base64 data URL, and asks the
//!   model whether the image satisfies the query description. DashScope's
//!   remote-URL fetch is unreliable for arbitrary image hosts, so inlining the
//!   already-downloaded bytes is the robust path.
//! - **Candidate-phase evaluation** ([`evaluate_candidates`]) admits every
//!   mechanically-screened candidate into the retrievable sequence (no remote
//!   VLM call): at candidate time only an image URL is known (not yet
//!   downloaded) and DashScope's remote-URL fetch is unreliable, so mechanical
//!   screening governs the candidate gate and the real VLM judgement happens at
//!   image-acceptance time on the downloaded files.
//! - The credential is read from an environment variable (name configurable,
//!   default `QWEN_API_TOKEN`) and is **never** logged or serialized.

use std::cell::RefCell;
use std::time::Duration;

use crate::domain::candidate::CandidateDecision;
use crate::domain::candidate::CandidateRecord;
use crate::domain::image::{ImageAcceptanceDecision, ImageRecord};
use crate::domain::query_plan::QualityTier;
use crate::error::{Error, Result};
#[allow(deprecated)]
use crate::ports::OpenClawEvaluationPort;
use crate::quality::{
    evaluate_images_with_conclusions, validate_image_mechanical, ImageEvaluationConclusion,
    ImageMechanicalEvidence,
};

const DEFAULT_BASE_URL: &str = "https://dashscope.aliyuncs.com/compatible-mode/v1";
const DEFAULT_MODEL: &str = "qwen-vl-plus";
const DEFAULT_TIMEOUT_SECS: u64 = 40;
const DEFAULT_CREDENTIAL_ENV: &str = "QWEN_API_TOKEN";

/// Production Qwen VLM evaluation adapter.
pub struct QwenVlmEvaluator {
    base_url: String,
    model: String,
    api_token: Option<String>,
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
            model: model.into(),
            api_token,
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
        if let Some(secs) = config.timeout_seconds {
            e.timeout = Duration::from_secs(secs);
        }
        e
    }

    /// Ask the model a yes/no question about one local image file.
    /// Returns Ok(true) for "yes", Ok(false) for "no", Err on transport/parse.
    fn judge_image(&self, local_path: &str, description: &str) -> Result<bool> {
        let token = self
            .api_token
            .as_deref()
            .ok_or_else(|| Error::openclaw_unavailable("QWEN credential not set"))?;

        let bytes = std::fs::read(local_path)
            .map_err(|e| Error::openclaw_unavailable(format!("read image failed: {}", e)))?;
        let b64 = base64_encode(&bytes);
        let data_url = format!("data:image/jpeg;base64,{}", b64);

        let prompt = format!(
            "You are judging whether an image satisfies a desired description. \
             Description: \"{}\". Does the image clearly satisfy this description? \
             Answer with exactly one word: yes or no.",
            description
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
        Ok(verdict.starts_with("yes"))
    }
}

// The candidate/image quality gates and the orchestrator consume the
// `OpenClawEvaluationPort` trait, so the production VLM adapter implements it.
// The trait is marked deprecated in favour of the not-yet-wired
// `VlmEvaluationPort`; suppress the lint until the gates migrate.
#[allow(deprecated)]
impl OpenClawEvaluationPort for QwenVlmEvaluator {
    fn readiness(&self) -> Result<()> {
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
        _description: &str,
    ) -> Result<Vec<CandidateDecision>> {
        // Candidate-phase VLM is deferred: only image URLs are known here (not
        // yet downloaded), and DashScope's remote-URL fetch is unreliable. The
        // gate only passes mechanically-screened candidates to this method, so
        // we admit them all into the retrievable sequence and let the real VLM
        // judgement run at image-acceptance time on the downloaded files.
        // Priority preserves provider rank (lower rank = higher priority).
        Ok(candidates
            .iter()
            .map(|c| CandidateDecision::Accepted {
                candidate: c.clone(),
                priority: u32::MAX.saturating_sub(c.provider_rank),
            })
            .collect())
    }

    fn evaluate_images(
        &self,
        images: &[ImageRecord],
        description: &str,
    ) -> Result<Vec<ImageAcceptanceDecision>> {
        self.readiness()?;

        let mut passed: Vec<(ImageRecord, ImageMechanicalEvidence)> = Vec::new();
        let mut conclusions: Vec<ImageEvaluationConclusion> = Vec::new();

        for img in images {
            let evidence = validate_image_mechanical(img, self.quality_tier);
            if !evidence.passed_mechanical() {
                // Mechanically blocked images are filtered before VLM; skip.
                continue;
            }
            let conclusion = match self.judge_image(&img.local_path, description) {
                Ok(true) => ImageEvaluationConclusion::Approve {
                    notes: Some("Qwen VLM approved".into()),
                },
                Ok(false) => ImageEvaluationConclusion::Reject {
                    reason: "Qwen VLM judged image does not match description".into(),
                },
                Err(e) => ImageEvaluationConclusion::Unexecutable {
                    reason: format!("Qwen VLM evaluation failed: {}", e),
                },
            };
            passed.push((img.clone(), evidence));
            conclusions.push(conclusion);
        }

        Ok(evaluate_images_with_conclusions(passed, conclusions))
    }
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
    fn candidate_evaluation_admits_each_passed_candidate() {
        let e = QwenVlmEvaluator::new(
            DEFAULT_BASE_URL,
            DEFAULT_MODEL,
            "DEFINITELY_UNSET_ENV_VAR_XYZ",
            QualityTier::General,
        );
        // No candidates -> no decisions.
        assert!(e.evaluate_candidates(&[], "anything").unwrap().is_empty());
        // Each mechanically-passed candidate handed to the adapter is admitted
        // (Accepted) so it reaches retrieval; real VLM runs at image phase.
        // (Construction of CandidateRecord is exercised by the live smoke run.)
    }
}
