# Configuration (Madmail-compatible)

madmail-v2 reads the same static configuration sources as **Madmail**:

| Source | Parser | Notes |
|--------|--------|-------|
| `maddy.conf` | `chatmail-config::parse_maddy_conf_str` | Primary production format |
| `chatmail.toml` | TOML subset | Optional simplified overlay |
| CLI | `--state-dir`, `--config` | Overrides paths only; `log` / `debug` in config file (see [`14-cli-tools.md`](14-cli-tools.md)) |

Reference: [`context/madmail/maddy.conf`](../../context/madmail/maddy.conf), [`settings_db.md`](../../context/madmail/docs/chatmail/settings_db.md).

## Global variables (`$(name) = value`)

| Variable | Used for |
|----------|----------|
| `$(hostname)` | SMTP EHLO, TLS, DKIM |
| `$(primary_domain)` | Local delivery domain |
| `$(local_domains)` | Accepted recipient domains (space-separated) |
| `$(public_ip)` | QR, TURN, Iroh discovery |

## Top-level directives

| Directive | Maps to `AppConfig` |
|-----------|---------------------|
| `state_dir` | Persistent data root (`credentials.db`, `imapsql.db`, maildir) |
| `runtime_dir` | PID / runtime sockets |
| `debug` | `yes` → debug logging |
| `log` | `stderr` / `off` / `syslog` (default: off when omitted) |
| `max_federation_size` | `max_federation_size` (e.g. `70M`) — `/mxdeliv` HTTP body cap; see [`07-federation.md`](07-federation.md) |
| `hostname` | SMTP hostname when not only in `$(hostname)` |
| `tls { loader … }` | Parsed as `tls_mode` hint; **runtime** uses `tls file` PEM paths only |
| `tls file <cert> <key>` | `tls_cert_path`, `tls_key_path` — used by madmail-v2 TLS listeners |

Environment substitution `{env:VAR}` in values is expanded when the variable is set.

## Module blocks parsed today

### `auth.pass_table`

| Directive | `AppConfig` field |
|-----------|-------------------|
| `auto_create yes` | `auth_auto_create` |
| `jit_domain` | `jit_domain` (defaults to `primary_domain`) |
| `table sql_table { driver; dsn }` | `credentials_driver`, `credentials_dsn` |
| `dsn credentials.db` | `credentials_dsn` (legacy / flat form, relative to `state_dir` for SQLite) |

### `storage.imapsql`

| Directive | `AppConfig` field |
|-----------|-------------------|
| `driver` / `dsn` | `imapsql_driver`, `imapsql_dsn` — `sqlite3` (default) or `postgres` (libpq DSN; Madmail schema import supported) |
| `default_quota` | `default_quota` (e.g. `1G`) |
| `retention` | `retention` (e.g. `24h`) — hourly maildir purge when server runs; see [`21-scheduled-maintenance.md`](21-scheduled-maintenance.md) |
| `unused_account_retention` | `unused_account_retention` (e.g. `720h`) — delete never-logged-in accounts |
| `appendlimit` | `appendlimit` (e.g. `32M`) |
| `mail_fsync` | `mail_fsync` — `always` (default), `optimized`, or `never` (Dovecot parity; see [`04-storage-layer.md`](04-storage-layer.md)) |
| `blob_dedup` | `blob_dedup` — `on` (default) or `off`; content-addressed dedup under `{state_dir}/blobs/` |

### `smtp` / `submission` blocks

| Directive | `AppConfig` field |
|-----------|-------------------|
| `max_message_size` | `max_message_size` (e.g. `100M`) — combined with `appendlimit` via `data_size::resolve_max_message_bytes` |

### `target.queue remote_queue`

Outbound federation retry queue (see [`07-federation.md`](07-federation.md)):

