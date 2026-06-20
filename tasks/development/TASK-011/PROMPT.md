# 开发任务提示：TASK-011 生产接入决策门禁与本地真实服务验证实现

## Mission

只完成 `TASK-011`，把生产接入决策和本地真实服务验证变成发布前不可绕过的实现与验收合同。

## Read First

- `docs/PRD.md:215-236`
- `docs/HLD.md:423-459`
- `docs/design/TASK-007-delivery-policy-observability-design.md`
- `tasks/development/development-planning.json`
- `tasks/development/TASK-011/task.md`

## Scope

在用户决策完成后实现已决生产 adapter 和真实服务验证；决策缺失时输出发布阻塞报告，不猜测、不绕过，并记录第四级抓取渠道决策状态。

## Acceptance Criteria

满足 task.md 中真实 provider、内置 provider 清单、受限/legacy provider 策略、OpenClaw 生产评价、普通 web fetch、channel fallback、禁用边界、第四级抓取渠道决策状态、脱敏和发布阻塞验收。

## Verification

运行格式、clippy、`cargo test --all`，并执行本地真实服务验证命令或记录明确阻塞。

## Stop Conditions

缺少 OpenClaw、默认真实 provider、内置 provider 清单、受限/legacy provider 策略、基础合规策略、付费边界或质量校准决策时停止生产验证。

## Handoff

报告真实服务验证结果、adapter 覆盖、阻塞项、敏感信息检查、第四级抓取渠道决策状态和用户决策清单。
