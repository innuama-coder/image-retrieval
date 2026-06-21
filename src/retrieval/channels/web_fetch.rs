//! Minimal web fetch retrieval channel.
//!
//! This is the tier-1 (default) retrieval channel. It performs plain HTTP GET
//! requests to download candidate images directly from their source URLs.
//!
//! # Boundaries
//!
//! - Does not execute JavaScript or handle dynamic page rendering.
//! - Does not follow authentication redirects or submit credentials.
//! - Does not bypass access controls (403/401 → `AccessRestricted` failure).
//! - Respects a configurable timeout; failures are normalised to
//!   [`RetrievalFailureCategory`].
//!
//! References: PRD §普通 web fetch 优先级, LLD §通道模型

use crate::domain::retrieval::{
    FallbackEligibilityFact, RetrievalBatch, RetrievalChannelTier, RetrievalFailure,
    RetrievalFailureCategory, RetrievalResult, RetrievalSuccess,
};
use crate::error::{Error, Result};
use crate::ports::BaseRetrievalChannel;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Default request timeout for the web fetch channel.
const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// Maximum response body size (16 MB) to prevent memory exhaustion.
const MAX_RESPONSE_SIZE: u64 = 16 * 1024 * 1024;

/// Content types that are recognised as images for the minimal check.
const IMAGE_CONTENT_TYPES: &[&str] = &[
    "image/jpeg",
    "image/png",
    "image/gif",
    "image/webp",
    "image/bmp",
    "image/svg+xml",
    "image/tiff",
];

/// The minimal web fetch retrieval channel.
///
/// Performs plain HTTP GET requests. This is the baseline channel that must
/// always be available; it is the implementation boundary for normal web fetch.
pub struct WebFetchChannel {
    /// Directory where downloaded images are stored.
    download_dir: PathBuf,

    /// Request timeout.
    timeout: Duration,

    /// Whether this channel instance is enabled.
    enabled: bool,
}

impl WebFetchChannel {
    /// Create a new web fetch channel.
    ///
    /// `download_dir` is the directory where fetched images will be saved.
    /// The directory is created if it does not exist.
    pub fn new(download_dir: impl Into<PathBuf>) -> Result<Self> {
        let dir: PathBuf = download_dir.into();
        fs::create_dir_all(&dir).map_err(|e| {
            Error::internal(format!(
                "cannot create download directory '{}': {}",
                dir.display(),
                e
            ))
        })?;
        Ok(Self {
            download_dir: dir,
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
            enabled: true,
        })
    }

