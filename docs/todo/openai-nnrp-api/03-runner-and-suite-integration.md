# Runner And Suite Integration

## Loader

- [x] Add loader support for API profile manifests without mixing them into protocol baseline manifests.
- [x] Keep protocol baselines under `protocol/<version>/`.
- [x] Add API profile fixtures under a separate profile-owned path.
- [ ] Make API profile selection depend on both the selected protocol baseline and the API capability manifest.

## Plan Generation

- [x] Add an `api-profile-plan` runner command.
- [x] Select mandatory Level 1 cases when `openai-compatible/1` Level 1 is claimed.
- [x] Select optional cases only when the operation or extension is claimed.
- [ ] Preserve not-claimed cases as compatibility matrix entries rather than failures.
- [x] Emit a stable API profile execution plan JSON artifact.

## Result Validation

- [x] Add a `validate-api-profile-results` runner command.
- [x] Validate event ordering for streaming recipes.
- [x] Validate required event fields without requiring provider-private fields.
- [ ] Validate error bodies and cancellation terminal outcomes.
- [x] Validate that non-critical extension fields do not affect baseline pass/fail.
- [ ] Emit compatibility matrix output for API profile coverage.

## GitHub Action

- [ ] Extend the suite-owned action with optional API profile inputs.
- [ ] Keep existing protocol conformance behavior unchanged when API profile inputs are absent.
- [ ] Upload API profile plan and result artifacts separately from protocol adapter artifacts.
