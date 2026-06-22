# TASK-006 Prompt

## Execution Objective

Create `docs/design/v1.1-TASK-006-testing-real-service-acceptance-design.md` as
the future detailed design for unit, integration, E2E, real-service smoke,
blocked-evidence, and final v1.1 testing acceptance.

## Scope

Use TASK-005 assumptions plus `AGENTS.md`, `docs/v1.1/PRD.md`,
`docs/v1.1/HLD.md`, `docs/v1.1/LLD.md`, release gates, and current test
structure.
Also read `tasks/design/v1.1-lld/fixtures/TASKS.md` for common required
detailed design content.

## Acceptance

The design maps every PRD acceptance criterion to a test, real-service check,
or explicit blocker; it covers provider credentials, Qwen 3.5 VLM availability,
retrieval channel availability, package validation, and cargo verification.

## Required Output Detail

Specify the AC-to-test matrix, fixture tests, provider contract tests, retrieval
artifact tests, Qwen 3.5 VLM boundary tests, CLI golden tests, package validation
tests, real-service smoke prerequisites, environment variables, skipped/blocked
evidence format, and exact verification commands that must not be claimed unless
run.

## Verification

Run the `spec.yaml` verify command against
`docs/design/v1.1-TASK-006-testing-real-service-acceptance-design.md`.

## Constraints

Do not implement code. Do not run service tests or modify test files in this
design task.

## Decision Principles

Prefer executable evidence, explicit blockers, reproducible fixtures, and clear
separation between local tests and real-service acceptance.
