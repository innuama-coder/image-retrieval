# v1.1 Post-Fix Review Issues

Review date: 2026-06-24

Reviewed branch: `codex/v1.1-production-pipeline-fix`

Reviewed commit: `54cbfe16722580c8060e223e29e21bb8abf2dfa8`

## Product Features Not Implemented

1. Tier 2 and tier 3 retrieval fallback adapters remain unimplemented.

   The product constitution requires retrieval fallback levels for normal web
   fetch, open-source self-hosted services, and paid online services. The v1.1
   implementation has a production normal web fetch channel, but
   `SelfHostedChannel` and `PaidChannel` still report unavailable even when
   configured with endpoint and credentials because the real service adapters
   are not implemented.

   Evidence:

   - `src/retrieval/channels/self_hosted.rs`
   - `src/retrieval/channels/paid.rs`

## Code Bug Findings

No new blocking code bugs were found in the reviewed fix commit.

The previously reported retry and fallback defects are covered by the latest
changes:

- Non-retryable blockers now stop additional full workflow retries.
- Partial package validation failures now produce a hard failure.
- Search scheduler attempt metadata now uses the active production attempt
  context.
- Retrieval fallback now retries only pending jobs instead of re-running the
  whole batch.

## Verification Notes

- `git diff --check origin/main...HEAD` passed in the review worktree before
  merge.
- Rust verification was not executed because `cargo` was not available in the
  current environment.
