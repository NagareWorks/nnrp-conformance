# Manifest And Recipe Schemas

## API Capability Manifest

- [ ] Add `schemas/api-profile-capabilities.schema.json`.
- [ ] Define required manifest fields: `adapter`, `profile`, `schema_version`, `compatibility_levels`, `operations`, and `extensions`.
- [ ] Require `profile = openai-compatible` and `schema_version = openai-compatible/1` for this workstream.
- [ ] Support operation flags for `streaming`, `non_streaming`, `tool_calls`, and cancellation behavior.
- [ ] Support extension declarations with `name`, `critical`, and optional `description`.
- [ ] Add a vLLM adapter capability manifest example.

## Recipe Source

- [ ] Add `schemas/api-profile-recipe.schema.json`.
- [ ] Define readable recipe fields: `id`, `profile`, `schema_version`, `operation`, `request`, `expect`, and optional `parameters`.
- [ ] Allow parameter placeholders such as `${MODEL_ID}` without hard-coding one provider or model.
- [ ] Represent expected event sequences with `type`, `optional`, `min_count`, and field predicates.
- [ ] Represent terminal expectations as `success`, `error`, or `cancelled`.
- [ ] Add sample recipes for streaming chat, non-streaming chat, invalid body, unsupported operation, usage, tool-call delta, cancellation, and backend error.

## Execution And Result Shapes

- [ ] Add `schemas/api-profile-execution-plan.schema.json`.
- [ ] Add `schemas/api-profile-case-results.schema.json`.
- [ ] Keep the suite-owned plan/result shape language-neutral and implementation-agnostic.
- [ ] Define how profile recipes compile into adapter-facing execution plans.
- [ ] Define how adapter case results report selected events, terminal outcome, diagnostics, and extension observations.

