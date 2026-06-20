# Claude Task Instructions: TASK-001

## Role

Claude 只负责 `TASK-001`：Rust CLI 工程骨架与领域基线实现。

## Required Reading

先读 `docs/PRD.md`、`docs/HLD.md`、`docs/design/rust-implementation-design.md`、`tasks/development/development-planning.json` 和本目录全部任务文件。

## Constraints

不得实现具体 provider/channel/OpenClaw 生产能力，不得新增产品范围，不得硬编码凭据，不得修改无关用户变更。

## Verification

执行 `cargo fmt --all -- --check`、`cargo clippy --all-targets --all-features -- -D warnings`、`cargo test --all`。

## Final Response

汇报任务 ID、变更区域、验证结果、下游接口和未决风险。
