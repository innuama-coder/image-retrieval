# 开发任务提示：TASK-006 图片验收与任务编排状态机实现

## Mission

只完成 `TASK-006`，实现图片验收和任务编排状态机。

## Read First

- `docs/design/TASK-006-image-acceptance-orchestrator-design.md`
- `docs/HLD.md:260-372`
- `tasks/development/development-planning.json`
- `tasks/development/TASK-006/task.md`

## Scope

实现完整尝试循环、图片验收、OpenClaw 图片结论归一、重试计数和交付决策。

## Acceptance Criteria

满足机械+主观双通过、OpenClaw 不可执行阻塞、达标完整交付、超限有限交付和计数语义验收。

## Verification

运行格式、clippy 和 `cargo test --all orchestrator`。

## Stop Conditions

上游端口事实不稳定或与 LLD 冲突时停止。

## Handoff

报告终态、尝试计数、合格图片、拒绝证据和指标事件。
