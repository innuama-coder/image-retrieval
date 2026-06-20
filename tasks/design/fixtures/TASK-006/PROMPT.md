# Detailed Design Task Prompt: TASK-006 Image Acceptance And Task Orchestrator State Machine Detailed Design

This is a documentation-only detailed design task. Do not implement code. Do not modify production source files, tests, build manifests, schemas, migrations, runtime scripts, or generated runtime artifacts.

## Mission

Complete only TASK-006. Your only deliverable is `docs/design/TASK-006-image-acceptance-orchestrator-design.md`.

## Read First

- `docs/PRD.md:121-159`
- `docs/PRD.md:208-211`
- `docs/HLD.md:260-372`
- `AGENTS.md:85-109`
- `docs/design/rust-implementation-design.md`
- `tasks/design/design-planning.json`

## Scope

Design image mechanical acceptance, OpenClaw image acceptance, decision normalization, task state machine, attempt counters, full delivery termination, execution blocking, retry, limited delivery, and no-real-image batch behavior.

Forbidden scope: Do not implement code. Do not change the initial attempt plus 3 retries rule. Do not use mock/fixture as production acceptance.

## Expected Outputs

Save the detailed design document exactly at `docs/design/TASK-006-image-acceptance-orchestrator-design.md`.

## Review Checks

Cover FR-008, FR-009, FR-011, AC-008, AC-009, and AC-011. Run the planning-aware validator.

## Stop Conditions

Stop if OpenClaw image evaluation responsibility cannot be represented without unsupported protocol details.

## Handoff

Report the document path, review checks, residual risks, and any planning mismatch.
