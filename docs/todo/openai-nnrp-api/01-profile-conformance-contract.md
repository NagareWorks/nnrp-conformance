# OpenAI NNRP API Profile Conformance Contract

## Scope

- [x] Treat `openai-compatible/1` as an API profile contract rather than an OpenAI HTTP clone.
- [x] Keep API profile conformance optional for implementations that do not advertise the profile.
- [x] Require profile conformance for adapters that claim `profile = openai-compatible`.
- [x] Validate shared semantic behavior and extension discipline instead of provider-specific model policy.
- [ ] Add an API-profile conformance section to the public suite documentation.
- [ ] Add an API-profile conformance section to the repository README after the first schemas land.

## Level 1 Baseline

- [x] Define Level 1 as `chat.completions.create` with streaming, non-streaming, cancellation, errors, usage, tool-call pass-through, and capability document behavior.
- [ ] Freeze Level 1 mandatory case ids.
- [ ] Freeze Level 1 optional case ids.
- [ ] Freeze Level 1 extension-case selection rules.
- [ ] Define how Level 1 maps onto a selected NNRP protocol baseline.

## Extension Discipline

- [ ] Require provider extensions to be declared in the API capability manifest.
- [ ] Distinguish `critical` extension cases from ignorable non-critical extension fields.
- [ ] Add negative cases that reject extensions which redefine standard profile fields.
- [ ] Add fixture examples showing provider-specific diagnostics that do not affect Level 1 pass/fail.

