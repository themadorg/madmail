# Local development — Phase 1

## Prerequisites

- Rust stable (≥ 1.75)
- `sqlite3` CLI (optional, for inspecting DB)

## Build

```
make build
# or: cargo build --workspace
```

## Run empty server (boot only)

```
make run-debug
# or: cargo run -p chatmail -- --state-dir ./data --config ./data/chatmail.toml
# (set `debug true` / `log stderr` in that file to enable tracing)
```

Background instance (restart after code changes):

```
make restart
make logs
```

Flags:

| Flag          | Default                     | Description                             |
| ------------- | ---------------------------- | --------------------------------------- |
| `--config`    | `/etc/madmail/madmail.conf` | Static config (TOML or Madmail `.conf`) |
| `--state-dir` | `/var/lib/madmail`          | DB, `admin_token`, runtime state        |

CLI (Madmail-compatible subcommands). From the repo root, paths default to **`./data`** when that directory exists (same as `make run-bg`):

```
cargo run -p chatmail -- --help
cargo run -p chatmail -- admin-token   # reads ./data/admin_token (not stored in SQLite)
cargo run -p chatmail -- version
```

The admin bearer token lives in **`{state_dir}/admin_token`** (created on first server boot). `chatmail.db` holds settings and accounts; verify the server ran once with `ls ./data/chatmail.db`.

**Admin web dashboard** (SvelteKit SPA from `external/madmail-admin-web` git submodule):

```
git submodule update --init external/madmail-admin-web

# Build SPA + re-embed into chatmail (must run cargo from madmail root, not admin-web/)
make build-with-admin-web
# or: make build-admin-web && cargo build -p chatmail

# In data/chatmail.toml: admin_web_path = "/admin"
make restart
open http://127.0.0.1:8080/admin/

cargo run -p chatmail -- admin-web status
cargo run -p chatmail -- admin-web path /xxx   # remount after restart
```

Deploy to Madmail test servers (signed binary, same flow as `context/madmail`):

```
make push    # needs ../imp/private_key.hex and REMOTE1/REMOTE2 in .env or context/madmail/.env
make log1    # journalctl on REMOTE1
```

Full CLI parity plan: [`docs/TDD/14-cli-tools.md`](https://github.com/themadorg/madmail/blob/main/docs/TDD/14-cli-tools.md).

Example TOML (`./data/chatmail.toml` is not auto-loaded; pass `--config`):

```
hostname = "mail.example.org"
primary_domain = "example.org"
state_dir = "./data"
tls_mode = "autocert"
```

## Inspect artifacts

```
sqlite3 ./data/chatmail.db ".schema"
ls -la ./data/admin_token
```

## No-Log check

Logging is off by default. With `log off` (or no `log` line) in `madmail.conf` / `chatmail.toml`, startup should not print INFO lines unless `debug true` is set in that file:

```
cargo run -p chatmail -- --state-dir ./data
# Enable tracing only via config, e.g. `log stderr` + restart
```

## Tests

```
make test
make test-integration
make test-imap
```

IMAP/SMTP/Secure Join against local chatmail (requires `make run-bg` and two `dclogin:` URIs in `.env`):

```
cp .env.example .env
# edit DCLOGIN1 / DCLOGIN2
make test-dclogin
```

```
cargo test -p chatmail-integration boot_test
```

## Docs

- Phase 1 steps: [plans/b1/README.md](https://github.com/themadorg/madmail/blob/main/docs/plans/b1/README.md)
- TDD index: [TDD/README.md](https://github.com/themadorg/madmail/blob/main/docs/TDD/README.md)