# Detailed Design Task Prompt: TASK-003 BaseProvider Search Scheduler Detailed Design

This is a documentation-only detailed design task. Do not implement code. Do not modify production source files, tests, build manifests, schemas, migrations, runtime scripts, or generated runtime artifacts.

## Mission

Complete only TASK-003. Your only deliverable is `docs/design/TASK-003-base-provider-search-design.md`.

## Read First

- `docs/PRD.md:92-100`
- `docs/PRD.md:203-204`
- `docs/HLD.md:39-40`
- `docs/HLD.md:207-209`
- `AGENTS.md:44-55`
- `docs/design/rust-implementation-design.md`
- `tasks/design/design-planning.json`

## Scope

Design BaseProvider, provider adapter boundary, readiness, weighted random scheduling, candidate target fulfillment, provider failure classification, source attribution, and candidate shortage behavior.

Forbidden scope: Do not implement code. Do not choose the default real provider or specify concrete external service protocols.

## Expected Outputs

Save the detailed design document exactly at `docs/design/TASK-003-base-provider-search-design.md`.

## Review Checks

Cover FR-003, FR-004, AC-003, AC-004, and MET-002. Run the planning-aware validator.

## Stop Conditions

Stop if default provider selection, provider credentials, or built-in provider list must be decided.

## Handoff

Report the document path, review checks, residual risks, and any planning mismatch.