    /// Set a custom request timeout.
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout = Duration::from_secs(secs);
        self
    }

    /// Enable or disable this channel.
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Return the download directory path.
    pub fn download_dir(&self) -> &Path {
        &self.download_dir
    }

    /// Determine the file extension from a Content-Type header value.
    fn extension_from_content_type(content_type: &str) -> &str {
        match content_type {
            "image/jpeg" => "jpg",
            "image/png" => "png",
            "image/gif" => "gif",
            "image/webp" => "webp",
            "image/bmp" => "bmp",
            "image/svg+xml" => "svg",
            "image/tiff" => "tiff",
            _ => "bin",
        }
    }

    /// Check whether a Content-Type value indicates an image.
    fn is_image_content_type(ct: &str) -> bool {
        IMAGE_CONTENT_TYPES
            .iter()
            .any(|valid| ct.starts_with(valid))
    }

    /// Attempt to fetch a single candidate image.
    fn fetch_one(&self, candidate_id: &str, url: &str, agent: &ureq::Agent) -> RetrievalResult {
        // Perform the HTTP GET request
        let response = match agent.get(url).call() {
            Ok(response) => response,
            Err(ureq::Error::Status(code, _response)) => {
                let category = match code {
                    401 | 403 => RetrievalFailureCategory::AccessRestricted,
                    _ => RetrievalFailureCategory::HttpStatus,
                };
                let allows_fallback =
                    !matches!(category, RetrievalFailureCategory::AccessRestricted);

                return RetrievalResult::Failure(RetrievalFailure::new(
                    candidate_id,
                    RetrievalChannelTier::WebFetch,
                    category,
                    format!("HTTP {} from {}", code, url),
                    allows_fallback,
                ));
            }
            Err(ureq::Error::Transport(transport)) => {
                return RetrievalResult::Failure(RetrievalFailure::new(
                    candidate_id,
                    RetrievalChannelTier::WebFetch,
                    RetrievalFailureCategory::Network,
                    format!("transport error fetching {}: {}", url, transport),
                    true,
                ));
            }
        };

        // Check Content-Type
        let content_type: Option<String> = response.header("Content-Type").map(|s| s.to_string());
        if let Some(ref ct) = content_type {
            if !Self::is_image_content_type(ct) {
                return RetrievalResult::Failure(RetrievalFailure::new(
                    candidate_id,
                    RetrievalChannelTier::WebFetch,
                    RetrievalFailureCategory::InvalidContent,
                    format!("non-image content type '{}' from {}", ct, url),
                    true,
                ));
            }
        }

        // Read the response body with a size limit
        let mut reader = response.into_reader();
        let mut buf: Vec<u8> = Vec::new();
        let mut chunk = [0u8; 8192];
        loop {
            let n = match std::io::Read::read(&mut reader, &mut chunk) {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) => {
                    return RetrievalResult::Failure(RetrievalFailure::new(
                        candidate_id,
                        RetrievalChannelTier::WebFetch,
                        RetrievalFailureCategory::Network,
                        format!("read error from {}: {}", url, e),
                        true,
                    ));
                }
            };
            buf.extend_from_slice(&chunk[..n]);
            if buf.len() as u64 > MAX_RESPONSE_SIZE {
                return RetrievalResult::Failure(RetrievalFailure::new(
                    candidate_id,
                    RetrievalChannelTier::WebFetch,
                    RetrievalFailureCategory::InvalidContent,
                    format!(
                        "response too large (>{} bytes) from {}",
                        MAX_RESPONSE_SIZE, url
                    ),
                    true,
                ));
            }
        }

        if buf.is_empty() {
            return RetrievalResult::Failure(RetrievalFailure::new(
                candidate_id,
                RetrievalChannelTier::WebFetch,
                RetrievalFailureCategory::InvalidContent,
                format!("empty response body from {}", url),
                true,
            ));
        }

        // Determine file extension and write to download directory
        let ext = content_type
            .as_deref()
            .map(Self::extension_from_content_type)
            .unwrap_or("bin");
        let file_name = format!("{}.{}", sanitise_filename(candidate_id), ext);
        let file_path = self.download_dir.join(&file_name);

        let file_size = buf.len() as u64;
        if let Err(e) = fs::write(&file_path, &buf) {
            return RetrievalResult::Failure(RetrievalFailure::new(
                candidate_id,
                RetrievalChannelTier::WebFetch,
                RetrievalFailureCategory::Other,
                format!("cannot write file '{}': {}", file_path.display(), e),
                true,
            ));
        }

        RetrievalResult::Success(RetrievalSuccess::new(
            candidate_id,
            file_path.to_string_lossy().to_string(),
            RetrievalChannelTier::WebFetch,
            content_type,
            file_size,
        ))
    }
}

impl BaseRetrievalChannel for WebFetchChannel {
    fn tier(&self) -> RetrievalChannelTier {
        RetrievalChannelTier::WebFetch
    }

    fn display_name(&self) -> &str {
        "Web Fetch (HTTP GET)"
    }

    fn readiness(&self) -> Result<()> {
        if !self.enabled {
            return Err(Error::retrieval_failure(
                None::<&str>,
                RetrievalChannelTier::WebFetch.to_string(),
                "web fetch channel is disabled",
            ));
        }
        // Basic sanity: check that the download directory is writable
        let test_file = self.download_dir.join(".write_test");
        fs::write(&test_file, b"")
            .map_err(|e| Error::internal(format!("download directory not writable: {}", e)))?;
        let _ = fs::remove_file(&test_file);
        Ok(())
    }