| Directive | `AppConfig.queue` field | Default |
|-----------|-------------------------|---------|
| `location` | `location` | `{state_dir}/remote_queue` |
| `max_tries` | `max_tries` | `3` |
| `max_parallelism` | `max_parallelism` | `16` |
| `initial_retry` | `initial_retry_secs` | `60` (1m) |
| `retry_time_scale` | `retry_time_scale` | `1.25` |
| `post_init_delay` | `post_init_delay_secs` | `10` |
| `max_delivery_time` / `delivery_timeout` | `max_delivery_secs` | `600` (10m) |

### Listen endpoints

Lines such as `smtp tcp://0.0.0.0:25`, `submission tls://… tcp://…`, `imap tls://… tcp://…`, `chatmail tls://…` populate:

- `smtp_listen`, `submission_listen`, `submission_tls_listen`
- `imap_listen`, `imap_tls_listen`
- `http_listen` (HTTPS admin + `/mxdeliv`)

Boot prefers `CHATMAIL_*_ADDR` env vars, then config listen addresses, then dev defaults (`2525` / `1143` / `8080`).

### `chatmail` block

| Directive | Field / behavior | Default |
|-----------|------------------|---------|
| `mail_domain` | `mail_domain` / `primary_domain` | — |
| `mx_domain` | `mx_domain` | — |
| `public_ip` | `public_ip` | — |
| `username_length` | Random localpart length for `POST /new` | `8` |
| `password_length` | Random password length for `POST /new` | `16` |
| `min_username_length` | Minimum localpart length (JIT create, login validation) | `8` |
| `max_username_length` | Maximum localpart length | `20` |
| `password_min_length` | Minimum password length (JIT create) | `8` |
| `admin_path` | Admin JSON-RPC URL path | `/api/admin` |
| `admin_web_path` | Embedded admin SPA mount path | `/admin` |
| `admin_token` | Literal bearer token or `disabled` | — |
| `language` | Default www UI language (`en`, `fa`, `ru`, `es`) | — |
| `www_dir` | External www root (`html-serve` override) | embedded assets |
| `ss_addr` / `ss_password` / `ss_cipher` / `ss_cert` / `ss_key` / `ss_allowed_ports` | Shadowsocks proxy (see [`11-proxy-services.md`](11-proxy-services.md)) | — |

Runtime SS config merges file directives with DB overrides (`__SS_ENABLED__`, `__SS_PORT__`, …) via `chatmail-shadowsocks::resolve_runtime`. Admin toggle `/admin/services/shadowsocks` requires `ss_addr` + `ss_password` in config.

Madmail reference: [`context/madmail/dist/config/maddy.example.conf`](../../context/madmail/dist/config/maddy.example.conf) (`username_length`, `password_length`, `min_username_length`, `max_username_length`). madmail-v2 also supports `password_min_length` (cmrelay `chatmail.ini` parity).

`username_length` is clamped to `[min_username_length, max_username_length]`. Generated passwords use `max(password_length, password_min_length)`.

### `imap` block (TURN + Iroh discovery)

| Directive | `AppConfig` field | Notes |
|-----------|-------------------|-------|
| `turn_enable` | `turn_enable` | TURN METADATA + embedded relay |
| `turn_server` / `turn_port` / `turn_secret` / `turn_ttl` | same | See [`11-proxy-services.md`](11-proxy-services.md) |
| `iroh_relay_url` | `iroh_relay_url`, sets `iroh_enable` | Advertised at `/shared/vendor/deltachat/irohrelay` |

### `turn { … }` block

| Directive | `AppConfig` field |
|-----------|-------------------|
| `realm` | `turn_realm` |
| `secret` | `turn_secret` (also sets `turn_enable`) |
| `relay_ip` | `turn_relay_ip` |
| `debug` | `turn_debug` |
| `test_force_relay` | `turn_test_force_relay` |

Listen addresses: `turn udp://… tcp://… { }` → `turn_listen_udp`, `turn_listen_tcp`.

Runtime overrides: `__TURN_*__`, `__IROH_*__` in the settings DB ([`09-admin-api.md`](09-admin-api.md)).

