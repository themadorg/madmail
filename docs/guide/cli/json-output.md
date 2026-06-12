# JSON output (`--json`)

Every `madmail` subcommand accepts a global **`--json`** flag. In JSON mode:

- **Stdout** receives a single JSON object (success envelope or legacy bare JSON — see below).
- **Stderr** receives `{"ok":false,"error":"..."}` on failure.
- Decorative text, emoji, and QR codes are **not** printed.
- Confirmation prompts still apply unless you pass `-y` / `--yes`.

```bash
madmail --json accounts status
madmail accounts status --json   # same — flag is global
```

Also on [global flags](global-flags.md).

---

## Success envelope

Most commands wrap results like this:

```json
{
  "ok": true,
  "command": "accounts status",
  "data": { }
}
```

Mutating commands often add a human-readable summary:

```json
{
  "ok": true,
  "command": "accounts create",
  "message": "Created account: alice@example.org",
  "data": {
    "username": "alice@example.org"
  }
}
```

Cancelled confirmations (`aborted`):

```json
{
  "ok": true,
  "command": "accounts delete",
  "message": "aborted",
  "data": {}
}
```

---

## Error envelope

Written to **stderr**; process exits with code `1`:

```json
{
  "ok": false,
  "error": "account already exists: bob@example.org"
}
```

---

## Exceptions

| Case | Behavior |
|------|----------|
| `create-user --json-only` (no `--json`) | Legacy bare JSON: `{"dclogin":"..."}` — no envelope |
| `create-user --json` or `accounts create-random --json` | Standard envelope with `username`, `password`, `email`, `dclogin` in `data` |
| `admin-token --raw` (no `--json`) | Raw token only on stdout |
| `admin-token --json` | Envelope with `token` and `api_url` (no QR) |
| `accounts export` | Writes JSON array to stdout (file or pipe) — same format with or without `--json` on the export file content; envelope not used for file body |
| Not-implemented commands | Error envelope |

---

## Server lifecycle

### `run`

Not applicable — server mode does not use ctl JSON output.

### `version`

```json
{
  "ok": true,
  "command": "version",
  "data": {
    "name": "madmail-v2",
    "version": "0.1.0"
  }
}
```

### `status`

```json
{
  "ok": true,
  "command": "status",
  "data": {
    "services": [
      { "name": "IMAP", "connections": 3, "unique_ips": 2 },
      { "name": "TURN", "connections": 0, "unique_ips": 0 },
      { "name": "Shadowsocks", "connections": 1, "unique_ips": 1 }
    ],
    "registered_users": 42,
    "ports": [
      {
        "port": "993",
        "proto": "tcp",
        "label": "IMAP TLS",
        "service": "IMAP",
        "connections": 3,
        "unique_ips": 2
      }
    ],
    "server_tracker": {
      "boot_time": 1718123456,
      "uptime_seconds": 86400,
      "unique_conn_ips": 15,
      "unique_domains": 8,
      "unique_ip_servers": 4
    }
  }
}
```

`ports` and `server_tracker` are omitted when unavailable. Use `--details` to include `ports`.

### `reload`

```json
{
  "ok": true,
  "command": "reload",
  "data": {
    "api_url": "https://mail.example.org/api/admin",
    "reloaded": true
  }
}
```

### `upgrade` / `update`

```json
{
  "ok": true,
  "command": "upgrade",
  "data": {}
}
```

### `install`

```json
{
  "ok": true,
  "command": "install",
  "message": "Install completed",
  "data": {
    "config_path": "/etc/madmail/madmail.conf",
    "state_dir": "/var/lib/madmail"
  }
}
```

### `uninstall`

Dry-run / completed uninstall returns paths and flags in `data`. When nothing to uninstall:

```json
{
  "ok": true,
  "command": "uninstall",
  "data": {
    "found": false
  }
}
```

---

## Admin & access

### `admin-token`

```json
{
  "ok": true,
  "command": "admin-token",
  "data": {
    "token": "abc123…",
    "api_url": "https://mail.example.org/api/admin"
  }
}
```

### `admin-web status`

