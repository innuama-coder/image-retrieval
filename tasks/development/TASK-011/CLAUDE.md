# Claude Task Instructions: TASK-011

## Role

Claude 只执行 `TASK-011`：生产接入决策门禁与本地真实服务验证。

## Required Reading

阅读 PRD 发布门禁、HLD 运维发布设计、交付/策略 LLD、规划 JSON 和本任务合同。

## Constraints

不得新增产品范围，不得替用户关闭未决决策，不得使用 fixture/mock 伪装真实服务验证，不得跳过内置 provider 清单、受限/legacy provider 策略或第四级抓取渠道决策，不得泄露凭据。

## Verification

运行 `cargo fmt --all -- --check`、`cargo clippy --all-targets --all-features -- -D warnings`、`cargo test --all`，并执行真实服务验证或记录明确发布阻塞。

## Final Response

说明真实服务验证结论、命令结果、adapter 覆盖、第四级抓取渠道决策状态、阻塞项和剩余发布风险。
