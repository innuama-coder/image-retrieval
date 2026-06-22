# image-retrieval v1.1 Development Tasks

## Purpose

This multi-subtask package plans implementation work for v1.1. The development goal is to make the Rust CLI perform image search plus artifact-backed image retrieval, then validate and package accepted images.

## Subtask Relationship And Dependency Graph

- TASK-001 creates the QueryPlan, RuntimeConfig, policy, redaction, and retry-counter foundation.
- TASK-002 depends on TASK-001 and implements image search, the BaseSearchProvider contract, weighted scheduling, candidate normalization, and the default SerpApi Google Images adapter.
- TASK-003 depends on TASK-001 and TASK-002 and implements candidate/image mechanical checks plus the direct Qwen 3.5 VLM provider boundary.
- TASK-004 depends on TASK-001 and TASK-003 and implements artifact-backed retrieval only for candidates that passed candidate mechanical and Qwen gates.
- TASK-005 depends on TASK-001 through TASK-004 and wires the full CLI run loop, canonical package, validation, self-check, and delivery statuses.
- TASK-006 depends on TASK-005 and produces local deterministic, fixture E2E, package validation, security, and real-service smoke evidence.
- TASK-007 depends on every previous subtask and writes the final delivery acceptance report.

## Execution Order

1. TASK-001
2. TASK-002
3. TASK-003
4. TASK-004
5. TASK-005
6. TASK-006
7. TASK-007

No parallel group is declared. The accepted LLD requires a direct/transitive dependency chain from QueryPlan/config to search, candidate quality, retrieval, orchestration, testing, and final acceptance.

## Shared Constraints

- Stay within the current subtask's allowed scope.
- Preserve unrelated user changes.
- Do not hard-code credentials or serialize resolved tokens.
- Do not use fixture provider, fixture retrieval, or fixture Qwen 3.5 VLM output as production acceptance evidence.
- Do not bypass paid, robots/site-rule, authorization, login, or paywall policy.
- Do not claim verification unless the command or check was actually run.

## Acceptance Aggregation And Handoff

Each subtask must report changed areas, verification performed or blocked, acceptance status, downstream dependency outputs, and unresolved risks. TASK-007 is the only final acceptance task. It aggregates handoffs, verification logs, package evidence, real-service smoke or blocked evidence, and release gates into `tasks/development/v1.1/acceptance-report.md`.
