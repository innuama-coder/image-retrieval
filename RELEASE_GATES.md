# Release Gates — Real Service Verification & MVP Release

This document records product, engineering, and compliance release-gate
decisions for v1.1. All gate decisions are closed by
`tasks/development/v1.1/release-gate-decisions.md`; real-service evidence is
tracked separately and must be regenerated explicitly after implementation
changes.

## Gate Categories

| Gate | Applies To | Description |
|---|---|---|
| **Real Service Verification** | Before running live service tests | Decisions needed to configure and run real search, Qwen 3.5 VLM, and retrieval channels. |
| **MVP Release** | Before declaring MVP shippable | Decisions needed to ship a production-local CLI to users. |
| **Deferred** | Post-MVP | Open questions that can be resolved after MVP release. |

---

## Pre-Verification Gates (Real Service Verification)

These decisions are closed for v1.1 and define how real-service validation is
configured.

### GATE-RSV-001: Default Real Image Search Provider

- **Source**: PRD DEP-002, planning JSON external_dependencies
- **Status**: **CLOSED** — see `tasks/development/v1.1/release-gate-decisions.md`
- **Impact**: Real service verification scope; user configuration complexity.
- **What's needed**: User/product owner must select the first version default
  real image search service (e.g., Brave, Bing, custom).
- **Blocking**: TASK-011 (real service validation) cannot proceed without this.
- **Handling**: SerpApi Google Images is the v1.1 default real provider.
  The adapter implements the provider contract and uses `SERPAPI_API_KEY`.

### GATE-RSV-002: Built-in Provider List & Restricted/Legacy Provider Policy

- **Source**: CLAUDE.md constraints, planning JSON external_dependencies
- **Status**: **CLOSED** — see `tasks/development/v1.1/release-gate-decisions.md`
- **Impact**: Which providers are built-in; which are restricted or legacy;
  validation coverage matrix for TASK-011.
- **What's needed**: List of providers to bundle; policy for restricted/legacy
  providers (disable by default? require explicit opt-in?).
- **Blocking**: TASK-011 real service verification matrix.
- **Handling**: `ProviderRegistry` supports arbitrary registration; `BaseProvider`
  is pluggable. No provider list is hardcoded in production paths.

### GATE-RSV-003: Paid Retrieval Channel Enablement Boundary

- **Source**: PRD RISK-004, planning JSON external_dependencies
- **Status**: **CLOSED** — see `tasks/development/v1.1/release-gate-decisions.md`
- **Impact**: Cost control, real service verification scope, fallback strategy.
- **What's needed**: At minimum, confirm whether paid channels may be used
  during verification. Long-term: define enablement boundary for production.
- **Blocking**: TASK-011 paid channel verification.
- **Handling**: Paid channels default to disabled. `paid_unconfirmed` readiness
  state produces a blocker. No paid channel is silently used.

### GATE-RSV-004: robots.txt / Site-Rule Compliance Strategy

- **Source**: PRD OQ-003, planning JSON open_questions
- **Status**: **CLOSED** — see `tasks/development/v1.1/release-gate-decisions.md`
- **Impact**: Retrievable range, basic compliance posture, real service
  verification entry point.
- **What's needed**: Decision on whether to enable site rules or robots.txt
  risk hints by default.
- **Blocking**: Real service verification entry; TASK-011 compliance boundary.
- **Handling**: v1.1 records robots/site-rule risk as evidence and warnings;
  it does not silently enforce stricter blocking without configuration.

### GATE-RSV-005: Quality Tier Calibration or Waiver

- **Source**: PRD RISK-001
- **Status**: **CLOSED** — see `tasks/development/v1.1/release-gate-decisions.md`
- **Impact**: Limited delivery frequency; image usability.
- **What's needed**: Calibrate quality thresholds against real tasks, or
  explicitly waive calibration for MVP.
- **Blocking**: Real service validation quality assessment.
- **Handling**: Current General/High/Strict thresholds are accepted for MVP and
  can be recalibrated with post-release real task data.

---

## Pre-Release Gates (MVP Release)

These decisions are closed for v1.1 MVP scope.

### GATE-MVP-001: Qwen 3.5 VLM Production Evaluation Usage & Responsibility

- **Source**: PRD DEP-001, planning JSON external_dependencies
- **Status**: **CLOSED** — see `tasks/development/v1.1/release-gate-decisions.md`
- **Impact**: Candidate subjective evaluation and image subjective acceptance
  cannot enter production without this.
- **What's needed**: Define how production Qwen 3.5 VLM is used, what the
  responsibility boundary is, and what protocol/skills are required.
- **Blocking**: MVP cannot be released without production Qwen 3.5 VLM path.
- **Handling**: `OpenClawEvaluationPort` remains the internal trait name for
  compatibility, and the production implementation is the direct Qwen 3.5 VLM
  adapter. When Qwen 3.5 VLM is unavailable, production tasks enter
  `execution_blocked`; fixture/mock results cannot substitute.

### GATE-MVP-002: Built-in Provider List & Restricted/Legacy Provider Policy

- **Source**: CLAUDE.md constraints (门禁必须覆盖内置 provider 清单、受限/legacy provider 策略)
- **Status**: **CLOSED** — see `tasks/development/v1.1/release-gate-decisions.md`
- **Impact**: Same as GATE-RSV-002 but also affects MVP release scope.
- **What's needed**: Same decision as RSV-002, finalized before MVP.
- **Blocking**: MVP release scope definition.
- **Handling**: No built-in provider list is hardcoded. `ProviderRegistry` is
  empty by default; fixture provider is test-only.

