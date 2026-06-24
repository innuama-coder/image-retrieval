# Changelog

## 1.1.0 - 2026-06-24

- Added the production QueryPlan workflow for real image search, retrieval,
  validation, and delivery packaging.
- Added SerpApi Google Images as the v1.1 default real search provider with
  externalized `SERPAPI_API_KEY` configuration.
- Added Qwen 3.5 VLM production evaluation using `qwen3-vl-plus` and
  externalized `QWEN_API_KEY` / `QWEN_API_BASE_URL` configuration.
- Added candidate text relevance gating before retrieval and local image
  artifact evaluation after download.
- Added artifact-backed web retrieval, canonical package building, package
  validation, self-check readiness, and real-service smoke evidence.
- Closed all 10 v1.1 release gates with real-service smoke evidence and
  validated package output.
