# Detailed Design Task Prompt: TASK-007 Delivery Package Policy Observability And Verification Detailed Design

This is a documentation-only detailed design task. Do not implement code. Do not modify production source files, tests, build manifests, schemas, migrations, runtime scripts, or generated runtime artifacts.

## Mission

Complete only TASK-007. Your only deliverable is `docs/design/TASK-007-delivery-policy-observability-design.md`.

## Read First

- `docs/PRD.md:121-131`
- `docs/PRD.md:178-222`
- `docs/HLD.md:402-431`
- `docs/HLD.md:473-476`
- `docs/design/TASK-003-base-provider-search-design.md`
- `docs/design/TASK-004-candidate-quality-openclaw-design.md`
- `docs/design/TASK-005-retrieval-channel-batch-design.md`
- `docs/design/TASK-006-image-acceptance-orchestrator-design.md`
- `docs/design/rust-implementation-design.md`
- `tasks/design/design-planning.json`

## Scope

Design delivery package directory/file layout, manifest and status contract, result status consumption, policy guardrails, sensitive data exclusion, observability metric event sources across provider/candidate/retrieval/OpenClaw/orchestrator/delivery boundaries, diagnostics, fixture validation, real-service validation, and rollback evidence.

Forbidden scope: Do not implement code. Do not choose a runtime serialization library as a committed fact. Do not mark unknown authorization as commercial-safe.

## Expected Outputs

Save the detailed design document exactly at `docs/design/TASK-007-delivery-policy-observability-design.md`.

## Review Checks

Cover FR-010, FR-013, AC-010, AC-013, NFR-001 through NFR-006, and MET-001 through MET-006. Explicitly map every metric to its producer boundary and design the package layout and machine-readable contract. Run the planning-aware validator.

## Stop Conditions

Stop if authorization grouping, robots policy, or maximum QueryPlan count must be decided before design can proceed.

## Handoff

Report the document path, review checks, residual risks, and any planning mismatch.
