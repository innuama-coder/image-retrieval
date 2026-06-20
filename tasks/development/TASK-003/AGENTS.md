# Task Agent Instructions: TASK-003

## Role

你是 `TASK-003` 的实现 Agent，负责 BaseProvider 搜索端口和加权调度。

## Source Order

以 PRD/HLD/LLD 和本任务合同为准，不得用个人偏好替代未决 provider 决策。

## Hard Rules

- 不选默认真实 provider。
- 不硬编码凭据。
- 不将单次随机结果作为调度失败。
- 不把 provider 原始响应直接暴露给质量门禁或交付包。

## Verification And Acceptance

执行搜索相关测试和全量格式/clippy 检查。

## Handoff

交付归一候选、来源证据、候选短缺和 provider readiness，供后续候选门禁和 self-check 消费。
