# v1.1 Testing Report

## Current Status

The v1.1 test and smoke evidence has been refreshed after the audit-chain fixes
made on 2026-06-24.

## Deterministic Tests

The deterministic suite covers:

- QueryPlan admission, defaults, retry counters, policy narrowing.
- Search provider readiness, weighted scheduling, dedupe, SerpApi normalization,
  and candidate target request sizing.
- Candidate mechanical validation and Qwen text relevance gating.
- Retrieval planning, fallback boundaries, artifact completeness, timestamps,
  task reports, and attempt traces.
- Image mechanical validation and Qwen image acceptance.
- Canonical package building, stale package cleanup, manifest links, validation,
  secret scanning, and fixture rejection in production mode.
- CLI behavior for `run`, `self-check`, and `validate-package`.

Latest full deterministic command to run before release:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
git diff --check
```

## Real-Service Smoke

Real-service smoke must be explicit. Ordinary `cargo test` runs do not rewrite
release evidence. To persist a smoke report, set
`IMAGE_RETRIEVAL_SMOKE_REPORT_PATH`.

Final accepted smoke run:

```bash
IMAGE_RETRIEVAL_REAL_SMOKE=1 \
IMAGE_RETRIEVAL_CONFIG=tests/fixtures/v1_1/configs/config-production-like.toml \
IMAGE_RETRIEVAL_QUERY_PLAN=tests/fixtures/v1_1/query-plans/query-plan-basic.json \
IMAGE_RETRIEVAL_OUTPUT_DIR=/private/tmp/image-retrieval-real-run-v11-fix-20260624-final \
IMAGE_RETRIEVAL_SMOKE_REPORT_PATH=tasks/development/v1.1/real-service-smoke-report.json \
cargo test --test real_service_smoke_test real_service_smoke_preconditions_report -- --nocapture
```

Result:

- self-check: ready
- run: passed
- validate-package: pass
- required image count: 1
- accepted image count: 1
- package:
  `/private/tmp/image-retrieval-real-run-v11-fix-20260624-final/package`

Baseline real-service regression is a separate opt-in suite:

```bash
IMAGE_RETRIEVAL_REAL_BASELINE=1 \
IMAGE_RETRIEVAL_BASELINE_CASES=rt_direct_ok_batch,dl_partial_delivery_with_accurate_gaps \
IMAGE_RETRIEVAL_BASELINE_CONFIG=tests/fixtures/v1_1/configs/config-production-like.toml \
cargo test --test baseline_real_service_test -- --nocapture
```

The baseline runner now fails the test when a real CLI case has a non-zero exit
code, an unexpected status, insufficient accepted images, or failed package
validation. Unit baseline tests cover candidate recall, candidate retrieval,
and delivery fixture thresholds separately from public-web live scenarios.

## Evidence Checks

The final smoke package confirms:

- `candidate_target=20`
- `candidate_count=20`
- exactly one delivered candidate quality evidence file
- manifest `search_ref` resolves to a real `image-recalls` candidate node
- task report timestamps are RFC3339 UTC strings
- task report includes one retrieval attempt
- candidate evaluation evidence source is `qwen_candidate_text_relevance`
- image evaluation evidence source is `qwen_image_evaluation`
- package validator reports `status=pass` with no issues

## Release Risks

No test-only fixture or mock evidence is counted as production evidence.
Post-MVP quality calibration remains an iteration item, not a v1.1 release
blocker.
