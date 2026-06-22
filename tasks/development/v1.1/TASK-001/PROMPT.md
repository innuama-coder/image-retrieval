# Development Task Prompt: TASK-001 QueryPlan config policy and shared domain foundation

You are working from a single planned development task from the image-retrieval v1.1 development plan.

## Mission

Complete only `TASK-001`: `QueryPlan config policy and shared domain foundation`.

## Execution Objective

Deliver the planned task outcome described by the source-backed task contract without expanding scope, inventing product facts, or performing work from other task IDs. This task contributes to the v1.1 goal: image search plus artifact-backed image retrieval.

## Read First

- Source refs:
- `docs/v1.1/PRD.md`
- `docs/v1.1/HLD.md`
- `docs/v1.1/LLD.md`
- `docs/design/v1.1-TASK-001-queryplan-config-policy-design.md`
- Planning JSON: `tasks/development/v1.1/development-planning.json`
- Task description: `tasks/development/v1.1/TASK-001/task.md`
- Task fixture: `tasks/development/v1.1/TASK-001/`

## Scope

Allowed scope:

- Implement v1.1 QueryPlanInput and NormalizedQueryPlan with required_image_count canonical field and required_count alias.
- Implement RuntimeConfig DTOs for providers, retrieval channels, Qwen 3.5 VLM, policy, output, quality defaults, and execution limits.
- Implement defaults and invariants: count=1, quality=general, candidate_target=20N, retrieval_batch_target=2N, retry_limit=3, full_attempt_limit=4.
- Implement policy narrowing, paid disabled by default, robots warn/block posture, redaction helpers, and machine-readable admission/config diagnostics.

Forbidden scope:

- Do not call external search providers, retrieval channels, or Qwen 3.5 VLM.
- Do not build delivery packages or implement CLI run orchestration.
- Do not serialize resolved credentials, tokens, cookies, signed URLs, or private keys.

## Boundaries And Constraints

- Stay inside the allowed scope for `TASK-001`.
- Preserve unrelated user changes and generated artifacts unless this task explicitly owns them.
- Do not invent requirements, APIs, data fields, runtime components, acceptance criteria, services, or technologies.
- Stop and report blockers when requirements, interfaces, schemas, migrations, permissions, environments, or acceptance evidence are missing or contradictory.
- Never hard-code or serialize resolved provider, retrieval, or Qwen credential values.

## Expected Outputs

- src/domain/query_plan.rs
- src/domain/config.rs
- src/domain/policy.rs
- src/domain/mod.rs
- src/policy/mod.rs
- src/error/mod.rs
- tests/domain_baseline_test.rs

## Acceptance Criteria

- Valid QueryPlan/config input produces NormalizedQueryPlan and RuntimeConfig with package-safe diagnostics.
- All target derivation and retry/full-attempt invariants are represented by typed data and tests.
- Policy config cannot silently broaden QueryPlan policy and cannot enable paid retrieval without explicit config and QueryPlan allowance.
- Redaction removes credential-like values from diagnostics and package-safe projections.

## Verification

- cargo fmt --all -- --check
- cargo clippy --all-targets --all-features -- -D warnings
- cargo test --test domain_baseline_test
- cargo test --all

## Acceptance Standards And Method

Acceptance is met only when the expected outputs are produced within the allowed scope, forbidden scope is untouched, verification or review checks have been run or honestly blocked, and downstream handoff requirements are satisfied.

## Decision Principles

- Prefer source-backed PRD/HLD/LLD and detailed design evidence over local convenience.
- Make the smallest task-scoped change that satisfies acceptance.
- Preserve existing Rust CLI architecture and conventions unless the source documents require a change.
- If two interpretations are plausible, choose the one with stronger source support or stop for clarification.

## Stop Conditions

- Stop if a source-backed schema change would alter PRD/LLD public QueryPlan or RuntimeConfig semantics.
- Stop if a requested implementation would require storing resolved secrets in public DTOs.
- Stop if cargo is unavailable and record the blocked command and residual risk.

## Handoff

Report the task ID, changed files or areas, verification performed, acceptance status, dependency outputs for downstream tasks, and unresolved risks or blockers. Do not perform work from other task IDs.
