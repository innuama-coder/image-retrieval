# image-retrieval v1.1 HLD

## System Boundary

`image-retrieval` is a local Rust CLI. It accepts a QueryPlan, searches image candidates, evaluates candidate relevance, retrieves local image artifacts, evaluates downloaded images, and writes a canonical delivery package.

External systems:

- SerpApi Google Images for v1.1 real image search.
- Qwen 3.5 VLM for production subjective candidate and image evaluation.
- Web/source hosts for normal artifact retrieval.
- Optional self-hosted and paid retrieval channels for fallback tiers.

## Architecture Principles

| ID | Principle |
| --- | --- |
| A-001 | Keep CLI-first Rust boundaries. |
| A-002 | Keep search providers, retrieval channels, validation, evaluation, and packaging pluggable and independently testable. |
| A-003 | Use structured domain models and traits instead of ad hoc conditionals. |
| A-004 | Production subjective evidence must come from Qwen 3.5 VLM; fixture evaluators are test-only. |
| A-005 | All failures must be honest and auditable through statuses, diagnostics, gaps, failure codes, and retry evidence. |

## Main Components

| Component | Responsibility |
| --- | --- |
| `domain` | QueryPlan, config, policy, candidate, retrieval, image, metrics, and delivery data models. |
| `search` | Provider registry, weighted scheduler, SerpApi adapter, fixture provider. |
| `quality` | Candidate mechanical checks, Qwen candidate text relevance, image mechanical checks, Qwen image evaluation. |
| `retrieval` | Batch planning, normal web fetch, fallback channel boundaries, artifact writing. |
| `orchestrator` / `pipeline` | Full run state, attempts, retries, gaps, and terminal decisions. |
| `delivery` | Canonical package generation and artifact localization. |
| `validation` | Deterministic package validation, manifest reference resolution, artifact checks, redaction checks. |
| `self_check` | Readiness diagnostics. |
| `main` | CLI command routing. |

## Runtime Flow

1. CLI loads config and QueryPlan.
2. QueryPlan is normalized and defaults are applied.
3. Search scheduler selects configured providers and requests the remaining candidate target.
4. SerpApi returns image candidates; the adapter normalizes and locally limits results to `SearchRequest.max_results`.
5. Candidate mechanical validation runs first.
6. Qwen evaluates candidate text context; only candidates above the relevance threshold can be retrieved.
7. Retrieval batch planner selects up to `required_image_count * 2` retrievable candidates.
8. Retrieval channels fetch local artifacts and write sidecars, summaries, task reports, visual descriptions, checksums, and traces.
9. Image mechanical validation runs on retrieved artifacts.
10. Qwen evaluates the downloaded local image bytes.
11. Accepted images are accumulated until the requested count is reached or retries are exhausted.
12. Delivery builder writes a clean canonical package.
13. Validator checks the package, including manifest JSON Pointer references.

## Key Decisions

| Decision | Outcome |
| --- | --- |
| Default search provider | SerpApi Google Images with `SERPAPI_API_KEY`. |
| Production subjective evaluator | Direct Qwen 3.5 VLM adapter with `QWEN_API_KEY`, model `qwen3-vl-plus`, endpoint `QWEN_API_BASE_URL`. |
| Candidate target | Roughly 20 candidates per required delivered image. |
| Retrieval target | 2 retrieval jobs per required delivered image. |
| Retry model | 1 initial full attempt plus up to 3 retries; `retry_count = full_attempt_count - 1`. |
| Release gates | All 10 v1.1 gates closed by `tasks/development/v1.1/release-gate-decisions.md`. |

## Reliability And Observability

- Search, retrieval, Qwen, policy, output, and validator readiness are exposed through `self-check`.
- External failures produce machine-readable diagnostics instead of silent success.
- Package evidence uses package-relative paths and redacted URLs.
- Retrieval task reports use RFC3339 UTC timestamps and include attempt traces.
- Canonical package rebuilds remove stale package files before writing new evidence.

## Release Evidence

Current accepted real-service package:

`/private/tmp/image-retrieval-real-run-v11-fix-20260624-final/package`

The package validates with `status=pass` and contains one accepted image for the v1.1 smoke QueryPlan.
