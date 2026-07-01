# image-retrieval v1.1 Low-Level Design

## Revision

| Version | Date | Author | Change | Sources |
| --- | --- | --- | --- | --- |
| v1.1-draft | 2026-06-21 | Codex | Initial aggregate LLD for v1.1 implementation. | PRD.md, HLD.md, LLD-TASKS.md, AGENTS.md |

## Scope

This LLD turns the v1.1 PRD/HLD into concrete Rust module, type, interface,
state, package, and test design. It does not implement code. It defines what
the development tasks must implement.

## Module Changes

| Module | Required v1.1 Changes |
| --- | --- |
| `src/main.rs` | `run` executes the full workflow; add `validate-package`; load config. |
| `src/domain/query_plan.rs` | Add normalized v1.1 QueryPlan content fields, count defaults, compatibility parsing, and admission diagnostics. |
| `src/domain/candidate.rs` | Add provenance, dedupe, mechanical evidence, evaluation decision references. |
| `src/domain/retrieval.rs` | Replace path-only success with artifact-backed retrieval job/result/attempt model. |
| `src/domain/image.rs` | Add image evidence, visual description, artifact integrity, acceptance linkage. |
| `src/domain/delivery.rs` | Add canonical package status, coverage, manifest, validation, review, handoff types. |
| `src/ports/mod.rs` | Split or alias `BaseSearchProvider`; extend retrieval and VLM evaluation contracts. |
| `src/search/` | Add provider adapters, readiness, weighted scheduling, normalization, dedupe. |
| `src/retrieval/` | Add artifact-producing channels, fallback executor, source-page resolve boundary. |
| `src/quality/` | Implement candidate/image mechanical gates and VLM evaluation DTO mapping. |
| `src/orchestrator/` | Implement full attempt/retry state machine. |
| `src/delivery/` | Generate canonical package files and copy images/evidence/diagnostics. |
| `src/self_check/` | Report provider/channel/VLM/policy readiness separately. |
| `tests/` | Add v1.1 unit, integration, E2E fixture, validation, and blocked real-service tests. |

## Normalized QueryPlan

```rust
pub struct NormalizedQueryPlan {
    pub query_plan_id: String,
    pub description: String,
    pub required_image_count: u32,
    pub query_texts: Vec<String>,
    pub visual_requirements: Vec<String>,
    pub negative_scope: Vec<String>,
    pub candidate_target: u32,
    pub retrieval_batch_target: u32,
    pub retry_limit: u8, // fixed runtime constitution default, not user policy
    pub full_attempt_limit: u8,
}
```

Rules:

- `required_image_count` defaults to `1`.
- `candidate_target = required_image_count * 20`.
- `retrieval_batch_target = required_image_count * 2`.
- Source, license, provider, retrieval, authorization, paywall, quality, retry,
  execution, and other non-image-content requirements are not production
  QueryPlan semantics. Legacy fields may be parsed for compatibility but must
  not influence search, retrieval, or Qwen acceptance.
- `retry_limit = 3`.

## Configuration Model

```rust
pub struct RuntimeConfig {
    pub providers: Vec<SearchProviderConfig>,
    pub retrieval_channels: Vec<RetrievalChannelConfig>,
    pub vlm_evaluation: VlmEvaluationConfig,
    pub policy: PolicyConfig,
    pub output: OutputConfig,
}
```

Configuration references environment variable names, not secret values. The
redaction layer must remove secret-like values from diagnostics and package
files.

## Search Provider Contract

```rust
pub trait BaseSearchProvider {
    fn provider_id(&self) -> &str;
    fn readiness(&self, config: &SearchProviderConfig) -> ProviderReadiness;
    fn search(&self, request: &SearchRequest) -> Result<SearchResponse, SearchError>;
    fn supported_constraints(&self) -> ProviderConstraintSupport;
}
```

Provider readiness must include credential, health, quota, constraint, failure
code, timestamp, and evidence. Search responses must preserve raw provider id,
round, rank, URLs, dimensions, license hints, and source page data.

Default v1.1 broad provider:

