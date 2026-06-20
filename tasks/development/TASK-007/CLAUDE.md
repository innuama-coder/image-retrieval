# Claude Task Instructions: TASK-007

## Role

Claude 只实现交付包、策略、安全与可观测性。

## Required Reading

阅读交付包 LLD、PRD/HLD 对交付和安全的要求、规划 JSON 和本任务合同。

## Constraints

不得加入未验收图片，不得泄露敏感配置，不得改变交付状态语义。

## Verification

运行 `cargo test --all delivery` 以及格式和 clippy 检查。

## Final Response

说明交付包结构、策略边界、指标来源、验证结果和剩余风险。
