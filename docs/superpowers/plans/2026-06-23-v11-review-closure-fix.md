# v1.1 Review Closure Fix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the remaining v1.1 review gaps so package evidence, retry behavior, readiness, and provider/channel config handling match the design contract.

**Architecture:** Keep the existing Rust CLI boundaries. Package construction will derive auditable delivered-image retrieval and acceptance evidence from accepted image records, validation will enforce those links, self-check will follow the same defaults as runtime adapters, and production retry/accounting will avoid duplicate accepted candidates.

**Tech Stack:** Rust, Cargo integration tests, existing domain/config/delivery/validation modules.

---

### Task 1: Failing Regression Tests

**Files:**
- Modify: `tests/cli_v1_1_test.rs`
- Modify: `src/delivery/mod.rs` tests
- Modify: `src/validation/mod.rs` tests
- Modify: `src/pipeline.rs` tests
- Modify: `src/orchestrator/mod.rs` tests

- [x] Add a CLI test proving production-like self-check is ready when credentials are present and the Qwen endpoint is satisfied by runtime defaults.
- [x] Add package builder/validator tests proving delivered images require non-empty `retrieval_results`, `image_acceptance_decisions`, and valid manifest refs.
- [x] Add a retrieval-channel test proving sorted channels keep their own config/readiness identity.
- [x] Add an orchestrator test proving duplicate accepted candidates do not increase accepted count across retries.
- [x] Add a provider registry/pipeline test proving multiple same-kind SerpApi providers use their own config.

### Task 2: Package Evidence Contract

**Files:**
- Modify: `src/delivery/mod.rs`
- Modify: `src/validation/mod.rs`
- Modify: `tests/fixtures/v1_1/packages/passed_minimal/*`
- Modify: `tests/fixtures/v1_1/golden/validate-package-passed-minimal.json`

- [x] Generate delivered-image retrieval evidence in `retrieved-images.json#/retrieval_results`.
- [x] Generate delivered-image acceptance evidence in `retrieved-images.json#/image_acceptance_decisions`.
- [x] Generate candidate-quality evidence files under `evidence/candidate-quality/`.
- [x] Make accepted-image artifact localization fail when source evidence is missing.
- [x] Validate manifest refs and delivered-image decision refs.

### Task 3: Runtime Readiness and Config Binding

**Files:**
- Modify: `src/main.rs`
- Modify: `src/pipeline.rs`

- [x] Make self-check use runtime-compatible Qwen endpoint defaults.
- [x] Keep retrieval channel instances paired with their sorted config.
- [x] Skip execution for channels whose readiness is not available.
- [x] Build provider adapters from the exact provider config instead of looking up the first provider by kind.

### Task 4: Retry De-Duplication

**Files:**
- Modify: `src/orchestrator/mod.rs`
- Modify: `src/pipeline.rs`

- [x] Treat `candidate_id`, `retrieval_job_id`, and checksum as duplicate guards for accepted images.
- [x] Exclude already accepted candidate ids before candidate quality/retrieval in later attempts.
- [x] Preserve retry counters and gap reporting.

### Task 5: Verification

**Commands:**
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all`
- `env SERPAPI_API_KEY=dummy QWEN_API_TOKEN=dummy target/debug/image-retrieval self-check --config tests/fixtures/v1_1/configs/config-production-like.toml --query-plan tests/fixtures/v1_1/query-plans/query-plan-basic.json --format json`
- `target/debug/image-retrieval validate-package --package-dir tests/fixtures/v1_1/packages/passed_minimal --format json`