Example:

```text
chatmail tls://0.0.0.0:443 {
    mail_domain $(primary_domain)
    username_length 8
    password_length 16
    min_username_length 8
    max_username_length 20
    password_min_length 8
}
```

Implementation: `chatmail-config::CredentialPolicy`, enforced in `chatmail-auth::validate_localpart_and_password` on JIT account creation.

## Dynamic settings (database)

Stored in the `settings` table (`key` / `value` TEXT). **Source of truth for key names:** `crates/chatmail-db/src/settings_keys.rs`. Madmail parity reference: [`context/madmail/docs/chatmail/settings_db.md`](../../context/madmail/docs/chatmail/settings_db.md) (partial — several Madmail keys are not implemented in madmail-v2; see gaps below).

Admin: `GET /admin/settings` (bulk) or `GET|POST /admin/settings/{name}` (`set` / `reset`). Service toggles use dedicated resources (`/admin/registration`, `/admin/services/turn`, …). Soft reload required for port/listener changes.

### Toggle settings (`"true"` / `"false"`)

| DB key | Default when unset | Admin resource / setting | Usage |
|--------|---------------------|--------------------------|-------|
| `__REGISTRATION_OPEN__` | `false` | `/admin/registration` | `POST /new` open/closed; CLI `madmail registration` |
| `__JIT_REGISTRATION_ENABLED__` | `true` | `/admin/registration/jit` | First-login account create; falls back to registration open in `AuthCache` hydrate |
| `__REGISTRATION_TOKEN_REQUIRED__` | `false` | `/admin/settings/registration_token_required` | Require token on `/new` |
| `__TURN_ENABLED__` | `true` | `/admin/services/turn` | Embedded TURN + IMAP METADATA |
| `__IROH_ENABLED__` | `true` | `/admin/services/iroh` | Embedded iroh-relay + IMAP metadata |
| `__SS_ENABLED__` | `true` (only if SS configured in file) | `/admin/services/shadowsocks` | Raw TCP Shadowsocks relay |
| `__SS_WS_ENABLED__` | — | `/admin/services/ss_ws` | **Not implemented** — always disabled |
| `__SS_GRPC_ENABLED__` | — | `/admin/services/ss_grpc` | **Not implemented** — always disabled |
| `__AUTO_PURGE_SEEN__` | `false` | `/admin/services/auto_purge_seen` | Delete maildir `cur/` every 15s |
| `__MESSAGE_RETENTION_ENABLED__` | `false` | settings bundle `message_retention_enabled` | Hourly `prune-old-messages` |
| `__HTTP_PROXY_ENABLED__` | — | `/admin/services/http_proxy` | **Not implemented** |
| `__ADMIN_WEB_ENABLED__` | `false` | `/admin/services/admin_web` | Embedded admin SPA |
| `__WEBIMAP_ENABLED__` | `false` | `/admin/services/webimap` | WebIMAP REST + WS |
| `__WEBSMTP_ENABLED__` | `false` | `/admin/services/websmtp` | WebSMTP submit API |
| `__PUSH_ENABLED__` | `false` | settings bundle `push_enabled` | Legacy mirror of push on/off |
| `__FEDERATION_ENABLED__` | `false` | `/admin/settings/federation` | Outbound federation master toggle |

**Push mode** (separate from boolean toggles): `__PUSH_MODE__` = `auto` \| `on` \| `off` (default **`off`**). Admin `/admin/services/push`, CLI `madmail push` — see [23-push-notifications.md](23-push-notifications.md).

**Federation policy** (string, not bool): `__FEDERATION_POLICY__` = `ACCEPT` \| `REJECT`. Admin `/admin/settings/federation`.

**Not in madmail-v2:** Madmail `__LOG_DISABLED__` / `/admin/services/log` — No-Log is static only (`log off` in `maddy.conf`; see [12-security.md](12-security.md)).

### Port overrides (empty = use `maddy.conf` listen address)

