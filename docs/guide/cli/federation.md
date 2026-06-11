# `federation`

Federation policy, per-domain rules, silent dismiss list, and live traffic diagnostics.


## Synopsis

```bash
madmail federation <subcommand>
```

## Global flags

| Flag | Alias | Environment | Default | Description |
|------|-------|-------------|---------|-------------|
| `--config` | — | `CHATMAIL_CONFIG` | `/etc/madmail/madmail.conf` (or `./data/chatmail.toml` when present) | Path to the server config file |
| `--state-dir` | `--libexec` | `CHATMAIL_STATE_DIR` | `/var/lib/madmail` (or `./data` when it contains state) | Persistent state directory (`credentials.db`, maildirs, `admin_token`, …) |


## Subcommands

| Subcommand | Description |
|------------|-------------|
| `policy <accept|reject>` | Set global posture |
| `block <DOMAIN>` | Add to rules (blocklist when policy is ACCEPT) |
| `allow <DOMAIN>` | Add to rules (allowlist when policy is REJECT) |
| `remove <DOMAIN>` | Remove domain from rules |
| `flush` | Remove all domain exceptions |
| `list` | Show policy and active rules |
| `status` | Live federation traffic diagnostics from DB |
| `dismiss <DOMAIN>` | Accept mail but do not deliver (silent drop) |
| `undismiss <DOMAIN>` | Remove from dismiss list |
| `dismiss-list` | List dismiss domains |
| `dismiss-flush` | Clear all dismiss domains |

## Examples

```bash
madmail federation list
madmail federation policy accept
madmail federation block spamdomain.net
madmail federation dismiss newsletter.example
madmail federation status
```

## Subcommand pages

- [`allow`](federation-allow.md) — `madmail federation allow`
- [`block`](federation-block.md) — `madmail federation block`
- [`dismiss`](federation-dismiss.md) — `madmail federation dismiss`
- [`dismiss-flush`](federation-dismiss-flush.md) — `madmail federation dismiss-flush`
- [`dismiss-list`](federation-dismiss-list.md) — `madmail federation dismiss-list`
- [`flush`](federation-flush.md) — `madmail federation flush`
- [`list`](federation-list.md) — `madmail federation list`
- [`policy`](federation-policy.md) — `madmail federation policy`
- [`remove`](federation-remove.md) — `madmail federation remove`
- [`status`](federation-status.md) — `madmail federation status`
- [`undismiss`](federation-undismiss.md) — `madmail federation undismiss`

## JSON output (`--json`)

```bash
madmail federation --json
```

Success stdout:

```json
{"ok": true, "command": "federation", "data": { ... }}
```

Schema: [json-output.md](json-output.md#federation).


---
[← CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/federation.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/federation.rs)
