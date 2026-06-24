# v1.1 Final Delivery Acceptance Report

## Verdict

**VERDICT: PASSED FOR v1.1 RELEASE CANDIDATE**

This report reflects the current `codex/v1.1-production-pipeline-fix` worktree
after the audit-chain and real-service fixes made on 2026-06-24.

v1.1 now has:

- A real SerpApi Google Images search path using `SERPAPI_API_KEY`.
- Qwen 3.5 VLM direct evaluation using `QWEN_API_KEY`, model `qwen3-vl-plus`.
- Candidate text relevance evaluation before retrieval.
- Downloaded local image artifact evaluation with Qwen image input after fetch.
- Canonical package validation with manifest reference resolution.
- Real-service smoke evidence generated explicitly, not by ordinary tests.

## Release Gate Status

All 10 release gates are closed by
`tasks/development/v1.1/release-gate-decisions.md`.

`RELEASE_GATES.md` has been updated to reflect the closed gate decisions and the
separate requirement that real-service smoke evidence must be generated with an
explicit `IMAGE_RETRIEVAL_SMOKE_REPORT_PATH`.

## Verification Commands

The following commands were run against the current worktree:

| Command | Status |
| --- | --- |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --all-targets` | PASS |
| `git diff --check` | PASS |
| Real-service smoke via `cargo test --test real_service_smoke_test real_service_smoke_preconditions_report -- --nocapture` with `IMAGE_RETRIEVAL_REAL_SMOKE=1` | PASS |

Final real-service package:

`/private/tmp/image-retrieval-real-run-v11-fix-20260624-final/package`

The generated `tasks/development/v1.1/real-service-smoke-report.json` records:

- `status = passed`
- `commands_run = ok, ok, ok`
- `package_dir = /private/tmp/image-retrieval-real-run-v11-fix-20260624-final/package`
- credentials redacted

## Real Package Evidence

Final package checks:

- `package-summary.json`: `status=passed`, `required_image_count=1`,
  `accepted_image_count=1`, `gap_count=0`.
- `image-recalls.json`: `candidate_target=20`, first attempt
  `candidate_count=20`.
- `retrieval-manifest.json`: `search_ref` resolves to
  `image-recalls.json#/attempts/0/candidates/0`.
- `task-report-task-report.json`: RFC3339 `started_at` and `completed_at`, with
  one recorded retrieval attempt.
- Candidate Qwen evidence: `qwen_candidate_text_relevance`,
  model `qwen3-vl-plus`, score approximately `0.95`.
- Image Qwen evidence: `qwen_image_evaluation`, model `qwen3-vl-plus`,
  decision `approve`.
- Delivered image: local JPEG artifact, 800x452.

## Issues Closed In This Acceptance Cycle

| Issue | Resolution |
| --- | --- |
| Stale release reports contradicted real-service status | Acceptance and release-gate reports updated to current evidence. |
| Manifest `search_ref` pointed to missing `image-recalls` candidates | Package builder writes delivered candidate recall nodes; validator checks `search_ref`. |
| Validator did not catch manifest JSON Pointer breaks | `search_ref` is now part of manifest link resolution checks. |
| Retrieval task reports had empty `attempts` | Web fetch task reports now contain retrieval attempt traces. |
| Retrieval timestamps were Unix seconds despite ISO contract | Web fetch and orchestrator attempt timestamps now use RFC3339 UTC seconds. |
| Source-page fallback could write direct-fetch attempt mode | Artifact writing now accepts the actual attempt mode. |
| SerpApi over-fetched candidates | Scheduler requests remaining candidate target; SerpApi adapter sends and enforces `max_results`. |
| Ordinary tests rewrote release smoke evidence | Smoke report writing now requires explicit `IMAGE_RETRIEVAL_SMOKE_REPORT_PATH`. |
| Reusing an output dir left stale evidence files | Canonical package builder removes stale `package/` before rebuilding. |
| Product docs used old Qwen env/model and OpenClaw wording | v1.1 docs now use `QWEN_API_KEY`, `qwen3-vl-plus`, and Qwen direct adapter wording. |

## Remaining Notes

This report does not claim broad search-quality calibration beyond the single
required v1.1 smoke QueryPlan. Post-MVP calibration remains a product iteration
activity, not a release blocker for this v1.1 acceptance.