| DB key | Admin path | Service |
|--------|------------|---------|
| `__SMTP_PORT__` | `/admin/settings/smtp_port` | SMTP inbound |
| `__SUBMISSION_PORT__` | `/admin/settings/submission_port` | Submission STARTTLS |
| `__SUBMISSION_TLS_PORT__` | `/admin/settings/submission_tls_port` | Submission SMTPS |
| `__IMAP_PORT__` | `/admin/settings/imap_port` | IMAP plain |
| `__IMAP_TLS_PORT__` | `/admin/settings/imap_tls_port` | IMAPS |
| `__TURN_PORT__` | `/admin/settings/turn_port` | TURN/STUN |
| `__SASL_PORT__` | `/admin/settings/sasl_port` | SASL (Madmail: `__DOVECOT_PORT__`) |
| `__IROH_PORT__` | `/admin/settings/iroh_port` | Iroh relay |
| `__SS_PORT__` | `/admin/settings/ss_port` | Shadowsocks |
| `__SS_WS_PORT__` | `/admin/settings/ss_ws_port` | Stored; transport disabled |
| `__SS_GRPC_PORT__` | `/admin/settings/ss_grpc_port` | Stored; transport disabled |
| `__HTTP_PORT__` | `/admin/settings/http_port` | HTTP plain |
| `__HTTPS_PORT__` | `/admin/settings/https_port` | HTTPS |
| `__HTTP_PROXY_PORT__` | `/admin/settings/http_proxy_port` | **Not implemented** |

### Per-port access (`"true"` = bind localhost only)

| DB key | Admin path |
|--------|------------|
| `__SMTP_LOCAL_ONLY__` | `/admin/settings/smtp_local_only` |
| `__SUBMISSION_LOCAL_ONLY__` | `/admin/settings/submission_local_only` |
| `__SUBMISSION_TLS_LOCAL_ONLY__` | `/admin/settings/submission_tls_local_only` |
| `__IMAP_LOCAL_ONLY__` | `/admin/settings/imap_local_only` |
| `__IMAP_TLS_LOCAL_ONLY__` | `/admin/settings/imap_tls_local_only` |
| `__TURN_LOCAL_ONLY__` | `/admin/settings/turn_local_only` |
| `__SASL_LOCAL_ONLY__` | `/admin/settings/sasl_local_only` |
| `__IROH_LOCAL_ONLY__` | `/admin/settings/iroh_local_only` |
| `__HTTP_LOCAL_ONLY__` | `/admin/settings/http_local_only` |
| `__HTTPS_LOCAL_ONLY__` | `/admin/settings/https_local_only` |

### Value settings (strings)

| DB key | Admin path | Usage |
|--------|------------|-------|
| `__SMTP_HOSTNAME__` | `smtp_hostname` | EHLO / autoconfig |
| `__TURN_REALM__` | `turn_realm` | TURN realm |
| `__TURN_SECRET__` | `turn_secret` | TURN REST HMAC secret |
| `__TURN_RELAY_IP__` | `turn_relay_ip` | Relay interface |
| `__TURN_TTL__` | `turn_ttl` | Credential TTL (seconds) |
| `__IROH_RELAY_URL__` | `iroh_relay_url` | IMAP `/shared/vendor/deltachat/irohrelay` |
| `__SS_CIPHER__` | `ss_cipher` | Shadowsocks cipher |
| `__SS_PASSWORD__` | `ss_password` | Shadowsocks password |
| `__HTTP_PROXY_PATH__` | `http_proxy_path` | **Not implemented** |
| `__HTTP_PROXY_USERNAME__` | `http_proxy_username` | **Not implemented** |
| `__HTTP_PROXY_PASSWORD__` | `http_proxy_password` | **Not implemented** |
| `__ADMIN_PATH__` | `admin_path` | JSON-RPC mount (default `/api/admin`) |
| `__ADMIN_WEB_PATH__` | `admin_web_path` | Admin SPA mount (default `/admin`) |
| `__DCLOGIN_IMAP_SECURITY__` | `dclogin_imap_security` | `ssl` / `starttls` / `plain` in dclogin URLs |
| `__DCLOGIN_SMTP_SECURITY__` | `dclogin_smtp_security` | Same for SMTP hints |
| `__LANGUAGE__` | `language` | www UI language (`en`, `fa`, `ru`, `es`) |
| `__APPENDLIMIT__` | `appendlimit` | IMAP append cap (e.g. `100M`) |
| `__MAX_MESSAGE_SIZE__` | `max_message_size` | SMTP cap; effective = min(appendlimit, max) |
| `__MAX_FEDERATION_SIZE__` | `max_federation_size` | `/mxdeliv` HTTP body cap (default `70M`; seeded on install) |
| `__MESSAGE_RETENTION__` | `message_retention` | Duration (`30d`, `720h`, …) when retention enabled |

