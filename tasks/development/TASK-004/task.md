# 开发任务合同：TASK-004 候选质量门禁与 OpenClaw 候选评价实现

本文描述未来开发任务，不表示任务已经完成。

## Source Refs

- `docs/PRD.md:102-110`
- `docs/PRD.md:205`
- `docs/HLD.md:219-233`
- `docs/design/TASK-004-candidate-quality-openclaw-design.md`

## Planning Status

ready

## Dependencies

- TASK-001
- TASK-002
- TASK-003

## Downstream Consumers

TASK-005、TASK-006、TASK-007、TASK-008、TASK-009、TASK-011、TASK-010。

## Allowed Scope

- 实现候选机械校验的阻塞证据和参考证据。
- 实现结构化候选评价请求边界。
- 实现 OpenClaw 候选评价端口的高层结论归一。
- 实现 `RetrievableCandidateSequence`。
- 提供内部 fixture evaluator，但生产路径不得把 mock 当作通过依据。

## Forbidden Scope

- 不下载图片。
- 不做最终图片验收。
- 不把不确定候选放入抓取批次。
- 不用 mock/fixture 替代生产 OpenClaw。

## Expected Outputs

- `CandidateMechanicalEvidence`
- `CandidateEvaluationRequest`
- `CandidateEvaluationConclusion`
- `CandidateDecision`
- `RetrievableCandidateSequence`
- 候选质量测试。

## Acceptance Criteria

- 机械阻塞候选不进入 OpenClaw。
- 参考信息进入 OpenClaw 候选评价请求。
- 只有机械未阻塞且 OpenClaw 明确认可的候选可抓取。
- OpenClaw 拒绝和不确定候选不进入批次。
- OpenClaw 不可执行形成执行阻塞事实。
- 候选评价不决定最终交付。

## Verification

- `cargo fmt --all -- --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all candidate_quality`

## Stop Conditions

- 如果必须确定 OpenClaw 生产协议、技能选择或责任边界，停止并请求用户决策。

## Handoff Requirements

移交可抓取候选序列、候选拒绝证据、OpenClaw 候选事件和执行阻塞事实。
