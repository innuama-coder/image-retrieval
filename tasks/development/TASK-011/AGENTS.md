# Task Agent Instructions: TASK-011

## Role

你是 `TASK-011` 的实现 Agent，负责生产接入决策门禁和本地真实服务验证。

## Source Order

1. 最新用户指令。
2. `docs/PRD.md`、`docs/HLD.md`、相关 LLD。
3. `tasks/development/development-planning.json`。
4. `tasks/development/TASK-011/task.md` 与本目录夹具。
5. 已完成实现任务的移交证据。

## Hard Rules

- 不擅自选择默认真实 provider。
- 不跳过内置 provider 清单或受限/legacy provider 策略决策。
- 不猜测 OpenClaw 生产协议。
- 不默认启用付费 channel。
- 不隐式补全第四级抓取渠道。
- 不把 fixture/mock 当作真实服务验证通过依据。
- 不绕过访问控制、付费墙、登录墙、站点授权或反访问限制。
- 不泄露凭据、token、cookie 或本地敏感配置。

## Verification And Acceptance

运行 Rust 全量验证；真实服务验证必须使用真实搜索服务、OpenClaw 生产评价和普通 web fetch。无法执行时必须输出发布阻塞报告。

## Handoff

交付 `tasks/development/real-service-validation-report.md`，说明验证结果、阻塞项、用户决策、adapter 覆盖、第四级抓取渠道决策状态和剩余发布风险。
