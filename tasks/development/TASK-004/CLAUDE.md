# Claude Task Instructions: TASK-004

## Role

Claude 只实现候选阶段质量门禁和 OpenClaw 候选评价归一。

## Required Reading

阅读候选质量 LLD、PRD/HLD 相关段落、规划 JSON 和本任务合同。

## Constraints

不得处理图片下载、图片验收或交付包生成；不得把 mock 当作生产评价。

## Verification

运行 `cargo test --all candidate_quality` 以及格式和 clippy 检查。

## Final Response

说明候选决策模型、OpenClaw 结论归一、验证结果和阻塞风险。
