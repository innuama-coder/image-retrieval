# TASK-001 Prompt

## Execution Objective

Create `docs/design/v1.1-TASK-001-queryplan-config-policy-design.md` as a
future detailed design document for QueryPlan, runtime config, policy, defaults,
retry counters, diagnostics, and redaction.

## Scope

Use source evidence from `AGENTS.md`, `docs/v1.1/PRD.md`,
`docs/v1.1/HLD.md`, `docs/v1.1/LLD.md`, and current `src/domain` modules.
Also read `tasks/design/v1.1-lld/fixtures/TASKS.md` for common required
detailed design content.

## Acceptance

The design covers required image count defaults, quality defaults, candidate
target calculation, retry_count versus full_attempt_count, config externalization,
policy validation, diagnostics, and serialization boundaries.

## Required Output Detail

Specify Rust module placement, `QueryPlan`, quality requirement, runtime config,
policy, retry, and diagnostic structs. Define field names, defaults, validation
rules, serde behavior, redaction rules, failure codes, and tests that prove the
domain/config contract.

## Verification

Run the `spec.yaml` verify command against
`docs/design/v1.1-TASK-001-queryplan-config-policy-design.md`.

## Constraints

Do not implement code. Do not edit tests, Cargo manifests, or runtime source.

## Decision Principles

Prefer explicit Rust structs, serde-ready schemas, clear errors, and source
traceability back to PRD/HLD requirements.
