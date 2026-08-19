# `madmail dkim status`

Parent: [`dkim`](dkim.md)

Summarize outbound federation DKIM: whether the local key exists and whether DNS `default._domainkey` matches. Does **not** create a key (unlike [`show`](dkim-show.md)).

## Synopsis

```bash
madmail dkim status [OPTIONS]
```

## Examples

```bash
madmail dkim status
madmail dkim status --json
```

## Notes

Always exits 0. For a non-zero exit when DNS does not match, use [`dkim check`](dkim-check.md).

| Field (JSON) | Meaning |
|--------------|---------|
| `key_present` | `{state_dir}/dkim/default.private` exists |
| `publishable` | DNS mail domain and a local TXT are available |
| `dns_checked` | A DNS query was made |
| `dns_matched` | Published TXT matches the local key |

IP-literal domains and missing keys skip DNS (`dns_checked: false`) and set `reason`.

## JSON output (`--json`)

```bash
madmail dkim status --json
```

Success stdout:

```json
{"ok": true, "command": "dkim status", "data": { ... }}
```

Schema: [json-output.md](json-output.md#dkim-status).


---
[← `dkim`](dkim.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/dkim.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/dkim.rs)
