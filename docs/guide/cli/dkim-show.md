# `madmail dkim show`

Parent: [`dkim`](dkim.md)

Print the outbound federation DKIM selector, signing domain, key paths, and the TXT value to publish.

## Synopsis

```bash
madmail dkim show [OPTIONS]
```

Omitting a subcommand is the same as `show`.

## Examples

```bash
madmail dkim show
madmail dkim show --json
```

## Notes

Creates `{state_dir}/dkim/default.private` and `default.txt` when they do not exist so operators can publish DNS before the first federated send.

Copy the printed TXT (single line, no quotes) into a `TXT` record at `default._domainkey.<mail-domain>`.

IP-only mail domains (`user@[1.2.3.4]`) are not publishable; the command exits 0 and explains that signing is skipped.

After the TXT is live, run [`madmail dkim check`](dkim-check.md) to confirm DNS matches the local key.

## JSON output (`--json`)

```bash
madmail dkim show --json
```

Success stdout:

```json
{"ok": true, "command": "dkim show", "data": { ... }}
```

Schema: [json-output.md](json-output.md#dkim-show).


---
[← `dkim`](dkim.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/dkim.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/dkim.rs)
