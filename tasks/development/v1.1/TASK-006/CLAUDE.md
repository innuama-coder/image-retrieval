# Claude Task Instructions: TASK-006

Claude must follow these instructions when working from one planned task contract in this target project's development plan.

## Role

You are a senior implementation collaborator assigned to `TASK-006`: `Local test suite and real service smoke evidence`. Use source-backed software development context under `docs/`, planning JSON under `tasks/development/v1.1/development-planning.json`, this task's `task.md`, and this fixture directory as the source of truth.

## Context Engineering Standard

Follow Karpathy-style Context Engineering: keep the current task, exact paths, source order, constraints, concrete examples or file paths, verification expectations, and handoff contract in context. Use these instructions to make the task executable, not merely described.

## Required Reading

Before changing files:

1. Read relevant source context documents under `docs/`, especially:
- `docs/v1.1/PRD.md`
- `docs/v1.1/LLD.md`
- `docs/design/v1.1-TASK-006-testing-real-service-acceptance-design.md`
- `RELEASE_GATES.md`
2. Read `tasks/development/v1.1/development-planning.json`.
3. Read `tasks/development/v1.1/TASK-006/task.md`, `PROMPT.md`, `AGENTS.md`, and `spec.yaml`.
4. Inspect source files needed for this task's allowed scope.

## Constraints

- Do not invent product facts or implementation scope.
- Do not perform work outside `TASK-006`.
- Do not silently change architecture, public APIs, schemas, migrations, state model, deployment behavior, or acceptance criteria.
- Preserve unrelated user changes.
- If a gap appears, state the blocker and request a planning or source-doc update before continuing.
- Do not create standalone starter prompt-template project artifacts.
- `PROMPT.md` defines the execution objective, boundaries, acceptance standards and method, constraints, and decision principles for this task.

## Verification

Run or document the verification required by the fixture:

- cargo fmt --all -- --check
- cargo clippy --all-targets --all-features -- -D warnings
- cargo test --all
- image-retrieval self-check --config "$IMAGE_RETRIEVAL_CONFIG" --query-plan "$IMAGE_RETRIEVAL_QUERY_PLAN" --format json
- image-retrieval run --query-plan "$IMAGE_RETRIEVAL_QUERY_PLAN" --config "$IMAGE_RETRIEVAL_CONFIG" --output-dir "$IMAGE_RETRIEVAL_OUTPUT_DIR" --mode production --format json
- image-retrieval validate-package --package-dir "$IMAGE_RETRIEVAL_OUTPUT_DIR/package" --format json

If verification is blocked, report the blocker, the command or check that could not run, and the residual delivery risk.

## Final Response

Summarize the task ID addressed, changed files or areas, verification performed, acceptance status, downstream dependency outputs, and remaining risks.
