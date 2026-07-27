# 03 - Low-code generator and docs

## Scope

Expose wire-level conformance declarations in the documentation generator without mixing them into
SDK capability or OpenAI API profile declarations.

## Tasks

- [x] Add wire-level target manifest examples.
- [x] Add fixture structs and tests for wire-level conformance JSON.
- [x] Keep target examples aligned with the preview4 TCP, QUIC, IPC, and WebSocket transport set.
- [x] Publish schema documentation for the wire target, execution plan, and result report.
- [x] Keep the docs generator synchronized with `wire-conformance/nnrp-1-preview4/manifest.json`.
- [x] Add migration notes explaining when to use adapter execution versus wire-level execution.
- [x] Extend the wire target generator for host-route scenarios.
  - [x] Generate one application endpoint plus transport-keyed client or server routes.
  - [x] Keep route-local locators separate from the application endpoint.
  - [x] Represent credential ownership and security mode without serializing secret bytes.
  - [x] Generate multi-route client and multi-listener server examples.
  - [x] Generate expected selection, rejection, bind, bound-endpoint, rollback, listener-failure, and active-transport evidence.
  - [x] Generate known-but-uninstalled and combined-failure rejection-precedence cases.
- [x] Publish the host-route schema and scenario documentation in both languages.
- [x] Add generator snapshot tests for every native carrier and browser WSS.

## Exit criteria

- The documentation generator can output a wire target manifest.
- The generator remains a third manifest type, separate from adapter and OpenAI API profile manifests.
