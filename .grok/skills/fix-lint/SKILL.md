---
name: fix-lint
description: >
  Run madmail Makefile quality checks (make lint) and fix all formatting,
  compiler warnings, clippy, and rustdoc issues until clean. Use when the user
  says fix lint, fix lints, fix warnings, run lint, clean up clippy, or runs
  /fix-lint.
---

# Fix Lint

Run the repo's **Makefile quality targets** from the **repository root** and fix every failure until `make lint` exits 0.

Do **not** substitute raw `cargo` commands when an equivalent `make` target exists.

## Makefile targets (authoritative)

| Target | What it runs |
|--------|----------------|
| `make fmt` | `cargo fmt --all` — auto-fix formatting |
| `make fmt-check` | `cargo fmt --all -- --check` — format only, no writes |
| `make check` | `RUSTFLAGS="-D warnings" cargo check --workspace --all-targets` |
| `make vet` | Alias for `make check` |
| `make lint` | `fmt-check` → `check` → `clippy -D warnings` → `cargo doc -D warnings` |

`make lint` is the **full gate**. All compiler warnings are errors (`-D warnings`).

## Workflow (loop until clean)

```
1. make fmt                    # auto-fix rustfmt issues first
2. make lint                   # full quality gate
3. Fix reported issues in code
4. make fmt                    # re-format after edits
5. make lint                   # verify
6. Repeat 3–5 until exit 0
```

Use `make check` mid-loop for a faster compile-only pass; always finish with `make lint`.

## Parsing failures

### fmt-check (first step of `make lint`)

```
Diff in /path/to/file.rs:42:
```

→ Run `make fmt`. If still failing, fix the file manually to match rustfmt.

### `cargo check` / rustc warnings

```
warning: …
   --> crates/foo/src/bar.rs:10:5
```

→ Fix the underlying issue (unused imports, dead code, type errors, etc.). Warnings are **errors** via `RUSTFLAGS="-D warnings"`.

### clippy

```
error: …
  --> crates/foo/src/bar.rs:20:9
   = help: …
   = note: `#[deny(clippy::…)]`
```

→ Apply the suggested fix. Prefer the minimal change clippy hints at. Do not `#[allow(...)]` unless the lint is a known false positive — ask the user before suppressing.

### rustdoc (`cargo doc`)

```
error: …
  --> crates/foo/src/lib.rs:5:1
```

→ Fix doc comments, broken intra-doc links, or missing backticks. Doc warnings are errors via `RUSTDOCFLAGS="-D warnings"`.

## Fix principles

- **Minimal diffs** — fix only what lint reports; no drive-by refactors.
- **Match existing style** — read surrounding code before editing.
- **Workspace scope** — `make lint` checks `--workspace --all-targets` (includes tests, benches, examples). Fix test code too.
- **No warning suppression by default** — resolve the root cause.
- **Commit-ready** — when `make lint` passes, summarize what was fixed.

## Common fixes

| Lint / warning | Typical fix |
|----------------|-------------|
| `unused_imports` | Remove the import |
| `dead_code` | Remove code, or use it; don't silence without reason |
| `needless_borrow` | Remove `&` |
| `clippy::too_many_arguments` | Group into a struct only if already idiomatic in the crate |
| `clippy::unwrap_used` | Replace with proper error handling matching nearby code |
| `missing_docs` on public items | Add a one-line `///` doc comment |
| Broken `#![deny(…)]` in lib | Fix the item, not the deny attribute |

## What this skill does NOT cover

| Target | Scope |
|--------|-------|
| `make test` / `make test-unit` | Tests — run separately after lint is clean |
| `make man-lint` | Man page groff — use `update-cli-docs` skill |
| `make cov` | Coverage — optional, not part of lint |
| `build-landing` / admin-web | Frontend — only if `make lint` somehow touches those (it doesn't today) |

## Quick reference

```bash
# User says "fix lint" — run this loop:
make fmt && make lint
# fix issues → make fmt && make lint
# repeat until clean
```

Success criterion: **`make lint` exits 0** with no errors in stdout/stderr.