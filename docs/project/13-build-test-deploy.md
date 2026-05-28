# 13 — Build, Test, and Deploy System

This is the practical "how do I actually do things" document. The `Makefile` is the primary interface for humans.

## The All-Important Makefile

Almost everything a developer or operator does goes through targets in the root `Makefile`.

Key categories:

### Build

- `make build` — normal debug build of `chatmail`
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
- `make test-deltachat` — full Delta Chat core E2E via incus + cmlxc (heavy but authoritative)
- `make test-dclogin` — relay-ping against two real accounts (requires DCLOGIN1/2 in .env)

### Deploy

- `make push`, `make push1`, `make push2` — via `scripts/deploy.sh`
- `make push-signed` — signed upgrade path (`scripts/deploy.sh HOST --signed`)
- `make sign` — `scripts/sign.sh` (private key in `../imp/private_key.hex`)
- `make log1` / `make log2` — `scripts/deploy.sh --log HOST`

Remote deploys assume you have `REMOTE1` / `REMOTE2` (and optionally keys) in `.env` or `context/madmail/.env`.

### Other

- `make t1-bench` — throughput benchmark (incus + cmlxc)
- `relay-ping-*` targets — build the separate relay-ping tool

## Embedding the Admin Web (the magic step)

Because the admin SPA is a separate SvelteKit project:

1. `git submodule update --init external/madmail-admin-web`
2. `cd external/madmail-admin-web && bun install && bun run build`
3. From repo root: `make build-with-admin-web` (or the two-step variant)
4. The `chatmail-admin-web/build.rs` sees `CHATMAIL_ADMIN_WEB_BUILD` env (or the default path) and copies `index.html` + assets into `embed/`
5. Cargo then `include_bytes!` or equivalent at compile time
6. At runtime the SPA is served from memory

If you forget step 3, the binary will still run but `/admin` will show a placeholder telling you to build the UI.

## Release Static Binary

`scripts/build-release-static.sh`:

- Runs `make build-admin-web`
- Does `cargo rustc ... -C target-feature=+crt-static`
- Verifies with `ldd` that the binary is fully static
- Result can be copied to a plain Debian 12 box (or similar) and will run without matching glibc

This is the artifact that gets signed and `scp`'d in production deploys.

## Testing Pyramid

1. **Unit** — inside each crate (`cargo test -p chatmail-foo`)
2. **Integration** — `tests/` workspace member (boots real servers, speaks SMTP/IMAP, exercises ctl)
3. **E2E with Delta Chat** — `make test-deltachat` (real Delta Chat desktop + core clients against the Rust server in Incus VMs)
4. **Throughput (T1)** — special benchmark comparing madmail (Go) vs madmailv2 (Rust) under load
5. **Manual / relay-ping** — `make test-dclogin` against two real accounts on test servers

The E2E suite is the ultimate source of truth for "does this still work like a chatmail server should?"

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
