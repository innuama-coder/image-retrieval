# Development Task Prompt: TASK-003 Candidate and image quality with Qwen 3.5 VLM adapter

You are working from a single planned development task from the image-retrieval v1.1 development plan.

## Mission

Complete only `TASK-003`: `Candidate and image quality with Qwen 3.5 VLM adapter`.

## Execution Objective

Deliver the planned task outcome described by the source-backed task contract without expanding scope, inventing product facts, or performing work from other task IDs. This task contributes to the v1.1 goal: image search plus artifact-backed image retrieval.

## Read First

- Source refs:
- `docs/v1.1/PRD.md`
- `docs/v1.1/HLD.md`
- `docs/v1.1/LLD.md`
- `docs/design/v1.1-TASK-003-quality-vlm-design.md`
- Planning JSON: `tasks/development/v1.1/development-planning.json`
- Task description: `tasks/development/v1.1/TASK-003/task.md`
- Task fixture: `tasks/development/v1.1/TASK-003/`

## Scope

Allowed scope:

- Implement typed mechanical blocking/reference metric facts for candidates and retrieved images.
- Upgrade VlmEvaluationPort with readiness, evaluate_candidates, and evaluate_images structured DTOs.
- Implement direct qwen_3_5_vlm adapter boundary using QWEN_API_KEY by default, runtime endpoint/base URL, model, timeout, and prompt templates.
- Implement candidate decisions so only mechanical pass plus Qwen approve enters RetrievableCandidateBatch.
- Implement the post-retrieval image acceptance API and decision logic over a stable artifact-evidence input DTO; TASK-005 invokes it after TASK-004 produces RetrievalBatchResult.
- Keep fixture evaluator explicitly test-only and blocked in production.

Forbidden scope:

- Do not implement retrieval execution or artifact creation.
- Do not treat mock/fixture VLM decisions as production acceptance.
- Do not hard-code Qwen endpoint, model override, or token value.
- Do not store raw Qwen credentials, raw unrestricted transcripts, or secret-bearing URLs.

## Boundaries And Constraints

- Stay inside the allowed scope for `TASK-003`.
- Preserve unrelated user changes and generated artifacts unless this task explicitly owns them.
- Do not invent requirements, APIs, data fields, runtime components, acceptance criteria, services, or technologies.
- Stop and report blockers when requirements, interfaces, schemas, migrations, permissions, environments, or acceptance evidence are missing or contradictory.
- Never hard-code or serialize resolved provider, retrieval, or Qwen credential values.

## Expected Outputs

- src/quality
- src/domain/image.rs
- src/domain/metrics.rs
- src/ports/mod.rs
- tests/candidate_quality_test.rs

## Acceptance Criteria

- Candidate quality rejects mechanically blocked candidates before VLM and sends only mechanically passed candidates to Qwen in production.
- Qwen unavailable, disabled, invalid response, missing decision, timeout, or fixture-in-production produces execution-blocked evidence.
- Image acceptance API tests reject missing local/source artifact, sidecar, summary, task report, visual description, checksum, media mismatch, metadata-only, and ownership mismatch before VLM using fixture artifact-evidence inputs.
- Quality outputs preserve trace links and expose candidate gates plus image acceptance API contracts for retrieval planning, orchestrator invocation, package review, and validation.

## Verification

- cargo fmt --all -- --check
- cargo clippy --all-targets --all-features -- -D warnings
- cargo test --test candidate_quality_test
- cargo test --all

## Acceptance Standards And Method

Acceptance is met only when the expected outputs are produced within the allowed scope, forbidden scope is untouched, verification or review checks have been run or honestly blocked, and downstream handoff requirements are satisfied.

## Decision Principles

- Prefer source-backed PRD/HLD/LLD and detailed design evidence over local convenience.
- Make the smallest task-scoped change that satisfies acceptance.
- Preserve existing Rust CLI architecture and conventions unless the source documents require a change.
- If two interpretations are plausible, choose the one with stronger source support or stop for clarification.

## Stop Conditions

- Stop if Qwen API contract cannot be represented by the VlmEvaluationPort without inventing unsupported fields.
- Stop if TASK-002 candidate fields or TASK-001 VLM config are missing.
- Stop if implementation would require serializing QWEN_API_KEY or other resolved secrets.

## Handoff

Report the task ID, changed files or areas, verification performed, acceptance status, dependency outputs for downstream tasks, and unresolved risks or blockers. Do not perform work from other task IDs.
