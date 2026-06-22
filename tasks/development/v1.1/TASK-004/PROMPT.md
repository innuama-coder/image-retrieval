# Development Task Prompt: TASK-004 Artifact backed retrieval channels and fallback execution

You are working from a single planned development task from the image-retrieval v1.1 development plan.

## Mission

Complete only `TASK-004`: `Artifact backed retrieval channels and fallback execution`.

## Execution Objective

Deliver the planned task outcome described by the source-backed task contract without expanding scope, inventing product facts, or performing work from other task IDs. This task contributes to the v1.1 goal: image search plus artifact-backed image retrieval.

## Read First

- Source refs:
- `docs/v1.1/PRD.md`
- `docs/v1.1/HLD.md`
- `docs/v1.1/LLD.md`
- `docs/design/v1.1-TASK-004-retrieval-artifact-channel-design.md`
- Planning JSON: `tasks/development/v1.1/development-planning.json`
- Task description: `tasks/development/v1.1/TASK-004/task.md`
- Task fixture: `tasks/development/v1.1/TASK-004/`

## Scope

Allowed scope:

- Implement RetrievalJob, RetrievalBatch, RetrievalArtifactResult, RetrievalBatchResult, attempt traces, fallback decisions, diagnostics, and channel readiness reports.
- Migrate BaseRetrievalChannel to structured readiness and retrieve_batch returning RetrievalBatchResult.
- Plan retrieval only from TASK-003 retrievable candidates and use retrieval_batch_target = required_image_count * 2.
- Implement normal_web_fetch.direct_image_fetch and normal_web_fetch.source_page_resolve as separate attempt modes.
- Generate local artifact, source artifact, source sidecar, content summary, task report, visual description, checksum, content-type evidence, dimensions, diagnostics, and fetch trace for complete results.
- Implement self-hosted and paid channel boundaries/readiness; paid stays disabled unless runtime config and QueryPlan allow it.

Forbidden scope:

- Do not retrieve candidates that failed candidate mechanical or Qwen candidate gates.
- Do not mark an image as delivered or semantically accepted.
- Do not accept metadata-only, thumbnail-only, source-page-only, summary-only, sidecar-only, or task-report-only results as complete.
- Do not bypass access restriction, robots blocker, prohibited source, login/paywall restriction, or paid-unconfirmed policy.
- Do not let fixture retrieval satisfy production evidence.

## Boundaries And Constraints

- Stay inside the allowed scope for `TASK-004`.
- Preserve unrelated user changes and generated artifacts unless this task explicitly owns them.
- Do not invent requirements, APIs, data fields, runtime components, acceptance criteria, services, or technologies.
- Stop and report blockers when requirements, interfaces, schemas, migrations, permissions, environments, or acceptance evidence are missing or contradictory.
- Never hard-code or serialize resolved provider, retrieval, or Qwen credential values.

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

## Acceptance Standards And Method

Acceptance is met only when the expected outputs are produced within the allowed scope, forbidden scope is untouched, verification or review checks have been run or honestly blocked, and downstream handoff requirements are satisfied.

## Decision Principles

- Prefer source-backed PRD/HLD/LLD and detailed design evidence over local convenience.
- Make the smallest task-scoped change that satisfies acceptance.
- Preserve existing Rust CLI architecture and conventions unless the source documents require a change.
- If two interpretations are plausible, choose the one with stronger source support or stop for clarification.

## Stop Conditions

- Stop if TASK-003 retrievable candidate contract is missing or bypassed.
- Stop if policy decisions are required to run paid, login, paywalled, robots-blocked, or authorization-sensitive retrieval.
- Stop if normal web fetch cannot produce complete local artifact evidence without expanding product scope.

## Handoff

Report the task ID, changed files or areas, verification performed, acceptance status, dependency outputs for downstream tasks, and unresolved risks or blockers. Do not perform work from other task IDs.
