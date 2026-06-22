# Development Task Contract: TASK-007 Final v1.1 delivery acceptance

This file describes a planned development task. It is not evidence that the task has been implemented.

## Source Refs

- `docs/v1.1/PRD.md`
- `docs/v1.1/HLD.md`
- `docs/v1.1/LLD.md`
- `docs/design/v1.1-TASK-007-detailed-design-acceptance-review.md`
- `tasks/development/v1.1/development-planning.json`

## Planning Status

Ready for implementation after reading `tasks/development/v1.1/development-planning.json` and this task package.

## Dependencies

- `TASK-001`
- `TASK-002`
- `TASK-003`
- `TASK-004`
- `TASK-005`
- `TASK-006`

## Downstream Consumers

None

## Allowed Scope

- Review all task outputs against PRD/HLD/LLD and this development plan.
- Verify requirement coverage, DAG completion, package output, local verification evidence, real-service smoke evidence, blockers, explicit release-waived notes that do not claim accepted completion, and no-new-scope compliance.
- Write a final acceptance report with accepted, blocked, or rejected verdict. Only accepted means the v1.1 expected target is proven.
- Record constrained defect findings discovered during acceptance; only fix documentation typos in the acceptance report itself.

## Forbidden Scope

- Do not add new product scope, provider behavior, retrieval behavior, or tests except acceptance-report documentation.
- Do not mark accepted when SerpApi, retrieval artifact, Qwen, package validation, or real-service smoke evidence is missing. A waiver may only produce blocked/release-waived language, not accepted completion.
- Do not claim cargo or smoke commands passed unless evidence from TASK-006 exists.

## Expected Outputs

- tasks/development/v1.1/acceptance-report.md

## Acceptance Criteria

- Acceptance report covers every FR/AC and states pass/fail/blocked/waived evidence.
- Final verdict is blocked or rejected when required task output, verification, package validation, or real-service smoke evidence is missing; waiver language must not convert missing expected-target evidence into accepted completion.
- Final verdict confirms image search + artifact-backed retrieval + Qwen acceptance + canonical package when accepted.
- Unresolved paid, robots/site-rule, authorization, quality calibration, or environment blockers are recorded with explicit handling.

## Verification

- Review tasks/development/v1.1/development-planning.json
- Review all TASK-001..TASK-006 handoffs
- Review TASK-006 verification logs and smoke reports
- Review generated package with validate-package evidence
- Review requirement_coverage_matrix and release gates
- Verify acceptance-report.md exists and names verdict, evidence, SerpApi, retrieval, Qwen, package, and real-service evidence status.

## Stop Conditions

- Stop and reject acceptance if any non-deferred task evidence is missing.
- Stop and reject or require blocked/release-waived handling if real-service smoke is blocked and the release still claims full expected-target completion.
- Stop if requirement coverage cannot be traced to implementation evidence.

## Handoff Requirements

- Final acceptance report gives the next user or release owner a clear pass/fail verdict, evidence index, and remaining blockers.
