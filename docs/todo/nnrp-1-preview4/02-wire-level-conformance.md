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
- [x] Add suite-owned host-route scenarios without calling the target's own peer adapter.
  - [x] Add a multi-route client plan with at least two simultaneously available suite endpoints.
  - [x] Assert deterministic candidate diagnostics and one selected runtime carrier.
  - [x] Add forced unresolved and security-incompatible client plans with no fallback.
  - [x] Add a multi-listener server plan and connect through every declared listener.
  - [x] Assert every actual bound provider endpoint.
  - [x] Assert active transport identity for each accepted server session.
  - [x] Inject one server bind failure and assert atomic rollback evidence.
  - [x] Inject one terminal listener failure and assert the logical set closes instead of shrinking.
  - [x] Add native and browser `nnrps://` security-intent matrices.
  - [x] Add known-but-uninstalled route and combined-failure rejection-precedence cases.
- [x] Extend target, execution-plan, and case-result schemas for route-set evidence.
  - [x] Represent application endpoint and transport-keyed provider routes separately.
  - [x] Record route-local locator and credential ownership without embedding secrets.
  - [x] Record every candidate rejection reason and selected transport.
  - [x] Record every opened, rolled-back, accepted, and closed server listener.
  - [x] Record every actual bound provider endpoint and terminal listener-set failure.
- [x] Add CI reference targets proving the host-route scenarios fail for singular-role implementations.

## Exit criteria

- A target implementation can be tested without calling its own SDK adapter.
- Result reports contain observed frames, terminal state, failure kind, timing, and evidence paths.
