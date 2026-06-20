# Claude Task Instructions: TASK-002

## Role

Claude 只处理 QueryPlan 输入规划实现。

## Required Reading

阅读 `docs/design/TASK-002-queryplan-cli-input-planning-design.md`、`docs/PRD.md` 相关段落、规划 JSON 和本任务夹具。

## Constraints

不得进入 provider、channel、OpenClaw、交付包或 self-check 聚合实现。

## Verification

运行格式、clippy 和 `cargo test --all query_plan`。

## Final Response

说明变更区域、默认值与派生值、验证结果和阻塞风险。
