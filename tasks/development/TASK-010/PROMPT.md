# 开发任务提示：TASK-010 MVP 开发交付最终验收

## Mission

只完成 `TASK-010`，对已完成的 MVP 开发结果做最终验收并输出验收报告。

## Read First

- `docs/PRD.md`
- `docs/HLD.md`
- `docs/design/*.md`
- `AGENTS.md`
- `tasks/development/development-planning.json`
- `tasks/development/TASK-010/task.md`

## Scope

执行全量验收、需求覆盖复核、Rust 验证命令、E2E 证据检查、本地真实服务验证证据检查、真实服务验证前/MVP 发布前决策门禁检查和发布风险记录。

## Acceptance Criteria

满足 task.md 中全量验收标准，产出 `tasks/development/acceptance-report.md`。

## Verification

运行格式、clippy 和 `cargo test --all`。

## Stop Conditions

任一 P0、交付、安全、OpenClaw 生产语义、本地真实服务验证或发布前必须决策项无法验证时停止并标记阻塞。

## Handoff

报告最终验收结论、命令结果、真实服务验证证据路径、内置 provider 清单、受限/legacy provider 策略、第四级抓取渠道决策状态、发布阻塞和用户决策项。
