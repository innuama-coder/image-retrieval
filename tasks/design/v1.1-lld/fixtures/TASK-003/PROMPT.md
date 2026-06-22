# TASK-003 Prompt

## Execution Objective

Create `docs/design/v1.1-TASK-003-quality-vlm-design.md` as the future
detailed design for candidate validation, retrieved image acceptance, mechanical
metrics, reference evidence, and Qwen 3.5 VLM subjective evaluation boundaries.

## Scope

Use TASK-001 and TASK-002 assumptions plus `AGENTS.md`, `docs/v1.1/PRD.md`,
`docs/v1.1/HLD.md`, `docs/v1.1/LLD.md`, and current `src/quality`/`src/ports`.
Also read `tasks/design/v1.1-lld/fixtures/TASKS.md` for common required
detailed design content.

## Acceptance

The design covers blocking metrics, reference metrics, Qwen 3.5 VLM request/response
DTOs, production-unavailable behavior, fixture-only restrictions, and accept
rules requiring both mechanical and subjective pass decisions.

## Required Output Detail

Specify candidate and image evidence structs, blocking/reference metric result
types, Qwen 3.5 VLM adapter trait, request/response schemas, fail-closed states,
decision merging rules, diagnostics, redaction, audit trace, and tests for
mechanical failure, subjective failure, and unavailable Qwen 3.5 VLM behavior.

## Verification

Run the `spec.yaml` verify command against
`docs/design/v1.1-TASK-003-quality-vlm-design.md`.

## Constraints

Do not implement code. Do not replace the production Qwen 3.5 VLM boundary with a
fixture-only shortcut.

## Decision Principles

Prefer explicit evidence structs, fail-closed production behavior, auditable
decisions, and source traceability for every acceptance rule.
