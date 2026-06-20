# 开发任务提示：TASK-003 BaseProvider 搜索端口、注册表与加权调度实现

## Mission

只完成 `TASK-003`，实现搜索 provider 可插拔端口和候选搜索调度。

## Read First

- `docs/design/TASK-003-base-provider-search-design.md`
- `docs/PRD.md:92-100`
- `tasks/development/development-planning.json`
- `tasks/development/TASK-003/task.md`

## Scope

实现 `BaseProvider`、registry、readiness、权重调度、候选归一、去重、来源追踪和测试 fixture。

## Acceptance Criteria

满足候选目标、加权随机、等权默认、异常权重诊断、候选短缺和凭据脱敏验收。

## Verification

运行格式、clippy 和 `cargo test --all search`。

## Stop Conditions

遇到默认真实 provider 或具体外部协议选择需求时停止。

## Handoff

报告 provider 合同、候选输出、调度事件和测试结果。
