# Configuration sources and protected settings

Madmail resolves operational settings in this order:

1. Environment variables (`CHATMAIL_<SETTING_NAME_IN_UPPERCASE>`).
2. Explicit settings in the selected configuration file.
3. Persisted database settings managed by CLI/API/admin web.
4. Built-in defaults.

An explicit environment or file value protects that setting. Database-backed set,
reset, enable, and disable commands cannot override it, even if the requested value
is the same. CLI commands fail; the admin API returns a 409 `configuration_locked`
status in its response envelope. The panel shows read-only settings and their source.
Explicit `false` is a protected value; omitting a setting permits database management.

Select one file with `--config /path/to/config.toml` (or `.conf`) for **both** the
server and CLI. The usual installed path is `/etc/madmail/madmail.conf`. These are
alternative formats, not two automatically merged files. `.toml` selects TOML;
`.conf` selects Madmail block syntax. Restart after changing files or environment.
`madmail reload` re-resolves database-backed runtime values; it does not reread files
or obtain changed systemd environment variables.

## Examples

Top-level TOML (place before any TOML table headers):

```toml
admin_web_enabled = true
admin_web_path = "/private-admin"
admin_path = "/private-api"
message_retention_enabled = true
message_retention = "14d"
turn_relay_port_min = 50000
turn_relay_port_max = 50100
```

Equivalent `.conf`:

```conf
settings {
    admin_web_enabled true
    admin_web_path /private-admin
    admin_path /private-api
    message_retention_enabled true
    message_retention 14d
    turn_relay_port_min 50000
    turn_relay_port_max 50100
}
```

The `settings` block accepts the operational names in the registry below. Existing
module directives still work: admin paths in `chatmail { ... }`, relay bounds in
`turn { ... }`, and `retention 14d` in `storage.imapsql { ... }`. Retention duration
alone does not enable deletion; also set `message_retention_enabled true` in
`settings`. Do not specify the same setting in multiple file locations; the explicit
`settings` value is intended to replace its module directive, not duplicate it.

For an environment override, use e.g. `CHATMAIL_ADMIN_WEB_PATH=/private-admin`.
Legacy listener overrides `CHATMAIL_SMTP_ADDR`, `CHATMAIL_IMAP_ADDR`, and
`CHATMAIL_HTTP_ADDR` also protect their effective port and bind mode.

## Diagnostics and migration

At startup Madmail writes source-only warnings to stderr, visible in systemd with
`journalctl -u madmail`. They name each protected setting and its controlling source,
never secret values. These operator configuration warnings are emitted even under
No-Log; mail and traffic logging remain disabled. `madmail status --json` includes
`configuration_locks`. The authenticated settings API includes
`configuration_sources` with `source`, `locked`, and `database_value_present`.

Earlier versions often preferred database values. After upgrading, explicit file
values take priority. Existing database values are preserved but inactive while
protected; removing the file/environment value and restarting exposes them again.
Review existing configuration and database settings before upgrading. Leave optional
file values commented out if you want to manage them from the panel or CLI.

The service writes source-only `configuration-locks.json` in its state directory.
Local CLI commands honor this snapshot even when their environment differs from
systemd. It remains conservative while the service is stopped: change the source
and restart to refresh locks. With no server snapshot, offline CLI commands use
only their selected file/environment. Status from an offline CLI describes its local effective values; the running API remains authoritative for live environment-derived values. Direct SQL edits are outside these safeguards.

Enabling admin-web no longer writes `/admin` into the database. If an older version
left a database path, the new file path wins. The web path and API path are separate
and must not overlap. Invalid/unknown TOML fields and unknown `settings` directives
fail validation; legacy module syntax remains compatible.

## Operational registry

The following names work as top-level TOML scalars and in `.conf` `settings`.
Their environment names are `CHATMAIL_` plus the uppercase name. Unsupported
legacy transports and private webhook JSON are not generic settings; webhooks
remain managed through their dedicated authenticated API.

