## Summary

- What baseline or release-preparation change is included?

## Versioning

- Target baseline or suite version:
- Why this version is needed:

## Baseline Impact

- [ ] Protocol manifest version or status changed
- [ ] Case manifest contents changed
- [ ] Schema or report contract changed
- [ ] Runner behavior changed
- [ ] CI workflow behavior changed

Describe the release-facing impact:

## Validation

- [ ] `cargo fmt --all --check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] Baseline summary and version alignment were checked
- [ ] Release workflow assumptions were checked if any exist

Commands or workflow runs used:

```text

```

## Manual Release Steps

- [ ] No manual release work required
- [ ] Git tag or GitHub release expectations were reviewed
- [ ] Historical baseline compatibility expectations were reviewed

Notes:

## Checklist

- [ ] Branch name matches repository conventions
- [ ] Commit messages follow Conventional Commits
- [ ] PR is squashed to one commit unless this is necessary `release/<version>` branch work
- [ ] Release notes or docs were updated if needed