# Contributing to nnrp-conformance

This repository publishes versioned protocol-conformance baselines and the reference runner that consumes them, so contribution flow needs to stay predictable.

## Branch Strategy

`main` is the protected integration branch.

This repository intentionally does not use `develop` or long-lived `release/*` branches for normal work. Unlike SDK repositories, it carries historical compatibility through explicit, append-only protocol baseline directories under `protocol/`.

Use short-lived topic branches for day-to-day work:

- `feature/<scope>-<topic>` for new baseline capabilities, runner features, or adapter contracts
- `fix/<scope>-<topic>` for bug fixes in manifests, schemas, runner logic, or CI
- `docs/<scope>-<topic>` for documentation-only changes
- `chore/<scope>-<topic>` for maintenance, tooling, or repository hygiene

Rules:

- Branch from the latest `main`.
- Keep topic branches focused on one slice of work.
- Rebase or merge from `main` regularly if the branch stays open.
- Merge back to `main` through a pull request.
- Do not push directly to `main`; enforce this with a GitHub ruleset or branch protection rule.
- Do not treat topic branches as canonical protocol baselines.
- This repository does not use long-lived `release/*` branches for normal work; `main` stays current and backward-compatible while historical protocol baselines remain append-only under `protocol/`.

## Commit Message Convention

Use Conventional Commits.

Preferred forms:

- `feat: add preview3 state-machine case manifest`
- `fix: reject mismatched protocol manifest versions`
- `docs: clarify capability manifest selection rules`
- `chore: tighten baseline summary CI`
- `test: add execution-plan regression coverage`
- `refactor: simplify runner selection logic`

Rules:

- Keep the subject line imperative.
- Keep the first line concise.
- Use a scope only when it adds clarity.
- You can use multiple local commits while iterating, but PRs must be squashed to exactly one commit before review.

## Pull Request Expectations

Every PR should:

- target `main`
- use the default GitHub PR template that auto-loads on the PR page; specialized reference variants remain in `.github/PULL_REQUEST_TEMPLATE/` when you need to adapt the structure
- explain the protocol-facing or engineering motivation
- summarize the main manifests, schemas, crates, or CI flows changed
- list the validation performed
- mention public baseline impact when protocol-facing artifacts changed
- contain exactly one commit before review
- pass the `verify` GitHub Actions job before merge

PRs that violate the normal one-commit rule are not reviewed until they are squashed.

## Validation Expectations

Before opening or merging a PR, prefer the narrowest validation that proves the touched slice:

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `python scripts/validate_public_json.py --protocol protocol/<protocol-version>/manifest.json` when changing public schemas, baseline JSON files, or schema-bound examples
- `cargo run -p nnrp-conformance-runner -- summary --protocol protocol/<protocol-version>/manifest.json --capabilities protocol/<protocol-version>/example-capabilities.json` when changing baseline selection, manifests, or report shape
- `cargo run -p nnrp-conformance-runner -- generate-vectors --recipe protocol/<protocol-version>/vectors/semantic-vectors.json --output artifacts/<protocol-version>-vectors.json` and `verify-vectors --recipe ... --manifest ...` when changing recipe-backed vector generation
- `cargo run -p nnrp-conformance-runner -- validate-adapter-results --plan <adapter-plan> --results <adapter-results>` when changing adapter result validation or the suite-owned action

PRs that affect CI, schemas, manifest contracts, or future adapter integration should include the exact command or workflow path used for validation.

## Baseline and Version Discipline

Do not silently rewrite historical protocol baselines. When protocol semantics move, add or update append-only baseline directories intentionally and document why the change preserves backward compatibility for already-published protocol lines.

This repository does not have a package-release or baseline-freeze workflow. Historical compatibility is carried by the checked-in `protocol/<protocol-version>/` directories plus CI that always selects explicit baselines from the current `main` branch.

When adding a new preview line, do not retrofit an older directory into the new protocol shape. Add a new `protocol/<protocol-version>/` directory, keep older directories intact, and let CI discover the additional `manifest.json` naturally.

The core rule of this repository is that CI must always select an explicit protocol baseline. Do not make conformance depend on branch naming, implicit latest-version assumptions, or repository-local implementation state.

## Review Guidelines

Review for:

- protocol and baseline compatibility risk
- schema or manifest contract regressions
- missing tests for changed runner behavior
- CI workflow correctness
- documentation drift when public conformance rules change

Do not start normal feature, fix, docs, or maintenance review while the PR still carries multiple commits.
