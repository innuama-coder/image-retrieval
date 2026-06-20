# 开发任务合同：TASK-011 生产接入决策门禁与本地真实服务验证实现

本文描述未来开发任务，不表示任务已经完成。

## Source Refs

- `docs/PRD.md:215-220`
- `docs/PRD.md:224-236`
- `docs/HLD.md:423-430`
- `docs/HLD.md:449-459`
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
- TASK-007
- TASK-008
- TASK-009

## Downstream Consumers

TASK-010。

## Allowed Scope

- 在用户确认默认真实搜索 provider、内置 provider 清单、受限/legacy provider 策略、OpenClaw 生产评价方式、基础合规策略、搜索权重默认口径、付费渠道边界和质量校准放行后，实现对应生产 adapter 或验证配置。
- 执行本地真实服务验证，覆盖真实搜索服务、OpenClaw 生产评价、普通 web fetch、所有已启用抓取 channel fallback、未启用 channel 禁用说明和敏感信息排除。
- 记录第四级抓取渠道决策状态；未关闭时作为 MVP 发布阻塞而非真实服务验证通过事实。
- 在必要决策或凭据缺失时输出明确发布阻塞报告。
- 产出 `tasks/development/real-service-validation-report.md`。

## Forbidden Scope

- 不擅自选择默认真实 provider。
- 不跳过内置 provider 清单或受限/legacy provider 策略决策。
- 不猜测 OpenClaw 生产协议或技能执行方式。
- 不默认启用付费抓取渠道。
- 不把 fixture/mock 结果作为真实服务验证通过依据。
- 不绕过登录、付费墙、访问控制、站点授权或反访问限制。
- 不替用户关闭授权阻塞细则、robots/site-rule 或第四级渠道决策。
- 不隐式补全第四级抓取渠道。

## Expected Outputs

- 生产接入决策门禁。
- 已决真实搜索 provider adapter 或阻塞报告。
- OpenClaw 生产评价 adapter 或阻塞报告。
- 本地真实服务验证场景。
- `tasks/development/real-service-validation-report.md`。

## Acceptance Criteria

- 真实服务验证前必须确认或记录 OpenClaw、默认真实 provider、内置 provider 清单、受限/legacy provider 策略、基础合规策略、搜索权重默认口径、付费渠道边界和质量校准状态。
- 至少一个已决真实搜索 provider 能按 `BaseProvider` 接入；若用户尚未决策，任务必须明确阻塞，且最终验收不得标记 MVP 可发布。
- OpenClaw 生产评价不可用时，合法任务进入 `execution_blocked`。
- 普通 web fetch 真实抓取路径被验证。
- 所有已启用 channel fallback 表现和未启用 channel 禁用说明被验证。
- 第四级抓取渠道决策状态被记录；未决时最终验收不得标记 MVP 可发布。
- 交付包和用户可见日志不包含敏感凭据。

## Verification

- `cargo fmt --all -- --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all`
- 执行本地真实服务验证命令，或记录因用户决策/凭据缺失导致的发布阻塞。

## Stop Conditions

- OpenClaw 生产方式、默认真实搜索 provider、内置 provider 清单、受限/legacy provider 策略、基础合规策略、付费渠道边界或质量校准未决时，停止生产验证并输出发布阻塞报告。
- 任何真实服务验证出现绕过访问控制、错误授权声明、OpenClaw 缺失或敏感凭据暴露时，停止并标记发布阻塞。

## Handoff Requirements

移交真实服务验证报告、已决/未决项、adapter 覆盖、验证命令、凭据脱敏证明、第四级抓取渠道决策状态、发布阻塞和用户决策清单。
