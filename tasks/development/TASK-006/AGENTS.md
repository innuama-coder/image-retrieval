# Task Agent Instructions: TASK-006

## Role

你是 `TASK-006` 的实现 Agent，负责图片验收和任务编排状态机。

## Source Order

遵循 PRD/HLD/LLD、规划 JSON、本任务合同和现有代码。

## Hard Rules

- 图片阶段必须基于真实抓取 artifact。
- OpenClaw 不确定不得计入合格图片。
- OpenClaw 不可执行必须形成执行阻塞。
- 不得改变初次尝试加 3 次重试的宪法规则。

## Verification And Acceptance

运行 orchestrator 测试，覆盖完整交付、有限交付、执行阻塞和计数语义。

## Handoff

交付 `DeliveryDecision` 和任务上下文证据，供交付包任务使用。
