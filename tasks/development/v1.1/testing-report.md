# v1.1 Testing Report — TASK-006

## Document Control

| Field | Value |
| --- | --- |
| Task ID | TASK-006 |
| Deliverable | tasks/development/v1.1/testing-report.md |
| Work type | Testing evidence and acceptance report |
| Git commit | `spec-exec/TASK-006-image-retrieval-v1-1-6F9BX5` |
| Date | 2026-06-22 |

## Scope

This report covers local deterministic test evidence and real-service smoke evidence for the image-retrieval v1.1 implementation (TASK-001 through TASK-005 outputs). It maps all PRD acceptance criteria AC-001 through AC-019 to specific test evidence.

## Commands Run

| Command | Status | Exit | Summary |
| --- | --- | --- | --- |
| `cargo fmt --all -- --check` | PASS | 0 | No formatting issues. |
| `cargo clippy --all-targets --all-features -- -D warnings` | PASS | 0 | No warnings or errors. |
| `cargo test --test domain_baseline_test` | PASS | 0 | 95 passed, 0 failed. |
| `cargo test --test search_integration_test` | PASS | 0 | 20 passed, 0 failed. |
| `cargo test --test candidate_quality_test` | PASS | 0 | 58 passed, 0 failed. |
| `cargo test --test retrieval_test` | PASS | 0 | 33 passed, 0 failed. |
| `cargo test --test e2e_fixture_test` | PASS | 0 | 48 passed, 0 failed. |
| `cargo test --test fixture_v1_1_test` | PASS | 0 | 30 passed, 0 failed. |
| **Total: `cargo test --all`** | **PASS** | **0** | **284 passed, 0 failed** across 6 test suites. |

## Real-Service Smoke Commands

| Command | Status | Reason |
| --- | --- | --- |
| `image-retrieval self-check --config "$IMAGE_RETRIEVAL_CONFIG" --query-plan "$IMAGE_RETRIEVAL_QUERY_PLAN" --format json` | **SKIPPED** | `IMAGE_RETRIEVAL_REAL_SMOKE` env var not set; no real provider credentials (`SERPAPI_API_KEY`), no Qwen endpoint/token (`QWEN_API_TOKEN`), no retrieval channel config. |
| `image-retrieval run --query-plan ... --config ... --output-dir ... --mode production --format json` | **SKIPPED** | Blocked by real-service smoke gate. |
| `image-retrieval validate-package --package-dir ...` | **SKIPPED** | Blocked by real-service smoke gate. |

Real-service smoke is **blocked** because no real provider credentials, Qwen 3.5 VLM endpoint, or production retrieval channel configuration is available. See `tasks/development/v1.1/real-service-smoke-report.json` for machine-readable blocked evidence.

## Test Suite Summary

### 1. Domain Baseline Tests (`tests/domain_baseline_test.rs`) — 95 tests

Covers:
- QueryPlanInput defaults, serde aliases, and admission
- NormalizedQueryPlan derivation and invariants
- RuntimeConfig DTOs, defaults, and serialization
- Policy narrowing (paid, robots, login, paywalled)
- Redaction helpers (bearer tokens, API keys, private keys, metadata)
- AttemptCounterState invariants
- AdmissionDiagnostic constructors
- AdmissionOutcome behavior
- Config enums serde round-trip
- Error family constructors

### 2. Search Integration Tests (`tests/search_integration_test.rs`) — 20 tests

Covers:
- Candidate target derivation (20N)
- Weighted scheduling with multiple providers
- Provider readiness (enabled, disabled, misconfigured/zero weight)
- Candidate shortage (non-blocking)
- Source traceability (provider id, search round, rank)
- Credential safety (no secrets in search evidence)
- Cross-provider deduplication
- Multi-batch exhaustion
- Provider registry with mixed readiness
- SerpApi fixture normalization and provenance
- Dedupe key construction and normalization

### 3. Candidate Quality Tests (`tests/candidate_quality_test.rs`) — 58 tests

