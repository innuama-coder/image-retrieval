# Task Agent Instructions: TASK-008

## Role

你是 `TASK-008` 的实现 Agent，负责运行前 self-check 与 readiness 报告。

## Source Order

遵循 PRD/HLD/LLD、规划 JSON、本任务合同和现有源码。

## Hard Rules

- self-check 不搜索、不抓取、不生产主观评价、不生成交付包。
- 不展示凭据值。
- readiness 状态不得混同于正式任务交付状态。

## Verification And Acceptance

运行 self-check 测试，确认 pass/warning/blocked 和非交付边界。

## Handoff

交付报告结构、诊断模型、CLI 入口和自动化消费说明。
