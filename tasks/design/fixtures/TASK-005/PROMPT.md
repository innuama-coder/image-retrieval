# Detailed Design Task Prompt: TASK-005 BaseRetrievalChannel Batch And Fallback Detailed Design

This is a documentation-only detailed design task. Do not implement code. Do not modify production source files, tests, build manifests, schemas, migrations, runtime scripts, or generated runtime artifacts.

## Mission

Complete only TASK-005. Your only deliverable is `docs/design/TASK-005-retrieval-channel-batch-design.md`.

## Read First

- `docs/PRD.md:112-119`
- `docs/PRD.md:206-207`
- `docs/HLD.md:41`
- `docs/HLD.md:211-212`
- `docs/HLD.md:260-315`
- `docs/HLD.md:352`
- `docs/HLD.md:417`
- `docs/design/rust-implementation-design.md`
- `tasks/design/design-planning.json`

## Scope

Design BaseRetrievalChannel, channel tiers, readiness/enabled state, retrieval batch planning, short batch continuation, fallback handoff, local rejection versus task-level execution blocking, paid/disabled channel boundary, and retrieval failure classification.

Forbidden scope: Do not implement code. Do not invent a fourth channel or bypass access/authorization limits.

## Expected Outputs

Save the detailed design document exactly at `docs/design/TASK-005-retrieval-channel-batch-design.md`.

## Review Checks

Cover FR-006, FR-007, AC-006, AC-007, and MET-005. Confirm fallback decisions remain owned by orchestration plus policy and cannot bypass access, authorization, or paid-channel boundaries. Run the planning-aware validator.

## Stop Conditions

Stop if paid channel enablement or fourth-channel semantics must be decided first.

## Handoff

Report the document path, review checks, residual risks, and any planning mismatch.
