# v1.1 Baseline Recall Test Plan

Date: 2026-06-24

Status: proposed baseline test design

## Goal

The baseline suite measures whether the CLI can stably retrieve high-quality
images according to a `QueryPlan`.

The suite is not only a pass/fail regression harness. It is also a capability
map. Each run should explain where recall is lost:

1. candidate recall,
2. candidate retrieval,
3. image acceptance and delivery.

The first baseline should favor small, inspectable cases over broad coverage.
It should expose weak links before optimization work starts.

## Completion Criteria

The test plan is complete only when all three capability units define:

- unit inputs and deterministic expectations;
- real-service scenario inputs and threshold expectations;
- report fields for stage-level failure attribution;
- analysis rules that map failures to next engineering work.

The implementation is complete only when:

- unit tests run without external services;
- scenario tests run the real CLI with real configured search providers, real
  retrieval channels, and the real VLM evaluator;
- scenario tests do not use mocked providers, fake channels, fixture evaluators,
  fixture provider responses, or synthetic delivery packages;
- scenario tests emit baseline reports under `target/baseline-reports/`;
- real-service scenario tests are gated by `IMAGE_RETRIEVAL_REAL_BASELINE=1`.

## Real Scenario Input Policy

Scenario tests may use controlled baseline resources only when they are real
externally reachable resources:

- real HTTP(S) image URLs;
- real HTTP(S) source pages with metadata such as `og:image`;
- real HTTP status responses for access restrictions;
- real downloadable corrupt or invalid files when the case requires it.

They must not use:

- in-process mock servers;
- fake search providers;
- fake retrieval channels;
- fixture VLM evaluators;
- prebuilt synthetic delivery packages;
- local-only fixture files as substitutes for retrieval.

This keeps scenario tests reproducible while still exercising the actual CLI,
network stack, retrieval channels, VLM evaluator, package builder, and package
validator.

## Test Units

The suite has three product capability units.

### 1. Candidate Recall

Scope:

- `QueryPlan` normalization and search request formation.
- Search provider parsing and dedupe.
- Candidate mechanical validation.
- Candidate subjective scoring/evaluation.
- Candidate ranking and retrievable handoff.

Primary question:

Can the tool recall the candidates it should recall, reject the obvious bad
candidates, and assign scores/ranking that match the expected relevance?

Inputs:

- A `QueryPlan`.
- Unit tests may use provider response fixtures.
- Scenario tests must use configured real search providers.
- A gold label set for candidate ids:
  - `must_recall`
  - `acceptable`
  - `must_reject`
  - `duplicate_of`
  - `low_quality`

Metrics:

- `unique_candidate_count`
- `expected_candidate_recall`
- `precision_at_5`
- `precision_at_10`
- `duplicate_rate`
- `false_accept_count`
- `false_reject_count`
- `score_mae` for unit fixture/scored evaluators
- `ranking_ndcg_at_10`

Unit pass condition:

- All `must_recall` candidates appear after provider parsing and dedupe.
- All `must_reject` candidates are rejected by mechanical or subjective
  evaluation.
- Ranking and score error stay within the case threshold.

Scenario pass condition:

- The CLI invokes real configured search providers and the real candidate
  evaluator.
- The scenario produces enough retrievable candidates for downstream retrieval,
  or attributes any shortage to a known reason.
- The scenario emits the standard baseline report shape.

### 2. Candidate Retrieval

Scope:

- `RetrievableCandidate` to `RetrievalBatch` planning.
- Channel ordering and fallback.
- Direct image fetch.
- Source-page fallback.
- Retrieval artifact writing and sidecar/report traces.
- Failure classification.

Primary question:

Can the configured channel combination retrieve the target artifacts under
different difficulty conditions?

Inputs:

- A set of retrievable candidates.
- A channel configuration.
- Unit tests may use local fixture inputs for deterministic classifiers.
- Scenario tests must use real channel execution against actual URLs/services.
- Expected retrieval outcomes per candidate.

Difficulty classes:

- `direct_ok`: primary image URL returns a valid image.
- `source_page_required`: direct fetch fails, source page exposes `og:image`.
- `hotlink_blocked`: direct fetch returns 403 or anti-hotlink response.
- `html_disguised_as_image`: URL returns HTML or wrong content type.
- `metadata_only`: channel returns metadata without a local image artifact.
- `timeout_or_slow`: request exceeds timeout.
- `corrupted_or_zero_byte`: downloaded bytes are invalid.
- `policy_blocked`: prohibited source or robots policy blocks retrieval.

