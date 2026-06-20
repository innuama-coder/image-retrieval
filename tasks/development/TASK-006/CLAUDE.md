# Claude Task Instructions: TASK-006

## Role

Claude 只实现图片验收和任务编排状态机。

## Required Reading

阅读图片验收 LLD、HLD 运行时视图、PRD 验收标准、规划 JSON 和本任务合同。

## Constraints

不得生成交付包，不得跳过 OpenClaw，不得把候选元信息当作真实图片验收。

## Verification

运行 `cargo test --all orchestrator` 以及格式和 clippy 检查。

## Final Response

说明状态机、计数、终态决策、验证结果和剩余风险。
