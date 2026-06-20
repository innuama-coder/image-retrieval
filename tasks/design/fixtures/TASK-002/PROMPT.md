# Detailed Design Task Prompt: TASK-002 QueryPlan CLI And Input Planning Detailed Design

This is a documentation-only detailed design task. Do not implement code. Do not modify production source files, tests, build manifests, schemas, migrations, runtime scripts, or generated runtime artifacts.

## Mission

Complete only TASK-002. Your only deliverable is `docs/design/TASK-002-queryplan-cli-input-planning-design.md`.

## Read First

- `docs/PRD.md:75-90`
- `docs/PRD.md:201-202`
- `docs/HLD.md:77-79`
- `docs/HLD.md:204-206`
- `docs/design/rust-implementation-design.md`
- `tasks/design/design-planning.json`

## Scope

Design QueryPlan input normalization, validation/defaults, derived candidate and batch values, retry planning values, CLI command boundary, input rejection, and input diagnostics.

Forbidden scope: Do not implement code. Do not select a concrete CLI parser library, change product defaults, or design provider/channel/OpenClaw readiness aggregation. Readiness self-check is TASK-008.

## Expected Outputs

Save the detailed design document exactly at `docs/design/TASK-002-queryplan-cli-input-planning-design.md`.

## Review Checks

Cover FR-001, FR-002, AC-001, and AC-002. Run the planning-aware validator.

## Stop Conditions

Stop if required QueryPlan fields or concrete CLI syntax require a new product decision.

## Handoff

Report the document path, review checks, residual risks, and any planning mismatch.
