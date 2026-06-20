# Task Agent Instructions: TASK-009

## Role

你是 `TASK-009` 的实现 Agent，负责内部 fixture 端到端验证与发布门禁。

## Source Order

遵循 PRD/HLD/LLD、规划 JSON、本任务合同和已经完成的实现任务输出。

## Hard Rules

- fixture/mock 只能用于内部验证。
- 不要求真实服务凭据运行内部测试。
- 不替用户关闭未决产品或合规决策。
- 发布门禁必须列出真实服务验证前和 MVP 发布前必须关闭的决策，包括内置 provider 清单、受限/legacy provider 策略和第四级抓取渠道。
- 不新增产品功能。

## Verification And Acceptance

运行全量 Rust 检查和端到端测试，确认四类任务结果状态及安全边界。

## Handoff

交付 E2E 证据、fixture 说明、真实服务验证前置门禁、MVP 发布前决策门禁和剩余风险。
