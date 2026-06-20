# 开发任务合同：TASK-002 QueryPlan CLI 输入、默认值与派生规划实现

本文描述未来开发任务，不表示任务已经完成。

## Source Refs

- `docs/PRD.md:75-90`
- `docs/PRD.md:201-202`
- `docs/HLD.md:204-206`
- `docs/design/TASK-002-queryplan-cli-input-planning-design.md`

## Planning Status

ready

## Dependencies

- TASK-001

## Downstream Consumers

TASK-003、TASK-004、TASK-005、TASK-006、TASK-007、TASK-008、TASK-011、TASK-010。

## Allowed Scope

- 实现 QueryPlan 输入归一、校验和默认值。
- 实现 `ValidatedQueryPlan` 与 `TaskPlan`。
- 派生 `candidate_target = required_count * 20`、`retrieval_batch_target = required_count * 2`、`retry_limit = 3`。
- 实现输入拒绝诊断和疑似敏感输入脱敏。
- 让正式任务和 self-check 共用同一输入规划逻辑。

## Forbidden Scope

- 不调用搜索 provider、抓取 channel 或 OpenClaw。
- 不生成交付包。
- 不擅自设置最大 QueryPlan 数量硬上限。
- 不改变 PRD 默认数量、默认质量和重试语义。

## Expected Outputs

- QueryPlan 输入类型和校验逻辑。
- 默认值解释与字段级诊断。
- CLI 命令边界中的正式运行和 self-check 输入分流。
- QueryPlan 单元测试。

## Acceptance Criteria

- 包含语义描述时生成有效规划。
- 缺少语义描述时输入拒绝且不进入搜索。
- 缺省数量为 1，缺省质量为通用质量，缺省重试为 3。
- 要求 3 张图片派生约 60 个候选。
- 要求 4 张图片派生 8 个抓取候选。
- 未知授权保持风险提示，不被描述为商用安全。

## Verification

- `cargo fmt --all -- --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all query_plan`

## Stop Conditions

- 如果 CLI 语法、输入格式或序列化格式需要用户决策才能继续，停止并上报。
- 如果下游要求改变 PRD 默认值，停止并请求产品决策。

## Handoff Requirements

移交 `TaskPlan`、输入拒绝诊断、默认值解释和可供 self-check 复用的规划接口。
