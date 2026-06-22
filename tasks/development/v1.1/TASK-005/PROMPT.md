# Development Task Prompt: TASK-005 Full run orchestrator canonical package validation and CLI

You are working from a single planned development task from the image-retrieval v1.1 development plan.

## Mission

Complete only `TASK-005`: `Full run orchestrator canonical package validation and CLI`.

## Execution Objective

Deliver the planned task outcome described by the source-backed task contract without expanding scope, inventing product facts, or performing work from other task IDs. This task contributes to the v1.1 goal: image search plus artifact-backed image retrieval.

## Read First

- Source refs:
- `docs/v1.1/PRD.md`
- `docs/v1.1/HLD.md`
- `docs/v1.1/LLD.md`
- `docs/design/v1.1-TASK-005-orchestrator-package-validation-cli-design.md`
- Planning JSON: `tasks/development/v1.1/development-planning.json`
- Task description: `tasks/development/v1.1/TASK-005/task.md`
- Task fixture: `tasks/development/v1.1/TASK-005/`

## Scope

Allowed scope:

- Replace MVP run path with full workflow: admission, readiness, search, candidate quality, retrieval, image acceptance, package build, validation, review, and handoff.
- Implement RunState, attempt records, retry invariants, accepted-image carry-forward, retry exhaustion, passed/partial/blocked status.
- Build canonical package files and directories: image-recalls.json, retrieved-images.json, coverage-report.json, retrieval-manifest.json, package-summary.json, delivery-report.json, validation.json, review.json, handoff-report.json, images, evidence, diagnostics.
- Implement PackageValidator and CLI validate-package; implement read-only inspect-package if practical for v1.1.
- Upgrade self-check to report search, retrieval, Qwen 3.5 VLM, policy, output, credential, validator, and release blocker readiness.
- Implement deterministic exit codes and JSON/human CLI output without secret leaks.

Forbidden scope:

- Do not redesign provider, Qwen, or retrieval internals owned by upstream tasks.
- Do not mark production packages passed with fixture provider/channel/VLM evidence.
- Do not deliver metadata-only, thumbnail-only, source-page-only, sidecar-only, summary-only, or task-report-only items.
- Do not silently enable paid retrieval or bypass robots/authorization policy.

## Boundaries And Constraints

- Stay inside the allowed scope for `TASK-005`.
- Preserve unrelated user changes and generated artifacts unless this task explicitly owns them.
- Do not invent requirements, APIs, data fields, runtime components, acceptance criteria, services, or technologies.
- Stop and report blockers when requirements, interfaces, schemas, migrations, permissions, environments, or acceptance evidence are missing or contradictory.
- Never hard-code or serialize resolved provider, retrieval, or Qwen credential values.

## Expected Outputs

- src/main.rs
- src/orchestrator
- src/delivery
- src/self_check
- src/validation
- src/domain/delivery.rs
- tests/e2e_fixture_test.rs

## Acceptance Criteria

- image-retrieval run executes the full search + quality + retrieval + image acceptance + package + validation flow and no longer stops at planning output.
- Package status is passed only when accepted images reach required count and validator passes; partial and blocked are evidence-backed.
- validate-package reproduces deterministic validation failure codes for missing artifact, checksum, metadata-only delivery, retry mismatch, broken links, fixture production pass, and secret leaks.
- self-check accurately reports readiness and blockers for SerpApi, artifact retrieval, Qwen 3.5 VLM, policy, output, and validator.

## Verification

- cargo fmt --all -- --check
- cargo clippy --all-targets --all-features -- -D warnings
- cargo test --test e2e_fixture_test
- cargo test --all

## Acceptance Standards And Method

Acceptance is met only when the expected outputs are produced within the allowed scope, forbidden scope is untouched, verification or review checks have been run or honestly blocked, and downstream handoff requirements are satisfied.

## Decision Principles

- Prefer source-backed PRD/HLD/LLD and detailed design evidence over local convenience.
- Make the smallest task-scoped change that satisfies acceptance.
- Preserve existing Rust CLI architecture and conventions unless the source documents require a change.
- If two interpretations are plausible, choose the one with stronger source support or stop for clarification.

## Stop Conditions

- Stop if any upstream task contract is missing or incompatible.
- Stop if package validation cannot prove delivered images are local artifact-backed images.
- Stop if CLI behavior would need undocumented product decisions.

## Handoff

Report the task ID, changed files or areas, verification performed, acceptance status, dependency outputs for downstream tasks, and unresolved risks or blockers. Do not perform work from other task IDs.
