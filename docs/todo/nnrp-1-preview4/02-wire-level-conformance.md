# 02 - Wire-level conformance

## Scope

Add an active conformance path where the suite can simulate a client, server, or proxy and exchange
NNRP frames directly.

## Tasks

- [x] Add a suite manifest for `wire-conformance/nnrp-1-preview4`.
- [x] Add scenario manifests for client, server, and proxy runner modes.
- [x] Add a target manifest schema for live endpoints.
- [x] Add an execution-plan schema for target-selected wire scenarios.
- [x] Add a case-results schema for observed frame reports.
- [x] Freeze TCP, QUIC, IPC, and WebSocket as preview4 wire target transports.
- [x] Add IPC and WebSocket scenario coverage so they are selectable by target manifests.
- [x] Add CI coverage for wire plan generation and result validation against an independent target process.
- [x] Implement the runner that drives declared TCP, QUIC, IPC, and WebSocket target endpoints directly.
- [x] Add timeout, close, backpressure, and frame-order injection evidence in proxy mode.
- [x] Add CI examples that launch the reference target as a separate process and exercise all selected roles over live endpoints.
- [x] Validate that cache-reference scenarios preserve `cache_namespace`, `cache_key_hi`, and
      `cache_key_lo` without collapsing the 128-bit key into a text alias.

## Exit criteria

- A target implementation can be tested without calling its own SDK adapter.
- Result reports contain observed frames, terminal state, failure kind, timing, and evidence paths.
