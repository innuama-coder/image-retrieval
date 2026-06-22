# Development Task Prompt: TASK-007 Final v1.1 delivery acceptance

You are working from a single planned development task from the image-retrieval v1.1 development plan.

## Mission

Complete only `TASK-007`: `Final v1.1 delivery acceptance`.

## Execution Objective

Deliver the planned task outcome described by the source-backed task contract without expanding scope, inventing product facts, or performing work from other task IDs. This task contributes to the v1.1 goal: image search plus artifact-backed image retrieval.

## Read First

- Source refs:
- `docs/v1.1/PRD.md`
- `docs/v1.1/HLD.md`
- `docs/v1.1/LLD.md`
- `docs/design/v1.1-TASK-007-detailed-design-acceptance-review.md`
- `tasks/development/v1.1/development-planning.json`
- Planning JSON: `tasks/development/v1.1/development-planning.json`
- Task description: `tasks/development/v1.1/TASK-007/task.md`
- Task fixture: `tasks/development/v1.1/TASK-007/`

## Scope

Allowed scope:

- Review all task outputs against PRD/HLD/LLD and this development plan.
- Verify requirement coverage, DAG completion, package output, local verification evidence, real-service smoke evidence, blockers, explicit release-waived notes that do not claim accepted completion, and no-new-scope compliance.
- Write a final acceptance report with accepted, blocked, or rejected verdict. Only accepted means the v1.1 expected target is proven.
- Record constrained defect findings discovered during acceptance; only fix documentation typos in the acceptance report itself.

Forbidden scope:

- Do not add new product scope, provider behavior, retrieval behavior, or tests except acceptance-report documentation.
- Do not mark accepted when SerpApi, retrieval artifact, Qwen, package validation, or real-service smoke evidence is missing. A waiver may only produce blocked/release-waived language, not accepted completion.
- Do not claim cargo or smoke commands passed unless evidence from TASK-006 exists.

## Boundaries And Constraints

- Stay inside the allowed scope for `TASK-007`.
- Preserve unrelated user changes and generated artifacts unless this task explicitly owns them.
- Do not invent requirements, APIs, data fields, runtime components, acceptance criteria, services, or technologies.
- Stop and report blockers when requirements, interfaces, schemas, migrations, permissions, environments, or acceptance evidence are missing or contradictory.
- Never hard-code or serialize resolved provider, retrieval, or Qwen credential values.

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

## Acceptance Standards And Method

Acceptance is met only when the expected outputs are produced within the allowed scope, forbidden scope is untouched, verification or review checks have been run or honestly blocked, and downstream handoff requirements are satisfied.

## Decision Principles

- Prefer source-backed PRD/HLD/LLD and detailed design evidence over local convenience.
- Make the smallest task-scoped change that satisfies acceptance.
- Preserve existing Rust CLI architecture and conventions unless the source documents require a change.
- If two interpretations are plausible, choose the one with stronger source support or stop for clarification.

## Stop Conditions

- Stop and reject acceptance if any non-deferred task evidence is missing.
- Stop and reject or require blocked/release-waived handling if real-service smoke is blocked and the release still claims full expected-target completion.
- Stop if requirement coverage cannot be traced to implementation evidence.

## Handoff

Report the task ID, changed files or areas, verification performed, acceptance status, dependency outputs for downstream tasks, and unresolved risks or blockers. Do not perform work from other task IDs.