Metrics:

- `retrieval_success_rate`
- `artifact_complete_count`
- `fallback_attempt_count`
- `fallback_success_count`
- `policy_blocked_count`
- `metadata_only_rejected_count`
- `failure_classification_accuracy`
- `mean_attempt_duration_ms`

Unit pass condition:

- Each retrieval result is classified with the expected status and failure code.
- Complete artifacts have local image, sidecar, summary, and task report refs.

Scenario pass condition:

- Channel fallback only processes pending jobs.
- Policy-blocked results do not proceed to higher fallback tiers.
- The scenario validates the real channel chain through actual execution.
- The scenario produces real local artifacts, or a partial result with accurate
  failure attribution.

### 3. Delivery

Scope:

- Retrieved image mechanical acceptance.
- Image subjective evaluation.
- Accepted image selection.
- Delivery package building.
- Package validation.
- Partial delivery behavior.

Primary question:

Given retrieved results, can the tool deliver all and only the images that
correctly satisfy the `QueryPlan`?

Inputs:

- A `QueryPlan`.
- Unit tests may use local artifact fixtures.
- Scenario tests must use retrieved results produced by a real scenario run.
- Gold labels for expected delivery:
  - `must_deliver`
  - `acceptable_delivery`
  - `must_not_deliver`
  - `duplicate_of`
  - `quality_blocked`

Metrics:

- `accepted_count`
- `delivery_recall`
- `delivery_precision`
- `wrong_image_delivered_count`
- `missing_expected_image_count`
- `package_validation_passed`
- `partial_delivery_expected`

Unit pass condition:

- Image mechanical and subjective decisions match gold labels.

Scenario pass condition:

- The orchestrator records accepted images and coverage gaps correctly.
- The scenario validates the package produced by a real CLI run and reports
  delivery quality thresholds rather than exact public web ids.

## Test Types

The same baseline cases can be consumed by two test types.

### Unit Tests

Purpose:

- Validate deterministic rules and scorer outputs.

Examples:

- Provider response parsing returns the expected candidates.
- Candidate mechanical validation rejects duplicate, invalid, prohibited, or
  negative-scope candidates.
- Retrieval result classification matches the expected failure code.
- Image mechanical validation rejects metadata-only, wrong content type,
  corrupted, or too-small images.

Execution:

```bash
cargo test --test baseline_candidate_recall_unit
cargo test --test baseline_candidate_retrieval_unit
cargo test --test baseline_delivery_unit
```

### Scenario Tests

Purpose:

- Validate the capability chain. This combines the previous integration and
  end-to-end layers so the first baseline stays simple.
- Scenario tests are real execution tests. They must not mock, fake, or fixture
  search providers, retrieval channels, VLM evaluators, or delivery packages.

Examples:

- `QueryPlan -> real search provider -> scheduler -> candidate gate`.
- `RetrievableCandidateBatch -> retrieval planner -> real retrieval channel`.
- `Retrieved images -> image gate -> orchestrator -> package validator`.
- CLI runs that produce a delivery package and baseline report.

Modes:

- `scenario_real_service`: opt-in mode, uses live search, retrieval, and VLM
  services.

Scenario tests must be opt-in because they use network services,
credentials, cost, and live provider state.

Execution:

```bash
IMAGE_RETRIEVAL_REAL_BASELINE=1 cargo test --test baseline_real_service_test
```

Required real-service environment variables:

- `SERPAPI_API_KEY` or provider-specific search key.
- `QWEN_API_KEY` or configured VLM credential env.

Optional runner environment variables:

- `IMAGE_RETRIEVAL_BASELINE_CONFIG`: real-service runtime config path. Defaults
  to `tests/fixtures/v1_1/configs/config-production-like.toml`.
- `IMAGE_RETRIEVAL_BASELINE_CASES`: comma-separated case ids to run. Omit to
  run all catalog cases.
- `IMAGE_RETRIEVAL_BASELINE_REPORT_DIR`: report output directory. Defaults to
  `target/baseline-reports`.

Scenario tests must never assert exact candidate ids from the public web.
They must assert stable CLI/package thresholds: command exit code, delivery
status, accepted image count relative to the QueryPlan count, and package
validation status. Metrics that depend on deterministic fixture labels belong
to unit thresholds, not live public-web scenario thresholds.

