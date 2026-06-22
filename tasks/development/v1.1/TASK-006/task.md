# Development Task Contract: TASK-006 Local test suite and real service smoke evidence

This file describes a planned development task. It is not evidence that the task has been implemented.

## Source Refs

- `docs/v1.1/PRD.md`
- `docs/v1.1/LLD.md`
- `docs/design/v1.1-TASK-006-testing-real-service-acceptance-design.md`
- `RELEASE_GATES.md`

## Planning Status

Ready for implementation after reading `tasks/development/v1.1/development-planning.json` and this task package.

## Dependencies

- `TASK-005`

## Downstream Consumers

- `TASK-007`

## Allowed Scope

- Migrate and extend deterministic unit, integration, fixture E2E, package validation, CLI golden, and security tests for AC-001 through AC-019.
- Add fixture configs, QueryPlans, provider responses, retrieval artifacts, package fixtures, and golden outputs under tests/fixtures/v1_1.
- Add real-service smoke harness gated by IMAGE_RETRIEVAL_REAL_SMOKE=1 and configured env paths/credentials.
- Emit blocked/skipped real-service evidence when credentials, Qwen endpoint, policy decisions, or release gates are absent; blocked/skipped smoke evidence is diagnostic only and cannot satisfy the expected-target completion claim.
- Run and report cargo fmt, cargo clippy, cargo test, and smoke commands when available.

## Forbidden Scope

- Do not add credentials or example secret values.
- Do not mark fixture evidence, skipped smoke, or blocked real-service gates as production acceptance.
- Do not change product behavior beyond narrow bug fixes needed to make tests reflect source-backed contracts.
- Do not claim commands passed unless they actually ran in this task execution cycle.

## Expected Outputs

- tests
- tests/fixtures/v1_1
- tasks/development/v1.1/testing-report.md
- tasks/development/v1.1/real-service-smoke-report.json

## Acceptance Criteria

- Every PRD AC-001 through AC-019 has test or acceptance evidence mapped in the testing report.
- Fixture E2E proves passed, partial, blocked, missing artifact, metadata-only, paid-unconfirmed, access-restricted, secret leak, and production-fixture rejection paths.
- Real-service smoke proves SerpApi search, non-fixture artifact retrieval, Qwen candidate/image evaluation, accepted image, validated package, and secret scan when prerequisites are present.
- When prerequisites are absent, smoke emits machine-readable blocked/skipped evidence and explicitly marks full expected-target completion as not proven.

## Verification

- cargo fmt --all -- --check
- cargo clippy --all-targets --all-features -- -D warnings
- cargo test --all
- image-retrieval self-check --config "$IMAGE_RETRIEVAL_CONFIG" --query-plan "$IMAGE_RETRIEVAL_QUERY_PLAN" --format json
- image-retrieval run --query-plan "$IMAGE_RETRIEVAL_QUERY_PLAN" --config "$IMAGE_RETRIEVAL_CONFIG" --output-dir "$IMAGE_RETRIEVAL_OUTPUT_DIR" --mode production --format json
- image-retrieval validate-package --package-dir "$IMAGE_RETRIEVAL_OUTPUT_DIR/package" --format json

## Stop Conditions

- Stop and record blocked evidence if cargo is unavailable.
- Stop real-service smoke and write blocked/skipped report if IMAGE_RETRIEVAL_REAL_SMOKE is not set or required credentials/endpoints/policy decisions are absent.
- Stop if tests reveal a source/design contradiction requiring PRD/HLD/LLD update.

## Handoff Requirements

- Testing report, verification logs or blocked-command records, fixture packages, and real-service smoke report are ready for final acceptance; the handoff must clearly state whether real SerpApi + retrieval + Qwen evidence proved the expected target.
