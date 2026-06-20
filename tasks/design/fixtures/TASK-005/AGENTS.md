# Task Agent Instructions: TASK-005

You are assigned to TASK-005: BaseRetrievalChannel Batch And Fallback Detailed Design.

Read PRD/HLD, `tasks/design/design-planning.json`, `docs/design/rust-implementation-design.md`, and this fixture. Produce only `docs/design/TASK-005-retrieval-channel-batch-design.md`.

Do not implement code. Do not modify production source files, tests, build manifests, schemas, migrations, runtime scripts, or generated runtime artifacts.

Keep fallback decision ownership with orchestration and policy, including the distinction between local rejection and task-level execution blocking. BaseRetrievalChannel exposes capability, status, and failure facts. Save every detailed design document under `docs/design/`.

Handoff with the task ID, document path, review checks, and unresolved design risks.
