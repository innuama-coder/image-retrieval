# TASK-004 Prompt

## Execution Objective

Create `docs/design/v1.1-TASK-004-retrieval-artifact-channel-design.md` as the
future detailed design for retrieval jobs, batches, artifact results, fallback
channels, sidecars, summaries, task reports, checksums, and fetch traces.

## Scope

Use TASK-001/TASK-002 assumptions plus `AGENTS.md`, `docs/v1.1/PRD.md`,
`docs/v1.1/HLD.md`, `docs/v1.1/LLD.md`, `src/retrieval`, and the
image-search-retrieval/content-retrieval artifact contract as capability
reference.
Also read TASK-003 output and `tasks/design/v1.1-lld/fixtures/TASKS.md` for
common required detailed design content.

## Acceptance

The design covers normal web fetch, self-hosted fallback, paid online fallback,
batch target `required_image_count * 2`, artifact validity, sidecar_required,
summary quality gate, visual description, media type, job ownership, and retry
evidence.

## Required Output Detail

Specify `RetrievalChannel`, `RetrievalJob`, `RetrievalBatch`,
`RetrievalArtifactResult`, attempt trace, channel tier, fallback policy, artifact
paths, sidecar schema, summary/task report requirements, checksum fields,
content-type sniffing, robots/authorization blockers, and tests for missing
artifact, missing sidecar, fallback escalation, and partial retrieval.

## Verification

Run the `spec.yaml` verify command against
`docs/design/v1.1-TASK-004-retrieval-artifact-channel-design.md`.

## Constraints

Do not implement code. Do not count metadata-only records as retrieved images.

## Decision Principles

Prefer artifact-backed retrieval, explicit fallback tiers, auditable attempts,
and fail-closed delivery qualification.
