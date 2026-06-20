# 开发任务提示：TASK-001 Rust CLI 工程骨架与领域基线实现

## Mission

只完成 `TASK-001`：建立 Rust CLI 工程骨架与后续任务共享的领域基线。

## Read First

- `docs/PRD.md`
- `docs/HLD.md`
- `docs/design/rust-implementation-design.md`
- `tasks/development/development-planning.json`
- `tasks/development/TASK-001/task.md`

## Scope

允许创建 Cargo 工程、CLI 入口、模块边界、领域类型基线、错误诊断基线和基础测试。禁止实现 provider、channel、OpenClaw 生产协议或交付业务闭环。

## Acceptance Criteria

工程可构建，模块边界与 LLD 一致，Rust 格式、clippy 和测试均通过。

## Verification

运行 `cargo fmt --all -- --check`、`cargo clippy --all-targets --all-features -- -D warnings`、`cargo test --all`。

## Stop Conditions

遇到未决技术选择、外部协议或用户改动冲突时停止并报告。

## Handoff

报告任务 ID、变更文件、验证结果、下游可复用接口和剩余风险。
