# 开发任务提示：TASK-005 BaseRetrievalChannel 批次抓取与 fallback 实现

## Mission

只完成 `TASK-005`，实现抓取通道端口、批次规划和 fallback 事实模型。

## Read First

- `docs/design/TASK-005-retrieval-channel-batch-design.md`
- `docs/PRD.md:112-119`
- `tasks/development/development-planning.json`
- `tasks/development/TASK-005/task.md`

## Scope

实现普通 web fetch 最小抓取通道，建立三类已确认抓取 tier、目标批次、短批次、失败归一、fallback handoff 和测试 fixture。

## Acceptance Criteria

满足普通 web fetch 最小能力、批次规模、短批次、只消费可抓取候选、访问限制不绕过和付费禁用验收。

## Verification

运行格式、clippy 和 `cargo test --all retrieval`。

## Stop Conditions

遇到第四级渠道、付费启用或真实服务协议未决时停止。

## Handoff

报告抓取批次、成功 artifact、失败事实、fallback 事实和策略阻塞。