Covers:
- Candidate mechanical validation (missing URL, invalid scheme, ownership mismatch, duplicate, prohibited source, negative scope)
- Reference metrics (dimensions, license, source page)
- Fixture-not-production blocking
- Image mechanical validation (missing local/source artifact, sidecar, summary, task report, visual description, checksum, media mismatch, metadata-only, ownership mismatch, incomplete retrieval, fixture-in-production)
- VLM evaluation port (readiness, credential missing, fixture-not-production, unavailable, error modes)
- VLM response cardinality validation
- Candidate quality decision merging (mechanical pass + VLM approve/reject/uncertain/unexecutable)
- RetrievableCandidateBatch construction
- ImageAcceptanceDecisionV11 merging (mechanical + VLM)
- ImageAcceptanceOutcome separation
- Quality trace links
- Redaction and security

### 4. Retrieval Tests (`tests/retrieval_test.rs`) — 33 tests

Covers:
- Batch planner (target derivation, short batch detection, empty batch)
- RetrievalJob construction and ownership preservation
- Artifact completeness (all required paths, integrity fields)
- Metadata-only rejection
- Channel fallback (network failure → self-hosted)
- Access-restricted stops fallback
- Paid channel disabled/skipped behavior
- Channel readiness (ready, disabled, fixture-blocked, paid-unconfirmed)
- Tier serialization (canonical names + backward-compat aliases)
- RetrievalFailureCode display
- RetrievalBatchShortage construction
- RetrievalBatch target/actual size
- RetrievalError to failure code mapping and fallback eligibility
- All channel types satisfy BaseRetrievalChannel trait

### 5. E2E Fixture Tests (`tests/e2e_fixture_test.rs`) — 48 tests

Covers:
- Input rejection (empty description, whitespace, retry limit exceeded)
- No delivery package produced for input rejection
- Full delivery complete pipeline (search → mechanical → candidate eval → retrievable sequence → image acceptance → delivery package)
- Limited delivery with 0 accepted images across all retries
- Execution blocked by OpenClaw/VLM unavailability
- Channel fallback and access restriction boundaries
- Sensitive info exclusion in delivery output
- Metadata sanitization and credential pattern detection
- Self-check (input rejected, OpenClaw unavailable, paid unconfirmed, no channels, missing credentials)
- Provider integration with fixture providers
- Retrieval channel mixed results
- Delivery package full structure and manifest verification
- Authorization risk boundaries
- Attempt counters (1 initial + 3 retries, zero retry limit)
- Orchestrator state transitions and terminal states
- Fixture providers explicitly non-production
- Search outcome source traceability

### 6. v1.1 Fixture Validation Tests (`tests/fixture_v1_1_test.rs`) — 30 tests

Covers:
- QueryPlan fixture file loading and admission
- Config TOML fixture loading (fixture-mode, minimal, production-like)
- Production-like config has env var names only, no secrets
- Package validation: positive fixture (all canonical files, valid JSON, manifest links, coverage consistency)
- Package validation: negative fixtures (missing file, invalid JSON, metadata-only, checksum missing, coverage mismatch, retry counter invalid, broken link, secret leak)
- Secret scanning across all fixture files
- Secret leak fixture detection
- Golden output field verification
- Cross-fixture consistency (all negative packages have validation.json)
- No fixture marked as production acceptance
- SerpApi fixture response normalization contract
- Config readiness contract

## PRD AC-to-Test Matrix

