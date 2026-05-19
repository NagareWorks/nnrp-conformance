# Preview3 Bootstrap Todo

## Repository Bootstrap

- [x] Initialize an isolated GitHub-backed repository.
- [x] Create the Rust workspace and split it into `fixtures` and `runner` crates.
- [x] Add GitHub Actions CI for formatting, linting, tests, and baseline summary.
- [ ] Add branch protection and required status checks on GitHub.
- [x] Publish the first repository README and contribution guidance.

## Public Manifest Contracts

- [x] Define the protocol manifest shape.
- [x] Define the case-manifest shape.
- [x] Define the capability-manifest shape.
- [x] Define the report shape.
- [x] Add schema validation as a first-class CI step rather than relying only on Rust deserialization.
- [x] Add a vector-manifest schema once Preview3 golden vectors freeze.

## Preview3 Minimum Mandatory Baseline

- [x] Create the first `nnrp-1-preview3` protocol manifest.
- [x] Add an initial mandatory core case manifest.
- [x] Freeze the exact Preview3 mandatory L0 header and fixed-metadata cases.
- [x] Freeze the exact Preview3 mandatory L0 invalid-length and reserved-bit error cases.
- [x] Freeze the exact Preview3 mandatory L1 handshake and capability-negotiation cases.
- [x] Freeze the exact Preview3 mandatory L1 session open / close cases.
- [x] Freeze the exact Preview3 mandatory L1 submit/result minimum interoperability cases.
- [x] Split currently broad placeholders into atomic case files once the wire and state-machine edges stop moving.

## Capability-Gated Development Workflow

- [x] Add a capability manifest example for partial implementation bring-up.
- [x] Make the runner distinguish selected cases from not-claimed cases.
- [x] Add explicit `optional` coverage examples where a feature is valid to omit.
- [x] Add explicit `experimental` coverage examples that report but do not gate.
- [x] Add a machine-readable compatibility matrix output for dashboards and release notes.

## Adapter Integration

- [x] Freeze the exporter-vs-adapter boundary in `nnrp-doc` before repository-local adapter work.
- [x] Define the suite-owned adapter execution-plan JSON contract.
- [x] Define the suite-owned case-result report JSON contract.
- [x] Add example adapter input/output payloads that third-party implementations can follow without reading Rust internals.
- [ ] Define the adapter command contract for `nnrp-rs`.
- [ ] Define the adapter command contract for `nnrp-py`.
- [ ] Define the adapter command contract for `nnrp-cs`.
- [x] Document how third-party implementations consume the baseline without depending on Rust internals.

## Preview3 Execution Layers

- [x] Land the first L0 byte-level golden vectors.
- [x] Land the first L1 state-machine scenario scripts.
- [x] Decide which Preview3 items are optional rather than mandatory.
- [x] Define whether any Preview3 L2 binding checks are stable enough to freeze now.
- [x] Keep L3 integration smoke outside the mandatory set until L0-L1 semantics stop moving.
- [x] Keep L4 performance checks out of the protocol pass/fail gate for the first bootstrap.

## Reporting And Release Discipline

- [x] Emit a stable JSON report file from the runner.
- [x] Define CI exit rules for `mandatory`, `optional`, `experimental`, and `deprecated` cases.
- [x] Tag Preview3 baseline revisions independently from SDK release tags.
- [x] Document how a new preview line is added without rewriting historical baselines.