    fn retrieve_batch(&self, batch: &RetrievalBatch) -> Result<Vec<RetrievalResult>> {
        // Build a ureq agent with the configured timeout
        let agent = ureq::AgentBuilder::new().timeout(self.timeout).build();

        let results: Vec<RetrievalResult> = batch
            .candidate_ids
            .iter()
            .map(|cid| match batch.url_for(cid) {
                Some(url) => self.fetch_one(cid, url, &agent),
                None => RetrievalResult::Failure(RetrievalFailure::new(
                    cid,
                    RetrievalChannelTier::WebFetch,
                    RetrievalFailureCategory::Other,
                    "no source URL in batch",
                    false,
                )),
            })
            .collect();

        Ok(results)
    }

    fn fallback_fact(&self, reason: &str) -> FallbackEligibilityFact {
        FallbackEligibilityFact::new(RetrievalChannelTier::WebFetch, reason, false)
    }
}

/// Sanitise a string for use as a filename component.
fn sanitise_filename(input: &str) -> String {
    input
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_fetch_channel_tier() {
        let dir = std::env::temp_dir().join("test-web-fetch-tier");
        let channel = WebFetchChannel::new(&dir).expect("create channel");
        assert_eq!(channel.tier(), RetrievalChannelTier::WebFetch);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn web_fetch_channel_display_name() {
        let dir = std::env::temp_dir().join("test-web-fetch-name");
        let channel = WebFetchChannel::new(&dir).expect("create channel");
        assert!(channel.display_name().contains("Web Fetch"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn web_fetch_channel_readiness_when_enabled() {
        let dir = std::env::temp_dir().join("test-web-fetch-ready");
        let channel = WebFetchChannel::new(&dir).expect("create channel");
        assert!(channel.readiness().is_ok());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn web_fetch_channel_readiness_when_disabled() {
        let dir = std::env::temp_dir().join("test-web-fetch-disabled");
        let channel = WebFetchChannel::new(&dir)
            .expect("create channel")
            .with_enabled(false);
        assert!(channel.readiness().is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn web_fetch_channel_fallback_fact() {
        let dir = std::env::temp_dir().join("test-web-fetch-fallback");
        let channel = WebFetchChannel::new(&dir).expect("create channel");
        let fact = channel.fallback_fact("test failure");
        assert_eq!(fact.failed_tier, RetrievalChannelTier::WebFetch);
        assert_eq!(fact.next_tier, Some(RetrievalChannelTier::SelfHosted));
        assert!(!fact.is_access_restricted);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn web_fetch_with_missing_url_returns_failure() {
        let dir = std::env::temp_dir().join("test-web-fetch-missing-url");
        let channel = WebFetchChannel::new(&dir).expect("create channel");
        let batch = RetrievalBatch::new(vec!["no-url".into()], 2);
        let results = channel
            .retrieve_batch(&batch)
            .expect("batch should not error");
        assert_eq!(results.len(), 1);
        assert!(results[0].is_failure());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn sanitise_filename_replaces_special_chars() {
        let clean = sanitise_filename("cand-001_test.jpg");
        // The dot is replaced because the extension is determined from
        // Content-Type, not from the original filename.
        assert_eq!(clean, "cand-001_test_jpg");

        let messy = sanitise_filename("http://example.com/img?x=1");
        // All non-alphanumeric characters (except - and _) are replaced
        assert_eq!(messy, "http___example_com_img_x_1");
    }

    #[test]
    fn is_image_content_type() {
        assert!(WebFetchChannel::is_image_content_type("image/jpeg"));
        assert!(WebFetchChannel::is_image_content_type("image/png"));
        assert!(WebFetchChannel::is_image_content_type("image/gif"));
        assert!(WebFetchChannel::is_image_content_type("image/webp"));
        assert!(!WebFetchChannel::is_image_content_type("text/html"));
        assert!(!WebFetchChannel::is_image_content_type("application/json"));
    }

    #[test]
    fn extension_from_content_type() {
        assert_eq!(
            WebFetchChannel::extension_from_content_type("image/jpeg"),
            "jpg"
        );
        assert_eq!(
            WebFetchChannel::extension_from_content_type("image/png"),
            "png"
        );
        assert_eq!(
            WebFetchChannel::extension_from_content_type("image/webp"),
            "webp"
        );
        assert_eq!(
            WebFetchChannel::extension_from_content_type("application/octet-stream"),
            "bin"
        );
    }
}
