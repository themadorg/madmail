# 05 — Boot Sequence and AppState (The Heart of the Running Server)

This document walks through exactly what happens from `cargo run` (or `systemd` start) until the server is accepting connections.

## Entry Point

`crates/chatmail/src/main.rs:24`

```rust
#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse_normalized();
    match cli.command {
        None | Some(Command::Run) => boot::run(cli.args).await,
        _ => ctl::dispatch(&cli).await,
    }
}
```

Two modes:
- No subcommand or `run` → full server boot.
- Everything else (`install`, `accounts`, `version`, `admin-token`, ...) → CLI dispatch (no listeners).

## `boot::run` (the normal server path)

File: `crates/chatmail/src/boot.rs`

High-level steps (with file:line anchors):

1. **Load static config**
   ```rust
   let file_config = load_file_config(&args.config)?;
   ```
   Falls back to `AppConfig::default()` if the file does not exist.

2. **Initialize logging** (respecting No-Log / maddy `log off`)
   ```rust
   let log_reload = init_logging(debug);
   if should_disable_logging(...) { set_no_log(...) }
   ```

3. **State dir + DB + admin token**
   ```rust
   let (artifacts, pool) = initialize_state(&state_dir, &file_config).await?;
   ```
   - Creates `state_dir`
   - Calls `init_db_from_config` (runs migrations if needed)
   - Reads or creates `admin_token` file (0600)

4. **Create AppState + hydrate**
   ```rust
   let app_state = Arc::new(AppState::with_quota_and_message_limit(...));
   app_state.hydrate(&pool, &file_config).await?;
   ```
   This is critical — see "Hydration" below.

5. **Start the flusher**
   ```rust
   let flusher = app_state.start_flusher(pool.clone());
   ```

6. **Start the supervisor** (listeners + sidecars + maintenance)
   ```rust
   let _supervisor = if !args.boot_once {
       crate::servers::start_servers(pool, Arc::clone(&app_state), ...).await?
   } ...
   ```

7. **Wait for shutdown signal**
   ```rust
   tokio::signal::ctrl_c().await?;
   flusher.shutdown().await;
   ```

`boot_once` mode (used in some tests) shuts down the flusher and exits early without listeners.

## `initialize_state`

- Ensures state dir exists.
- Computes effective DB path (can be SQLite file or `:memory:` for tests).
- Opens SQLx pool and runs migrations.
- Resolves the 64-byte admin token (from `data/admin_token` or config override).

## `AppState::hydrate`

Defined in `chatmail-state/src/lib.rs`.

It loads the "hot" working set from durable storage so that the first mail delivery doesn't have to hit disk for policy/quota decisions:

- `message_size.hydrate(...)`
- `quota.hydrate(pool, &mailbox_store)` — walks Maildir to compute current used bytes per user
- `federation_policy.hydrate(pool)`
- `federation_silent_dismiss.hydrate(pool)`

After hydrate, the server is ready to make fast in-RAM decisions.

## `ServerSupervisor::start` (the real server)

File: `crates/chatmail/src/supervisor.rs:115`

This is where the "living" server is assembled.

Key things it does (in order):

1. Creates the outbound delivery queue (`start_outbound_queue`).
2. Builds `SmtpSessionConfig` for inbound (25) and submission (auth required).
3. Starts TURN, Iroh, and Shadowsocks sidecars (conditional on config + DB toggles).
4. Builds `ImapSessionConfig` containing the TURN/Iroh discovery info (so IMAP METADATA can serve it).
5. Spawns the maintenance scheduler (`chatmail_tasks`).
6. Creates the shared `SupervisorInner`.
7. Calls `rebuild_http_routers()` — merges admin API + admin-web SPA + public www routes.
8. Calls `start_listeners()` — binds all the TCP listeners (SMTP, IMAP, HTTP, etc.).
9. Starts OpenMetrics listener if configured.
10. Spawns a background task that listens on the reload channel and calls `soft_reload`.
11. Notifies systemd (if running under it) that we are ready.

## Listener Management & Reload

`ActiveListeners` struct holds `ListenerSlot` (CancellationToken + JoinHandle) for each service.

On `POST /admin/reload` (or SIGHUP path):
- The reload task receives on the channel.
- `soft_reload` cancels existing listeners, rebuilds HTTP routers (in case admin path or token changed), re-reads port overrides from DB, and starts fresh listeners.
- Old tasks are awaited with timeout.

This gives near-zero-downtime config changes for most settings.

## The Flusher

`AppState::start_flusher` spawns a background task that periodically calls:
- `flush_federation_stats`
- message stats flush
- etc.

On graceful shutdown it is explicitly told to do a final flush.

## Shutdown

- Ctrl-C (or SIGTERM) is caught in `boot::run`.
- Flusher is shut down (final persist).
- Supervisor is dropped → its `Drop` impl shuts down the maintenance handle.
- Tokio runtime exits.

No complex distributed shutdown dance — the single-process model makes this simple.

## Why This Structure Exists

- **Boot is separate from "run the listeners"** so that CLI commands and tests can reuse the DB + state initialization without starting network services.
- **AppState is the single source of truth for hot data** — everything that needs fast reads/writes goes through it.
- **Supervisor owns the reload story** — listeners are the things that must be rebound; everything else can react to DB changes.
- **Sidecars are started early** so that IMAP METADATA can immediately advertise TURN/Iroh addresses.

## Common Places People Get Lost

- "Where do the listeners actually get bound?" → `supervisor.rs` → `start_listeners` (and the `ListenerPorts` helpers).
- "When does the DB get opened vs migrated?" → `initialize_state` → `chatmail_db::init_db_from_config`.
- "How does reload work for ports/TLS?" → `soft_reload` + `ListenerPortsStore`.
- "Why does quota hydrate walk the entire Maildir?" → Because used bytes are not stored durably per-message; we compute on boot and maintain in RAM + periodic checkpoints.

## Next

- See exactly how configuration (static + dynamic) flows into all of the above: [06-configuration-system.md](./06-configuration-system.md)
- See how authentication and JIT creation fit into the boot/auth path: [07-authentication-and-jit.md](./07-authentication-and-jit.md)
