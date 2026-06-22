# TASK-002 Prompt

## Execution Objective

Create `docs/design/v1.1-TASK-002-search-provider-candidate-design.md` as the
future detailed design for BaseSearchProvider, provider readiness, weighted
scheduling, candidate normalization, dedupe, provenance, and search diagnostics.

## Scope

Use TASK-001 design assumptions plus `AGENTS.md`, `docs/v1.1/PRD.md`,
`docs/v1.1/HLD.md`, `docs/v1.1/LLD.md`, and current `src/search` modules.
Also read `tasks/design/v1.1-lld/fixtures/TASKS.md` for common required
detailed design content.

## Acceptance

The design explains pluggable provider contracts, external configuration,
weighted random provider selection, roughly 20 candidates per required image,
candidate IDs, source URLs, dimensions, licensing evidence, and failure codes.

## Required Output Detail

Specify `BaseSearchProvider`, provider registry/readiness contracts,
SearchRequest/SearchResponse/Candidate DTOs, weighted scheduling algorithm,
dedupe keys, provenance fields, provider error taxonomy, metrics/logs, and tests
for provider substitution and candidate target calculation.

## Verification

Run the `spec.yaml` verify command against
`docs/design/v1.1-TASK-002-search-provider-candidate-design.md`.

## Constraints

Do not implement code. Do not design retrieval execution outside candidate data
needed by retrieval.

## Decision Principles

Prefer trait contracts, structured candidate data, deterministic diagnostics,
and source traceability over ad hoc provider conditionals.
