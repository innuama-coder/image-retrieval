# Detailed Design Task Prompt: TASK-008 Readiness Self-check Integration Detailed Design

This is a documentation-only detailed design task. Do not implement code. Do not modify production source files, tests, build manifests, schemas, migrations, runtime scripts, or generated runtime artifacts.

## Mission

Complete only TASK-008. Your only deliverable is `docs/design/TASK-008-readiness-self-check-design.md`.

## Read First

- `docs/PRD.md:175`
- `docs/PRD.md:212`
- `docs/HLD.md:78`
- `docs/HLD.md:205`
- `docs/HLD.md:219-233`
- `docs/HLD.md:235-257`
- `docs/HLD.md:420`
- `docs/design/TASK-002-queryplan-cli-input-planning-design.md`
- `docs/design/TASK-003-base-provider-search-design.md`
- `docs/design/TASK-004-candidate-quality-openclaw-design.md`
- `docs/design/TASK-005-retrieval-channel-batch-design.md`
- `docs/design/TASK-006-image-acceptance-orchestrator-design.md`
- `docs/design/TASK-007-delivery-policy-observability-design.md`
- `tasks/design/design-planning.json`

## Scope

Design the readiness self-check integration after QueryPlan, provider, channel, candidate OpenClaw, image OpenClaw, and policy readiness boundaries are detailed. Cover aggregation control flow, input data, output report, diagnostics, automation-readable status, and the rule that self-check does not search, retrieve, perform production subjective evaluation, or generate a delivery package.

Forbidden scope: Do not implement code. Do not modify production files. Do not perform search/retrieval/production subjective evaluation in self-check. Do not generate a delivery package from self-check.

## Expected Outputs

Save the detailed design document exactly at `docs/design/TASK-008-readiness-self-check-design.md`.

## Review Checks

Cover FR-012 and AC-012. Confirm readiness inputs are sourced from TASK-002, TASK-003, TASK-004, TASK-005, TASK-006, and TASK-007 rather than invented locally. Run the planning-aware validator.

## Stop Conditions

Stop if candidate OpenClaw readiness, image OpenClaw readiness, provider readiness, retrieval channel enablement, or policy blocker semantics cannot be represented from upstream design documents.

## Handoff

Report the document path, review checks, blockers, residual risks, and required updates.
