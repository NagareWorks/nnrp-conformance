# nnrp-conformance

Canonical conformance baseline for versioned NNRP protocol lines.

This repository keeps the protocol-facing conformance contract separate from any single SDK repository. The core runner and fixture tooling live in Rust so the baseline can stay strict at the byte and state-machine level, while the public artifacts remain language-neutral JSON.

## Scope

This repository owns:

1. Versioned protocol baselines such as `protocol/nnrp-1-preview3`.
2. Public machine-readable manifests for cases, capabilities, and reports.
3. A Rust reference runner that loads the versioned baseline and produces execution-plan/report outputs.
4. CI checks that keep the repository itself buildable and the published baseline internally consistent.

This repository does not own host-language API ergonomics, runtime-private test harnesses, or repository-local regressions for one SDK.

## Layout

1. `crates/nnrp-conformance-fixtures`: shared JSON-backed manifest/report types.
2. `crates/nnrp-conformance-runner`: Rust runner and CLI entrypoint.
3. `protocol/`: versioned protocol baselines and canonical manifests.
4. `schemas/`: JSON schema files for public manifests and reports.
5. `docs/todo/`: repository-local execution backlog, split by protocol line and workstream.

## Current Bootstrap Boundary

The initial repository bootstrap establishes the minimum Preview3 baseline contract:

1. A versioned Preview3 protocol manifest.
2. A first mandatory case manifest for the minimum frozen core.
3. A capability-manifest contract so implementations only run cases for declared capabilities.
4. A report contract so CI can prove which protocol line was selected.

The initial runner intentionally stops at plan construction and version-alignment checks. It does not yet execute transport flows or byte-vector assertions.

## Local Commands

Run the full local verification set:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Print a Preview3 execution-plan summary against the sample capability manifest:

```bash
cargo run -p nnrp-conformance-runner -- \
  summary \
  --protocol protocol/nnrp-1-preview3/manifest.json \
  --cases protocol/nnrp-1-preview3/cases/mandatory-core.json \
  --capabilities protocol/nnrp-1-preview3/example-capabilities.json
```

## CI Contract

CI must never infer the target protocol line from branch naming or implementation code shape. It must always select an explicit baseline, then verify that:

1. The protocol manifest version matches the case manifest version.
2. The implementation capability manifest declares the same protocol version.
3. Only claimed capabilities are promoted into the runnable mandatory/optional set.

That rule is the core mechanism that allows development-time testing to stay aligned with completed capabilities rather than with guessed implementation progress.
