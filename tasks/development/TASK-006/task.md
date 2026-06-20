# 开发任务合同：TASK-006 图片验收与任务编排状态机实现

本文描述未来开发任务，不表示任务已经完成。

## Source Refs

- `docs/PRD.md:121-159`
- `docs/PRD.md:208-211`
- `docs/HLD.md:260-372`
- `docs/design/TASK-006-image-acceptance-orchestrator-design.md`

## Planning Status

ready

## Dependencies

- TASK-001
- TASK-002
- TASK-003
- TASK-004
- TASK-005

## Downstream Consumers

TASK-007、TASK-008、TASK-009、TASK-011、TASK-010。

## Allowed Scope

- 实现 Task Orchestrator 的完整尝试循环。
- 实现图片机械验收和 OpenClaw 图片评价归一。
- 维护 `full_attempt_count` 和 `retry_count`。
- 实现完整交付、有限交付和执行阻塞的 `DeliveryDecision`。
- 累积合格图片、拒绝证据、缺口、终态和指标事件。

## Forbidden Scope

- 不生成交付包文件。
- 不把候选元信息当作真实图片验收。
- 不把 OpenClaw 不确定当作通过。
- 不改变初次尝试加最多 3 次重试规则。
- 不使用 mock/fixture 作为生产验收依据。

## Expected Outputs

- `TaskOrchestrator`
- `ImageAcceptanceGate`
- `AttemptCounter`
- `ImageAcceptanceDecision`
- `DeliveryDecision`
- orchestrator 状态机测试。

## Acceptance Criteria

- 只有机械验收通过且 OpenClaw 明确通过的图片计入合格。
- 机械失败、主观拒绝和不确定均不计入合格。
- OpenClaw 图片评价不可执行形成执行阻塞。
- 达到所需数量时立即完整交付。
- 不足时重复完整流程，而不是只重复抓取。
- 初次尝试加 3 次重试后有限交付，合格图片可以为 0。
- `retry_count` 与 `full_attempt_count` 不混淆。

## Verification

- `cargo fmt --all -- --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all orchestrator`

## Stop Conditions

- 如果上游任务无法提供稳定候选、抓取或 OpenClaw 事实模型，停止并返回对应任务修复。

## Handoff Requirements

移交 `DeliveryDecision`、合格图片列表、拒绝图片证据、执行阻塞原因、尝试计数、缺口和指标事件。