- `provider_id = "serpapi_google_images"`
- `provider_kind = "serpapi_google_images"`
- `endpoint = "https://serpapi.com/search"`
- `credential_env = "SERPAPI_API_KEY"`
- `default_query_params.engine = "google_images"`

The adapter must map SerpApi `image_results[]` into canonical
`CandidateRecord` values and must never serialize the resolved API key.

## Candidate Record

```rust
pub struct CandidateRecord {
    pub candidate_id: String,
    pub query_plan_id: String,
    pub provider_id: String,
    pub search_round: u32,
    pub provider_rank: u32,
    pub image_url: String,
    pub source_page_url: Option<String>,
    pub thumbnail_url: Option<String>,
    pub title: Option<String>,
    pub snippet: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub mime_type: Option<String>,
    pub license_hint: Option<String>,
    pub dedupe_key: String,
    pub origin_candidate_ids: Vec<String>,
    pub provenance: CandidateProvenance,
}
```

Deduplication must preserve `origin_candidate_ids` and provider provenance.

## Candidate Quality Contract

Candidate evaluation has two outputs:

- `CandidateMechanicalEvidence`
- `CandidateEvaluationDecision`

The mechanical gate owns blocking/reference metrics. The configured VLM
evaluation provider owns subjective decision. A candidate is retrievable only
when both pass.

Default v1.1 subjective evaluation provider:

- `provider_id = "qwen_3_5_vlm"`
- `provider_kind = "qwen_3_5_vlm"`
- `model = "qwen3-vl-plus"`
- `credential_env = "QWEN_API_KEY"`

The endpoint/base URL is externalized in runtime config. The resolved token
must never be serialized. Production VLM unavailable means
`execution_blocked`; fixture evaluator is test-only.

## Retrieval Contract

```rust
pub struct RetrievalJob {
    pub retrieval_job_id: String,
    pub candidate_id: String,
    pub query_plan_id: String,
    pub target: RetrievalTarget,
    pub requested_outputs: Vec<RequestedOutput>,
}

pub struct RetrievalArtifactResult {
    pub retrieval_job_id: String,
    pub candidate_id: String,
    pub query_plan_id: String,
    pub channel_id: String,
    pub channel_tier: RetrievalChannelTier,
    pub retrieval_status: RetrievalStatus,
    pub local_artifact_path: Option<PathBuf>,
    pub source_artifact_path: Option<PathBuf>,
    pub source_sidecar_path: Option<PathBuf>,
    pub content_summary_path: Option<PathBuf>,
    pub task_report_path: Option<PathBuf>,
    pub visual_description_path: Option<PathBuf>,
    pub checksum_sha256: Option<String>,
    pub content_type: Option<String>,
    pub file_size_bytes: Option<u64>,
    pub image_dimensions: Option<ImageDimensions>,
    pub media_type_match: bool,
    pub fetch_trace: Vec<RetrievalAttempt>,
    pub failure_reason: Option<RetrievalFailureReason>,
}
```

`BaseRetrievalChannel` returns `RetrievalBatchResult` containing one result per
job plus attempts for all fallback paths. Direct HTTP success still must create
sidecar, summary, task report, visual description, checksum, content-type and
dimension evidence, and trace artifacts.

## Fallback Policy

Fallback order:

1. `normal_web_fetch.direct_image_fetch`
2. `normal_web_fetch.source_page_resolve`
3. `self_hosted_service`
4. `paid_online_service`

The implementation may group 1 and 2 under one tier, but attempts must be
separate. Paid fallback is disabled unless explicitly enabled in config.

## Image Acceptance

```rust
pub struct ImageAcceptanceDecision {
    pub candidate_id: String,
    pub retrieval_job_id: String,
    pub mechanical_passed: bool,
    pub vlm_passed: bool,
    pub delivered_qualified: bool,
    pub blocking_reasons: Vec<String>,
    pub reference_metrics: Vec<MetricFact>,
}
```

Delivered qualification requires all artifact fields, checksum, media match,
mechanical pass, VLM pass, ownership match, and no negative-scope blocker.

## Orchestrator State Machine

```rust
pub struct RunState {
    pub full_attempt_count: u8,
    pub retry_count: u8,
    pub accepted_images: Vec<AcceptedImage>,
    pub rejected_candidates: Vec<CandidateRejection>,
    pub retrieval_failures: Vec<RetrievalFailure>,
    pub gaps: Vec<CoverageGap>,
}
```

