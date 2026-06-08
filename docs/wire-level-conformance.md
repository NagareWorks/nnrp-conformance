# Wire-level conformance

Wire-level conformance is the preview4 path for testing NNRP protocol behavior without calling an
implementation-owned SDK adapter. The suite can act as a client, server, or proxy and exchange NNRP
frames directly with a live target.

Adapter execution remains useful for SDK ergonomics and language bindings. Wire-level execution is
the stricter semantic check: it verifies that independent implementations agree on frame ordering,
terminal states, backpressure, cancellation, drop reasons, and trace propagation at the protocol
boundary.

## Public documents

| Document | Schema | Owner | Purpose |
| --- | --- | --- | --- |
| Wire suite manifest | `schemas/wire-conformance-suite.schema.json` | Suite | Freezes the preview4 runner modes, transport set, scenario manifests, and result schemas. |
| Wire scenario manifest | `schemas/wire-conformance-scenario.schema.json` | Suite | Lists frame-level scenarios selected by target mode, transport, and capability tokens. |
| Wire target manifest | `schemas/wire-conformance-target.schema.json` | Target implementation | Declares live endpoints and capabilities exposed to the wire runner. |
| Wire execution plan | `schemas/wire-conformance-execution-plan.schema.json` | Runner | Contains the scenarios selected for a target and the expected artifact locations. |
| Wire case results | `schemas/wire-conformance-case-results.schema.json` | Runner or target harness | Reports observed frames, terminal state, timing evidence, and failure messages. |

The preview4 transport set is frozen as `tcp`, `quic`, `ipc`, and `websocket`. Implementations may
declare only the transports they actually expose. The plan builder selects scenarios only when the
target manifest declares the required mode, transport, and capability tokens.

## Target manifest

Target manifests are live-endpoint declarations. They are not capability manifests for SDK adapter
execution and they are not API profile capability manifests.

```json
{
  "$schema": "../../schemas/wire-conformance-target.schema.json",
  "target_name": "sample-preview4-target",
  "protocol_version": "nnrp-1-preview4",
  "suite_version": "0.1.0",
  "wire_conformance": {
    "modes": ["suite_as_client", "suite_as_server", "suite_as_proxy"],
    "transports": [
      { "name": "tcp", "endpoint": "127.0.0.1:19091", "tls": false },
      { "name": "quic", "endpoint": "127.0.0.1:19092", "tls": false },
      { "name": "ipc", "endpoint": "npipe://nnrp-preview4-sample", "tls": false },
      { "name": "websocket", "endpoint": "ws://127.0.0.1:19093/nnrp", "tls": false }
    ],
    "capabilities": ["control.cancel_abort", "control.trace_context"],
    "limits": {
      "max_frame_bytes": 16777216,
      "max_in_flight": 256
    }
  }
}
```

Endpoint strings are transport-owned. TCP and QUIC use host/port endpoints. IPC endpoints may use
platform-specific URI schemes such as `npipe://` or `unix://`. WebSocket endpoints use `ws://` or
`wss://`.

## Execution plan

The runner builds a suite-owned execution plan from the suite manifest, target manifest, and scenario
manifests.

```bash
cargo run -p nnrp-conformance-runner -- \
  wire-plan \
  --suite wire-conformance/nnrp-1-preview4/manifest.json \
  --target docs/examples/wire-conformance-target.sample.json \
  --output artifacts/wire-plan.json \
  --results-path artifacts/wire-results.json \
  --evidence-dir artifacts/wire-evidence
```

The plan contains only selected scenarios. A scenario is selected when all three gates match:

1. The target declares the scenario mode, such as `suite_as_client`.
2. The target declares the scenario transport, such as `ipc`.
3. The target declares every required capability token.

## Result report

Result reports are machine-readable observations for the selected plan. They must keep the scenario
IDs from the plan exactly, report the terminal state, and include enough observed frame evidence for
the suite to validate expected frames.

```bash
cargo run -p nnrp-conformance-runner -- \
  validate-wire-results \
  --plan artifacts/wire-plan.json \
  --results artifacts/wire-results.json
```

Result reports should include evidence paths for frame logs, timing traces, or packet captures when
available. The public result contract is intentionally JSON-shaped so SDKs and third-party targets do
not need to link against Rust crate internals.

## Adapter execution versus wire execution

Use adapter execution when the implementation only exposes an SDK-level entrypoint or when the goal
is to validate language binding semantics. Adapter execution consumes a capability manifest and an
adapter execution plan.

Use wire execution when the implementation exposes a live NNRP endpoint and the goal is to prove
cross-implementation protocol semantics. Wire execution consumes a target manifest and a wire
execution plan.

The two paths should not be collapsed:

1. Adapter execution can hide protocol drift if the adapter encodes local assumptions.
2. Wire execution can validate protocol behavior without knowing the target's SDK API.
3. Benchmark execution stays informational and should not become a correctness gate.

## Current implementation boundary

The current runner can build and validate wire execution plans and result reports. Live endpoint
driving is intentionally tracked as a later preview4 implementation step, because it depends on
reference TCP, QUIC, IPC, and WebSocket transport endpoints.