```json
{
  "ok": true,
  "command": "admin-web status",
  "data": {
    "enabled": true,
    "path": "/admin"
  }
}
```

### `admin-web enable` / `disable` / `path`

```json
{
  "ok": true,
  "command": "admin-web enable",
  "message": "Admin web dashboard enabled",
  "data": {
    "enabled": true
  }
}
```

### `certificate status`

```json
{
  "ok": true,
  "command": "certificate status",
  "data": {
    "tls_mode": "autocert",
    "domain": "mail.example.org",
    "valid": true,
    "expires_at": "2026-09-01T12:00:00Z"
  }
}
```

### `certificate get` / `regenerate`

```json
{
  "ok": true,
  "command": "certificate get",
  "message": "Certificate issued",
  "data": {
    "domain": "mail.example.org",
    "action": "issued"
  }
}
```

### `certificate autocert status` / `enable`

```json
{
  "ok": true,
  "command": "certificate autocert status",
  "data": {
    "enabled": true,
    "email": "admin@example.org"
  }
}
```

---

## Accounts & registration

### accounts status

```json
{
  "ok": true,
  "command": "accounts status",
  "data": {
    "login_count": 12,
    "registration_open": false,
    "token_required": true,
    "jit_enabled": true,
    "blocklisted": 2,
    "mail_directories": 12,
    "state_dir": "/var/lib/madmail",
    "database": "/var/lib/madmail/credentials.db"
  }
}
```

### `accounts info`

```json
{
  "ok": true,
  "command": "accounts info",
  "data": {
    "username": "alice@example.org",
    "credentials": true,
    "blocklisted": false,
    "block_reason": null,
    "created_at": 1718000000,
    "first_login_at": 1718001000,
    "last_login_at": 1718100000,
    "maildir_present": true,
    "maildir_path": "/var/lib/madmail/mail/a/alice@example.org"
  }
}
```

### `accounts create`

```json
{
  "ok": true,
  "command": "accounts create",
  "message": "Created account: alice@example.org",
  "data": {
    "username": "alice@example.org"
  }
}
```

### `accounts create-random` / `create-user`

```json
{
  "ok": true,
  "command": "create-user",
  "data": {
    "username": "x7k2m9p4q1w8",
    "password": "…",
    "email": "x7k2m9p4q1w8@example.org",
    "dclogin": "dclogin:x7k2m9p4q1w8@example.org/?p=…&…"
  }
}
```

### `accounts delete` / `ban` / `delete` (top-level)

```json
{
  "ok": true,
  "command": "accounts delete",
  "message": "Deleted and blocklisted: alice@example.org",
  "data": {
    "username": "alice@example.org",
    "reason": "account deleted via CLI"
  }
}
```

### `accounts unban`

```json
{
  "ok": true,
  "command": "accounts unban",
  "message": "Unbanned: alice@example.org",
  "data": {
    "username": "alice@example.org"
  }
}
```

### `accounts ban-list` / `ban-list` / `blocklist list`

```json
{
  "ok": true,
  "command": "accounts ban-list",
  "data": {
    "entries": [
      {
        "username": "spammer@example.org",
        "reason": "spam",
        "blocked_at": "2026-06-01T10:00:00Z"
      }
    ]
  }
}
```

Empty list: `"entries": []`

### `blocklist add` / `remove`

```json
{
  "ok": true,
  "command": "blocklist add",
  "message": "Blocked: bad@example.org (spam)",
  "data": {
    "username": "bad@example.org",
    "reason": "spam"
  }
}
```

### `accounts export`

With `-o file`: file contains JSON array (unchanged). Without `-o` and `--json`:

```json
{
  "ok": true,
  "command": "accounts export",
  "data": {
    "users": [
      { "username": "alice@example.org", "hash": "…" }
    ]
  }
}
```

### `accounts import` / `delete-all`

```json
{
  "ok": true,
  "command": "accounts import",
  "message": "Imported 5 account(s)",
  "data": {
    "imported": 5,
    "skipped": 0
  }
}
```

### `registration status`

```json
{
  "ok": true,
  "command": "registration status",
  "data": {
    "open": false
  }
}
```

