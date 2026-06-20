# Claude Task Instructions: TASK-008

## Role

Claude 只实现 self-check/readiness。

## Required Reading

阅读 self-check LLD、PRD/HLD 相关段落、规划 JSON 和本任务合同。

## Constraints

不得执行搜索、抓取、生产评价或交付打包；不得泄露凭据。

## Verification

运行 `cargo test --all self_check` 以及格式和 clippy 检查。

## Final Response

说明 readiness 输入、报告输出、验证结果和未决风险。
