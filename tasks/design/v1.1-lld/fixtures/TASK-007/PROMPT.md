# TASK-007 Prompt

## Execution Objective

Create `docs/design/v1.1-TASK-007-detailed-design-acceptance-review.md` as the
future final design acceptance report for all v1.1 detailed design documents.

## Scope

Review the six planned detailed design documents, their source traceability,
validator outputs, dependency coverage, unresolved blockers, and handoff
readiness for development planning.
Also read `tasks/design/v1.1-lld/fixtures/TASKS.md` for common required
detailed design content.

## Acceptance

The report inventories every planned `docs/design/*.md` deliverable, records
pass/fail verdicts, cites source coverage, identifies missing or failed evidence,
and states whether development planning may proceed.

## Required Output Detail

Include a document inventory, validator evidence table, source coverage matrix,
cross-document dependency review, implementation-readiness verdict, unresolved
blocker list, required fixes, and go/no-go decision for development planning.

## Verification

Run the `spec.yaml` verify command with acceptance mode against
`docs/design/v1.1-TASK-007-detailed-design-acceptance-review.md`.

## Constraints

Do not implement code. Do not rewrite other design documents during this final
acceptance task.

## Decision Principles

Prefer fail-closed acceptance, complete evidence, source traceability, and clear
handoff blockers over optimistic readiness claims.