| Setting | Database key |
|---|---|
| `registration_open` | `__REGISTRATION_OPEN__` |
| `jit_registration_enabled` | `__JIT_REGISTRATION_ENABLED__` |
| `registration_token_required` | `__REGISTRATION_TOKEN_REQUIRED__` |
| `turn_enabled` | `__TURN_ENABLED__` |
| `iroh_enabled` | `__IROH_ENABLED__` |
| `ss_enabled` | `__SS_ENABLED__` |
| `auto_purge_seen` | `__AUTO_PURGE_SEEN__` |
| `message_retention_enabled` | `__MESSAGE_RETENTION_ENABLED__` |
| `message_retention` | `__MESSAGE_RETENTION__` |
| `admin_web_enabled` | `__ADMIN_WEB_ENABLED__` |
| `webimap_enabled` | `__WEBIMAP_ENABLED__` |
| `websmtp_enabled` | `__WEBSMTP_ENABLED__` |
| `webmail_cors_origins` | `__WEBMAIL_CORS_ORIGINS__` |
| `push_enabled` | `__PUSH_ENABLED__` |
| `push_mode` | `__PUSH_MODE__` |
| `federation_policy` | `__FEDERATION_POLICY__` |
| `federation_enabled` | `__FEDERATION_ENABLED__` |
| `allow_inbound_remote_rcpt` | `__ALLOW_INBOUND_REMOTE_RCPT__` |
| `shared_port_imap` | `__SHARED_PORT_IMAP__` |
| `shared_port_smtp` | `__SHARED_PORT_SMTP__` |
| `smtp_port` | `__SMTP_PORT__` |
| `submission_port` | `__SUBMISSION_PORT__` |
| `submission_tls_port` | `__SUBMISSION_TLS_PORT__` |
| `imap_port` | `__IMAP_PORT__` |
| `imap_tls_port` | `__IMAP_TLS_PORT__` |
| `turn_port` | `__TURN_PORT__` |
| `sasl_port` | `__SASL_PORT__` |
| `iroh_port` | `__IROH_PORT__` |
| `ss_port` | `__SS_PORT__` |
| `http_port` | `__HTTP_PORT__` |
| `https_port` | `__HTTPS_PORT__` |
| `http_enabled` | `__HTTP_ENABLED__` |
| `https_enabled` | `__HTTPS_ENABLED__` |
| `smtp_local_only` | `__SMTP_LOCAL_ONLY__` |
| `submission_local_only` | `__SUBMISSION_LOCAL_ONLY__` |
| `submission_tls_local_only` | `__SUBMISSION_TLS_LOCAL_ONLY__` |
| `imap_local_only` | `__IMAP_LOCAL_ONLY__` |
| `imap_tls_local_only` | `__IMAP_TLS_LOCAL_ONLY__` |
| `turn_local_only` | `__TURN_LOCAL_ONLY__` |
| `sasl_local_only` | `__SASL_LOCAL_ONLY__` |
| `iroh_local_only` | `__IROH_LOCAL_ONLY__` |
| `http_local_only` | `__HTTP_LOCAL_ONLY__` |
| `https_local_only` | `__HTTPS_LOCAL_ONLY__` |
| `smtp_hostname` | `__SMTP_HOSTNAME__` |
| `turn_realm` | `__TURN_REALM__` |
| `turn_secret` | `__TURN_SECRET__` |
| `turn_relay_ip` | `__TURN_RELAY_IP__` |
| `turn_relay_port_min` | `__TURN_RELAY_PORT_MIN__` |
| `turn_relay_port_max` | `__TURN_RELAY_PORT_MAX__` |
| `turn_ttl` | `__TURN_TTL__` |
| `iroh_relay_url` | `__IROH_RELAY_URL__` |
| `ss_cipher` | `__SS_CIPHER__` |
| `ss_password` | `__SS_PASSWORD__` |
| `admin_path` | `__ADMIN_PATH__` |
| `admin_web_path` | `__ADMIN_WEB_PATH__` |
| `dclogin_imap_security` | `__DCLOGIN_IMAP_SECURITY__` |
| `dclogin_smtp_security` | `__DCLOGIN_SMTP_SECURITY__` |
| `language` | `__LANGUAGE__` |
| `appendlimit` | `__APPENDLIMIT__` |
| `max_message_size` | `__MAX_MESSAGE_SIZE__` |
| `max_federation_size` | `__MAX_FEDERATION_SIZE__` |

## Reproducing the source-precedence checks

After building the shipping image (`docker build -t madmail-local:test-docker .`),
run this from the repository root:

```sh
python3 tests/configuration-sources-docker.py --image madmail-local:test-docker
```

The check uses the TOML and `.conf` examples above in disposable Docker containers.
It verifies file-over-database and environment-over-file precedence, explicit false,
rejected CLI/API writes and resets, environment-only protection when the CLI lacks
the service environment, restart behavior, and restored database values after
removing protections. It also rejects unknown settings, invalid booleans and
reversed TURN ranges. It removes its containers and volumes afterward. Docker and
Python 3 are required; production state is not used.
