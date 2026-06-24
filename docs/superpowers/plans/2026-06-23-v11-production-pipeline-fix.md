# v1.1 Production Pipeline Fix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the v1.1 CLI production path match the PRD/HLD/LLD contract for retries, config readiness, candidate VLM gating, retrieval evidence, package validation, and checksums.

**Architecture:** Keep the existing module boundaries. Repair the newest production pipeline so it propagates existing v1.1 domain evidence instead of converting through lossy legacy DTOs, and make CLI commands consume runtime config consistently.

**Tech Stack:** Rust, Cargo tests, `ureq`, `serde_json`, `toml`, and `sha2` for real SHA-256 checksums.

---

### Task 1: Regression Tests

**Files:**
- Modify: `tests/e2e_fixture_test.rs`
- Modify: `tests/candidate_quality_test.rs`
- Modify: `tests/retrieval_test.rs`
- Modify: `tests/fixture_v1_1_test.rs`

- [ ] Add tests that prove CLI/RunOrchestrator exhausts retry counters when the target is not met.
- [ ] Add tests that prove Qwen candidate evaluation fails closed when credentials are absent.
- [ ] Add tests that prove web fetch checksum is real SHA-256.
- [ ] Add tests that prove package validation rejects absolute or escaping artifact paths.
- [ ] Run targeted tests and confirm they fail for the current implementation.

### Task 2: CLI Run Attempt Loop

**Files:**
- Modify: `src/main.rs`
- Modify: `src/pipeline.rs`
- Modify: `src/orchestrator/mod.rs`

- [ ] Replace the single attempt in `cmd_run` with a loop over initial attempt plus retries.
- [ ] Populate `RunAttemptRecord` counters from `ProductionAttemptSummary`.
- [ ] Keep fixture mode honest: no fixture evidence is delivered as production, and unmet targets exhaust retry counters before blocked/partial.
- [ ] Run targeted e2e tests.

### Task 3: Config-Backed Self Check

**Files:**
- Modify: `src/main.rs`

- [ ] Load `RuntimeConfig` for `self-check --config`.
- [ ] Build provider, retrieval channel, VLM, and policy readiness from config rather than hardcoded SerpApi/Qwen defaults.
- [ ] Keep missing config behavior explicit and machine readable.
- [ ] Run self-check tests and the fixture self-check CLI command.

### Task 4: Candidate VLM Gating

**Files:**
- Modify: `src/quality/qwen_vlm.rs`
- Modify: `src/pipeline.rs`

- [ ] Make `QwenVlmEvaluator::evaluate_candidates` call the configured VLM endpoint when candidates pass mechanical checks.
- [ ] Treat missing credentials, transport failures, parse failures, and non-cardinality responses as execution blocked.
- [ ] Stop fabricating `vlm_passed: true` with no decision in the production retrievable batch conversion.
- [ ] Run candidate quality tests.

### Task 5: Retrieval Evidence Propagation

**Files:**
- Modify: `src/pipeline.rs`
- Modify: `src/delivery/mod.rs`

- [ ] Build accepted delivered records from the matching `RetrievalArtifactResult`.
- [ ] Preserve retrieval job id, sidecar, summary, task report, visual description, checksum, content type, dimensions, and decision refs.
- [ ] Copy accepted artifacts and evidence into package subdirectories and rewrite delivered paths to package-relative paths.
- [ ] Run validation and fixture tests.

### Task 6: Retrieval Channel Configuration

**Files:**
- Modify: `src/pipeline.rs`

- [ ] Build enabled retrieval channels from `RuntimeConfig.retrieval_channels`.
- [ ] Execute channels by fallback tier, preserving readiness/attempt evidence.
- [ ] Keep self-hosted and paid as explicit boundary channels until their services are configured.
- [ ] Run retrieval tests.

### Task 7: Integrity and Validator Hardening

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/retrieval/channels/web_fetch.rs`
- Modify: `src/validation/mod.rs`

- [ ] Replace `DefaultHasher` pseudo-checksum with real SHA-256.
- [ ] Reject absolute artifact paths and paths that escape the package root.
- [ ] Run package validation tests.

### Task 8: Final Verification

**Files:**
- All touched files

- [ ] Run `cargo fmt --all -- --check`.
- [ ] Run `cargo clippy --all-targets --all-features -- -D warnings`.
- [ ] Run `cargo test --all`.
- [ ] Run CLI fixture self-check/run/validate-package smoke commands.
