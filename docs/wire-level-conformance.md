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
| Wire execution plan | `schemas/wire-conformance-execution-plan.schema.json` | Runner | Contains the scenarios selected for a target, the target provider-availability snapshot, and the expected artifact locations. |
| Wire case results | `schemas/wire-conformance-case-results.schema.json` | Runner or target harness | Reports observed frames, terminal state, timing evidence, and failure messages. |
| Host-route readiness | `schemas/wire-host-route-ready.schema.json` | Host-route driver | Reports the actual provider endpoints after every server listener has bound. |

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
      {
        "name": "quic",
        "endpoint": "127.0.0.1:19092",
        "tls": true,
        "security": {
          "server_name": "localhost",
          "trusted_certificate_der_path": "certs/server.der",
          "certificate_der_path": "certs/server.der",
          "private_key_pkcs8_der_path": "certs/server-key.der"
        }
      },
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

QUIC and secure WebSocket endpoints require all four `security` fields. Security paths are relative
to the target manifest. The trusted certificate and server name authenticate implementation servers
for `suite_as_client` and `suite_as_proxy`; the certificate/private-key pair configures the suite
listener for `suite_as_server`. Plain TCP, IPC, and `ws` endpoints set `tls` to `false` and omit
`security`. In proxy mode, the declared endpoint is the implementation-server upstream; the suite
owns its ephemeral front endpoint and probe client.

Endpoint strings are transport-owned. TCP and QUIC use host/port endpoints. IPC endpoints may use
platform-specific URI schemes such as `npipe://` or `unix://`. WebSocket endpoints use `ws://` or
`wss://`.

## Execution plan

The runner builds a target-specific execution plan from the suite manifest, target manifest, and
scenario manifests. Plan generation does not contact the endpoint; execution does.

```bash
cargo run -p nnrp-conformance-runner -- \
  wire-plan \
  --suite wire-conformance/nnrp-1-preview4/manifest.json \
  --target docs/examples/wire-conformance-target.sample.json \
  --output artifacts/wire-plan.json \
  --results-path artifacts/wire-results.json \
  --evidence-dir artifacts/wire-evidence
```

The plan contains only selected scenarios. A scenario is selected when all applicable gates match:

1. The target declares the scenario mode, such as `suite_as_client`.
2. A frame scenario uses a declared transport, such as `ipc`.
3. A host-route scenario uses provider identities declared by the target, including explicit
   known-but-uninstalled providers used by negative cases.
4. The target declares every required capability token.

## Result report

Result reports are machine-readable observations for the selected plan. Before `wire-run`, the
target process must publish its manifest and keep every declared endpoint live for the roles it
claims. The runner then connects to target listeners in `suite_as_client`, opens suite listeners for
target clients in `suite_as_server`, and places a live suite proxy between its probe and the target
in `suite_as_proxy`.

```bash
cargo run -p nnrp-conformance-runner -- \
  wire-run \
  --plan artifacts/wire-plan.json \
  --target docs/examples/wire-conformance-target.sample.json \
  --host-route-target target/debug/nnrp-wire-host-route-reference-target \
  --output artifacts/wire-results.json
```

Host-route cases use the executable supplied through `--host-route-target` as an independent SDK
driver. The runner writes the frozen scenario plus a resolved copy containing suite-allocated
provider locators, then invokes the driver with paths supplied by `--scenario`,
`--resolved-scenario`, `--output`, `--ready-output`, and `--artifacts`, plus identity values supplied
by `--suite-version` and `--target-name`. The driver must call the SDK's public
multi-route client or multi-listener server API and emit one ordinary
`WireConformanceCaseResultReport`; it does not encode, decode, or relay NNRP frames on the suite's
behalf. A target manifest and each scenario route set must use at most one provider per canonical
transport and must not repeat a provider ID.

A target may expose frame-level transports, host-route providers, or both. The target schema
requires at least one of those execution surfaces, so a host-route-only driver uses an empty
`transports` array together with a non-empty `host_route_providers` array.

For a successful server case, network locators use an OS-assigned port (`:0`). After the SDK has
bound the complete logical listener set, the driver atomically writes a
`WireHostRouteReadyReport` to `--ready-output`. The suite connects only to the actual endpoints in
that report, so parallel runners never depend on a released port reservation.

For client cases, the suite owns every live server endpoint and verifies the selected carrier from
the accepted connection. For server cases, the suite connects through every listener and verifies
rollback or logical-set closure by attempting to reconnect after the driver exits. A selected
host-route case without a driver is a hard error and is never converted to `skipped`.