| AC | Requirement | Status | Test Evidence |
| --- | --- | --- | --- |
| AC-001 | `run` executes full workflow (admission → search → quality → retrieval → acceptance → package) | **PASSED** | `domain_baseline_test.rs`: admission and NormalizedQueryPlan tests; `e2e_fixture_test.rs`: `e2e_full_delivery_complete_pipeline`, `e2e_full_delivery_single_image_immediate`; `fixture_v1_1_test.rs`: `fixture_package_passed_minimal_has_all_canonical_files` |
| AC-002 | Defaults: count=1, quality=general, invalid QueryPlan rejected | **PASSED** | `domain_baseline_test.rs`: `default_required_image_count_is_1`, `default_quality_is_general`, `admit_missing_description_rejected`, `admit_retry_limit_exceeds_max_rejected`; `e2e_fixture_test.rs`: `e2e_input_rejected_missing_description` |
| AC-003 | Candidate target = 20N | **PASSED** | `domain_baseline_test.rs`: `candidate_target_is_20n`; `search_integration_test.rs`: `search_target_for_3_images_is_60`, `candidate_target_is_20n` |
| AC-004 | Missing provider credential → unavailable readiness with machine-readable reason | **PASSED** | `search_integration_test.rs`: `serpapi_credential_missing_readiness`, `serpapi_readiness_no_credential_leak`; `e2e_fixture_test.rs`: `e2e_self_check_provider_missing_credentials_blocked` |
| AC-005 | Weighted scheduling preserves provider, round, rank, provenance | **PASSED** | `search_integration_test.rs`: `multi_provider_weighted_scheduling`, `candidate_source_traceability`, `cross_provider_dedup` |
| AC-006 | Candidates reach retrieval only after mechanical + VLM pass; VLM unavailable blocks production | **PASSED** | `candidate_quality_test.rs`: `mechanical_pass_plus_vlm_approve_equals_retrievable`, `mechanical_pass_plus_no_vlm_equals_execution_blocked`, `vlm_unavailable_produces_execution_block`, `fixture_evaluator_blocked_in_production` |
| AC-007 | Retrieval batch target = 2N | **PASSED** | `domain_baseline_test.rs`: `retrieval_batch_target_is_2n`; `retrieval_test.rs`: `batch_target_for_1_is_2`, `batch_target_for_4_is_8` |
| AC-008 | Retrieval success includes local artifact, sidecar, summary, task report, visual description, checksum, trace | **PASSED** | `retrieval_test.rs`: `complete_result_has_all_fields`; `candidate_quality_test.rs`: `complete_result_passes_mechanical_with_reference_metrics`; `fixture_v1_1_test.rs`: `fixture_package_passed_minimal_manifest_links_are_consistent` |
| AC-009 | Fallback: direct fetch → source-page → self-hosted → paid; paid disabled unless explicit | **PASSED** | `retrieval_test.rs`: `fallback_to_self_hosted_on_network_failure`, `access_restricted_stops_fallback`, `paid_channel_skipped_when_not_allowed`, `paid_channel_readiness_unconfirmed_by_default`; `e2e_fixture_test.rs`: `e2e_channel_fallback_blocked_by_access_restriction`, `e2e_channel_paid_unconfirmed_blocked` |
| AC-010 | Delivered images require mechanical + VLM image acceptance | **PASSED** | `candidate_quality_test.rs`: `image_vlm_approve_plus_mechanical_pass_equals_delivered_qualified`, `mechanical_block_prevents_delivery_even_with_vlm_approve`, `image_missing_local_artifact_is_blocked`, `image_missing_checksum_is_blocked` |
| AC-011 | 1 initial + max 3 retries; counters distinct | **PASSED** | `domain_baseline_test.rs`: `attempt_counter_initial_state`, `attempt_counter_advance`, `attempt_counter_exhausted_after_limit`, `attempt_counter_retry_count_equals_full_attempt_minus_one`; `e2e_fixture_test.rs`: `e2e_attempt_counter_one_initial_plus_three_retries` |
| AC-012 | Package root contains canonical files, manifest links | **PASSED** | `fixture_v1_1_test.rs`: `fixture_package_passed_minimal_has_all_canonical_files`, `fixture_package_passed_minimal_all_json_valid`, `fixture_package_passed_minimal_manifest_links_are_consistent`; `e2e_fixture_test.rs`: `e2e_delivery_package_full_structure` |
| AC-013 | Metadata-only, missing artifact, missing sidecar/summary/report cause validation fail | **PASSED** | `candidate_quality_test.rs`: `image_metadata_only_is_blocked`, all missing artifact tests; `fixture_v1_1_test.rs`: `fixture_package_metadata_only_delivered_detected`, `fixture_package_checksum_missing_detected`, `fixture_package_broken_manifest_link_detected` |
| AC-014 | `self-check` reports search, retrieval, VLM, policy, output, credential readiness | **PASSED** | `domain_baseline_test.rs`: `readiness_report_all_ready`, `readiness_report_with_blockers`; `e2e_fixture_test.rs`: `e2e_self_check_input_rejected_produces_blocked`, `e2e_self_check_openclaw_unavailable_produces_blocked`, `e2e_self_check_paid_channel_unconfirmed_blocked`, `e2e_self_check_does_not_produce_delivery_artifacts` |
| AC-015 | `validate-package` reads existing package and returns pass/fail + issue list | **PASSED** | `fixture_v1_1_test.rs`: `fixture_package_missing_canonical_file_has_issue`, `fixture_package_coverage_count_mismatch_detected`, `fixture_package_retry_counter_invalid_detected`, `fixture_package_secret_leak_detected`, `golden_validate_package_passed_minimal_matches_actual` |
| AC-016 | Package, logs, diagnostics contain no credentials/tokens/cookies | **PASSED** | `domain_baseline_test.rs`: `redact_bearer_token`, `redact_api_key`, `redact_authorization_header`, `redact_pem_private_key`, `admission_redacts_bearer_token_in_description`; `e2e_fixture_test.rs`: `e2e_sensitive_info_not_in_delivery_output`, `e2e_sanitize_removes_credentials_from_log_text`; `fixture_v1_1_test.rs`: `no_fixture_package_file_contains_real_credentials`, `secret_leak_fixture_contains_seeded_secret` |
| AC-017 | robots/site-rule, paid, authorization decisions are warnings/blockers, never silent | **PASSED** | `domain_baseline_test.rs`: `policy_cannot_silently_broaden`; `e2e_fixture_test.rs`: `e2e_authorization_prohibited_local_reject`, `e2e_channel_paid_unconfirmed_blocked`, `e2e_channel_disabled_no_fallback_bypass`; `retrieval_test.rs`: `paid_channel_readiness_unconfirmed_by_default` |
| AC-018 | Provider/channel/validation/evaluation/package boundaries independently testable | **PASSED** | All 6 test suites compile and run independently: `domain_baseline_test`, `search_integration_test`, `candidate_quality_test`, `retrieval_test`, `e2e_fixture_test`, `fixture_v1_1_test`. Each suite covers exactly one boundary cluster. |
| AC-019 | External unavailability → machine-readable failure_code, gap, retry evidence | **PASSED** | `search_integration_test.rs`: `empty_registry_returns_no_available_provider`, `all_providers_not_ready_returns_no_available`; `candidate_quality_test.rs`: `vlm_unavailable_produces_execution_block`, `vlm_evaluation_error_covers_required_modes`; `e2e_fixture_test.rs`: `e2e_execution_blocked_openclaw_unavailable`, `e2e_execution_blocked_by_retrieval_blocking_fact` |

