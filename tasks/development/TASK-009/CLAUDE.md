# Claude Task Instructions: TASK-009

## Role

Claude 只实现内部 fixture E2E 验证和发布前门禁。

## Required Reading

阅读 PRD 发布门禁、HLD 运维发布设计、详细设计验收报告、规划 JSON 和本任务合同。

## Constraints

不得把 fixture 当作生产评价，不得要求真实凭据运行内部测试，不得关闭未决用户决策；门禁必须覆盖内置 provider 清单、受限/legacy provider 策略和第四级抓取渠道。

## Verification

运行 `cargo test --all` 以及格式和 clippy 检查。

## Final Response

说明 E2E 覆盖、真实服务验证前门禁、MVP 发布前门禁、验证结果和剩余发布风险。
