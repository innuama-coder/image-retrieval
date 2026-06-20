# 开发任务合同：TASK-007 交付包、策略、安全与可观测性实现

本文描述未来开发任务，不表示任务已经完成。

## Source Refs

- `docs/PRD.md:178-222`
- `docs/HLD.md:402-431`
- `docs/design/TASK-007-delivery-policy-observability-design.md`

## Planning Status

ready

## Dependencies

- TASK-001
- TASK-002
- TASK-003
- TASK-004
- TASK-005
- TASK-006

## Downstream Consumers

TASK-008、TASK-009、TASK-011、TASK-010。

## Allowed Scope

- 实现 `DeliveryPackageBuilder`。
- 实现 `status.json`、`manifest.json`、`summary.md`、`images/`、`evidence/`、`diagnostics/`。
- 实现策略与守护边界：凭据排除、未知授权风险、明确禁止来源拒绝或阻塞、付费默认禁用、fallback 合规。
- 汇总 MET-001 至 MET-006 的事件输入或摘要。
- 实现敏感信息脱敏和交付包契约测试。

## Forbidden Scope

- 不把输入拒绝包装成交付包。
- 不把未知授权写成可商用或无风险。
- 不把未验收图片放入 `images/` 或 `accepted_images`。
- 不在交付包、日志或指标中写入密钥、token、cookie 或敏感配置。
- 不触发新的搜索、抓取或主观评价。

## Expected Outputs

- Delivery package writer。
- `status.json` schema_version 1 和必填字段。
- `manifest.json` schema_version 1 和必填字段。
- 人类可读摘要、脱敏证据、诊断和指标摘要。
- delivery 测试。

## Acceptance Criteria

- 完整交付包含足量合格图片。
- 有限交付可为 0 张并解释缺口。
- 执行阻塞说明阻塞原因。
- 自动化调用方可稳定读取 `status.json`。
- `manifest.json` 包含 accepted_images、gap、candidate/retrieval/acceptance/risk/metrics/evidence_refs。
- MET-001 至 MET-006 均有事件来源。
- 敏感凭据不会进入用户可见结果。

## Verification

- `cargo fmt --all -- --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all delivery`

## Stop Conditions

- 如果交付包契约需要修改 LLD 已固定字段，停止并请求设计更新。
- 如果授权风险细则未决导致无法判定，应按风险提示或执行阻塞边界处理，不得猜测。

## Handoff Requirements

移交交付包路径规则、机器可读契约、策略判断、指标摘要和脱敏证据。