## Fixture E2E Scenario Coverage

The TASK-006 acceptance criteria require proof of the following fixture E2E paths:

| Scenario | Status | Test |
| --- | --- | --- |
| `passed` package | **COVERED** | `fixture_v1_1_test.rs`: `fixture_package_passed_minimal_*` (5 tests); `e2e_fixture_test.rs`: `e2e_full_delivery_complete_pipeline` |
| `partial` package (accepted images < required) | **COVERED** | `e2e_fixture_test.rs`: `e2e_limited_delivery_zero_images_all_rejected` (4 attempts, 0 accepted) |
| `blocked` — no provider | **COVERED** | `search_integration_test.rs`: `empty_registry_returns_no_available_provider`; `fixture_v1_1_test.rs`: `golden_self_check_blocked_no_providers_has_blockers` |
| `blocked` — VLM unavailable | **COVERED** | `e2e_fixture_test.rs`: `e2e_execution_blocked_openclaw_unavailable` |
| Missing artifact (retrieval missing evidence) | **COVERED** | `candidate_quality_test.rs`: all `image_missing_*_is_blocked` tests (9 tests) |
| Metadata-only rejection | **COVERED** | `fixture_v1_1_test.rs`: `fixture_package_metadata_only_delivered_detected`; `retrieval_test.rs`: `metadata_only_result_is_not_complete` |
| Paid-unconfirmed | **COVERED** | `e2e_fixture_test.rs`: `e2e_channel_paid_unconfirmed_blocked`; `retrieval_test.rs`: `paid_channel_readiness_unconfirmed_by_default` |
| Access-restricted | **COVERED** | `retrieval_test.rs`: `access_restricted_stops_fallback`; `e2e_fixture_test.rs`: `e2e_channel_fallback_blocked_by_access_restriction` |
| Secret leak | **COVERED** | `fixture_v1_1_test.rs`: `fixture_package_secret_leak_detected`, `secret_leak_fixture_contains_seeded_secret`; `e2e_fixture_test.rs`: `e2e_sensitive_info_not_in_delivery_output` |
| Production-fixture rejection | **COVERED** | `candidate_quality_test.rs`: `fixture_candidate_in_production_is_blocked`, `fixture_retrieval_in_production_is_blocked`, `fixture_evaluator_blocked_in_production`; `fixture_v1_1_test.rs`: `no_fixture_marked_as_production_acceptance` |

