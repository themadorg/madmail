# `madmail accounts ban-list`

Parent: [`accounts`](accounts.md)

List blocklisted usernames

## Synopsis

```bash
madmail accounts ban-list [OPTIONS]
```


## Notes

Top-level alias: [`ban-list`](ban-list.md).

## JSON output (`--json`)

```bash
madmail accounts ban list --json
```

Success stdout:

```json
{"ok": true, "command": "accounts ban list", "data": { ... }}
```

Schema: [json-output.md](json-output.md#accounts-ban-list).


---
[← `accounts`](accounts.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/accounts.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/accounts.rs)
