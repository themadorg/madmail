# Contributing to Madmail

Thank you for helping improve Madmail, the Rust Chatmail relay for [Delta Chat](https://delta.chat).

This document is the short path into the project. For a deeper tour of the codebase, see [`docs/project/`](docs/project/README.md) and especially [`docs/project/17-extend-and-contribute.md`](docs/project/17-extend-and-contribute.md).

## How we work

- **GitHub pull requests** are the normal way to propose changes.
- **Issues** track bugs and enhancements: <https://github.com/themadorg/madmail/issues>
- Design source of truth for significant behavior: [`docs/TDD/`](docs/TDD/).
- Client and federation **interop with Madmail v1** matters; document intentional differences.

## Development setup

Requirements:

- Rust (see `rust-version` in the root `Cargo.toml`)
- System packages typically needed for builds: SQLite dev headers, `pkg-config`, Perl (see CI workflow)

```bash
git clone https://github.com/themadorg/madmail.git
cd madmail
cargo build -p chatmail
cargo test --workspace
```

Operator install and local runs:

- [Quick start](docs/project/user-guide/02-quick-start.md)
- [Local development](docs/local-dev.md)

## Quality bar (required before review)

Run from the repo root:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Or the Makefile targets used by maintainers (`make fmt`, `make lint`, and the relevant test subset).

CI enforces **fmt**, **clippy (`-D warnings`)**, **tests**, and **cargo audit** on pull requests.

## Pull requests

1. Prefer **small, reviewable** changes (vertical slices over large mixed PRs).
2. Use the [pull request template](.github/pull_request_template.md).
3. Write a clear summary: what changed, why, and how you tested it.
4. Use [Conventional Commits](https://www.conventionalcommits.org/) when practical (`feat:`, `fix:`, `docs:`, `test:`, `chore:`, …). Releases use semantic-release from these messages.
5. Do **not** commit secrets, `data/`, `target/`, or `node_modules/`.

### Tests

- New behavior should include **automated tests** (unit next to the code, and/or E2E under `tests/`).
- **Security-sensitive** changes (PGP gate, authentication, admin tokens, federation policy, logging / No-Log) **must** include tests that lock the security property, and must not leak sensitive detail in error messages.
- Expand or update `docs/TDD/16-testing.md` when you change coverage expectations.

### Documentation

- Operator-visible behavior → update the user guide under `docs/project/user-guide/` when needed.
- Design-level changes → update the matching TDD section under `docs/TDD/`.
- Architecture / crate wiring → update `docs/project/` when the mental model changes.

## Coding standards

| Area | Expectation |
|------|-------------|
| Formatting | `rustfmt` (`cargo fmt --all`) |
| Lints | `clippy` with `-D warnings` |
| Style | Idiomatic Rust; prefer existing crate boundaries under `crates/` |
| Safety | Avoid `unwrap()` on production hot paths; handle errors explicitly |
| Security | Prefer allowlist validation on untrusted input; review crypto and auth carefully |

## Submodules and reference trees

- `external/madmail-admin-web` — UI work goes there; update the parent pointer when needed.
- `context/*` — reference material. Prefer upstream fixes there rather than drive-by commits in this monorepo.

## Security reports

Do **not** open a public issue for undisclosed vulnerabilities. See [SECURITY.md](SECURITY.md).

## Code of conduct

Participation is governed by our [Code of Conduct](CODE_OF_CONDUCT.md).

## Questions

- Start with `docs/project/` and `docs/TDD/`.
- Integration tests under `tests/` are executable examples of expected behavior.
- Discussion and review happen on GitHub Issues and Pull Requests.
