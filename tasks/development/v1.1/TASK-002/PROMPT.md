# Development Task Prompt: TASK-002 Search provider registry scheduler and SerpApi adapter

You are working from a single planned development task from the image-retrieval v1.1 development plan.

## Mission

Complete only `TASK-002`: `Search provider registry scheduler and SerpApi adapter`.

## Execution Objective

Deliver the planned task outcome described by the source-backed task contract without expanding scope, inventing product facts, or performing work from other task IDs. This task contributes to the v1.1 goal: image search plus artifact-backed image retrieval.

## Read First

- Source refs:
- `docs/v1.1/PRD.md`
- `docs/v1.1/HLD.md`
- `docs/v1.1/LLD.md`
- `docs/design/v1.1-TASK-002-search-provider-candidate-design.md`
- Planning JSON: `tasks/development/v1.1/development-planning.json`
- Task description: `tasks/development/v1.1/TASK-002/task.md`
- Task fixture: `tasks/development/v1.1/TASK-002/`

## Scope

Allowed scope:

- Introduce or migrate to BaseSearchProvider with readiness, supported constraints, structured SearchRequest, and SearchResponse.
- Implement provider registry from RuntimeConfig.providers with readiness reports and effective weighted scheduling.
- Implement default serpapi_google_images adapter using endpoint https://serpapi.com/search, engine=google_images, and SERPAPI_API_KEY or configured credential env var.
- Normalize SerpApi image_results[] into CandidateRecord with image URL, source page, thumbnail, rank, dimensions, MIME/license hints, provenance, dedupe key, and origin IDs.
- Preserve deterministic RandomSource for scheduler tests and mark fixtures as fixture-only.

Forbidden scope:

- Do not retrieve images or mark candidates as accepted for delivery.
- Do not perform candidate mechanical or VLM subjective evaluation.
- Do not hard-code credentials or serialize resolved SERPAPI_API_KEY values.
- Do not let fixture providers satisfy production readiness or production package evidence.

## Boundaries And Constraints

- Stay inside the allowed scope for `TASK-002`.
- Preserve unrelated user changes and generated artifacts unless this task explicitly owns them.
- Do not invent requirements, APIs, data fields, runtime components, acceptance criteria, services, or technologies.
- Stop and report blockers when requirements, interfaces, schemas, migrations, permissions, environments, or acceptance evidence are missing or contradictory.
- Never hard-code or serialize resolved provider, retrieval, or Qwen credential values.

## Expected Outputs

- src/ports/mod.rs
- src/domain/search.rs
- src/domain/candidate.rs
- src/search
- tests/search_integration_test.rs

## Acceptance Criteria

- At least one ready real provider can be scheduled when configured; missing SerpApi credentials produce PROVIDER_CREDENTIAL_MISSING readiness evidence.
- SerpApi image_results[] fixtures normalize into package-safe CandidateRecord values with provenance and dedupe evidence.
- Weighted scheduling preserves provider id, search round, rank, usage events, and shortage diagnostics.
- Search output is ready for TASK-003 and image-recalls.json without leaking secrets.

## Verification

- cargo fmt --all -- --check
- cargo clippy --all-targets --all-features -- -D warnings
- cargo test --test search_integration_test
- cargo test --all

## Acceptance Standards And Method

Acceptance is met only when the expected outputs are produced within the allowed scope, forbidden scope is untouched, verification or review checks have been run or honestly blocked, and downstream handoff requirements are satisfied.

## Decision Principles

- Prefer source-backed PRD/HLD/LLD and detailed design evidence over local convenience.
- Make the smallest task-scoped change that satisfies acceptance.
- Preserve existing Rust CLI architecture and conventions unless the source documents require a change.
- If two interpretations are plausible, choose the one with stronger source support or stop for clarification.

## Stop Conditions

- Stop if SerpApi response fields cannot be mapped without fabricating required candidate facts.
- Stop if provider credential handling would leak resolved secrets into public DTOs, logs, or package evidence.
- Stop if TASK-001 config/domain contracts are missing or incompatible.

## Handoff

Report the task ID, changed files or areas, verification performed, acceptance status, dependency outputs for downstream tasks, and unresolved risks or blockers. Do not perform work from other task IDs.
