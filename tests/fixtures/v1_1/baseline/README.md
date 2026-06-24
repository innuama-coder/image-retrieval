# v1.1 Baseline Recall Fixtures

This directory defines the baseline test set for measuring whether the CLI can
stably retrieve high-quality images according to a `QueryPlan`.

The baseline has three capability units:

1. `candidate_recall`: `QueryPlan -> Search Provider -> Candidate Evaluation`.
2. `candidate_retrieval`: `Retrievable Candidates -> Channels -> Artifacts`.
3. `delivery`: `Retrieved Results -> Image Evaluation -> Delivery Package`.

The canonical case list is `case-catalog.json`.

## Intended Test Layers

The same case metadata should drive:

- unit tests for deterministic parsers, validators, classifiers, and scorers;
- scenario fixture tests for capability chains using fixtures and/or the CLI;
- opt-in real-service scenario tests that emit baseline reports.

Scenario tests intentionally combine the previous integration and end-to-end
layers for the first baseline. Real-service scenarios must be opt-in and must
never assert exact public web result ids. They should assert thresholds and
report trend metrics.

## Report Output

Baseline runs should write reports under:

```text
target/baseline-reports/
```

Reports should include per-case metrics, stage-level failure attribution, and
recommended next work.
