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
- CLI commands like `chatmail admin-token` read the same file.
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

- The decision is made very early in `boot.rs` based on the static config `log_target`.
- `maddy_log_off(None)` and `should_disable_logging` are the key functions.
- `debug true` in the config forces some INFO lines even in No-Log mode.

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

- Normal production: almost nothing (No-Log).
- `debug true` in config + `RUST_LOG=info` → reasonable server chatter.
- `RUST_LOG=chatmail=trace` → extremely verbose (protocol frames, every DB query, etc.). Use sparingly.

The `logging.rs` module has the reloadable `tracing` subscriber and the special `NopOutput` backend for No-Log.

## The Full Testing Story (Pyramid)

1. **Unit tests** inside each crate (`cargo test -p chatmail-pgp` etc.)
2. **Crate integration tests** (e.g. `chatmail-turn/tests/`)
3. **Workspace integration tests** (`tests/` crate) — boots real `chatmail` processes, speaks real protocols, exercises ctl binary, checks OpenMetrics, does SecureJoin, exercises TURN, etc.
4. **Delta Chat client E2E** (`make test-deltachat`) — spins up VMs with incus + cmlxc, deploys the exact static binary you just built, runs real Delta Chat desktop and core clients through registration, messaging, calls, etc.
5. **Throughput benchmarks** (T1) — controlled 1 CPU / 1 GiB environment comparing Go Madmail vs Rust chatmail under load.
6. **Manual / relay-ping** against real test servers (`make test-dclogin`).

The higher levels are slow and require extra tooling (incus, uv, cmlxc, sometimes physical test servers), which is why they are not run on every `cargo test`.

## When a Test Is Flaky

- Federation tests are sensitive to timing and port conflicts between parallel runs.
- TURN tests need the relay to actually be reachable (the force-relay test mode exists for this).
- Always check that you don't have a stale `target/debug/chatmail` from a previous `make restart` that is still running on old code.

## Next (and Final)

The last document in the series tells you how to extend the project, where to look when adding features, and the contribution conventions.

→ [17-extend-and-contribute.md](./17-extend-and-contribute.md)
