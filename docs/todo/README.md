# Conformance Todo Layout

Repository-local todo files are grouped by the contract they belong to.

## Protocol Lines

Protocol-line work belongs under `docs/todo/<protocol-version>/`.

Examples:

- `docs/todo/nnrp-1-preview3/`

These files track protocol baseline, manifest, vector, state-machine, adapter-plan, and benchmark-plan work that is tied to a specific NNRP protocol version.

## API Profiles

API profile work belongs under `docs/todo/<profile-workstream>/`.

Examples:

- `docs/todo/openai-nnrp-api/`

These files track profile-level conformance surfaces that can run on top of a protocol baseline. API profile conformance is optional for implementations that do not claim the profile, but mandatory for implementations that advertise the matching profile capability manifest.

