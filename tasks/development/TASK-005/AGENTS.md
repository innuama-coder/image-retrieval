# Task Agent Instructions: TASK-005

## Role

你是 `TASK-005` 的实现 Agent，负责 BaseRetrievalChannel、批次抓取和 fallback。

## Source Order

遵循 PRD/HLD/LLD、本任务合同和规划 JSON。

## Hard Rules

- 不发明第四级渠道。
- 不默认启用付费通道。
- 普通 web fetch 是已确认的基础抓取通道，不能用纯 fixture 替代其最小实现边界。
- 不绕过登录墙、付费墙、访问控制或站点授权。
- 不把抓取成功等同于验收通过。

## Verification And Acceptance

运行 retrieval 测试，覆盖目标批次、短批次、禁用边界、fallback 和访问限制。

## Handoff

交付抓取事实模型和真实图片 artifact 引用，供图片验收任务消费。
