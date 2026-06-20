# 开发任务合同：TASK-008 运行前 self-check 与 readiness 报告实现

本文描述未来开发任务，不表示任务已经完成。

## Source Refs

- `docs/PRD.md:175`
- `docs/PRD.md:212`
- `docs/HLD.md:235-257`
- `docs/design/TASK-008-readiness-self-check-design.md`

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

## Downstream Consumers

TASK-009、TASK-011、TASK-010。

## Allowed Scope

- 实现 self-check CLI 入口。
- 聚合 QueryPlan、provider、channel、candidate OpenClaw、image OpenClaw 和 policy readiness。
- 输出 `SelfCheckReport`，区分 pass、warning、blocked。
- 实现脱敏诊断和自动化可读报告。

## Forbidden Scope

- 不搜索候选。
- 不下载图片。
- 不执行生产主观评价。
- 不生成交付包。
- 不通过 self-check 绕过策略或权限。
- 不展示凭据值。

## Expected Outputs

- `SelfCheckRequest`
- `ProviderReadinessSummary`
- `RetrievalChannelReadinessSummary`
- `OpenClawReadinessSummary`
- `PolicyReadinessSummary`
- `SelfCheckReport`
- self-check 测试。

## Acceptance Criteria

- 非法 QueryPlan 产生 blocked。
- provider 缺凭据可见但不泄露值。
- 无 enabled channel 可见。
- 付费 channel 未确认可见。
- candidate OpenClaw 与 image OpenClaw readiness 分别报告。
- 策略 blocker 脱敏展示。
- self-check 不生成 `images/`、`status.json` 或 `manifest.json`。

## Verification

- `cargo fmt --all -- --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all self_check`

## Stop Conditions

- 如果 readiness 探针需要未决生产协议，按 blocked/warning 事实处理，不得猜测协议。

## Handoff Requirements

移交 self-check 命令、报告结构、诊断分类、脱敏规则和非交付边界证明。
