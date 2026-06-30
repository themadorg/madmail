## Summary

<!-- What does this PR change and why? Link related issues: Fixes #123 -->

## Type of change

- [ ] Bug fix (non-breaking change that fixes an issue)
- [ ] New feature (non-breaking change that adds functionality)
- [ ] Breaking change (fix or feature that would cause existing behavior to change)
- [ ] Documentation only
- [ ] Build / CI / tooling
- [ ] Refactor (no functional change)

## Area

<!-- Check all that apply -->

- [ ] SMTP
- [ ] IMAP
- [ ] Federation / delivery queue
- [ ] Authentication / JIT
- [ ] Admin API / admin web UI (`external/madmail-admin-web`)
- [ ] TURN / Iroh / proxy services
- [ ] Storage / quota / DB migrations
- [ ] Configuration / CLI
- [ ] Docs (`docs/project/`, `docs/TDD/`, user guide)
- [ ] Other: <!-- describe -->

## Testing

<!-- How did you verify this? Be specific — CI alone is not enough for security-sensitive paths. -->

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace` (or targeted crate/tests: <!-- list if subset -->)
- [ ] Manual / E2E verification (describe below)

**Manual verification (if any):**

```text
<!-- commands run, Delta Chat scenario, federation test, etc. -->
```

## Documentation

- [ ] No doc updates needed
- [ ] Updated operator/user docs (`docs/project/user-guide/`, install guides)
- [ ] Updated developer docs (`docs/project/`)
- [ ] Updated or added TDD section (`docs/TDD/`) — required for significant design changes

## Security & privacy

<!-- Required for changes to PGP gate, auth, admin tokens, federation policy, or logging -->

- [ ] Not security-sensitive
- [ ] Security-sensitive — added/updated tests for the security property
- [ ] Reviewed error messages for information leakage

## Commits

<!-- Main uses semantic-release with conventional commits. Squash or rebase so merge commits follow: -->

- `feat:` new feature
- `fix:` bug fix
- `docs:` documentation
- `refactor:` code change without feature/fix
- `test:` tests only
- `chore:` tooling, deps, release

## Checklist

- [ ] PR is focused and reviewable (small vertical slices preferred)
- [ ] No secrets, `data/`, `target/`, or `node_modules/` committed
- [ ] Submodule pointers updated if `external/madmail-admin-web` changed
- [ ] Behavior parity with Madmail v1 considered (or intentional difference documented)