Loop:

1. Start `full_attempt_count = 1`, `retry_count = 0`.
2. Search/evaluate/retrieve/evaluate/package evidence for each attempt.
3. Stop as `passed` when accepted images reach required count.
4. Retry while `retry_count < 3`.
5. After exhaustion, return `partial` if accepted images exist, otherwise
   `blocked`.

## Package Files

| File | Producer | Main Contents |
| --- | --- | --- |
| `image-recalls.json` | search | Provider readiness, raw/normalized recalls, provenance. |
| `retrieved-images.json` | retrieval + acceptance | Processed candidates, artifact results, delivered images. |
| `coverage-report.json` | orchestrator | Required count, accepted count, gaps, source diversity. |
| `retrieval-manifest.json` | orchestrator | Candidate status progression and artifact links. |
| `package-summary.json` | delivery | Overall status, attempts, key reasons. |
| `delivery-report.json` | delivery | Per-candidate delivery qualification evidence. |
| `validation.json` | validation | Deterministic validation verdict and issues. |
| `review.json` | review/VLM | Semantic review verdict. |
| `handoff-report.json` | delivery | Downstream readiness and blockers. |

## Validation Checks

Validation must fail when:

- Required package file is missing.
- Delivered image lacks retrieval job id.
- Delivered image lacks any required artifact path.
- Required artifact path does not exist.
- Checksum is missing.
- Media type does not match.
- Candidate ownership does not match QueryPlan.
- Metadata-only, thumbnail-only, or source-page-only result enters delivered.
- Coverage counts do not match package contents.
- Retry counters violate 1 initial + 3 retry semantics.
- Sensitive credentials appear in package files.

## CLI Commands

```bash
image-retrieval run --query-plan query-plan.json --config config.toml --output-dir out/
image-retrieval self-check --config config.toml
image-retrieval validate-package --package-dir out/package
image-retrieval inspect-package --package-dir out/package
```

`inspect-package` is optional for v1.1. `run`, `self-check`, and
`validate-package` are required.

## Test Matrix

| Requirement Area | Tests |
| --- | --- |
| QueryPlan/config | defaults, invalid input, policy parsing, redaction |
| Search | missing credential, provider readiness, scheduling provenance, dedupe |
| Candidate quality | mechanical block, VLM unavailable, accepted candidate handoff |
| Retrieval | direct success, source-page fallback, missing sidecar/summary/report, paid disabled |
| Image acceptance | metadata-only reject, missing artifact reject, VLM pass/fail |
| Orchestrator | full delivery, partial, blocked, retry exhaustion, attempt counters |
| Package | canonical files, manifest links, validation failures, sensitive data scan |
| CLI | run executes full flow, self-check readiness, validate-package verdict |
| Real-service smoke | configured provider + retrieval + Qwen 3.5 VLM can deliver at least one accepted image |

## Development And Release Gates

The following items are in-scope implementation and verification gates for
v1.1. Development tasks must implement them, expose readiness diagnostics, and
produce blocked evidence when required runtime prerequisites are unavailable:

- SerpApi Google Images adapter for the default ImageSearch provider, including
  `image_results[]` normalization, `SERPAPI_API_KEY` or configured credential
  env readiness, redaction, and real-service smoke evidence.
- Qwen 3.5 VLM evaluation adapter, including endpoint/base URL and model config,
  `QWEN_API_KEY` or configured credential env readiness, prompt templates,
  response validation, fail-closed production behavior, and real-service smoke
  evidence.
- Artifact-capable normal web retrieval channel and package validation evidence
  proving that real retrieved images are local, checksummed, traceable, and not
  metadata-only.

The following product or release decisions remain outside local implementation
authority. Development tasks may implement explicit policy boundaries, defaults,
diagnostics, blocked behavior, and waiver recording for them, but final v1.1
acceptance cannot pass until each item is closed or explicitly waived:

- Paid retrieval enablement and budget.
- robots/site-rule and authorization blocking policy.
- Quality threshold calibration or waiver.
