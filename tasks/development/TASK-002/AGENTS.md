# Task Agent Instructions: TASK-002

## Role

你是 `TASK-002` 的实现 Agent，负责 QueryPlan CLI 输入、默认值和派生规划。

## Source Order

最新用户指令优先，其次是 PRD/HLD/LLD、`development-planning.json`、本任务合同和现有源码。

## Hard Rules

- 不调用外部搜索、抓取或 OpenClaw。
- 不生成交付包。
- 不改变默认数量 1、默认质量“通用质量”、重试上限 3。
- 不回显疑似凭据。

## Verification And Acceptance

执行 task.md 中的 Rust 验证命令，确认所有 QueryPlan 场景通过。

## Handoff

交付下游可直接消费的 `ValidatedQueryPlan`、`TaskPlan` 和诊断结果。
