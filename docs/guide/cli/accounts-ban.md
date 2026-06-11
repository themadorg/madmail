# `madmail accounts ban`

Parent: [`accounts`](accounts.md)

Same as delete with moderation reason

## Synopsis

```bash
madmail accounts ban [OPTIONS] <USERNAME> [REASON]
```

## Options

| Option | Description |
|--------|-------------|
| `-y`, `--yes` | Skip confirmation prompt |
## Examples

```bash
madmail accounts ban spammer@example.org "spam" --yes
```

## Notes

Same as delete but stores a moderation `REASON` on the blocklist (default: `banned via CLI`).

## JSON output (`--json`)

```bash
madmail accounts ban --json
```

Success stdout:

```json
{"ok": true, "command": "accounts ban", "data": { ... }}
```

Schema: [json-output.md](json-output.md#accounts-ban).


---
[← `accounts`](accounts.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/accounts.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/accounts.rs)
