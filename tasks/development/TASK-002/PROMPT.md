# 开发任务提示：TASK-002 QueryPlan CLI 输入、默认值与派生规划实现

## Mission

只完成 `TASK-002`，实现 QueryPlan 输入规划，不进入搜索、抓取、评价或交付。

## Read First

- `docs/design/TASK-002-queryplan-cli-input-planning-design.md`
- `docs/PRD.md:75-90`
- `tasks/development/development-planning.json`
- `tasks/development/TASK-002/task.md`

## Scope

实现输入校验、默认值、派生规划值、输入拒绝和 CLI 输入边界。

## Acceptance Criteria

满足 task.md 中全部 QueryPlan 默认值、派生值和输入拒绝验收。

## Verification

运行 `cargo fmt --all -- --check`、`cargo clippy --all-targets --all-features -- -D warnings`、`cargo test --all query_plan`。

## Stop Conditions

遇到未决 CLI 语法或产品默认值冲突时停止。

## Handoff

报告 `TaskPlan` 字段、诊断模型、测试结果和下游消费说明。
