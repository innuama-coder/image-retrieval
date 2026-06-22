# TASK-005 Prompt

## Execution Objective

Create `docs/design/v1.1-TASK-005-orchestrator-package-validation-cli-design.md`
as the future detailed design for the complete run loop, retry semantics,
canonical package, package validation, CLI commands, and self-check behavior.

## Scope

Use TASK-001 through TASK-004 assumptions plus `AGENTS.md`,
`docs/v1.1/PRD.md`, `docs/v1.1/HLD.md`, `docs/v1.1/LLD.md`,
`src/orchestrator`, `src/delivery`, and `src/main.rs`.
Also read `tasks/design/v1.1-lld/fixtures/TASKS.md` for common required
detailed design content.

## Acceptance

The design covers full_attempt_count, retry_count, insufficient-image limited
delivery, canonical files, manifest links, validation fail rules, run/self-check/
validate-package commands, and production-blocking external dependency states.

## Required Output Detail

Specify orchestrator states, retry loop pseudocode, attempt/gap accounting,
package file schemas (`image-recalls.json`, `retrieved-images.json`,
`coverage-report.json`, `retrieval-manifest.json`, `package-summary.json`,
`delivery-report.json`, `validation.json`, `review.json`, `handoff-report.json`),
CLI flags, exit codes, validation rules, self-check output, and tests for full,
partial, blocked, and invalid-package paths.

## Verification

Run the `spec.yaml` verify command against
`docs/design/v1.1-TASK-005-orchestrator-package-validation-cli-design.md`.

## Constraints

Do not implement code. Do not add provider or retrieval protocol details owned
by earlier tasks beyond integration contracts.

## Decision Principles

Prefer explicit state transitions, independent package validation, deterministic
CLI exits, and source traceability to PRD acceptance criteria.
