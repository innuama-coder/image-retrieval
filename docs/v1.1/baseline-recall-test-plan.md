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
- One or more provider response fixtures, or an opt-in real provider run.
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
- `score_mae` for fixture/scored evaluators
- `ranking_ndcg_at_10`

Unit pass condition:

- All `must_recall` candidates appear after provider parsing and dedupe.
- All `must_reject` candidates are rejected by mechanical or subjective
  evaluation.
- Ranking and score error stay within the case threshold.

Integration pass condition:

- The search scheduler, provider fixture, candidate gate, and retrievable batch
  produce the expected candidate ids in priority order.

E2E pass condition:

- The CLI run produces enough retrievable candidates for downstream retrieval,
  and the baseline report attributes any shortage to a known reason.

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
- A local fixture server or fixture artifact set for retrieval behavior.
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

Integration pass condition:

- Channel fallback only processes pending jobs.
- Policy-blocked results do not proceed to higher fallback tiers.

E2E pass condition:

- The CLI can produce real local artifacts for the expected candidates, or a
  partial result with accurate failure attribution.

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
- A retrieved result set with local artifact fixtures.
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

Integration pass condition:

- The orchestrator records accepted images and coverage gaps correctly.

E2E pass condition:

- The delivery package validates and contains the expected accepted image set.

## Test Types

The same baseline cases can be consumed by three test types.

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

### Integration Tests

Purpose:

- Validate module call chains without real external services.

Examples:

- `QueryPlan -> fixture provider -> scheduler -> candidate gate`.
- `RetrievableCandidateBatch -> retrieval planner -> fixture/web fetch channel`.
- `Retrieved images -> image gate -> orchestrator -> package validator`.

Execution:

```bash
cargo test --test baseline_candidate_recall_integration
cargo test --test baseline_candidate_retrieval_integration
cargo test --test baseline_delivery_integration
```

### End-to-End Tests

Purpose:

- Validate CLI behavior and produce baseline reports.

Fixture E2E should run by default once implemented. Real-service E2E must be
opt-in because it uses network services, credentials, cost, and live provider
state.

Execution:

```bash
cargo test --test baseline_e2e_fixture
IMAGE_RETRIEVAL_REAL_BASELINE=1 cargo test --test baseline_e2e_real_service
```

Required real-service environment variables:

- `SERPAPI_API_KEY` or provider-specific search key.
- `QWEN_API_KEY` or configured VLM credential env.

Real-service tests must never assert exact candidate ids from the public web.
They should assert thresholds and produce a report for trend comparison.

## Case Design

Each case should declare:

- `case_id`
- `unit`
- `supported_test_types`
- `query_plan`
- `difficulty_profile`
- `fixtures`
- `gold_labels`
- `expected_metrics`
- `analysis_tags`

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
  "execution_mode": "fixture",
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
- Low `expected_candidate_recall` with good provider fixture data means
  parsing, dedupe, query propagation, or candidate scoring is the bottleneck.
- High duplicate rate means provider diversity, dedupe keys, pagination, or
  retry feedback need work.
- Good fixture E2E but poor real-service E2E means live provider/query strategy
  or network retrieval is the bottleneck.
- High false accept rate in delivery is more severe than low recall because it
  risks delivering wrong images.

## Baseline Thresholds

Initial thresholds should be intentionally conservative. They are meant to map
capability, not to block all development.

Suggested fixture thresholds:

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

Suggested real-service thresholds:

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

