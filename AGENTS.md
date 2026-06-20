# AGENTS.md

Purpose: keep future agent work aligned with the product constitution for this
repo. Keep this file short, concrete, and durable. Do not add generic coding
advice, temporary plans, or tool-specific routing rules.

## Prime Directive

`image-retrieval` is a Rust CLI for general-purpose image search, retrieval,
validation, and delivery packaging.

All product, architecture, and implementation work must follow the constitution
below. If a requirement is unclear, underspecified, or conflicts with a proposed
change, stop and escalate to the user for a decision.

Do not use or reintroduce gstack skills, gstack routing rules, or gstack
workspace files in this repository.

## Product Flow

The CLI workflow is:

1. Accept a `QueryPlan`.
2. Search for image candidates from pluggable image search engine providers.
3. Mechanically validate and subjectively rank candidates.
4. Retrieve candidates in batches through pluggable retrieval channels.
5. Mechanically validate and subjectively evaluate retrieved images.
6. Build a delivery package.

## QueryPlan

The core of a `QueryPlan` is the semantic description of the desired image.

It must also include:

- Required image count.
- Quality requirements.
- Defaults for count and quality when omitted.

Candidate search should target roughly 20 candidates per required delivered
image. If one requirement asks for multiple delivered images, increase the
candidate target accordingly.

## Search Providers

Image search must use configurable, pluggable providers, such as Brave Image
Search or other external image search services.

Design rules:

- Define a `BaseSearchProvider` contract for all providers.
- Any provider satisfying that contract can be plugged in.
- Multiple providers are selected with weighted random scheduling.
- Provider configuration must be externalized; do not hard-code credentials.

## Candidate Validation

After search, candidates must be filtered and ranked with both mechanical
validation and subjective evaluation.

Mechanical validation has two metric classes:

- Blocking metrics: decide accept or discard.
- Reference metrics: provide evidence for subjective evaluation.

Subjective evaluation must call an OpenClaw agent to execute the relevant skill.

## Retrieval Channels

Batch retrieval must use pluggable retrieval channels with fallback levels:

1. Normal web fetch.
2. Open-source self-hosted services.
3. Paid online services.

Prefer free and efficient channels first. Fall back to higher levels only when
needed.

Each retrieval batch should attempt:

`QueryPlan.required_image_count * 2`

candidates.

## Image Acceptance

Retrieved images must pass both mechanical acceptance and subjective evaluation.

Mechanical acceptance has two metric classes:

- Blocking metrics: decide accept or discard.
- Reference metrics: provide evidence for subjective evaluation.

Subjective evaluation must call an OpenClaw agent to execute the relevant skill.
An image counts as accepted only when both checks pass.

## Completion Rules

A `QueryPlan` is fully delivered only when accepted images reach the requested
count.

If accepted images are insufficient, repeat the whole workflow until either:

- The requested count is reached.
- The initial attempt plus up to 3 retries have been made.

After the retry limit, perform limited delivery with the accepted images that
exist. In product and implementation documents, distinguish `retry_count` from
`full_attempt_count`: the constitution allows 1 initial attempt and 3 retries.

## Engineering Rules

- Rust is the implementation language.
- Build a CLI first; do not create a web app unless the user changes scope.
- Keep provider, retrieval channel, validation, evaluation, and packaging
  boundaries explicit and independently testable.
- Prefer traits and structured data models over ad hoc conditionals.
- Write tests for product rules before implementation changes when feasible.
- Do not claim verification unless the command was actually run.

Expected Rust commands once the Cargo project exists:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all
```

If the Cargo project does not exist yet, say so instead of pretending these
commands ran.
