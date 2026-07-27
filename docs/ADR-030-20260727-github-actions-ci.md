# ADR-030: GitHub Actions CI for Automated Tests and Lint

## Context

Cryptomeria is a dual-language project (Python + Rust) with no automated CI pipeline. All quality gates (lint, test) were run locally via `make check`, with no enforcement on pull requests or merges to main. This created risk of:

- Merging code that fails existing tests
- Forgetting to run linters, leading to stylistic drift
- No visibility into build status for reviewers

A CI pipeline is needed to enforce quality gates automatically on every PR and push to main.

## Options Considered

1. **No CI (status quo)** — rely on local `make check` and manual discipline
   - No enforcement; easy to skip or forget
   - No visibility for reviewers

2. **Single monolithic workflow** — one `ci.yml` running both Python and Rust jobs
   - Simpler file structure
   - Single point of failure; harder to reason about per-language failures
   - Both jobs run even if only one language changed

3. **Separate per-language workflows** — `py-test.yml` and `rust-test.yml`
   - Clear separation of concerns
   - Each workflow independently shows pass/fail
   - Easier to extend (e.g., add caching, matrix builds per language)
   - Reviewer sees language-specific status at a glance

## Decision

Option 3: Two separate workflow files — `py-test.yml` for Python (ruff lint + pytest) and `rust-test.yml` for Rust (clippy lint + cargo test).

Each workflow triggers on `push` to `main` and `pull_request` to `main`, ensuring all proposed changes pass both language quality gates before merge.

## Consequences

### Positive
- Automated enforcement of lint and test on every PR
- Language-specific workflow files are independently readable and maintainable
- Per-language pass/fail visible in the PR status checks UI
- Foundation for future CI extensions (caching, coverage, matrix builds)

### Negative
- Workflow duplication (trigger config, checkout step repeated)
- More files to manage as CI complexity grows
- No cross-language orchestration (e.g., skip Rust tests if only Python changed)

## Status

Accepted

## Related

- Issue #128
