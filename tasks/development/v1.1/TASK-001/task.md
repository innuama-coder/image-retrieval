# Development Task Contract: TASK-001 QueryPlan config policy and shared domain foundation

This file describes a planned development task. It is not evidence that the task has been implemented.

## Source Refs

- `docs/v1.1/PRD.md`
- `docs/v1.1/HLD.md`
- `docs/v1.1/LLD.md`
- `docs/design/v1.1-TASK-001-queryplan-config-policy-design.md`

## Planning Status

Ready for implementation after reading `tasks/development/v1.1/development-planning.json` and this task package.

## Dependencies

None

## Downstream Consumers

- `TASK-002`
- `TASK-003`
- `TASK-004`
- `TASK-005`
- `TASK-007`

## Allowed Scope

- Implement v1.1 QueryPlanInput and NormalizedQueryPlan with required_image_count canonical field and required_count alias.
- Implement RuntimeConfig DTOs for providers, retrieval channels, Qwen 3.5 VLM, policy, output, quality defaults, and execution limits.
- Implement defaults and invariants: count=1, quality=general, candidate_target=20N, retrieval_batch_target=2N, retry_limit=3, full_attempt_limit=4.
- Implement policy narrowing, paid disabled by default, robots warn/block posture, redaction helpers, and machine-readable admission/config diagnostics.

## Forbidden Scope

- Do not call external search providers, retrieval channels, or Qwen 3.5 VLM.
- Do not build delivery packages or implement CLI run orchestration.
- Do not serialize resolved credentials, tokens, cookies, signed URLs, or private keys.

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

## Stop Conditions

- Stop if a source-backed schema change would alter PRD/LLD public QueryPlan or RuntimeConfig semantics.
- Stop if a requested implementation would require storing resolved secrets in public DTOs.
- Stop if cargo is unavailable and record the blocked command and residual risk.

## Handoff Requirements

- NormalizedQueryPlan, RuntimeConfig, policy/redaction helpers, failure codes, and tests are ready for search, quality, retrieval, and orchestrator tasks.
