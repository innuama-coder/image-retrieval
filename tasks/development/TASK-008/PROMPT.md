# 开发任务提示：TASK-008 运行前 self-check 与 readiness 报告实现

## Mission

只完成 `TASK-008`，实现正式运行前自助检查。

## Read First

- `docs/design/TASK-008-readiness-self-check-design.md`
- `docs/HLD.md:235-257`
- `tasks/development/development-planning.json`
- `tasks/development/TASK-008/task.md`

## Scope

实现 readiness 聚合、self-check 状态、诊断脱敏和自动化可读报告。

## Acceptance Criteria

满足 QueryPlan、provider、channel、OpenClaw、policy readiness 和非交付边界验收。

## Verification

运行格式、clippy 和 `cargo test --all self_check`。

## Stop Conditions

遇到未决生产协议时不得猜测，输出 blocked/warning 并上报。

## Handoff

报告 self-check 入口、报告结构、诊断类别、验证结果和风险。