## Security / Redaction Summary

- **Secret leak scan across all fixture files**: PASS (except intentionally seeded secret-leak fixture)
- **Credential pattern detection**: Tests cover Bearer tokens, API keys, Authorization headers, PEM private keys, signed URLs, and cookies
- **Redaction in admission**: Sensitive input in QueryPlan description is redacted with warning
- **Redaction in metadata**: Keys and values containing credential patterns are redacted
- **Redaction in VLM responses**: `redaction_applied` flag tracked
- **Production-like config**: Contains only env var names, no resolved secret values

## Known Residual Risks

| ID | Risk | Severity | Handling |
| --- | --- | --- | --- |
| R-001 | Real-service smoke blocked — no SerpApi credentials, Qwen endpoint, or production retrieval config available | HIGH | Documented as blocked in this report and `real-service-smoke-report.json`. Cannot close v1.1 full expected-target completion. |
| R-002 | All release gates (GATE-RSV-001 through GATE-MVP-005) remain OPEN | HIGH | TASK-007 must record each gate as blocking or waived. |
| R-003 | Paid retrieval, robots/site-rule, authorization blocking rules undecided | MEDIUM | Current behavior is fail-closed (disabled, warned, or blocked). Test coverage proves this. |
| R-004 | Quality threshold calibration open | LOW | General/High/Strict tiers defined but uncalibrated. Tests use tier-agnostic mechanical checks. |

## Final Verdict

**Local deterministic tests: ACCEPTED** — All 284 tests pass across 6 test suites. Every PRD AC-001 through AC-019 has mapped test evidence with passed status.

**Real-service smoke: BLOCKED** — `IMAGE_RETRIEVAL_REAL_SMOKE` env var is not set. No SerpApi credentials (`SERPAPI_API_KEY`), Qwen 3.5 VLM endpoint/token (`QWEN_API_TOKEN`), or production retrieval channel configuration is available. Smoke evidence is diagnostic only and cannot satisfy the expected-target completion claim.

**Overall TASK-006 status: ACCEPTED with real-service smoke blocked** — The task has delivered all local deterministic test coverage, fixture E2E evidence, package validation tests, and security scans. Real-service smoke prerequisites remain absent; the blocked evidence is properly recorded for TASK-007 final acceptance to evaluate.
