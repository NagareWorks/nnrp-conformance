## Summary

- PR type: feature / bugfix / docs / maintenance
- What changed?
- Why is it needed now?

## Implementation

- Main crates, manifests, schemas, or workflows changed:
- Any protocol-baseline, contract, or CI assumptions introduced:
- Follow-up work, if any:

## Validation

- [ ] `cargo fmt --all --check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] Baseline summary or targeted manifest validation checked if protocol-facing artifacts changed

Commands or workflow runs used:

```text

```

## Baseline Impact

- [ ] No public baseline contract change
- [ ] Case manifest contents changed
- [ ] Schema or report contract changed
- [ ] Runner selection or summary behavior changed
- [ ] CI workflow behavior changed

## Checklist

- [ ] Branch name matches repository conventions
- [ ] Commit messages follow Conventional Commits
- [ ] PR is squashed to one commit
- [ ] Documentation was updated when public behavior changed

## Notes

- Specialized reference templates still live in `.github/PULL_REQUEST_TEMPLATE/`.
- GitHub does not show an automatic chooser for those files on the standard PR page, so this file acts as the default template.