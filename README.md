# image-retrieval

Rust CLI tool for general-purpose image search, retrieval, validation, and
delivery packaging.

See [AGENTS.md](AGENTS.md) for the project constitution. All future work in this
repository must follow it. Claude-specific agent guidance lives in
[CLAUDE.md](CLAUDE.md).

## Development

### Prerequisites

- Rust toolchain (edition 2021)
- `cargo`, `rustfmt`, `clippy`

### Build

```bash
cargo build
```

### Verification Commands

```bash
# Format check
cargo fmt --all -- --check

# Lint (warnings as errors)
cargo clippy --all-targets --all-features -- -D warnings

# Run all tests (unit + integration + E2E fixtures)
cargo test --all
```

### Test Categories

| Test file / module | Coverage |
|---|---|
| Unit tests (`src/**/*.rs` with `#[cfg(test)]`) | Domain types, ports, quality gates, orchestrator, policy, delivery, search, retrieval, self-check |
| `tests/domain_baseline_test.rs` | Cross-module domain integration |
| `tests/candidate_quality_test.rs` | Candidate quality gate regression |
| `tests/retrieval_test.rs` | Retrieval channel contracts |
| `tests/search_integration_test.rs` | Search scheduler integration |
| `tests/e2e_fixture_test.rs` | **E2E fixture validation** — full pipeline using only internal fixtures |

### E2E Fixture Tests

The E2E fixture tests (`tests/e2e_fixture_test.rs`) validate the complete
pipeline using internal fixture providers, channels, and evaluators. They
cover:

- `input_rejected` — QueryPlan rejection before any execution
- `full_delivery` — Complete pipeline from search through delivery
- `limited_delivery` (0 images) — All images rejected across all retries
- `execution_blocked` — OpenClaw unavailability produces correct blocking
- Channel fallback disabled / access restriction boundaries
- Sensitive information exclusion from delivery packages
- Self-check readiness reporting (non-delivery)
- Authorization risk boundaries
- Attempt counters and retry logic

**Fixtures are for internal testing only.** They do not use real credentials,
network access, or production OpenClaw. Fixture results are never production
delivery evidence.

### Release Gates

See [RELEASE_GATES.md](RELEASE_GATES.md) for the list of open decisions that
must be resolved before real service verification and MVP release. Key gates
cover:

- Default real image search provider selection
- Built-in provider list and restricted/legacy provider policy
- OpenClaw production evaluation usage and wire protocol
- Paid retrieval channel enablement boundary
- Authorization blocking detailed rules
- robots.txt / site-rule compliance strategy
- Fourth retrieval channel decision
- Quality tier calibration

All gates are currently **OPEN** and must be resolved by the user/product
owner. No gate is silently closed by the implementation.

### Architecture

```
src/
  domain/         Core domain types (QueryPlan, Candidate, Image, Delivery, etc.)
  error/          Error families and diagnostic models
  ports/          BaseProvider, BaseRetrievalChannel, OpenClawEvaluationPort traits
  search/         Provider registry, weighted scheduler, fixture provider
  retrieval/      Batch planner, web fetch channel, fixture channel
  quality/        Candidate and image quality gates (mechanical + OpenClaw)
  orchestrator/   Task state machine and attempt counters
  delivery/       Delivery package builder (status.json, manifest.json, summary.md)
  policy/         Policy evaluation, guardrails, sensitive data redaction
  observability/  Metric events and diagnostic recording
  self_check/     Pre-flight readiness reporter (no search/retrieval/delivery)
  main.rs         CLI entry point (run and self-check subcommands)
  lib.rs          Library root
```
