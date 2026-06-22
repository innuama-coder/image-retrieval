# Claude Task Instructions: TASK-007

Claude must follow these instructions when working from one planned task contract in this target project's development plan.

## Role

You are a senior implementation collaborator assigned to `TASK-007`: `Final v1.1 delivery acceptance`. Use source-backed software development context under `docs/`, planning JSON under `tasks/development/v1.1/development-planning.json`, this task's `task.md`, and this fixture directory as the source of truth.

## Context Engineering Standard

Follow Karpathy-style Context Engineering: keep the current task, exact paths, source order, constraints, concrete examples or file paths, verification expectations, and handoff contract in context. Use these instructions to make the task executable, not merely described.

## Required Reading

Before changing files:

1. Read relevant source context documents under `docs/`, especially:
- `docs/v1.1/PRD.md`
- `docs/v1.1/HLD.md`
- `docs/v1.1/LLD.md`
- `docs/design/v1.1-TASK-007-detailed-design-acceptance-review.md`
- `tasks/development/v1.1/development-planning.json`
2. Read `tasks/development/v1.1/development-planning.json`.
3. Read `tasks/development/v1.1/TASK-007/task.md`, `PROMPT.md`, `AGENTS.md`, and `spec.yaml`.
4. Inspect source files needed for this task's allowed scope.

## Constraints

- Do not invent product facts or implementation scope.
- Do not perform work outside `TASK-007`.
- Do not silently change architecture, public APIs, schemas, migrations, state model, deployment behavior, or acceptance criteria.
- Preserve unrelated user changes.
- If a gap appears, state the blocker and request a planning or source-doc update before continuing.
- Do not create standalone starter prompt-template project artifacts.
- `PROMPT.md` defines the execution objective, boundaries, acceptance standards and method, constraints, and decision principles for this task.

## Verification

Run or document the verification required by the fixture:

- Review tasks/development/v1.1/development-planning.json
- Review all TASK-001..TASK-006 handoffs
- Review TASK-006 verification logs and smoke reports
- Review generated package with validate-package evidence
- Review requirement_coverage_matrix and release gates
- Verify acceptance-report.md exists and names verdict, evidence, SerpApi, retrieval, Qwen, package, and real-service evidence status.

If verification is blocked, report the blocker, the command or check that could not run, and the residual delivery risk.

## Final Response

Summarize the task ID addressed, changed files or areas, verification performed, acceptance status, downstream dependency outputs, and remaining risks.
