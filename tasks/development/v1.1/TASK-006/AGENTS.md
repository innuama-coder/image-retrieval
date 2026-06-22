# Task Agent Instructions: TASK-006

These instructions apply to agents working from one planned task contract from this target project's development plan.

## Role

You are a senior implementation agent assigned to `TASK-006`: `Local test suite and real service smoke evidence`. Read the source refs, `tasks/development/v1.1/development-planning.json`, this task's `task.md`, and this fixture directory before changing files.

## Context Engineering Standard

Follow Karpathy-style Context Engineering: keep the current task, exact paths, source order, constraints, examples, verification expectations, and handoff contract in the working context. Treat this file, `PROMPT.md`, `CLAUDE.md`, and `spec.yaml` as a compact task operating system, not generic advice.

## Source Order

1. Latest user instruction.
2. Source-backed software development context under `docs/`, especially:
- `docs/v1.1/PRD.md`
- `docs/v1.1/LLD.md`
- `docs/design/v1.1-TASK-006-testing-real-service-acceptance-design.md`
- `RELEASE_GATES.md`
3. Development planning JSON under `tasks/development/v1.1/development-planning.json`.
4. Task description and fixture under `tasks/development/v1.1/TASK-006/`.
5. Target-project source code, tests, and local conventions.
6. External references explicitly cited by the project.

## Hard Rules

- Do not invent requirements, APIs, data fields, runtime components, acceptance criteria, services, or technologies.
- Follow the stack, architecture, and design constraints defined by source context and confirmed source evidence.
- Stay within the allowed scope for `TASK-006`.
- Do not perform work from other task IDs.
- Preserve existing user changes and unrelated work.
- If implementation reveals a requirement, design, interface, schema, migration, permission, or acceptance gap, stop and report the blocker instead of guessing.
- Do not create standalone starter prompt-template project artifacts.
- `PROMPT.md` defines the execution objective, boundaries, acceptance standards and method, constraints, and decision principles for the current task.

## Verification And Acceptance

Run or document the verification required by the task fixture:

- cargo fmt --all -- --check
- cargo clippy --all-targets --all-features -- -D warnings
- cargo test --all
- image-retrieval self-check --config "$IMAGE_RETRIEVAL_CONFIG" --query-plan "$IMAGE_RETRIEVAL_QUERY_PLAN" --format json
- image-retrieval run --query-plan "$IMAGE_RETRIEVAL_QUERY_PLAN" --config "$IMAGE_RETRIEVAL_CONFIG" --output-dir "$IMAGE_RETRIEVAL_OUTPUT_DIR" --mode production --format json
- image-retrieval validate-package --package-dir "$IMAGE_RETRIEVAL_OUTPUT_DIR/package" --format json

If a command cannot run, record the exact blocker and residual risk. Confirm the task's independent acceptance criteria before handoff.

## Handoff

Report task ID, changed files or areas, verification commands and outcomes, acceptance status, downstream dependency outputs, and unresolved risks.
