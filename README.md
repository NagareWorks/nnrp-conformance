# nnrp-conformance

Canonical conformance baseline for versioned NNRP protocol lines.

This repository keeps the protocol-facing conformance contract separate from any single SDK repository. The core runner and fixture tooling live in Rust so the baseline can stay strict at the byte and state-machine level, while the public artifacts remain language-neutral JSON.

## Scope

This repository owns:

1. Versioned protocol baselines such as `protocol/nnrp-1-preview2` and `protocol/nnrp-1-preview3`.
2. Public machine-readable manifests for cases, capabilities, and reports.
3. A Rust reference runner that loads a selected baseline, summarizes capability coverage, generates canonical vector manifests, verifies recipe determinism, and compares SDK-exported manifests against canonical output.
4. A suite-owned GitHub composite action that executes the canonical conformance flow for SDK repositories.
5. CI checks that keep the repository itself buildable and the published baselines internally consistent.

This repository does not own host-language API ergonomics, runtime-private test harnesses, or repository-local regressions for one SDK.

## Layout

1. `crates/nnrp-conformance-fixtures`: shared JSON-backed manifest/report types.
2. `crates/nnrp-conformance-runner`: Rust runner and CLI entrypoint.
3. `protocol/`: versioned protocol baselines and canonical manifests.
4. `schemas/`: JSON schema files for public manifests and reports.
5. `docs/todo/`: repository-local execution backlog, split by protocol line and workstream.

## Current Suite Boundary

The current repository state establishes the shared conformance entrypoint described by the protocol design docs:

1. Multiple versioned protocol baselines can coexist under `protocol/`.
2. Capability manifests decide which `mandatory` and `optional` cases are actually selected for a given implementation.
3. Recipe-backed canonical vector manifests can be generated and deterministically verified inside the suite.
4. SDK repositories integrate through the suite-owned `run-conformance` action plus an SDK exporter command, not by embedding suite conformance into local pytest/xUnit coverage jobs.

## Local Commands

Run the full local verification set:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Print an execution-plan summary against a versioned sample capability manifest:

```bash
cargo run -p nnrp-conformance-runner -- \
  summary \
  --protocol protocol/nnrp-1-preview2/manifest.json \
  --capabilities protocol/nnrp-1-preview2/example-capabilities.json
```

The `summary` command emits the public conformance report shape defined by `schemas/report.schema.json`. It is not a capability manifest and should never be stored or labeled as one.

Generate and verify the canonical vector manifest from a recipe-backed baseline:

```bash
cargo run -p nnrp-conformance-runner -- \
  generate-vectors \
  --recipe protocol/nnrp-1-preview2/vectors/semantic-vectors.json \
  --output artifacts/local-preview2-vectors.json

cargo run -p nnrp-conformance-runner -- \
  verify-vectors \
  --recipe protocol/nnrp-1-preview2/vectors/semantic-vectors.json \
  --manifest artifacts/local-preview2-vectors.json
```

Compare an SDK-exported manifest against the canonical manifest:

```bash
cargo run -p nnrp-conformance-runner -- \
  compare-vector-manifests \
  --expected artifacts/local-preview2-vectors.json \
  --actual /path/to/sdk-vector-manifest.json
```

## CI Contract

CI must never infer the target protocol line from branch naming or implementation code shape. It must always select an explicit baseline. In the suite repository, that means dynamically enumerating `protocol/*/manifest.json` and running the same suite-owned action against each baseline. In SDK repositories, that means using the suite-owned `run-conformance` action in a dedicated `conformance` job. The suite then verifies that:

1. The protocol manifest version matches the case manifest version.
2. The implementation capability manifest declares the same protocol version.
3. Only claimed capabilities are promoted into the runnable mandatory/optional set.

That rule is the core mechanism that allows development-time testing to stay aligned with completed capabilities rather than with guessed implementation progress.
