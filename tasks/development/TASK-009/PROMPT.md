# 开发任务提示：TASK-009 内部 fixture 端到端验证与发布门禁实现

## Mission

只完成 `TASK-009`，建立内部端到端 fixture 验证和发布前门禁。

## Read First

- `docs/PRD.md:224-236`
- `docs/HLD.md:426-430`
- `docs/design/TASK-009-detailed-design-acceptance-review.md`
- `tasks/development/development-planning.json`
- `tasks/development/TASK-009/task.md`

## Scope

实现无需真实凭据的内部 E2E fixture，覆盖主要终态、自检和安全边界。

## Acceptance Criteria

满足输入拒绝、完整交付、有限交付 0 张、执行阻塞、fallback 禁用边界和敏感信息排除验收。

## Verification

运行格式、clippy 和 `cargo test --all`。

## Stop Conditions

上游核心行为缺失时停止并指向对应任务。

## Handoff

报告 E2E 场景、验证结果、fixture 非生产边界，以及 OpenClaw、默认 provider、内置 provider 清单、受限/legacy provider 策略、付费边界、授权细则、站点规则和第四级抓取渠道发布门禁。