CLI: [`madmail port`](../guide/cli/port.md), [`madmail message-size`](../guide/cli/message-size.md). Ports and dclogin hints are read via `chatmail-config::effective_*` at listener bind and on www page render.

`log off` (or omit `log`) is the default (No-Log). Use `log stderr`, `log /path/to/file`, or both (`log stderr /var/lib/madmail/madmail.log`) to enable tracing. `log syslog` currently maps to stderr. Restart required for static `log` / `debug` directives.

Boolean flags in `maddy.conf` / `chatmail.toml` and DB settings accept flexible enable/disable forms (case-insensitive):

| Enable | Disable (default for unknown) |
|--------|-------------------------------|
| `true`, `yes`, `y`, `1`, `on`, `enable`, `enabled`, `t` | `false`, `no`, `n`, `0`, `off`, `disable`, `disabled`, `f`, empty |

Shared parser: `chatmail_config::parse_bool_str` (used by config file, `get_bool_setting` / `get_enabled_setting`, admin toggles).

## Database layout vs Madmail

| Madmail | madmail-v2 |
|---------|-------------|
| `state_dir/credentials.db` (passwords KV + settings) | Single `state_dir/chatmail.db` by default |
| `state_dir/imapsql.db` (quotas, federation, mail index) | Same tables in `chatmail.db` |

When importing a Madmail `passwords` table (`key`/`value` columns), `chatmail-db::passwords` auto-detects the schema.

## TLS certificates

Top-level directives:

| Directive | `AppConfig` field |
|-----------|-------------------|
| `acme_email` | `acme_email` — ACME contact (default: `admin@<domain>` via `effective_acme_email`) |
| `tls_mode autocert` | `tls_mode` — enables in-process daily renewal when server runs (see [`19-certificates.md`](19-certificates.md)) |

madmail-v2 does not run maddy’s in-process `autocert` TLS loader on first connection. Use `madmail install` / `madmail certificate get` (instant-acme HTTP-01) and `tls file` paths, or `tls_mode autocert` for scheduled renewal via `chatmail-tasks`.

## OpenMetrics

| Directive | Field |
|-----------|-------|
| `openmetrics tcp://host:port { }` | `openmetrics_listen` — Prometheus scrape endpoint (`chatmail-metrics`) |

## Implementation references

| Component | Path |
|-----------|------|
| Maddy parser | `crates/chatmail-config/src/maddy.rs` |
| AST lexer/parser | `crates/chatmail-config/src/madmail_lexer.rs`, `madmail_parse.rs` |
| Queue settings | `crates/chatmail-config/src/queue.rs` |
| Data sizes (`1G`, `100M`) | `crates/chatmail-config/src/data_size.rs` |
| Effective listen ports | `crates/chatmail-config/src/client_mail.rs` |
| DB path resolution | `crates/chatmail-config/src/db_path.rs` |
| Credential limits | `crates/chatmail-config/src/credential_policy.rs` |
| Length validation | `crates/chatmail-auth/src/validate.rs` |
| TOML loader | `crates/chatmail-config/src/parse.rs` |
| Settings keys | `crates/chatmail-db/src/settings_keys.rs` |
| Storage policy wiring | `crates/chatmail-storage/src/storage_policy.rs`, `chatmail-state/src/lib.rs` |
| Message retention (DB) | `crates/chatmail-db/src/message_retention.rs` |
| CLI operators | [`../guide/cli/README.md`](../guide/cli/README.md) · TDD [14-cli-tools.md](14-cli-tools.md) |
| Madmail settings constants | `context/madmail/internal/api/admin/resources/settings.go` |
| ACME / install | `crates/chatmail-acme/`, `crates/chatmail-config/src/config_autocert.rs`, `chatmail/src/ctl/install/`, `ctl/certificate.rs` |

