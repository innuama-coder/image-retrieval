# v1.1 Test Fixtures

Fixture directory for TASK-006 testing, providing QueryPlans, configs, provider responses, package fixtures, and golden outputs.

## Directory Layout

```
tests/fixtures/v1_1/
├── README.md
├── query-plans/
│   ├── query-plan-basic.json            # Basic QueryPlan (required_image_count=1)
│   ├── query-plan-invalid-empty-description.json  # Invalid: empty description
│   └── query-plan-high-quality-multi.json  # High quality, multi-image (count=2)
├── configs/
│   ├── config-fixture.toml              # Fixture-mode config
│   ├── config-minimal.toml              # Minimal empty config
│   └── config-production-like.toml      # Production-like template (no secrets)
├── provider-responses/
│   └── serpapi-google-images-success.json  # Sample SerpApi image_results[]
├── packages/
│   ├── passed_minimal/                  # Positive: fully valid canonical package
│   ├── missing-canonical-file/          # Negative: missing required file
│   ├── invalid-json/                    # Negative: invalid JSON
│   ├── metadata-only-delivered/         # Negative: metadata-only delivery
│   ├── checksum-missing/                # Negative: missing checksum
│   ├── coverage-count-mismatch/         # Negative: coverage count mismatch
│   ├── retry-counter-invalid/           # Negative: retry counter invariant violation
│   ├── broken-manifest-link/            # Negative: broken artifact path
│   └── secret-leak/                     # Negative: seeded fake secret
└── golden/
    ├── self-check-fixture-ready.json    # Golden: self-check output (ready)
    ├── self-check-blocked-no-providers.json  # Golden: self-check output (blocked)
    ├── validate-package-passed-minimal.json  # Golden: validate-package pass
    └── validate-package-failed-metadata-only.json  # Golden: validate-package fail
```

## Usage

These fixtures are referenced by:
- Unit, integration, and E2E tests in `tests/`
- CLI golden tests
- Package validation tests
- Security/redaction tests

## Rules

- No real credentials or secret values anywhere in this tree.
- Fixture packages carry `fixture_evidence: true` and are never production acceptance.
- Golden files use stable paths for comparison.