### `registration open` / `close`

```json
{
  "ok": true,
  "command": "registration open",
  "message": "Registration is now OPEN",
  "data": {
    "open": true
  }
}
```

### `registration-tokens list`

```json
{
  "ok": true,
  "command": "registration-tokens list",
  "data": {
    "tokens": [
      {
        "token": "abc123",
        "max_uses": 5,
        "uses": 2,
        "comment": "Team invite",
        "expires_at": "2026-06-15T00:00:00Z",
        "created_at": "2026-06-01T00:00:00Z"
      }
    ]
  }
}
```

### `registration-tokens create`

```json
{
  "ok": true,
  "command": "registration-tokens create",
  "message": "Token created",
  "data": {
    "token": "abc123",
    "max_uses": 5,
    "expires_at": "2026-06-15T00:00:00Z"
  }
}
```

### `registration-tokens status`

```json
{
  "ok": true,
  "command": "registration-tokens status",
  "data": {
    "token": "abc123",
    "max_uses": 5,
    "uses": 2,
    "remaining": 3,
    "comment": "Team invite",
    "expires_at": "2026-06-15T00:00:00Z",
    "expired": false
  }
}
```

---

## Policy & delivery

### `federation list`

```json
{
  "ok": true,
  "command": "federation list",
  "data": {
    "policy": "ACCEPT",
    "rules": [
      { "domain": "evil.net", "action": "block" }
    ]
  }
}
```

### `federation status`

```json
{
  "ok": true,
  "command": "federation status",
  "data": {
    "traffic": [
      {
        "domain": "partner.org",
        "sent": 120,
        "failed": 2,
        "latency_ms": 450
      }
    ]
  }
}
```

### `federation policy` / `block` / `allow` / `remove` / `flush`

```json
{
  "ok": true,
  "command": "federation block",
  "message": "Success: 'evil.net' added to rules.",
  "data": {
    "domain": "evil.net",
    "action": "block",
    "total": 3
  }
}
```

### `federation dismiss-list`

```json
{
  "ok": true,
  "command": "federation dismiss-list",
  "data": {
    "domains": [
      { "domain": "newsletter.example", "added": "2026-06-01" }
    ],
    "total": 1
  }
}
```

### `endpoint-cache list`

```json
{
  "ok": true,
  "command": "endpoint-cache list",
  "data": {
    "entries": [
      {
        "lookup_key": "mail.partner.com",
        "target_host": "smtp.partner.com",
        "comment": "Route via partner"
      }
    ]
  }
}
```

### `endpoint-cache set` / `get` / `remove`

```json
{
  "ok": true,
  "command": "endpoint-cache set",
  "message": "Entry saved",
  "data": {
    "lookup_key": "mail.partner.com",
    "target_host": "smtp.partner.com",
    "comment": "Route via partner"
  }
}
```

### `sharing list`

```json
{
  "ok": true,
  "command": "sharing list",
  "data": {
    "links": [
      {
        "slug": "alice",
        "url": "https://example.org/alice.vcf",
        "name": "Alice"
      }
    ]
  }
}
```

### `sharing create` / `edit` / `reserve` / `remove`

```json
{
  "ok": true,
  "command": "sharing create",
  "message": "Share link created",
  "data": {
    "slug": "alice",
    "url": "https://example.org/alice.vcf",
    "name": "Alice"
  }
}
```

---

## Services & limits

### `port status`

```json
{
  "ok": true,
  "command": "port status",
  "data": {
    "services": [
      { "name": "smtp", "port": "25", "mode": "public" },
      { "name": "https", "port": "443", "mode": "public" }
    ]
  }
}
```

### `port smtp set` / `local` / `public` / `reset`

```json
{
  "ok": true,
  "command": "port smtp set",
  "message": "SMTP port set to 2525",
  "data": {
    "name": "smtp",
    "port": "2525"
  }
}
```

### `message-size status`

```json
{
  "ok": true,
  "command": "message-size status",
  "data": {
    "appendlimit": "100M",
    "max_message_size": "100M",
    "effective_bytes": 104857600,
    "source": "db"
  }
}
```

