# v1.1 Release Gate Decisions — 生产门禁决策清单

| Field | Value |
|---|---|
| Date | 2026-06-22 |
| Decision maker | 产品决策者（用户） |
| Basis | `RELEASE_GATES.md`（TASK-009 生成）+ `docs/v1.1/PRD.md` + TASK-007 acceptance-report |
| Status | **ALL 10 GATES CLOSED** — 可进入真实服务验证与 MVP 发布准备 |

本文件关闭 v1.1 `RELEASE_GATES.md` 中列出的全部 10 项门禁（RSV-001~005、MVP-001~005）。
TASK-007 acceptance-report 中的早期阻塞结论以本决策和后续真实服务验证结果为准更新。

---

## 1. 决策汇总表

| Gate | 决策项 | 决策 |
|---|---|---|
| **RSV-001** | 默认真实图片搜索 provider | **SerpApi**（v1.1 已实现 `src/search/serpapi.rs`，凭据 `SERPAPI_API_KEY`） |
| **RSV-002 / MVP-002** | 内置 provider 清单与受限/legacy 策略 | **仅内置 SerpApi**；受限/legacy provider 默认禁用，需显式 opt-in |
| **RSV-003** | 付费抓取渠道（tier 3）启用边界 | **按 fallback 链接入**；配了凭据即可用，不强制每次显式确认 |
| **RSV-004** | robots.txt / 站点规则合规 | **仅记录不阻断**：检测并记录 robots/站点规则状态，但不阻断抓取（用户自担风险） |
| **RSV-005** | 质量档位校准 | **现阈值放行**：接受当前 General/High/Strict 阈值为 MVP 初始值，上线后用真实任务数据迭代 |
| **MVP-001 / MVP-005** | 主观评价生产接入与 wire protocol | **Qwen 3.5 VLM 直连**（DashScope，凭据 `QWEN_API_KEY`）。门禁文档的 "OpenClaw" 为 v1.0 遗留表述，**v1.1 以 Qwen 3.5 VLM 为准**；不定义 OpenClaw 远程 wire protocol |
| **MVP-003** | 授权风险阻塞细则 | **保持当前默认**：未知授权 → warn（保留交付 + 风险提示）；明确禁止 → block（局部拒绝） |
| **MVP-004** | 第四级抓取渠道 | **确认为表述误差，定为三级**：渠道分类锁定 `web_fetch → self_hosted → paid`，不新增第四级 |

> 注：本批决策与 v1.0 `production-gate-decisions.md` 高度一致，仅两处随 v1.1 实现调整：
> 默认搜索 provider 由 Brave 改为 **SerpApi**；主观评价由 OpenClaw 改为 **Qwen 3.5 VLM**——
> 因为 v1.1 实际交付的就是这两个 adapter。

---

## 2. 各决策的实现影响

### RSV-001 / RSV-002 / MVP-002 — 搜索 provider
- `src/search/serpapi.rs` 已实现 `BaseProvider`（SerpApi Google Images），定为默认生产 provider。
- `ProviderRegistry`（`src/search/registry.rs`）默认仅注册 SerpApi；受限/legacy provider 需显式 opt-in。
- 凭据外部化：`SERPAPI_API_KEY` 从环境变量读取，禁止硬编码（现有敏感信息检查已通过）。

### MVP-001 / MVP-005 — 主观评价（Qwen 3.5 VLM）
- v1.1 已实现 Qwen 3.5 VLM adapter（`OpenClawEvaluationPort` trait 的生产实现），直连 DashScope，覆盖候选评价与图片评价两个边界。
- 凭据 `QWEN_API_KEY` + endpoint `QWEN_API_BASE_URL`（DashScope）。
- **不定义 OpenClaw 远程 wire protocol**（MVP-005 据此关闭——v1.1 用 Qwen 直连，非 OpenClaw）。
- 保留执行阻塞语义：VLM 不可用时任务进入 `execution_blocked`，**不得降级为 fixture/mock**（现有实现一致）。

### RSV-003 — 付费渠道
- `paid` channel（tier 3）按 fallback 链 `web_fetch → self_hosted → paid` 接入。
- 配了付费渠道凭据即视为可用（`paid_unconfirmed` → 配置后就绪），不强制每次任务显式确认。
- 仍遵循「优先免费高效通道」：付费仅作 fallback 末级。

### RSV-004 — robots / 合规
- 检测并在诊断中记录 robots.txt / 站点规则状态（复用 `is_access_restricted` 标记），**但不阻断抓取**。
- ⚠️ 此决策较宽松，合规风险由用户承担；后续面向严格合规场景可收紧为「明确禁止则拒抓」。

### RSV-005 — 质量档位
- 当前 General/High/Strict 阈值原样放行作为 MVP 初始值，上线后基于真实任务反馈迭代，发布前不额外校准。

### MVP-003 — 授权风险
- 维持现有实现：未知授权 → warn；明确禁止 → block（局部拒绝）。已有测试覆盖，无需改动。

### MVP-004 — 渠道级数
- `RetrievalChannelTier`（`src/domain/retrieval.rs`）保持三变体，fallback 链不变，不新增第四级。

---

## 3. 关闭后的实现/验证路径

1. 真实服务冒烟（独立环节）：配 `SERPAPI_API_KEY` + `QWEN_API_KEY` + `QWEN_API_BASE_URL`，跑完整任务流程（SerpApi 搜索 + Qwen 评价 + web fetch 抓取 + 包校验）。
2. 验证 channel fallback 边界（含付费末级）。
3. 更新 self-check 反映真实生产配置就绪状态。
4. 将 `real-service-smoke-report.json` 从 `skipped` 更新为实际验证结果，并把 acceptance-report 的 VERDICT 从 BLOCKED 更新为实际结论。

---

## 4. Post-MVP 延期项（维持 DEFERRED）

| ID | 问题 |
|---|---|
| OQ-001 | 单个 QueryPlan 最大交付数量是否需要限制 |
| OQ-002 | 交付结果是否需要按授权风险分组展示 |

---

## 5. 与 v1.0 决策的关系

本清单是 v1.1 的独立门禁决策（门禁由 TASK-009 重新生成）。与 v1.0 `tasks/development/production-gate-decisions.md` 的差异仅两处，均因 v1.1 的具体技术实现：
- 默认搜索 provider：Brave → **SerpApi**
- 主观评价：OpenClaw → **Qwen 3.5 VLM**

其余 8 项决策与 v1.0 保持一致。
