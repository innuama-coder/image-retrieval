# Development Task Prompt: TASK-006 Local test suite and real service smoke evidence

You are working from a single planned development task from the image-retrieval v1.1 development plan.

## Mission

Complete only `TASK-006`: `Local test suite and real service smoke evidence`.

## Execution Objective

Deliver the planned task outcome described by the source-backed task contract without expanding scope, inventing product facts, or performing work from other task IDs. This task contributes to the v1.1 goal: image search plus artifact-backed image retrieval.

## Read First

- Source refs:
- `docs/v1.1/PRD.md`
- `docs/v1.1/LLD.md`
- `docs/design/v1.1-TASK-006-testing-real-service-acceptance-design.md`
- `RELEASE_GATES.md`
- Planning JSON: `tasks/development/v1.1/development-planning.json`
- Task description: `tasks/development/v1.1/TASK-006/task.md`
- Task fixture: `tasks/development/v1.1/TASK-006/`

## Scope

Allowed scope:

- Migrate and extend deterministic unit, integration, fixture E2E, package validation, CLI golden, and security tests for AC-001 through AC-019.
- Add fixture configs, QueryPlans, provider responses, retrieval artifacts, package fixtures, and golden outputs under tests/fixtures/v1_1.
- Add real-service smoke harness gated by IMAGE_RETRIEVAL_REAL_SMOKE=1 and configured env paths/credentials.
- Emit blocked/skipped real-service evidence when credentials, Qwen endpoint, policy decisions, or release gates are absent; blocked/skipped smoke evidence is diagnostic only and cannot satisfy the expected-target completion claim.
- Run and report cargo fmt, cargo clippy, cargo test, and smoke commands when available.

Forbidden scope:

- Do not add credentials or example secret values.
- Do not mark fixture evidence, skipped smoke, or blocked real-service gates as production acceptance.
- Do not change product behavior beyond narrow bug fixes needed to make tests reflect source-backed contracts.
- Do not claim commands passed unless they actually ran in this task execution cycle.

## Boundaries And Constraints

- Stay inside the allowed scope for `TASK-006`.
- Preserve unrelated user changes and generated artifacts unless this task explicitly owns them.
- Do not invent requirements, APIs, data fields, runtime components, acceptance criteria, services, or technologies.
- Stop and report blockers when requirements, interfaces, schemas, migrations, permissions, environments, or acceptance evidence are missing or contradictory.
- Never hard-code or serialize resolved provider, retrieval, or Qwen credential values.

## Expected Outputs

- tests
- tests/fixtures/v1_1
- tasks/development/v1.1/testing-report.md
- tasks/development/v1.1/real-service-smoke-report.json

## Acceptance Criteria

- Every PRD AC-001 through AC-019 has test or acceptance evidence mapped in the testing report.
- Fixture E2E proves passed, partial, blocked, missing artifact, metadata-only, paid-unconfirmed, access-restricted, secret leak, and production-fixture rejection paths.
- Real-service smoke proves SerpApi search, non-fixture artifact retrieval, Qwen candidate/image evaluation, accepted image, validated package, and secret scan when prerequisites are present.
- When prerequisites are absent, smoke emits machine-readable blocked/skipped evidence and explicitly marks full expected-target completion as not proven.

## Verification

- cargo fmt --all -- --check
- cargo clippy --all-targets --all-features -- -D warnings
- cargo test --all
- image-retrieval self-check --config "$IMAGE_RETRIEVAL_CONFIG" --query-plan "$IMAGE_RETRIEVAL_QUERY_PLAN" --format json
- image-retrieval run --query-plan "$IMAGE_RETRIEVAL_QUERY_PLAN" --config "$IMAGE_RETRIEVAL_CONFIG" --output-dir "$IMAGE_RETRIEVAL_OUTPUT_DIR" --mode production --format json
- image-retrieval validate-package --package-dir "$IMAGE_RETRIEVAL_OUTPUT_DIR/package" --format json

## Acceptance Standards And Method

Acceptance is met only when the expected outputs are produced within the allowed scope, forbidden scope is untouched, verification or review checks have been run or honestly blocked, and downstream handoff requirements are satisfied.

## Decision Principles

- Prefer source-backed PRD/HLD/LLD and detailed design evidence over local convenience.
- Make the smallest task-scoped change that satisfies acceptance.
- Preserve existing Rust CLI architecture and conventions unless the source documents require a change.
- If two interpretations are plausible, choose the one with stronger source support or stop for clarification.

## Stop Conditions

- Stop and record blocked evidence if cargo is unavailable.
- Stop real-service smoke and write blocked/skipped report if IMAGE_RETRIEVAL_REAL_SMOKE is not set or required credentials/endpoints/policy decisions are absent.
- Stop if tests reveal a source/design contradiction requiring PRD/HLD/LLD update.

## Handoff

Report the task ID, changed files or areas, verification performed, acceptance status, dependency outputs for downstream tasks, whether real SerpApi + retrieval + Qwen evidence proved the expected target, and unresolved risks or blockers. Do not perform work from other task IDs.