`source` is `"db"` or `"config"`. Null `appendlimit` / `max_message_size` when unset in DB.

### `message-size set` / `reset`

```json
{
  "ok": true,
  "command": "message-size set",
  "message": "Message size limit set to 100M",
  "data": {
    "appendlimit": "100M",
    "max_message_size": "100M",
    "effective_bytes": 104857600
  }
}
```

### `language status`

```json
{
  "ok": true,
  "command": "language status",
  "data": {
    "current": "en",
    "config_default": "en",
    "source": "config"
  }
}
```

`source` is `"db"` when a DB override is active.

### `language set` / `reset`

```json
{
  "ok": true,
  "command": "language set",
  "message": "Website language set to fa",
  "data": {
    "language": "fa"
  }
}
```

### `webimap status` / `websmtp status`

```json
{
  "ok": true,
  "command": "webimap status",
  "data": {
    "enabled": true,
    "service": "WebIMAP HTTP API"
  }
}
```

### `webimap enable` / `disable` (and `websmtp`)

```json
{
  "ok": true,
  "command": "webimap enable",
  "message": "WebIMAP HTTP API enabled",
  "data": {
    "enabled": true
  }
}
```

### `push status`

```json
{
  "ok": true,
  "command": "push status",
  "data": {
    "mode": "auto",
    "runtime_enabled": true,
    "failures": 0,
    "auto_disable_threshold": 5
  }
}
```

### `push auto` / `on` / `off`

```json
{
  "ok": true,
  "command": "push",
  "message": "Push mode set to auto",
  "data": {
    "mode": "auto",
    "runtime_enabled": true
  }
}
```

### `tasks list`

```json
{
  "ok": true,
  "command": "tasks list",
  "data": {
    "tasks": [
      {
        "name": "prune-old-messages",
        "description": "Delete messages older than retention",
        "enabled": true
      }
    ],
    "message_retention": "Some(720h)",
    "unused_account_retention": null,
    "periodic_jobs_enabled": true
  }
}
```

### `tasks run`

```json
{
  "ok": true,
  "command": "tasks run",
  "data": {
    "task": "prune-old-messages",
    "deleted_messages": 42,
    "deleted_accounts": 0
  }
}
```

Certificate renewal task:

```json
{
  "ok": true,
  "command": "tasks run",
  "data": {
    "task": "renew-certificate",
    "skipped": false,
    "renewed": true,
    "detail": "certificate renewed"
  }
}
```

### `tasks run-all`

```json
{
  "ok": true,
  "command": "tasks run-all",
  "data": {
    "outcomes": [
      { "task": "prune-old-messages", "deleted_messages": 10 },
      { "task": "prune-unused-accounts", "deleted_accounts": 1 }
    ]
  }
}
```

---

## Web content

### `html-export`

```json
{
  "ok": true,
  "command": "html-export",
  "message": "Exported default HTML",
  "data": {
    "dest": "/opt/www-backup",
    "files": 24
  }
}
```

### `html-serve`

```json
{
  "ok": true,
  "command": "html-serve",
  "message": "WWW directory updated",
  "data": {
    "www_dir": "/opt/custom-www"
  }
}
```

Setting `embedded` reverts to built-in HTML.

---

## Planned commands

These commands are not implemented yet. With `--json`, they return the **error envelope**:

```json
{
  "ok": false,
  "error": "'madmail creds' is not implemented in madmail-v2 yet."
}
```

Affected: `creds`, `exchanger`, `hash`, `imap-acct`, `imap-mboxes`, `imap-msgs`, `migrate-pgp-config`, `queue`, `submission-access`.

---

## Scripting example

```bash
#!/bin/sh
set -e
TOKEN=$(madmail --json admin-token | jq -r '.data.token')
OPEN=$(madmail --json registration status | jq -r '.data.open')
echo "token length: ${#TOKEN}, registration open: $OPEN"
```

Parse failures:

```bash
if ! madmail --json accounts create bob@example.org --password secret 2>err.json; then
  jq -r '.error' err.json
fi
```

---
[← CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/output.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/output.rs)
