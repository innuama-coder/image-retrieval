# Claude Task Instructions: TASK-003

## Role

Claude 只实现搜索 provider 端口、注册表和加权调度。

## Required Reading

阅读 `docs/design/TASK-003-base-provider-search-design.md`、PRD/HLD 相关段落、规划 JSON 和本任务夹具。

## Constraints

不得实现未决真实 provider，不得硬编码凭据，不得处理候选主观评价或图片验收。

## Verification

运行 `cargo test --all search` 以及任务要求的格式和 clippy 检查。

## Final Response

说明搜索合同、权重规则、候选来源、验证结果和未决风险。
