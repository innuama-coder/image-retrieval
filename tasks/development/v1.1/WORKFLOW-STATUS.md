# v1.1 Workflow Status

## Required Workflow

```text
PRD -> HLD -> LLD tasks -> LLD -> development tasks -> development -> testing -> acceptance
```

## Current Status

| Phase | Artifact | Status | Notes |
| --- | --- | --- | --- |
| PRD | `docs/v1.1/PRD.md` | Complete | Defines v1.1 product target and acceptance criteria. |
| HLD | `docs/v1.1/HLD.md` | Complete | Defines high-level architecture and decisions. |
| LLD tasks | `tasks/design/v1.1-lld/design-planning.json` | Complete | Created with `design-planning`; `docs/v1.1/LLD-TASKS.md` is now an index. |
| LLD | `docs/design/v1.1-TASK-001..007-*.md` | Complete | Formal detailed design tasks executed and accepted by TASK-007. |
| Development tasks | `tasks/development/v1.1/` | Complete | Defines TASK-001 through TASK-007. |
| Development | Product source changes | Not started | Must execute TASK-001 through TASK-005. |
| Testing | `tasks/development/v1.1/testing-report.md` | Not started | Must execute TASK-006 after development. |
| Acceptance | `tasks/development/v1.1/acceptance-report.md` | Not started | Must execute TASK-007 after testing. |

## Blocking Decisions Before Final v1.1 Acceptance

- SerpApi Google Images adapter implementation, `SERPAPI_API_KEY` readiness,
  and real-service smoke evidence for the default ImageSearch provider.
- Qwen 3.5 VLM adapter implementation, `QWEN_API_TOKEN` readiness,
  externalized endpoint/model configuration, and real-service smoke evidence.
- Paid retrieval channel enablement and budget boundary.
- robots.txt / site-rule behavior.
- Authorization blocking rules.
- Quality threshold calibration or explicit waiver.

## Execution Rule

The project may proceed through local implementation and fixture testing before
all external decisions are closed. Final v1.1 acceptance must remain blocked if
any required real-service, Qwen 3.5 VLM implementation evidence, paid-channel,
or compliance decision is still unresolved.
