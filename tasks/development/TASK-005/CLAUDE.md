# Claude Task Instructions: TASK-005

## Role

Claude 只实现抓取通道端口、批次规划和 fallback 事实。

## Required Reading

阅读抓取 LLD、PRD/HLD 相关段落、规划 JSON 和本任务合同。

## Constraints

不得定义第四级渠道，不得默认启用付费通道，不得绕过访问控制，不得做图片验收；普通 web fetch 必须作为基础抓取通道实现边界出现。

## Verification

运行 `cargo test --all retrieval` 以及格式和 clippy 检查。

## Final Response

说明通道模型、批次语义、fallback 边界、验证结果和风险。
