# 开发任务提示：TASK-007 交付包、策略、安全与可观测性实现

## Mission

只完成 `TASK-007`，实现正式任务终态的交付包、策略和指标表达。

## Read First

- `docs/design/TASK-007-delivery-policy-observability-design.md`
- `docs/PRD.md:121-131`
- `docs/PRD.md:178-222`
- `tasks/development/development-planning.json`
- `tasks/development/TASK-007/task.md`

## Scope

实现交付包、状态文件、manifest、summary、证据、诊断、策略守护和指标摘要。

## Acceptance Criteria

满足完整交付、有限交付、执行阻塞、自动化消费、脱敏和 MET 覆盖验收。

## Verification

运行格式、clippy 和 `cargo test --all delivery`。

## Stop Conditions

交付契约或策略细则与 LLD 冲突时停止。

## Handoff

报告交付包契约、策略模型、指标来源、验证结果和风险。
