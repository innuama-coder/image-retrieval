# 开发任务提示：TASK-004 候选质量门禁与 OpenClaw 候选评价实现

## Mission

只完成 `TASK-004`，实现候选阶段机械校验、结构化主观评价边界和结果归一。

## Read First

- `docs/design/TASK-004-candidate-quality-openclaw-design.md`
- `docs/HLD.md:219-233`
- `tasks/development/development-planning.json`
- `tasks/development/TASK-004/task.md`

## Scope

实现候选机械证据、评价请求、OpenClaw 候选结论归一、可抓取序列和测试替身。

## Acceptance Criteria

满足机械阻塞、参考证据、明确认可、拒绝/不确定排除、不可执行阻塞等验收。

## Verification

运行格式、clippy 和 `cargo test --all candidate_quality`。

## Stop Conditions

遇到 OpenClaw 生产协议或技能执行模型未决时停止。

## Handoff

报告可抓取序列、拒绝证据、评价事件和阻塞事实。
