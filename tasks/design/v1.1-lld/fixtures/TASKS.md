# v1.1 LLD Design Subtask Package

This package contains design-planning subtask fixtures for v1.1 low-level
design work.

## Subtask Relationship

Each subtask owns one future `docs/design/*.md` deliverable. The relationship is
source-driven: PRD/HLD/LLD source evidence feeds detailed design documents, and
the final acceptance subtask reviews the whole design set.

## Dependency And Execution Order

1. TASK-001 has no dependency and defines QueryPlan/config/policy foundations.
2. TASK-002 depends on TASK-001 and designs search/candidate behavior.
3. TASK-003 depends on TASK-001 and TASK-002 and designs quality/Qwen 3.5 VLM behavior.
4. TASK-004 depends on TASK-001, TASK-002, and TASK-003 and designs retrieval artifacts.
5. TASK-005 depends on TASK-001 through TASK-004 and designs orchestration/package/CLI.
6. TASK-006 depends on TASK-005 and designs testing/real-service acceptance.
7. TASK-007 depends on every previous subtask and performs final design acceptance.

## Acceptance And Handoff

Every subtask must handoff one validated `docs/design/*.md` file. The acceptance
subtask must review source traceability, dependency coverage, validator output,
and unresolved blockers before implementation planning is treated as final.

## Required Design Detail

Every detailed design subtask must be concrete enough for a development agent to
implement without rereading the whole planning thread. Each design document must
include:

- Source traceability to PRD/HLD/AGENTS requirements and the current source files.
- Scope boundaries, non-goals, and unresolved decisions.
- Rust module impact and exact trait/type/DTO names with important fields.
- Control flow, data flow, state/persistence behavior, and retry/fallback rules.
- Error codes, diagnostics, redaction/security behavior, and observability.
- Package or interface contracts consumed by downstream tasks.
- Verification plan mapping requirements to unit, integration, E2E, or blocker evidence.
- Handoff checklist for the development task that will implement the design.
