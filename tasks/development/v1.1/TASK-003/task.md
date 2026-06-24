# Development Task Contract: TASK-003 Candidate and image quality with Qwen 3.5 VLM adapter

This file describes a planned development task. It is not evidence that the task has been implemented.

## Source Refs

- `docs/v1.1/PRD.md`
- `docs/v1.1/HLD.md`
- `docs/v1.1/LLD.md`
- `docs/design/v1.1-TASK-003-quality-vlm-design.md`

## Planning Status

Ready for implementation after reading `tasks/development/v1.1/development-planning.json` and this task package.

## Dependencies

- `TASK-001`
- `TASK-002`

## Downstream Consumers

- `TASK-004`
- `TASK-005`
- `TASK-007`

## Allowed Scope

- Implement typed mechanical blocking/reference metric facts for candidates and retrieved images.
- Upgrade VlmEvaluationPort with readiness, evaluate_candidates, and evaluate_images structured DTOs.
- Implement direct qwen_3_5_vlm adapter boundary using QWEN_API_KEY by default, runtime endpoint/base URL, model, timeout, and prompt templates.
- Implement candidate decisions so only mechanical pass plus Qwen approve enters RetrievableCandidateBatch.
- Implement the post-retrieval image acceptance API and decision logic over a stable artifact-evidence input DTO; TASK-005 invokes it after TASK-004 produces RetrievalBatchResult.
- Keep fixture evaluator explicitly test-only and blocked in production.

## Forbidden Scope

- Do not implement retrieval execution or artifact creation.
- Do not treat mock/fixture VLM decisions as production acceptance.
- Do not hard-code Qwen endpoint, model override, or token value.
- Do not store raw Qwen credentials, raw unrestricted transcripts, or secret-bearing URLs.

## Expected Outputs

- src/quality
- src/domain/image.rs
- src/domain/metrics.rs
- src/ports/mod.rs
- tests/candidate_quality_test.rs

## Acceptance Criteria

- Candidate quality rejects mechanically blocked candidates before VLM and sends only mechanically passed candidates to Qwen in production.
- Qwen unavailable, disabled, invalid response, missing decision, timeout, or fixture-in-production produces execution-blocked evidence.
- Image acceptance API tests reject missing local/source artifact, sidecar, summary, task report, visual description, checksum, media mismatch, metadata-only, and ownership mismatch before VLM using fixture artifact-evidence inputs.
- Quality outputs preserve trace links and expose candidate gates plus image acceptance API contracts for retrieval planning, orchestrator invocation, package review, and validation.

## Verification

- cargo fmt --all -- --check
- cargo clippy --all-targets --all-features -- -D warnings
- cargo test --test candidate_quality_test
- cargo test --all

## Stop Conditions

- Stop if Qwen API contract cannot be represented by the VlmEvaluationPort without inventing unsupported fields.
- Stop if TASK-002 candidate fields or TASK-001 VLM config are missing.
- Stop if implementation would require serializing QWEN_API_KEY or other resolved secrets.

## Handoff Requirements

- CandidateQualityOutcome, RetrievableCandidateBatch, image acceptance API/decision types, VLM readiness reports, and quality diagnostics are ready for retrieval and orchestrator integration.
