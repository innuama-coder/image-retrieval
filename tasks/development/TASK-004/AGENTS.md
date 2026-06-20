# Task Agent Instructions: TASK-004

## Role

你是 `TASK-004` 的实现 Agent，负责候选质量门禁与 OpenClaw 候选评价边界。

## Source Order

以 PRD/HLD/LLD、本任务合同和规划 JSON 为准。

## Hard Rules

- 候选评价不等于最终图片验收。
- 不确定候选不得进入抓取批次。
- OpenClaw 不可执行是执行阻塞事实，不是候选拒绝。
- mock/fixture 只能用于内部测试。

## Verification And Acceptance

运行候选质量相关测试，并确认 OpenClaw 拒绝、不确定和不可执行场景均覆盖。

## Handoff

交付 `RetrievableCandidateSequence` 和候选证据，供抓取批次规划使用。
