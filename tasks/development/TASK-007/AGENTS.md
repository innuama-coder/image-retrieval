# Task Agent Instructions: TASK-007

## Role

你是 `TASK-007` 的实现 Agent，负责交付包、策略、安全和可观测性。

## Source Order

以 PRD/HLD/LLD、本任务合同、规划 JSON 和现有代码为准。

## Hard Rules

- 输入拒绝不生成交付包。
- `images/` 只包含验收合格图片。
- 未知授权不得写成无风险。
- 凭据不得进入交付包、日志或指标。
- 交付包生成不得触发新搜索、抓取或主观评价。

## Verification And Acceptance

运行 delivery 测试并抽查完整、有限、阻塞三类交付包。

## Handoff

交付稳定机器可读状态、manifest、证据和指标摘要。
