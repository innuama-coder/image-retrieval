# 开发任务合同：TASK-003 BaseProvider 搜索端口、注册表与加权调度实现

本文描述未来开发任务，不表示任务已经完成。

## Source Refs

- `docs/PRD.md:92-100`
- `docs/PRD.md:203-204`
- `docs/HLD.md:207-209`
- `docs/design/TASK-003-base-provider-search-design.md`

## Planning Status

ready

## Dependencies

- TASK-001
- TASK-002

## Downstream Consumers

TASK-004、TASK-006、TASK-007、TASK-008、TASK-009、TASK-011、TASK-010。

## Allowed Scope

- 实现 `BaseProvider` 搜索端口、provider registry、enabled/ready/weight 配置边界。
- 实现加权随机调度，支持测试可控随机源。
- 实现候选归一、去重、来源追踪、候选短缺和 MET-002 事件。
- 提供内部 fixture provider，用于自动化测试。

## Forbidden Scope

- 不选择默认真实搜索 provider。
- 不实现 Brave 等具体生产 HTTP 协议，除非用户另行决策。
- 不硬编码凭据。
- 不让 provider 决定候选是否可抓取或图片是否合格。

## Expected Outputs

- `BaseProvider` 与 provider readiness 模型。
- `SearchScheduler`、有效权重表、调度事件和候选短缺诊断。
- `CandidateRecord` 与来源证据。
- 搜索调度测试。

## Acceptance Criteria

- 要求 3 张图片时搜索目标约 60。
- 多个 enabled 且 ready provider 可按权重参与调度。
- 未指定权重时等权。
- 非数值、负数或零权重产生配置诊断并排除。
- 候选不足保留说明但不直接执行阻塞。
- 候选来源可解释，凭据不进入用户可见证据。

## Verification

- `cargo fmt --all -- --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all search`

## Stop Conditions

- 如果需要决定默认真实 provider、内置 provider 清单或受限 provider 策略，停止并请求用户决策。

## Handoff Requirements

移交 `CandidateRecord`、`CandidateSource`、`SearchUsageEvent`、候选短缺证据和 provider readiness 摘要。