Real-service scenario tests require all configured search providers, retrieval
channels, and Qwen evaluators to be real. They do not require unconfigured
self-hosted or paid fallback services to succeed. Self-hosted/paid fallback
success is covered by deterministic unit fixtures until a service-specific
adapter and credentials are explicitly configured for a real-service suite.

## Case Design

Each case should declare:

- `case_id`
- `unit`
- `supported_test_types`
- `query_plan`
- `difficulty_profile`
- `fixtures`
- `gold_labels`
- `expected_metrics.unit`
- `expected_metrics.scenario_real_service`
- `analysis_tags`
- `execution_policy`

The case catalog lives at:

```text
tests/fixtures/v1_1/baseline/case-catalog.json
```

The first baseline should include five cases per unit:

- 5 candidate recall cases
- 5 candidate retrieval cases
- 5 delivery cases

That gives enough coverage to classify failures without creating a large
maintenance burden.

## Result Report

Every baseline run should emit a machine-readable report:

```text
target/baseline-reports/<timestamp>-baseline-v1.1.json
```

Recommended report shape:

```json
{
  "schema_version": 1,
  "suite_id": "baseline_recall_v1_1",
  "run_id": "baseline-...",
  "git_commit": "...",
  "execution_mode": "real_service",
  "cases": [],
  "stage_summary": {},
  "regressions": [],
  "recommended_next_work": []
}
```

Each case result should include:

- `case_id`
- `status`: `pass`, `fail`, `regression`, `investigate`, or `skipped`
- `unit`
- `test_type`
- `metrics`
- `thresholds`
- `failure_stage`
- `failure_reason_code`
- `evidence_refs`

## Result Interpretation

Use a stage waterfall to classify each failure:

1. Search did not produce enough unique candidates.
2. Candidate evaluator rejected good candidates.
3. Retrieval could not create complete artifacts.
4. Image evaluator rejected good images or accepted bad images.
5. Package validation failed after acceptance.

Recommended interpretation rules:

- High `expected_candidate_recall` but low `delivery_recall` means retrieval or
  delivery is the bottleneck.
- Low `expected_candidate_recall` with healthy provider readiness means query
  propagation, search strategy, dedupe, or candidate scoring is the bottleneck.
- High duplicate rate means provider diversity, dedupe keys, pagination, or
  retry feedback need work.
- Good unit tests but poor scenario tests mean live provider/query strategy,
  network retrieval, VLM stability, or CLI orchestration is the bottleneck.
- High false accept rate in delivery is more severe than low recall because it
  risks delivering wrong images.

## Baseline Thresholds

Initial thresholds should be intentionally conservative. They are meant to map
capability, not to block all development.

Suggested unit thresholds:

- Candidate recall:
  - `expected_candidate_recall >= 0.90`
  - `precision_at_10 >= 0.80`
  - `duplicate_rate <= 0.30`
- Candidate retrieval:
  - `failure_classification_accuracy >= 0.95`
  - `artifact_complete_count == expected_complete_count`
  - `metadata_only_rejected_count == expected_metadata_only_count`
- Delivery:
  - `delivery_precision == 1.00`
  - `package_validation_passed == true` when full delivery is expected
  - `wrong_image_delivered_count == 0`

Suggested scenario thresholds:

- `delivered_count >= required_count` for easy cases.
- `delivered_count > 0` and accurate shortage reasons for hard cases.
- No package validation failures for accepted images.
- No leaked credentials in package or report.

## Analysis Workflow

For each baseline run:

1. Sort failing cases by severity:
   - wrong image delivered
   - package validation failed
   - expected good image missed
   - retrieval failure misclassified
   - low recall or high duplicate rate
2. Group failures by stage.
3. Compare against the previous accepted baseline.
4. Write a short conclusion:
   - current best capability,
   - weakest stage,
   - next engineering task,
   - whether the change is a regression.

## Optimization Backlog Derived From Baseline

The baseline suite should drive these work items:

1. Preserve full `NormalizedQueryPlan` through the production pipeline.
2. Add retry feedback so rejected, failed, and already-attempted candidates are
   not repeatedly processed.
3. Add query rotation, query rewrite, and provider pagination.
4. Apply `QualityRequirements` to candidate and image mechanical gates.
5. Implement real tier 2 and tier 3 retrieval adapters.
6. Add VLM batching, partial failure isolation, retry/backoff, and structured
   score parsing.
7. Add real-service trend reports for recall, precision, latency, and cost.
