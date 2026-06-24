# image-retrieval v1.1 PRD

## Purpose

`image-retrieval` v1.1 is a Rust CLI for real image search, retrieval, validation, and delivery packaging. It must not deliver URL metadata as a substitute for local image artifacts.

## Product Goals

| ID | Goal | Success signal |
| --- | --- | --- |
| G-001 | Execute a full QueryPlan workflow | `run` searches, evaluates, retrieves, validates, and packages images. |
| G-002 | Use pluggable real search providers | SerpApi Google Images is the v1.1 default provider; provider config is externalized. |
| G-003 | Retrieve local artifacts | Delivered images have local files, sidecars, summaries, task reports, visual descriptions, checksums, and fetch traces. |
| G-004 | Keep acceptance honest | Package status is only `passed`, `partial`, or `blocked`, with machine-readable gaps and retry evidence. |
| G-005 | Use real production subjective evidence | v1.1 production subjective evaluation uses Qwen 3.5 VLM; fixture/mock evidence is test-only. |

## QueryPlan Requirements

A QueryPlan must include a semantic image description. It may also specify required image count, quality requirements, query texts, source policy, retrieval policy, and retry limit.

Defaults:

- `required_image_count = 1`
- `quality = general`
- `retry_limit = 3`
- `candidate_target = required_image_count * 20`
- `retrieval_batch_target = required_image_count * 2`

## Functional Requirements

| ID | Requirement |
| --- | --- |
| FR-001 | Accept and normalize a QueryPlan before execution. |
| FR-002 | Search candidates through configurable pluggable providers. |
| FR-003 | Use weighted provider scheduling when multiple providers are available. |
| FR-004 | Target roughly 20 candidates per required delivered image. |
| FR-005 | Mechanically validate candidates with blocking and reference metrics. |
| FR-006 | Evaluate candidate text relevance with Qwen before retrieval. |
| FR-007 | Retrieve only candidates that pass mechanical and Qwen candidate checks. |
| FR-008 | Retrieve images through pluggable channels with fallback tiers: normal web fetch, self-hosted services, paid services. |
| FR-009 | Mechanically validate retrieved artifacts. |
| FR-010 | Evaluate downloaded local image artifacts with Qwen before delivery. |
| FR-011 | Count an image as accepted only when mechanical and Qwen image checks both pass. |
| FR-012 | Retry the full workflow until the requested count is met or the initial attempt plus 3 retries are exhausted. |
| FR-013 | Build a canonical delivery package with verifiable local artifacts and evidence. |
| FR-014 | Provide `self-check` readiness diagnostics for config, search, retrieval, Qwen, policy, output, and validator components. |
| FR-015 | Provide `validate-package` with deterministic pass/fail output and issues. |

## Production Integrations

| Integration | v1.1 decision |
| --- | --- |
| Search provider | SerpApi Google Images, credential env `SERPAPI_API_KEY`. |
| Subjective evaluator | Qwen 3.5 VLM direct adapter, model `qwen3-vl-plus`, credential env `QWEN_API_KEY`. |
| Qwen endpoint | Externalized with `QWEN_API_BASE_URL`. |
| Retrieval | Normal web fetch enabled by default; self-hosted and paid tiers remain pluggable fallback channels. |

## Acceptance Criteria

| ID | Acceptance criterion |
| --- | --- |
| AC-001 | `run` executes the full workflow from QueryPlan to package. |
| AC-002 | Defaults for count and quality are applied when omitted. |
| AC-003 | Candidate target is 20 per required delivered image. |
| AC-004 | Missing search credentials produce machine-readable readiness blockers. |
| AC-005 | Weighted scheduling preserves provider provenance. |
| AC-006 | Candidate retrieval requires mechanical pass plus Qwen text relevance score above threshold. |
| AC-007 | Retrieval batch target is 2 per required delivered image. |
| AC-008 | Retrieval success includes local artifact, source artifact, sidecar, summary, task report, visual description, checksum, and trace. |
| AC-009 | Fallback order is direct/source-page web fetch, self-hosted, then paid; paid is not silently used. |
| AC-010 | Delivered images require mechanical pass plus Qwen image approval. |
| AC-011 | The retry model distinguishes `full_attempt_count` from `retry_count`: 1 initial attempt plus up to 3 retries. |
| AC-012 | Canonical package files and directories exist. |
| AC-013 | Broken manifest links, metadata-only delivery, missing artifacts, and missing checksums fail validation. |
| AC-014 | `self-check` reports readiness without writing delivery artifacts. |
| AC-015 | `validate-package` reports pass/fail and concrete issues. |
| AC-016 | Credentials are never serialized into packages, logs, diagnostics, or reports. |
| AC-017 | Policy decisions for robots/site rules, paid channels, and authorization are explicit warnings or blockers, never silent. |
| AC-018 | Provider, retrieval, validation, evaluation, and packaging boundaries are independently testable. |
| AC-019 | External unavailability produces machine-readable failure codes, gaps, and retry evidence. |

## Release Evidence

v1.1 release acceptance requires deterministic tests and a real-service smoke run. Real-service smoke must be explicitly opted in and must write its machine-readable report only when `IMAGE_RETRIEVAL_SMOKE_REPORT_PATH` is set.

Current accepted smoke package:

`/private/tmp/image-retrieval-real-run-v11-fix-20260624-final/package`
