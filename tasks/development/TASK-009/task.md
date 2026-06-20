# 开发任务合同：TASK-009 内部 fixture 端到端验证与发布门禁实现

本文描述未来开发任务，不表示任务已经完成。

## Source Refs

- `docs/PRD.md:224-236`
- `docs/HLD.md:426-430`
- `docs/design/TASK-009-detailed-design-acceptance-review.md`

## Planning Status

ready

## Dependencies

- TASK-001
- TASK-002
- TASK-003
- TASK-004
- TASK-005
- TASK-006
- TASK-007
- TASK-008

## Downstream Consumers

TASK-011、TASK-010。

## Allowed Scope

- 实现内部 fixture provider、fixture channel 和 fixture evaluator 的端到端测试场景。
- 覆盖输入拒绝、完整交付、有限交付、执行阻塞和 self-check。
- 验证 mock/fixture 只能用于内部测试，不能作为生产交付依据。
- 记录真实服务验证前置门禁和未决用户决策项。
- 必要时更新 README 中的本地开发和验证说明。

## Forbidden Scope

- 不把 fixture 作为生产交付依据。
- 不要求真实服务凭据才能运行内部测试。
- 不关闭 OpenClaw、默认真实 provider、内置 provider 清单、受限/legacy provider 策略、付费边界、授权细则、站点规则、第四级抓取渠道等未由用户决策的问题。
- 不新增产品范围。

## Expected Outputs

- `tests/e2e` 或等效端到端测试。
- `tests/fixtures` 或等效内部测试替身。
- 发布前门禁说明。
- README 中必要的开发验证说明。

## Acceptance Criteria

- E2E fixture 覆盖 input_rejected。
- E2E fixture 覆盖 full_delivery。
- E2E fixture 覆盖 limited_delivery 0 张。
- E2E fixture 覆盖 OpenClaw 不可用导致 execution_blocked。
- E2E fixture 覆盖 channel fallback 禁用/访问限制边界。
- E2E fixture 覆盖敏感信息不进入交付包。
- 发布前门禁列出 OpenClaw、默认真实 provider、内置 provider 清单、受限/legacy provider 策略、付费边界、授权细则、站点规则和第四级抓取渠道等必须关闭或放行的开放决策。

## Verification

- `cargo fmt --all -- --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all`

## Stop Conditions

- 如果 TASK-001 至 TASK-008 的任一核心行为缺失，停止并返回对应任务修复。

## Handoff Requirements

移交端到端证据、fixture 范围说明、发布前门禁、真实服务验证前和 MVP 发布前仍需用户决策的事项，供 TASK-011 与 TASK-010 消费。
