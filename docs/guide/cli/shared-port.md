# `shared-port`

Serve IMAP and SMTP submission on the **HTTPS port (443)**, alongside the website, or stop doing so.

This is what keeps mail working on networks that block 993 and 465. Settings map to:

- DB: `__SHARED_PORT_IMAP__` / `__SHARED_PORT_SMTP__`
- Config file: `alpn_imap` / `alpn_smtp` in the `chatmail tls://…` block (used when the DB key is unset)

The `alpn_*` directives are defaults; they never block these commands. To protect the
value, set `shared_port_imap` / `shared_port_smtp` in a `settings { }` block or through
`CHATMAIL_SHARED_PORT_IMAP` / `CHATMAIL_SHARED_PORT_SMTP`; `enable` / `disable` then
exit with `configuration_locked` and `status` shows the protecting source.

Each protocol is independent: submission can stay on 443 while IMAP does not.

**A `chatmail tls://…` block must exist in the config file.** Without it the HTTPS port is a plain listener with nothing to demultiplex, and these settings do nothing until one is added and the server restarts. `status` says so when that is the case.


## Synopsis

```bash
madmail shared-port <status|enable|disable> [imap|smtp|both]
madmail alpn <status|enable|disable> [imap|smtp|both]   # same command
```

`alpn` is an alias, named after the `alpn_imap` / `alpn_smtp` directives operators
configure. The command covers more than ALPN — it also gates the hostname (SNI)
and first-bytes routes — which is why `shared-port` is the primary name.

## Global flags

| Flag | Alias | Environment | Default | Description |
|------|-------|-------------|---------|-------------|
| `--config` | — | `CHATMAIL_CONFIG` | `/etc/madmail/madmail.conf` (or `./data/chatmail.toml` when present) | Path to the server config file |
| `--state-dir` | `--libexec` | `CHATMAIL_STATE_DIR` | `/var/lib/madmail` (or `./data` when it contains state) | Persistent state directory (`credentials.db`, maildirs, `admin_token`, …) |


## Subcommands

| Subcommand | Description |
|------------|-------------|
| `status` | Show which protocols the HTTPS port serves, the file defaults, any protecting source, and the stored DB overrides |
| `enable [imap\|smtp\|both]` | Serve that protocol on the HTTPS port (default `both`) |
| `disable [imap\|smtp\|both]` | Stop serving it (default `both`) |

```bash
madmail shared-port status
madmail shared-port disable imap
madmail reload
madmail shared-port enable both
madmail reload
```

After `enable` / `disable`, run:

```bash
madmail reload
```

so a running server re-hydrates the live flags (soft reload). The admin API toggle
(`/admin/settings/shared_port_imap`, `/admin/settings/shared_port_smtp`) applies
immediately instead, because it runs in-process: the listener reads the flags once
per connection and picks the matching TLS config, so the next connection sees the
change without a restart.

## What changes when a protocol is disabled

- The port stops **advertising** that ALPN token, so a client offering only that
  token is refused at the handshake rather than negotiating it and landing on the
  website.
- Hostname (SNI) and first-bytes identification stop selecting it.
- Autoconfig stops advertising 443 for it, so clients are not told to use a port
  that no longer serves them.
- The dedicated ports (993 / 465) are unaffected.

## See also

- [`status`](status.md) — shows the shared-port row among the listeners
- `docs/TDD/13-configuration.md` — the `chatmail` block and the ALPN / SNI surface
- `tests/highdpi-smoke/` — measure whether 443 actually reaches clients on a filtered network