### GATE-MVP-003: Authorization Blocking Detailed Rules

- **Source**: PRD RISK-002, planning JSON external_dependencies
- **Status**: **CLOSED** — see `tasks/development/v1.1/release-gate-decisions.md`
- **Impact**: Qualified image count; compliance posture; user risk exposure.
- **What's needed**: Define exact rules for authorization blocking: when does
  unknown authorization trigger a block vs. a warning? How are explicitly
  prohibited sources identified?
- **Blocking**: MVP compliance posture.
- **Handling**: Current default: unknown authorization -> allow with risk
  retention; explicitly prohibited -> local reject.

### GATE-MVP-004: Fourth Retrieval Channel Decision

- **Source**: PRD OQ-004, planning JSON external_dependencies
- **Status**: **CLOSED** — see `tasks/development/v1.1/release-gate-decisions.md`
- **Impact**: Retrieval channel product scope; fallback acceptance criteria;
  downstream technical design input.
- **What's needed**: Confirm whether a fourth tier exists, whether the current
  three-tier list is correct, or whether existing categories should be split.
- **Blocking**: MVP release scope for retrieval channels.
- **Handling**: Only three confirmed tiers are modeled (`web_fetch`, `self_hosted`,
  `paid`). No fourth tier is synthesized.

### GATE-MVP-005: Qwen 3.5 VLM Production Wire Protocol

- **Source**: PRD DEP-001, HLD §主观评价架构边界, planning JSON
- **Status**: **CLOSED** — see `tasks/development/v1.1/release-gate-decisions.md`
- **Impact**: Production evaluation cannot proceed; `execution_blocked` is the
  only legal outcome when Qwen 3.5 VLM is unreachable.
- **What's needed**: Define the production protocol, endpoint, and skill set
  for Qwen 3.5 VLM evaluation (both candidate-phase and image-phase).
- **Blocking**: MVP release.
- **Handling**: The production adapter is implemented. The self-check separately
  reports candidate and image Qwen 3.5 VLM readiness.

---

## Deferred Open Questions (Post-MVP)

### OQ-001: Maximum QueryPlan Delivery Count

- **Source**: PRD OQ-001
- **Status**: Deferred (MVP 发布后可延期)
- **Impact**: Cost, latency, candidate scale, limited delivery probability.
- **Handling**: MVP exposes large-count risk via self-check warnings and
  limited delivery explanations.

### OQ-002: Authorization Risk Grouping in Delivery Results

- **Source**: PRD OQ-002
- **Status**: Deferred (MVP 发布后可延期)
- **Impact**: User experience when consuming images with mixed authorization.
- **Handling**: MVP retains authorization risk hints per image. Grouped display
  is a post-MVP enhancement.

---

## Gate Status Summary

| Gate | Category | Status | Blocks |
|---|---|---|---|
| GATE-RSV-001 | Default real provider | CLOSED | Real service verification |
| GATE-RSV-002 | Built-in/restricted provider policy | CLOSED | Real service verification |
| GATE-RSV-003 | Paid channel enablement | CLOSED | Real service verification |
| GATE-RSV-004 | robots/site-rule strategy | CLOSED | Real service verification |
| GATE-RSV-005 | Quality tier calibration | CLOSED | Real service verification |
| GATE-MVP-001 | Qwen 3.5 VLM production usage | CLOSED | MVP release |
| GATE-MVP-002 | Provider list/policy (MVP) | CLOSED | MVP release |
| GATE-MVP-003 | Authorization blocking rules | CLOSED | MVP release |
| GATE-MVP-004 | Fourth retrieval channel | CLOSED | MVP release |
| GATE-MVP-005 | Qwen 3.5 VLM wire protocol | CLOSED | MVP release |
| OQ-001 | Max QueryPlan count | DEFERRED | — |
| OQ-002 | Auth risk grouping | DEFERRED | — |

---

## Verification Evidence

- `cargo fmt --all -- --check` — **PASS**
- `cargo clippy --all-targets --all-features -- -D warnings` — **PASS**
- `cargo test --all-targets` — **PASS** (`573` lib tests plus integration
  suites passing)
- Real-service smoke with `IMAGE_RETRIEVAL_REAL_SMOKE=1` and explicit
  `IMAGE_RETRIEVAL_SMOKE_REPORT_PATH` — **PASS**

Internal E2E fixture tests cover: `input_rejected`, `full_delivery`,
`limited_delivery` (0 images), `execution_blocked` (Qwen 3.5 VLM unavailable),
channel fallback disabled/access restriction boundaries, sensitive info
exclusion, self-check readiness, and authorization risk boundaries.

Deterministic tests do not require real credentials or network access.
Real-service smoke evidence must be generated explicitly with
`IMAGE_RETRIEVAL_REAL_SMOKE=1` and `IMAGE_RETRIEVAL_SMOKE_REPORT_PATH`; ordinary
tests must not rewrite release evidence.

---

## Real-Service Evidence

The accepted v1.1 smoke package is:

`/private/tmp/image-retrieval-real-run-v11-fix-20260624-final/package`

The generated `tasks/development/v1.1/real-service-smoke-report.json` records
`status=passed`, 10 closed gates, successful `self-check`, successful
production `run`, and successful `validate-package`.

Deferred OQs may be resolved post-MVP at product owner discretion.
