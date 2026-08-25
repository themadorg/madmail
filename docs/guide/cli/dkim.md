# `dkim`

Print the outbound federation DKIM selector, signing domain (`d=`), key paths, and the single-line TXT to publish at `default._domainkey`. `check` looks that TXT up in DNS and compares it to the local key. `status` summarizes local key presence and DNS match without creating a key.


## Synopsis

```bash
madmail dkim [show|check|status]
```

## Global flags

| Flag | Alias | Environment | Default | Description |
|------|-------|-------------|---------|-------------|
| `--config` | — | `CHATMAIL_CONFIG` | `/etc/madmail/madmail.conf` (or `./data/chatmail.toml` when present) | Path to the server config file |
| `--state-dir` | `--libexec` | `CHATMAIL_STATE_DIR` | `/var/lib/madmail` (or `./data` when it contains state) | Persistent state directory (`credentials.db`, maildirs, `admin_token`, …) |


## Subcommands

| Subcommand | Description |
|------------|-------------|
| `show` | Print selector, `d=`, paths, and the TXT record (default) |
| `check` | Look up `default._domainkey` in DNS and compare it to the local TXT |
| `status` | Summarize local key and DNS match (does not create a key) |

## Examples

```bash
madmail dkim
madmail dkim show
madmail dkim show --json
madmail dkim check
madmail dkim check --json
madmail dkim status
madmail dkim status --json
```

## Notes

Selector is always **`default`**. Keys live at `{state_dir}/dkim/default.private` (0600) and `{state_dir}/dkim/default.txt`.

If the private key is missing, `show` creates it (same side effect as `madmail install` or the first outbound federated send). Use this on existing 2.18.x hosts **before** sending to cmdeploy so you can publish DNS first.

`d=` is the mail domain from config (`primary_domain` / `mail_domain` / `hostname`). IP-literal domains are not signed (filtermail treats that as a no-op); `show` reports that instead of writing a key.

cmdeploy `filtermail` still returns `554 5.7.1 No valid DKIM signature found` until the TXT is live at `default._domainkey.<domain>`.

`dkim check` queries public DNS for `default._domainkey.<domain>` and compares it to the local key (whitespace/quoting ignored; `p=` is case-sensitive). Missing or wrong TXT exits non-zero. IP-literal domains skip the lookup.

`dkim status` never writes a key. It reports whether the private key is on disk and, when it is, whether DNS matches. Exit code is always 0; use `check` in scripts that should fail on a mismatch.

Admin API: `GET /admin/dkim` matches `show`; `GET /admin/dkim/check` matches `check`; `GET /admin/dkim/status` matches `status` (see [09-admin-api.md](../../TDD/09-admin-api.md)).

See [DNS and Mail Authentication](../../project/user-guide/12-dns-mail-auth.md) and [Federation](../../TDD/07-federation.md).

## Subcommand pages

- [`show`](dkim-show.md) — `madmail dkim show`
- [`check`](dkim-check.md) — `madmail dkim check`
- [`status`](dkim-status.md) — `madmail dkim status`

## JSON output (`--json`)

```bash
madmail dkim show --json
madmail dkim check --json
madmail dkim status --json
```

Success stdout:

```json
{"ok": true, "command": "dkim show", "data": { ... }}
{"ok": true, "command": "dkim check", "data": { ... }}
{"ok": true, "command": "dkim status", "data": { ... }}
```

Schema: [json-output.md](json-output.md#dkim-show) · [json-output.md](json-output.md#dkim-check) · [json-output.md](json-output.md#dkim-status).


---
[← CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/dkim.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/dkim.rs)
