# Development Task Contract: TASK-004 Artifact backed retrieval channels and fallback execution

This file describes a planned development task. It is not evidence that the task has been implemented.

## Source Refs

- `docs/v1.1/PRD.md`
- `docs/v1.1/HLD.md`
- `docs/v1.1/LLD.md`
- `docs/design/v1.1-TASK-004-retrieval-artifact-channel-design.md`

## Planning Status

Ready for implementation after reading `tasks/development/v1.1/development-planning.json` and this task package.

## Dependencies

- `TASK-001`
- `TASK-003`

## Downstream Consumers

- `TASK-005`
- `TASK-007`

## Allowed Scope

- Implement RetrievalJob, RetrievalBatch, RetrievalArtifactResult, RetrievalBatchResult, attempt traces, fallback decisions, diagnostics, and channel readiness reports.
- Migrate BaseRetrievalChannel to structured readiness and retrieve_batch returning RetrievalBatchResult.
- Plan retrieval only from TASK-003 retrievable candidates and use retrieval_batch_target = required_image_count * 2.
- Implement normal_web_fetch.direct_image_fetch and normal_web_fetch.source_page_resolve as separate attempt modes.
- Generate local artifact, source artifact, source sidecar, content summary, task report, visual description, checksum, content-type evidence, dimensions, diagnostics, and fetch trace for complete results.
- Implement self-hosted and paid channel boundaries/readiness; paid stays disabled unless runtime config and QueryPlan allow it.

## Forbidden Scope

- Do not retrieve candidates that failed candidate mechanical or Qwen candidate gates.
- Do not mark an image as delivered or semantically accepted.
- Do not accept metadata-only, thumbnail-only, source-page-only, summary-only, sidecar-only, or task-report-only results as complete.
- Do not bypass access restriction, robots blocker, prohibited source, login/paywall restriction, or paid-unconfirmed policy.
- Do not let fixture retrieval satisfy production evidence.

## Expected Outputs

- src/retrieval
- src/domain/retrieval.rs
- src/ports/mod.rs
- tests/retrieval_test.rs

## Acceptance Criteria

- Every RetrievalStatus::Complete result has all artifact paths, checksum, content type, dimensions or diagnostic, media match, ownership, and trace evidence.
- Fallback order is direct image fetch, source-page resolve, self-hosted service, paid online service, with policy blockers recorded.
- Metadata-only and remote-only service responses are rejected, not accepted as image retrieval success.
- Retrieval output is ready for TASK-005 to invoke the TASK-003 image acceptance API and then build/package validated delivery results.

## Verification

- cargo fmt --all -- --check
- cargo clippy --all-targets --all-features -- -D warnings
- cargo test --test retrieval_test
- cargo test --all

## Stop Conditions

- Stop if TASK-003 retrievable candidate contract is missing or bypassed.
- Stop if policy decisions are required to run paid, login, paywalled, robots-blocked, or authorization-sensitive retrieval.
- Stop if normal web fetch cannot produce complete local artifact evidence without expanding product scope.

## Handoff Requirements

- RetrievalBatchResult and artifact evidence are ready for image acceptance, canonical package building, validation, and real-service smoke.
