# 02 - Wire-level conformance

## Scope

Add an active conformance path where the suite can simulate a client, server, or proxy and exchange
NNRP frames directly.

## Tasks

- [x] Add a suite manifest for `wire-conformance/nnrp-1-preview4`.
- [x] Add scenario manifests for client, server, and proxy runner modes.
- [x] Add a target manifest schema for live endpoints.
- [x] Add an execution-plan schema for suite-owned wire plans.
- [x] Add a case-results schema for observed frame reports.
- [ ] Implement the runner that can drive TCP and QUIC endpoints directly.
- [ ] Add timeout, close, backpressure, and frame-order injection in proxy mode.
- [ ] Add CI examples that run against a local reference endpoint.

## Exit criteria

- A target implementation can be tested without calling its own SDK adapter.
- Result reports contain observed frames, terminal state, failure kind, timing, and evidence paths.
