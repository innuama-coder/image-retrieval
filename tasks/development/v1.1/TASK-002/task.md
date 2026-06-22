# Development Task Contract: TASK-002 Search provider registry scheduler and SerpApi adapter

This file describes a planned development task. It is not evidence that the task has been implemented.

## Source Refs

- `docs/v1.1/PRD.md`
- `docs/v1.1/HLD.md`
- `docs/v1.1/LLD.md`
- `docs/design/v1.1-TASK-002-search-provider-candidate-design.md`

## Planning Status

Ready for implementation after reading `tasks/development/v1.1/development-planning.json` and this task package.

## Dependencies

- `TASK-001`

## Downstream Consumers

- `TASK-003`
- `TASK-005`
- `TASK-007`

## Allowed Scope

- Introduce or migrate to BaseSearchProvider with readiness, supported constraints, structured SearchRequest, and SearchResponse.
- Implement provider registry from RuntimeConfig.providers with readiness reports and effective weighted scheduling.
- Implement default serpapi_google_images adapter using endpoint https://serpapi.com/search, engine=google_images, and SERPAPI_API_KEY or configured credential env var.
- Normalize SerpApi image_results[] into CandidateRecord with image URL, source page, thumbnail, rank, dimensions, MIME/license hints, provenance, dedupe key, and origin IDs.
- Preserve deterministic RandomSource for scheduler tests and mark fixtures as fixture-only.

## Forbidden Scope

- Do not retrieve images or mark candidates as accepted for delivery.
- Do not perform candidate mechanical or VLM subjective evaluation.
- Do not hard-code credentials or serialize resolved SERPAPI_API_KEY values.
- Do not let fixture providers satisfy production readiness or production package evidence.

## Expected Outputs

- src/ports/mod.rs
- src/domain/search.rs
- src/domain/candidate.rs
- src/search
- tests/search_integration_test.rs

## Acceptance Criteria

- At least one ready real provider can be scheduled when configured; missing SerpApi credentials produce PROVIDER_CREDENTIAL_MISSING readiness evidence.
- SerpApi image_results[] fixtures normalize into package-safe CandidateRecord values with provenance and dedupe evidence.
- Weighted scheduling preserves provider id, search round, rank, usage events, and shortage diagnostics.
- Search output is ready for TASK-003 and image-recalls.json without leaking secrets.

## Verification

- cargo fmt --all -- --check
- cargo clippy --all-targets --all-features -- -D warnings
- cargo test --test search_integration_test
- cargo test --all

## Stop Conditions

- Stop if SerpApi response fields cannot be mapped without fabricating required candidate facts.
- Stop if provider credential handling would leak resolved secrets into public DTOs, logs, or package evidence.
- Stop if TASK-001 config/domain contracts are missing or incompatible.

## Handoff Requirements

- SearchSessionOutcome, ProviderReadinessReport, normalized CandidateRecord, dedupe/usage diagnostics, and SerpApi adapter tests are ready for quality and orchestration.
