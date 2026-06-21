# 生产接入门禁决策清单 — Production Gate Decisions

| Field | Value |
|---|---|
| Date | 2026-06-21 |
| Decision maker | 产品决策者（用户） |
| Basis | `AGENTS.md` 宪法 + `docs/PRD.md` + TASK-011 `real-service-validation-report.md` |
| Status | **ALL 10 GATES CLOSED** — 可进入真实服务验证与生产 adapter 实现 |

本文件关闭 TASK-011 报告中列出的全部 10 项发布阻塞门禁（RSV-001~005、MVP-001~005）。
TASK-010 / TASK-011 报告中的 "MVP 发布：阻塞" 结论以本决策为准更新。

---

## 1. 决策汇总表

| Gate | 决策项 | 决策 |
|---|---|---|
| **RSV-001** | 默认真实图片搜索 provider | **Brave Image Search**（宪法点名的示例 provider） |
| **RSV-002 / MVP-002** | 内置 provider 清单与受限/legacy 策略 | **仅内置 Brave**；受限/legacy provider 默认禁用，需显式 opt-in |
| **RSV-003** | 付费抓取渠道（tier 3）启用边界 | **按 fallback 链正常接入**；配置了凭据即可用，不强制每次显式确认 |
| **RSV-004** | robots.txt / 站点规则合规 | **仅记录不阻断**：检测并记录 robots/站点规则状态，但不阻断抓取（用户自担风险） |
| **RSV-005** | 质量档位校准 | **现阈值放行**：接受当前 General/High/Strict 阈值为 MVP 初始值，上线后用真实任务数据迭代 |
| **MVP-001 / MVP-005** | OpenClaw 生产评价接入与 wire protocol | **本地 OpenClaw（embedded/CLI）**：调用本机已装 OpenClaw 执行 skill，无需远程 wire protocol |
| **MVP-003** | 授权风险阻塞细则 | **保持当前默认**：未知授权 → warn（保留交付 + 风险提示）；明确禁止 → block（局部拒绝） |
| **MVP-004** | 第四级抓取渠道 | **确认为表述误差，定为三级**：最终渠道分类锁定 `web_fetch → self_hosted → paid`，不新增第四级 |

---

## 2. 各决策的实现影响

### RSV-001 / RSV-002 / MVP-002 — 搜索 provider
- 实现 `src/search/adapters/brave.rs`，实现 `BaseProvider` trait（端口见 `src/ports/mod.rs:31-56`）。
- `ProviderRegistry`（`src/search/registry.rs`）默认仅注册 Brave 生产 provider。
- 受限/legacy provider：默认禁用，需用户显式 opt-in 才注册。
- 凭据外部化（宪法："provider 配置必须外部化，不得硬编码凭据"）：Brave API key 从环境变量/配置读取。

### MVP-001 / MVP-005 — OpenClaw 评价
- 实现本地 OpenClaw adapter（如 `src/quality/openclaw_local.rs`），实现 `OpenClawEvaluationPort` trait（端口见 `src/ports/mod.rs:100-128`），覆盖 `evaluate_candidates` 与 `evaluate_images` 两个边界。
- 经 embedded/CLI 调用本机 OpenClaw 执行相关 skill；**不定义远程 HTTP wire protocol**（MVP-005 据此关闭）。
- 保留执行阻塞语义：OpenClaw 不可用时任务进入 `execution_blocked`，**不得降级为 mock/fixture**（宪法 + 现有实现一致）。

### RSV-003 — 付费渠道
- `paid` channel（tier 3）按现有 fallback 链 `web_fetch → self_hosted → paid` 接入。
- 配置了付费渠道凭据即视为可用（`paid_unconfirmed` → 配置后转为就绪），不强制每次任务显式确认。
- 仍遵循宪法"优先免费高效通道"：付费仅作为 fallback 末级，前级失败才触发。

### RSV-004 — robots / 合规
- 检测并在诊断中记录 robots.txt / 站点规则状态（复用现有 `is_access_restricted` 标记），**但不阻断抓取**。
- 注意：此决策较宽松，合规风险由用户承担；若后续面向更严格合规场景，可收紧为"明确禁止则拒抓"。

### RSV-005 — 质量档位
- 当前 General/High/Strict 阈值原样放行作为 MVP 初始值。
- 上线后基于真实任务的"有限交付率 / 可用率"反馈迭代阈值，无需在发布前额外校准轮。

### MVP-003 — 授权风险
- 维持现有实现：未知授权 → warn；明确禁止 → block（局部拒绝）。已有测试覆盖，无需改动。

### MVP-004 — 渠道级数
- `RetrievalChannelTier`（`src/domain/retrieval.rs:40-54`）保持三变体，`next_fallback()` 链不变。
- 宪法"四级"表述判定为笔误，不新增第四级 tier。

---

## 3. 关闭后的实现路径（更新自 TASK-011 报告 §10）

1. 实现 Brave 搜索 provider adapter（`BaseProvider`）。
2. 实现本地 OpenClaw 评价 adapter（`OpenClawEvaluationPort`，候选 + 图片）。
3. 配置 `ProviderRegistry`：仅注册 Brave，凭据外部化。
4. 付费渠道按 fallback 链接入（配置即用）。
5. robots：检测+记录、不阻断。
6. 执行真实服务验证：Brave 搜索 + 本地 OpenClaw 评价 + web fetch 抓取，跑完整任务流程。
7. 验证 channel fallback 边界（含付费末级）。
8. 更新 self-check 反映真实生产配置就绪状态。
9. 将 `real-service-validation-report.md` 结论从 BLOCKED 更新为实际验证结果。

---

## 4. Post-MVP 延期项（维持 DEFERRED）

| ID | 问题 |
|---|---|
| OQ-001 | 单个 QueryPlan 最大交付数量是否需要限制 |
| OQ-002 | 交付结果是否需要按授权风险分组展示 |

---

## 5. 注意事项

- **RSV-004 决策偏宽松**（仅记录不阻断 robots）。这是用户的明确选择，风险自担；记录在此以便日后审计/收紧。
- **凭据管理**：所有生产 adapter（Brave key、付费渠道凭据、OpenClaw 配置）均须外部化，禁止硬编码（宪法强约束，现有敏感信息检查已通过）。