The report keeps scenario IDs from the plan exactly, reports the terminal state, and includes the
frames observed on the live connection. Every result points to a JSONL evidence file written by the
runner. Scenario timeout hints bound protocol progress; the runner adds a fixed five-second
connection and teardown allowance so process startup is not confused with protocol latency.
`wire-run` writes the complete report before returning a nonzero process status when any selected
scenario fails.

```bash
cargo run -p nnrp-conformance-runner -- \
  validate-wire-results \
  --plan artifacts/wire-plan.json \
  --results artifacts/wire-results.json
```

The public result contract is intentionally JSON-shaped so SDKs and third-party targets do not need
to link against Rust crate internals. Implementations may add packet captures or target-side traces,
but those artifacts supplement rather than replace the runner's observed-frame evidence.

## Independent reference target

Repository CI launches `nnrp-wire-reference-target` as a separate operating-system process. That
process binds TCP, QUIC, IPC, and secure WebSocket endpoints, writes a dynamic target manifest, and
implements the target half of the selected scenarios. `nnrp-conformance-runner` runs independently
against that manifest. The two binaries share only the public wire protocol and manifest contract;
the runner does not call a target adapter or target implementation function.

CI also launches `nnrp-wire-host-route-reference-target` once per selected host-route scenario. The
native installed-provider profile and the isolated uninstalled-QUIC profile together exercise ten
route scenarios through the Rust SDK host APIs. Keeping those profiles separate preserves the
one-provider-per-transport registry contract. Neither profile claims the browser WebSocket provider
identity, so neither can impersonate or accidentally satisfy the browser WSS case; that case is
selected only by a browser-capable target.

Two negative reference targets expose only the client or server host role. CI executes the complete
installed native host-route-only plan against each target, requires supported-role scenarios to
pass, and requires every opposite-role scenario to return a failed case result. The runner and result
validator then verify different properties: `wire-run` must return a failing process status, while
result validation must accept the complete and truthful mixed report. This prevents a singular-role
implementation from being accepted as complete host-route coverage without discarding its evidence.

The same path is available locally:

```powershell
./scripts/run_wire_e2e.ps1 -ArtifactDirectory artifacts/wire-e2e-local
```

The command fails when either process fails, when a selected scenario fails validation, or when the
target process remains alive after all scenarios complete.

## Adapter execution versus wire execution

Use adapter execution when the implementation only exposes an SDK-level entrypoint or when the goal
is to validate language binding semantics. Adapter execution consumes a capability manifest and an
adapter execution plan.

Use wire execution when the implementation exposes a live NNRP endpoint and the goal is to prove
cross-implementation protocol semantics. Wire execution consumes a target manifest and a wire
execution plan.

Host-route plans retain providers that are known but not installed. Such providers are not callable:
the target must keep them unselected and report `local-unavailable`. This lets the suite execute the
negative case and verify that no provider endpoint was contacted instead of silently filtering the
case out during planning.

Fault injection names are suite controls, not public rejection tokens. Their frozen mapping is:

| Suite fixture control | Public evidence |
|---|---|
| `route_unresolved` | `route-unresolved` rejection |
| `security_incompatible` | `security-unsatisfied` rejection |
| `bind_failure` | Atomic listener rollback evidence |
| `terminal_listener_failure` | Terminal listener and logical-set closure evidence |

The public rejection field accepts only the eight values frozen by the transport rejection registry.

The two paths should not be collapsed:

1. Adapter execution can hide protocol drift if the adapter encodes local assumptions.
2. Wire execution can validate protocol behavior without knowing the target's SDK API.
3. Benchmark execution stays informational and should not become a correctness gate.

The priority/deadline proxy case injects `EXPIRE_AT` with absolute Unix millisecond value `1`.
This is a deterministic already-expired timestamp, not a relative one-millisecond duration; `0`
remains invalid/unset under the scheduling metadata contract.

## Current implementation boundary

The current runner has typed executors for all six frame-level preview4 scenarios and all eleven
host-route scenarios. Repository CI selects the six frame scenarios and ten native host-route
scenarios across the nine installed-provider cases and the uninstalled-QUIC target profile, drives
independent target processes over TCP, QUIC, IPC, and secure WebSocket endpoints, and validates both generated result
reports with zero skipped cases. It also runs the nine installed native host-route scenarios
against client-only and server-only targets and requires the unsupported half to fail. The browser
WSS scenario remains part of the mandatory suite and is selected when a target declares the browser
provider identity.

Third-party live endpoints use the same target manifest and execution-plan contract. Adapter
execution remains separate and should not be used to prove wire-level semantics.
