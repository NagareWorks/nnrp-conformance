# 01 - Runtime object and control protocol

## Scope

Freeze the preview4 protocol work that moves NNRP from token-only streaming toward runtime object
and control semantics.

## Tasks

- [x] Define the preview4 control-frame capability catalog.
- [x] Define the preview4 runtime object capability catalog.
- [x] Keep cache reference as an optional object-lifecycle strategy rather than a universal latency
      promise.
- [x] Add canonical semantic vectors for the listed control and runtime object frames, including the
      shared cache namespace and 128-bit cache identity across hot-path and control-plane layouts.
- [x] Split the listed capability tokens into mandatory, optional, and experimental groups.

## Exit criteria

- Protocol docs describe the preview4 profile families.
- Conformance manifests can refer to preview4 control and object capability tokens.
