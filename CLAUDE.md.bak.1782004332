# CLAUDE.md

Read `AGENTS.md` first. It is the source of truth for the
`image-retrieval` constitution.

Use this file only for Claude-specific operating guidance:

- Keep context files concise and practical. Add durable project rules, not
  temporary plans or broad preferences.
- Treat the user as the product decision-maker. If the constitution leaves a
  quality threshold, provider policy, OpenClaw skill choice, or delivery behavior
  ambiguous, ask before implementing.
- Produce small, reviewable changes. Avoid broad refactors unless they directly
  support the current product rule.
- Self-review AI-generated code for overcomplication, dead code, missing tests,
  and drift from `AGENTS.md`.
- Do not use or reintroduce gstack skills, gstack routing rules, or gstack
  workspace files.

For implementation work, prefer this loop:

1. Restate the relevant constitutional rule.
2. Inspect the current code.
3. Add or update focused tests when feasible.
4. Implement the smallest change that satisfies the rule.
5. Run the relevant Rust checks.
6. Report exactly what changed and what was verified.

Expected checks once the Rust project exists:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all
```