## Related RFCs

Configuration drives TLS listeners, submission ports, and certificate automation. Index: [`RFC/README.md`](RFC/README.md).

| RFC | Topic | Local file |
|-----|-------|------------|
| [8314](https://datatracker.ietf.org/doc/html/rfc8314) | TLS for SMTP submission (465/587) | [rfc8314.txt](RFC/rfc8314.txt) |
| [8446](https://datatracker.ietf.org/doc/html/rfc8446) | TLS 1.3 (`tls file`, HTTPS/IMAP/SMTP) | [rfc8446.txt](RFC/rfc8446.txt) |
| [8555](https://datatracker.ietf.org/doc/html/rfc8555) | ACME (Let's Encrypt via `chatmail-acme`) | [rfc8555.txt](RFC/rfc8555.txt) |
| [6409](https://datatracker.ietf.org/doc/html/rfc6409) | Message submission ports | [rfc6409.txt](RFC/rfc6409.txt) |
| [3501](https://datatracker.ietf.org/doc/html/rfc3501) | IMAP listener settings | [rfc3501.txt](RFC/rfc3501.txt) |
| [5321](https://datatracker.ietf.org/doc/html/rfc5321) | SMTP listener settings | [rfc5321.txt](RFC/rfc5321.txt) |
| [9110](https://datatracker.ietf.org/doc/html/rfc9110) | HTTP listener settings | [rfc9110.txt](RFC/rfc9110.txt) |
| [8615](https://datatracker.ietf.org/doc/html/rfc8615) | `/.well-known/` URIs (autoconfig path prefix) | [rfc8615.txt](RFC/rfc8615.txt) |
| [2595](https://datatracker.ietf.org/doc/html/rfc2595) | IMAP STARTTLS on cleartext port 143 | [rfc2595.txt](RFC/rfc2595.txt) |
| [3207](https://datatracker.ietf.org/doc/html/rfc3207) | SMTP STARTTLS on submission port 587 | [rfc3207.txt](RFC/rfc3207.txt) |

**Autoconfig XML** (`/.well-known/autoconfig/mail/config-v1.1.xml`) is **not** an IETF RFC — it follows the Mozilla ISPDB format; see [`RFC/README.md` — Autoconfig](RFC/README.md#autoconfig-not-an-ietf-rfc).

Implementation: `chatmail-config::autoconfig`, served by `chatmail-www` at `GET /.well-known/autoconfig/mail/config-v1.1.xml`.

| Behaviour | Notes |
|-----------|--------|
| Advertises SSL + STARTTLS IMAP/SMTP entries when both listener types are bound | Ports from runtime listeners + DB overrides |
| **Does not** advertise IMAP-over-HTTPS ALPN on port 443 | `has_imap_alpn_https` is always false until `chatmail-fed` implements ALPN IMAP |
| TLS certificate required when plain IMAP/submission bound | Supervisor calls `listeners_need_tls_cert` — PEM loaded for STARTTLS upgrade on 143/587 |

Unit tests: `autoconfig_includes_ssl_and_starttls_when_both_listeners`, `autoconfig_omits_https_alpn_even_when_http_tls_bound`, `mail_autoconfig_omits_https_alpn_entry` (www integration).
