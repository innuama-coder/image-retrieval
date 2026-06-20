# 开发任务合同：TASK-001 Rust CLI 工程骨架与领域基线实现

本文描述未来开发任务，不表示任务已经完成。

## Source Refs

- `docs/PRD.md:59-73`
- `docs/HLD.md:162-217`
- `docs/design/rust-implementation-design.md`
- `AGENTS.md`

## Planning Status

ready

## Dependencies

无。

## Downstream Consumers

TASK-002、TASK-003、TASK-004、TASK-005、TASK-006、TASK-007、TASK-008、TASK-009、TASK-011、TASK-010 均依赖本任务提供的 Cargo 工程、模块边界、领域类型基线、错误诊断基线和测试命令。

## Allowed Scope

- 创建最小 Rust Cargo CLI 工程。
- 建立 `src/` 模块边界，覆盖 CLI、domain、ports、quality、retrieval、orchestrator、delivery、policy、observability、self-check 的可扩展位置。
- 定义后续任务可复用的领域类型族、错误族、诊断结果和测试基础设施。
- 让 `cargo fmt --all -- --check`、`cargo clippy --all-targets --all-features -- -D warnings`、`cargo test --all` 可运行。

## Forbidden Scope

- 不实现具体图片搜索服务、抓取服务或 OpenClaw 生产协议。
- 不创建 Web UI、SaaS、多用户服务端、任务队列或账号体系。
- 不硬编码凭据、token、cookie 或真实外部服务配置。
- 不把未决产品决策写成既定实现。

## Expected Outputs

- `Cargo.toml`
- `src/main.rs`
- `src/lib.rs`
- 领域、端口、错误、诊断和测试模块骨架
- 最小可运行 CLI 入口
- 基础单元测试

## Acceptance Criteria

- 仓库成为可构建 Rust CLI 项目。
- 模块职责与 `docs/design/rust-implementation-design.md` 一致。
- 后续任务可以引用统一领域类型和错误诊断基线。
- Rust 格式、clippy 和测试命令均通过。

## Verification

- `cargo fmt --all -- --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all`

## Stop Conditions

- 如果建立工程骨架必须先选择 PRD/HLD/LLD 未确定的外部协议或生产服务，停止并上报决策缺口。
- 如果现有用户改动与工程初始化冲突，停止并说明冲突文件。

## Handoff Requirements

移交时说明新增模块、公共类型、错误/诊断模型、测试命令结果、未决实现点和下游任务可消费的接口。
