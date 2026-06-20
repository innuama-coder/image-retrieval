# Claude Task Instructions: TASK-010

## Role

Claude 只执行 MVP 开发交付最终验收。

## Required Reading

阅读 PRD、HLD、全部 LLD、AGENTS.md、开发规划 JSON、本任务合同和所有实现任务移交证据。

## Constraints

不得新增范围，不得伪造验证，不得替用户关闭真实服务、OpenClaw、内置 provider 清单、受限/legacy provider 策略、付费、授权、站点规则或第四级抓取渠道决策；真实服务验证缺失或失败时不得给出 MVP 可发布结论。

## Verification

运行 `cargo fmt --all -- --check`、`cargo clippy --all-targets --all-features -- -D warnings`、`cargo test --all`，并核对 TASK-011 真实服务验证报告和发布前必须决策项。

## Final Response

说明验收结论、验证命令、证据路径、阻塞项和剩余发布风险。
