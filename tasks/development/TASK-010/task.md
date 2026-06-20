# 开发任务合同：TASK-010 MVP 开发交付最终验收

本文描述未来开发验收任务，不表示验收已经完成。

## Source Refs

- `docs/PRD.md`
- `docs/HLD.md`
- `docs/design/*.md`
- `AGENTS.md`
- `tasks/development/development-planning.json`

## Planning Status

ready

## Dependencies

- TASK-001
- TASK-002
- TASK-003
- TASK-004
- TASK-005
- TASK-006
- TASK-007
- TASK-008
- TASK-009
- TASK-011

## Downstream Consumers

无。本任务是最终开发交付验收任务。

## Allowed Scope

- 执行全量验收，核对 PRD/HLD/LLD 与实现一致性。
- 运行 Rust 格式、clippy 和测试。
- 检查 FR-001 至 FR-013、AC-001 至 AC-013、NFR-001 至 NFR-006、MET-001 至 MET-006 覆盖证据。
- 检查交付包、安全、OpenClaw 生产语义、fallback 合规、自检、E2E fixture 和本地真实服务验证。
- 产出 `tasks/development/acceptance-report.md`。
- 仅允许修复验收报告自身的记录问题；产品缺陷必须记录为阻塞或返回对应实现任务。

## Forbidden Scope

- 不新增产品范围。
- 不替用户关闭开放决策。
- 不跳过失败命令并宣称通过。
- 不把内部 fixture 结果当作真实服务验证通过。
- 不在本地真实服务验证缺失或失败时给出 MVP 可发布结论。

## Expected Outputs

- `tasks/development/acceptance-report.md`
- 全量命令输出摘要。
- 需求覆盖和证据路径。
- 发布阻塞、剩余风险和用户决策项。
- `tasks/development/real-service-validation-report.md` 的结论引用。

## Acceptance Criteria

- 所有非延期实现任务完成。
- 所有 FR/AC/NFR/MET 均有实现和验收证据。
- `cargo fmt --all -- --check` 通过。
- `cargo clippy --all-targets --all-features -- -D warnings` 通过。
- `cargo test --all` 通过。
- 本地真实服务验证已经通过，或最终验收结论必须明确标记为 MVP 发布阻塞而非通过。
- 最终报告明确真实服务验证、OpenClaw、默认真实 provider、内置 provider 清单、受限/legacy provider 策略、付费边界、授权细则、站点规则和第四级抓取渠道相关证据或阻塞。

## Verification

- `cargo fmt --all -- --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all`

## Stop Conditions

- 任一 P0 产品规则、交付状态、安全边界或 OpenClaw 生产语义无法验证时，不得通过最终验收。
- 本地真实服务验证未执行、失败或被用户决策阻塞时，不得给出 MVP 发布通过结论。
- 任一真实服务验证前或 MVP 发布前必须决策项未关闭且未形成明确阻塞结论时，不得给出 MVP 发布通过结论。
- 任一验证命令失败时，不得声明完成。

## Handoff Requirements

移交最终验收报告、命令结果、覆盖证据、发布阻塞和后续决策清单。
