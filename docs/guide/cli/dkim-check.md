# `madmail dkim check`

Parent: [`dkim`](dkim.md)

Look up the published `default._domainkey` TXT and compare it to the local federation DKIM key.

## Synopsis

```bash
madmail dkim check [OPTIONS]
```

## Examples

```bash
madmail dkim check
madmail dkim check --json
```

## Notes

Uses the system resolver (Cloudflare `1.1.1.1` if `/etc/resolv.conf` cannot be read). Concatenates split TXT strings and ignores quotes/whitespace. The RSA `p=` value is compared case-sensitively.

| Result | Exit | Meaning |
|--------|------|---------|
| `matched: true` | 0 | DNS TXT matches `{state_dir}/dkim/default.txt` |
| `checked: false` | 0 | Mail domain is an IP literal — signing is skipped |
| `matched: false` | 1 | No TXT, wrong key, or lookup error |

Publish first with [`dkim show`](dkim-show.md). DNS TTL may delay a fresh record.

## JSON output (`--json`)

```bash
madmail dkim check --json
```

Success stdout:

```json
{"ok": true, "command": "dkim check", "data": { ... }}
```

Schema: [json-output.md](json-output.md#dkim-check).


---
[← `dkim`](dkim.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/dkim.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/dkim.rs)
