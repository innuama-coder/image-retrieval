# Development Task Contract: TASK-005 Full run orchestrator canonical package validation and CLI

This file describes a planned development task. It is not evidence that the task has been implemented.

## Source Refs

- `docs/v1.1/PRD.md`
- `docs/v1.1/HLD.md`
- `docs/v1.1/LLD.md`
- `docs/design/v1.1-TASK-005-orchestrator-package-validation-cli-design.md`

## Planning Status

Ready for implementation after reading `tasks/development/v1.1/development-planning.json` and this task package.

## Dependencies

- `TASK-001`
- `TASK-002`
- `TASK-003`
- `TASK-004`

## Downstream Consumers

- `TASK-006`
- `TASK-007`

## Allowed Scope

- Replace MVP run path with full workflow: admission, readiness, search, candidate quality, retrieval, image acceptance, package build, validation, review, and handoff.
- Implement RunState, attempt records, retry invariants, accepted-image carry-forward, retry exhaustion, passed/partial/blocked status.
- Build canonical package files and directories: image-recalls.json, retrieved-images.json, coverage-report.json, retrieval-manifest.json, package-summary.json, delivery-report.json, validation.json, review.json, handoff-report.json, images, evidence, diagnostics.
- Implement PackageValidator and CLI validate-package; implement read-only inspect-package if practical for v1.1.
- Upgrade self-check to report search, retrieval, Qwen 3.5 VLM, policy, output, credential, validator, and release blocker readiness.
- Implement deterministic exit codes and JSON/human CLI output without secret leaks.

## Forbidden Scope

- Do not redesign provider, Qwen, or retrieval internals owned by upstream tasks.
- Do not mark production packages passed with fixture provider/channel/VLM evidence.
- Do not deliver metadata-only, thumbnail-only, source-page-only, sidecar-only, summary-only, or task-report-only items.
- Do not silently enable paid retrieval or bypass robots/authorization policy.

## Expected Outputs

- src/main.rs
- src/orchestrator
- src/delivery
- src/self_check
- src/validation
- src/domain/delivery.rs
- tests/e2e_fixture_test.rs

## Acceptance Criteria

- image-retrieval run executes the full search + quality + retrieval + image acceptance + package + validation flow and no longer stops at planning output.
- Package status is passed only when accepted images reach required count and validator passes; partial and blocked are evidence-backed.
- validate-package reproduces deterministic validation failure codes for missing artifact, checksum, metadata-only delivery, retry mismatch, broken links, fixture production pass, and secret leaks.
- self-check accurately reports readiness and blockers for SerpApi, artifact retrieval, Qwen 3.5 VLM, policy, output, and validator.

## Verification

- cargo fmt --all -- --check
- cargo clippy --all-targets --all-features -- -D warnings
- cargo test --test e2e_fixture_test
- cargo test --all

## Stop Conditions

- Stop if any upstream task contract is missing or incompatible.
- Stop if package validation cannot prove delivered images are local artifact-backed images.
- Stop if CLI behavior would need undocumented product decisions.

## Handoff Requirements

- A fixture-capable full CLI workflow and canonical package validator are ready for testing and real-service smoke harness.
