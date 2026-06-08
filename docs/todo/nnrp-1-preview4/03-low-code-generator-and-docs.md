# 03 - Low-code generator and docs

## Scope

Expose wire-level conformance declarations in the documentation generator without mixing them into
SDK capability or OpenAI API profile declarations.

## Tasks

- [x] Add wire-level target manifest examples.
- [x] Add fixture structs and tests for wire-level conformance JSON.
- [x] Keep target examples aligned with the preview4 TCP, QUIC, IPC, and WebSocket transport set.
- [ ] Publish schema documentation for the wire target, execution plan, and result report.
- [ ] Keep the docs generator synchronized with `wire-conformance/nnrp-1-preview4/manifest.json`.
- [ ] Add migration notes explaining when to use adapter execution versus wire-level execution.

## Exit criteria

- The documentation generator can output a wire target manifest.
- The generator remains a third manifest type, separate from adapter and OpenAI API profile manifests.
