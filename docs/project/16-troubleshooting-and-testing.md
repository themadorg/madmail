# 16 — Troubleshooting, Common Gotchas, and the Testing Story

## Common "It Doesn't Work" Scenarios

### Server starts but no one can connect (connection refused)

- Check what is actually listening: `sudo ss -tlnp | grep -E '25|143|587|993|8080'`
- Or use the admin `listener-ports` resource.
- In dev you probably need `make dev-certs` and the ports from your `data/chatmail.toml` (or the defaults the effective_* functions chose).
- Linux only: non-root processes cannot bind <1024. Use `make dev-bind-cap` or higher ports in dev.

### "admin token invalid" or 401 on /api/admin

- The token is in `data/admin_token` (64 hex chars).
- It is also printed on first boot if you have logging enabled.
- CLI commands like `madmail admin-token` read the same file.
- If you overrode it in config to the string "disabled", the whole admin API is gone.

### Quota looks wrong / users can't receive mail after a crash

- The in-memory `QuotaCache` is the source of truth between flushes.
- On next boot `hydrate` re-walks the entire `mail/` tree and corrects it.
- You can force a restart or wait for the next flusher tick + manual inspection.

### Changes to a migration .sql are ignored

- SQLx migrations are versioned by filename timestamp.
- If you edit an already-applied migration, SQLite will see a hash mismatch.
- Solution: `make reset-db` (or delete the DB files) and restart.

### Admin web SPA shows placeholder or 503

- You built the Rust binary without the SPA embedded.
- Run `make build-with-admin-web` (which requires the submodule and bun/npm).
- Then restart.

### Delta Chat cannot register or login (but CLI tools can)

- Check the `jit_domain` logic. When clients connect by IP they often need the server to accept the IP literal as a valid JIT domain.
- Also check `credential_policy` length/charset rules.
- Look at the IMAP/SMTP logs with `RUST_LOG=debug`.

### TURN calls don't work

- Verify `__TURN_ENABLED__` is true in settings (admin toggle).
- Check that the IMAP METADATA responses actually contain the turn server info (use a raw IMAP client or the E2E tests).
- The dedicated TURN E2E scripts and `scripts/turn-debug-env.sh` are your friends.

### "No-Log" is not working (still seeing INFO logs)

- The decision is made very early in `boot.rs` based on the static config `log` / `debug` lines.
- `maddy_log_off`, `logging_enabled`, and `should_disable_logging` in `logging.rs` are the key functions.
- `debug true` / `yes` / `1` / `enable` in the config **overrides** No-Log (stderr + `debug` filter).
- Check you did not set `log stderr` or `log /path/to/file` while expecting silence.

### Enable operator logs (opt-in)

```conf
log stderr
# or: log /var/lib/madmail/madmail.log
# or: log stderr /var/lib/madmail/madmail.log
debug True   # optional: verbose; also accepts true/yes/1/enable
```

Restart required after changing static `log` / `debug`.

## Database Inspection Cheat Sheet

```sql
-- All dynamic settings
SELECT * FROM settings ORDER BY key;

-- Accounts that have ever logged in
SELECT * FROM quotas ORDER BY last_login_at DESC;

-- Recent federation activity
SELECT * FROM federation_stats ORDER BY last_updated DESC LIMIT 30;

-- Blocked users
SELECT * FROM blocked_users;

-- Registration tokens and usage
SELECT * FROM registration_tokens;
```

## Log Levels & Noise

- Normal production: almost nothing (No-Log — `log off` or omit `log`).
- `log stderr` without `debug` → filter defaults to `warn` (or `RUST_LOG` if set).
- `debug true` in config → filter `debug` (overrides No-Log; stderr if no log target).
- `RUST_LOG=chatmail=trace` → extremely verbose when logging is enabled. Use sparingly.

The `logging.rs` module owns the reloadable `tracing` subscriber, No-Log filter, and maddy-compatible log destinations (stderr and/or file).

## The Full Testing Story (Pyramid)

1. **Unit tests** inside each crate (`cargo test -p chatmail-pgp` etc.)
2. **Crate integration tests** (e.g. `chatmail-turn/tests/`)
3. **Workspace integration tests** (`tests/` crate) — boots real `madmail` processes, speaks real protocols, exercises ctl binary, checks OpenMetrics, does SecureJoin, exercises TURN, etc.
4. **Delta Chat client E2E** (`make test-deltachat`) — spins up VMs with incus + cmlxc, deploys the exact static binary you just built, runs real Delta Chat desktop and core clients through registration, messaging, calls, etc.
5. **Docker TURN E2E** (`make test-docker-turn-e2e`) — builds the local image, bootstraps with `install --simple --ip 127.0.0.1`, runs `context/relay-ping` connectivity (SMTP/IMAP + TURN GETMETADATA), then a Rust probe that TURN-allocates on the configured relay UDP range through Docker port mappings. Script: `scripts/docker-turn-e2e.sh`. Env: `DOCKER_TURN_BUILD=0` to skip rebuild; `DOCKER_TURN_SKIP_ALLOCATE=1` for relay-ping only.
6. **Throughput benchmarks** (T1) — controlled 1 CPU / 1 GiB environment comparing Go Madmail vs Rust madmail under load.
7. **Manual / relay-ping** against real test servers (`make test-dclogin`).

The higher levels are slow and require extra tooling (incus, uv, cmlxc, sometimes physical test servers), which is why they are not run on every `cargo test`.

## When a Test Is Flaky

- Federation tests are sensitive to timing and port conflicts between parallel runs.
- TURN tests need the relay to actually be reachable (the force-relay test mode exists for this).
- Always check that you don't have a stale `target/debug/madmail` from a previous `make restart` that is still running on old code.

## Next (and Final)

The last document in the series tells you how to extend the project, where to look when adding features, and the contribution conventions.

→ [17-extend-and-contribute.md](./17-extend-and-contribute.md)
