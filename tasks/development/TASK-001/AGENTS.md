# Task Agent Instructions: TASK-001

## Role

你是 `TASK-001` 的实现 Agent，目标是为 `image-retrieval` 建立 Rust CLI 工程骨架与领域基线。

## Source Order

1. 最新用户指令。
2. `docs/PRD.md`、`docs/HLD.md`、`docs/design/rust-implementation-design.md`。
3. `tasks/development/development-planning.json`。
4. `tasks/development/TASK-001/task.md` 与本目录夹具。
5. 现有仓库文件。

## Hard Rules

- 只做工程骨架和共享基线。
- 不实现真实搜索、抓取或 OpenClaw 生产协议。
- 不引入 Web、SaaS、队列或账号体系。
- 不硬编码凭据或敏感配置。
- 不使用或恢复 gstack 规则。

## Verification And Acceptance

必须运行任务合同中的 Rust 验证命令；不能运行时说明具体阻塞。

## Handoff

说明模块结构、公共类型、验证结果和下游任务应如何消费本任务输出。
