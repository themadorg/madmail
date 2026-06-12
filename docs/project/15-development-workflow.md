# 15 — Development Workflow (The Edit → Build → Test Loop)

This is the "I just want to make a change and see it work" guide.

## Getting Started: Local Development & Testing (First Time)

This section is the full onboarding flow for developers who are building and testing madmail-v2 from source.

### Requirements
- Rust (stable, ≥ 1.75)
- `make`
- (Optional but recommended) `bun` or `npm` — only needed if you plan to work on or preview the admin web UI

### Steps

```bash
# 1. Get the code
git clone ...   # or however you obtained the repository
cd madmailv2

# 2. Build the server
make build

# 3. Start it in the background with debug logging
make restart

# 4. Watch the logs in another terminal
make logs
```

The server is now running with sensible development defaults.

### Connecting Delta Chat Clients for Testing

Point Delta Chat (Desktop or Android) at:

- **IMAP**: `127.0.0.1` port `1143` (or `993` for TLS)
- **SMTP / Submission**: `127.0.0.1` port `1025` (or `465`/`587`)
- Address: anything@127.0.0.1 or anything@localhost (dev mode accepts these)
- Password: anything — the account is created automatically (JIT)

You can also reach the admin interface at:

- http://127.0.0.1:8080/admin/
- Token is in the file `data/admin_token`

### Useful Make Targets for Daily Development Work

- `make restart` — rebuild + restart (the most common command)
- `make logs` — follow the log file
- `make stop`
- `make reset-db` — wipe the database (keeps the `mail/` folder)
- `make dev-certs` — regenerate self-signed TLS certificates for 127.0.0.1

`make restart` is deliberately lightweight and does **not** rebuild the admin web SPA.

## The Golden Local Loop (90% of daily work)

```bash
# 1. Make your edit in crates/ or docs/
# 2. Rebuild + restart the dev server
make restart          # stop + dev-certs + run-bg (with debug logging)

# 3. Watch logs in another terminal
make logs

# 4. Exercise your change
#    - Use Delta Chat desktop pointed at 127.0.0.1:1143 / 1025 (or whatever your config says)
#    - Or use the relay-ping tool
#    - Or curl the admin API with the token from data/admin_token
#    - Or open http://127.0.0.1:8080/admin/ (after make build-with-admin-web)

# 5. When done
make stop
```

`make restart` is deliberately fast for the common case (it does **not** rebuild the admin web SPA).

## When You Change the Admin Web SPA

```bash
# Edit files in external/madmail-admin-web/src/...

# From repo root:
make build-with-admin-web   # builds SPA + re-embeds + rebuilds madmail
make restart
```

Then hard-reload the browser at `/admin/`. The version.json stamp helps with service worker cache.

## Working with the Database

```bash
# Inspect
sqlite3 data/chatmail.db ".schema"
sqlite3 data/chatmail.db "SELECT * FROM settings;"
sqlite3 data/chatmail.db "SELECT key, substr(value,1,60) FROM settings;"

# After editing a migration .sql file you often need a clean DB
make reset-db
make restart
```

`make reset-db` removes both the app DB and the credentials DB (but keeps the `mail/` tree and config).

## Adding a New CLI Command (Madmail Parity Style)

1. Add the clap definition in `chatmail-config/src/cli.rs`.
2. Add a handler module in `crates/chatmail/src/ctl/your_cmd.rs`.
3. Wire it in `ctl/dispatch.rs` (or the big match).
4. Add a test in `ctl/dispatch_tests.rs` or `ops_tests.rs`.
5. Document the parity status in `docs/TDD/14-cli-tools.md`.

Many "stub" commands exist that just print "not yet implemented — use the Admin API instead".

## Adding a New Admin Resource

1. Create `crates/chatmail-admin/src/resources/your_thing.rs`.
2. Implement the handler function(s) that read/write the DB or AppState.
3. Register the method in the big dispatch in `handler.rs` or `router.rs`.
4. Add a matching entry in the Svelte admin UI (if you want it visible in the dashboard).
5. Add tests (unit in the admin crate or E2E via the ctl binary).

## Working on SMTP / IMAP Changes

- Unit tests in the crate itself are fast.
- For protocol-level work, use `make run-bg` + a tool like `swaks`, `openssl s_client -crlf`, or a Python IMAP client against the dev ports.
- The full E2E (`make test-e2e` or specific `cargo test -p chatmail-integration imap_`) is the final gate.

## Working on Federation or Delivery

- Stand up two local instances (different state dirs + ports).
- Or use the existing multi-server E2E tests.
- Inspect the outbound queue with the admin `queue` resource or by looking in `data/remote_queue/`.

## Debugging a Running Server

- `make logs` (follows the nohup log from run-bg)
- `RUST_LOG=debug,chatmail=trace cargo run ...` for extreme verbosity (when `debug true` is also in the config)
- `sudo ss -tlnp | grep madmail` or the admin listener-ports resource to see what is actually bound
- `sqlite3 data/chatmail.db "SELECT * FROM federation_stats ORDER BY last_updated DESC LIMIT 20;"`
- For TURN: the dedicated debug env script `scripts/turn-debug-env.sh`

## When You Need a Completely Clean Slate

```bash
make stop
make reset-db
rm -rf data/mail data/certs
make dev-certs
make restart
```

## Working on Tests

- Fast unit: `cargo test -p chatmail-foo`
- E2E that still uses the just-built binary: `make test-e2e`
- Specific E2E filter: `cargo test -p chatmail-integration -- turn_`
- Full Delta Chat client E2E (slow, needs incus + uv + cmlxc): `make test-deltachat`

## Git & Submodules

- Normal Rust changes: just commit in `crates/`, `docs/`, `scripts/`, `Makefile`.
- Admin web changes: edit in `external/madmail-admin-web/`, commit there (it is a separate repo), then the parent repo records the new submodule pointer.
- Never commit the huge `target/` or `node_modules/` trees.
- `.env` and `data/` are gitignored (or should be).

## Next

The final document in the series covers troubleshooting, common gotchas, and the full testing story.

→ [16-troubleshooting-and-testing.md](./16-troubleshooting-and-testing.md)
