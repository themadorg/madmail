# 13 — Build, Test, and Deploy System

This is the practical "how do I actually do things" document. The `Makefile` is the primary interface for humans.

## The All-Important Makefile

Almost everything a developer or operator does goes through targets in the root `Makefile`.

Key categories:

### Build

- `make build` — normal debug build of `madmail`
- `make build-admin-web` — builds the Svelte SPA from the submodule and stamps `version.json`
- `make build-with-admin-web` — builds SPA + embeds it into the Rust binary
- `make build-release` — release + admin web embedded
- `make build-release-static` — fully static-pie binary (no glibc dependency on target)
- `make build-all` — cross-compile release for x86_64 and aarch64

The static build is the one you usually `scp` to production servers.

### Run / Dev

- `make run`, `make run-debug`
- `make run-bg` + `make logs`
- `make restart` — stop + dev-certs + run-bg (the common edit-compile-test loop)
- `make dev-certs` — self-signed TLS for 127.0.0.1
- `make reset-db` — blow away SQLite (useful after migration edits)
- `make install` — build-with-admin-web + restart (local dev with UI)

### Quality

- `make check`, `make lint` (clippy -D warnings), `make fmt`
- `make test-unit`, `make test`
- `make test-e2e` — integration tests (builds first)
- `make test-turn`, `make test-imap`, `make test-maintenance`
- `make test-core-turn` — core TURN E2E helper (when present)
- `make test-deltachat` — full Delta Chat core E2E via incus + cmlxc (heavy; closest to real client behaviour)
- Optional external probe: **[relay-ping](https://github.com/themadorg/relay-ping)** — separate Go tool for SMTP/IMAP connectivity, SecureJoin init, throughput, and multi-server latency matrices. Not vendored in this repo; clone and build there, then point it at your madmail host (see that project’s README).

### Deploy

- `make push`, `make push1`, `make push2` — via `scripts/deploy.sh`
- `make push-signed` — signed upgrade path (`scripts/deploy.sh HOST --signed`)
- `make sign` — `scripts/sign.sh` (private key in `../imp/private_key.hex`)
- `make log1` / `make log2` — `scripts/deploy.sh --log HOST`

Remote deploys assume you have `REMOTE1` / `REMOTE2` (and optionally keys) in `.env` or `context/madmail/.env`.

## Embedding the Admin Web (the magic step)

Because the admin SPA is a separate SvelteKit project:

1. `git submodule update --init external/madmail-admin-web`
2. `cd external/madmail-admin-web && bun install && bun run build`
3. From repo root: `make build-with-admin-web` (or the two-step variant)
4. The `chatmail-admin-web/build.rs` sees `CHATMAIL_ADMIN_WEB_BUILD` env (or the default path) and copies `index.html` + assets into `embed/`
5. Cargo then `include_bytes!` or equivalent at compile time
6. At runtime the SPA is served from memory

If you forget step 3, the binary will still run but `/admin` will show a placeholder telling you to build the UI.

## Public release automation

Pushes to `main` run `scripts/release.py`. Release-worthy Conventional Commits bump `.version`, `Cargo.toml`, and `Cargo.lock`, then tag `vX.Y.Z-unstable`. CI then publishes an unsigned GitHub prerelease with six server builds, their archives, a Windows tray, and an unsigned testing installer, plus the `X.Y.Z-unstable` / `unstable` container tags. That publish waits until those builds and the tests pass. A pull request runs the same six recipes and stages the same files without a tag or a release upload. `docs`, `chore`, `ci`, and `test` commits do not bump the version. The `madmail-linux-amd64-legacy` prerelease file is built with the public static-pie recipe below. `scripts/build-release-static.sh` is not in this repository; CI passes `-C target-feature=+crt-static` to `cargo rustc` directly and rejects a dynamic binary.

Stable binaries are built, signed and published using owner-maintained tooling kept outside this repository. Stable container tags are published only after that tooling publishes a stable GitHub Release. Details: [release automation](../TDD/release-automation.md).

## Release Static Binary

`scripts/build-release-static.sh`:

- Runs `make build-admin-web`
- Does `cargo rustc ... -C target-feature=+crt-static`
- Verifies with `ldd` that the binary is fully static
- Result can be copied to a plain Debian 12 box (or similar) and will run without matching glibc

That script is not in this repository. Unsigned prereleases publish a binary built with the same public `cargo rustc -- -C target-feature=+crt-static` recipe and checked for a static ELF. They do not run the absent script, and the workflow does not boot that binary on Debian 12.

## Testing Pyramid

1. **Unit** — inside each crate (`cargo test -p chatmail-foo`)
2. **Integration** — `tests/` workspace member (boots real servers, speaks SMTP/IMAP, exercises ctl)
3. **E2E with Delta Chat** — `make test-deltachat` (real Delta Chat desktop + core clients against the Rust server in Incus VMs)
4. **TURN** — `make test-turn` / crate tests; optional live Docker probe: `tests/docker_turn_e2e.rs` (`#[ignore]`, set env vars)
5. **relay-ping** (optional, separate repo) — [themadorg/relay-ping](https://github.com/themadorg/relay-ping) against a live host
6. **Throughput (T1)** — special benchmark comparing madmail (Go) vs madmailv2 (Rust) under load

The E2E suite is the main integration check for "does this still work like a chatmail server should?"

## Continuous Integration (implied)

The repo expects:
- `cargo fmt -- --check`
- `cargo clippy -- -D warnings`
- `cargo test --workspace`
- The heavy E2E and T1 jobs are usually run manually or on dedicated hardware because they need incus + time.

## .env and Overrides

Many Makefile variables can be overridden via `.env` (sourced at the top of the Makefile):

- `STATE_DIR`, `CONFIG`
- `REMOTE1`, `REMOTE2`
- `DCLOGIN1`, `DCLOGIN2`
- `PRIV_KEY_FILE`
- `ADMIN_WEB_DIR`

This lets you point at different test servers or local state without editing the Makefile.

## Next

Now you know how to build and ship the thing.

The next document explains the giant `context/` and `external/` directories that are not part of the shipping product but are essential for development and understanding.

→ [14-understanding-context-and-references.md](./14-understanding-context-and-references.md)
