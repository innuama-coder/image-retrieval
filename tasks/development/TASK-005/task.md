# 开发任务合同：TASK-005 BaseRetrievalChannel 批次抓取与 fallback 实现

本文描述未来开发任务，不表示任务已经完成。

## Source Refs

- `docs/PRD.md:112-119`
- `docs/PRD.md:206-207`
- `docs/HLD.md:211-212`
- `docs/design/TASK-005-retrieval-channel-batch-design.md`

## Planning Status

ready

## Dependencies

- TASK-001
- TASK-002
- TASK-004

## Downstream Consumers

TASK-006、TASK-007、TASK-008、TASK-009、TASK-011、TASK-010。

## Allowed Scope

- 实现 `BaseRetrievalChannel` 抓取能力端口。
- 实现普通 web fetch 最小抓取通道，并实现自托管、付费在线服务两类已确认 tier 的模型和默认启用边界。
- 实现每个完整尝试的 `required_count * 2` 目标批次和短批次。
- 实现 fallback eligibility fact、局部拒绝和任务级阻塞事实。
- 提供内部 fixture channel。

## Forbidden Scope

- 不发明第四级渠道。
- 不默认启用付费渠道。
- 不通过 fallback 绕过登录、付费墙、访问控制或站点授权。
- 不把未验收图片加入合格清单。

## Expected Outputs

- `BaseRetrievalChannel`
- `RetrievalBatchPlanner`
- `RetrievalChannelReadiness`
- `RetrievalSuccess` / `RetrievalFailure`
- `FallbackEligibilityFact`
- 普通 web fetch 最小抓取通道。
- retrieval 测试。

## Acceptance Criteria

- 要求 4 张图片时目标批次为 8。
- 普通 web fetch 通道具备最小真实图片抓取能力，或在合规限制下返回明确失败事实。
- 可抓取候选不足时形成短批次，不无限补抓。
- 拒绝、不确定或待补充证据候选不进入批次。
- 普通失败可 fallback。
- 访问控制或授权限制不得通过升级渠道绕过。
- 付费通道未启用时不会被静默使用。

## Verification

- `cargo fmt --all -- --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all retrieval`

## Stop Conditions

- 如果需要第四级渠道、付费策略或真实抓取商业服务决策，停止并请求用户决策。

## Handoff Requirements

移交真实图片 artifact、失败事实、fallback 事件、局部拒绝、任务级阻塞和短批次证据。
