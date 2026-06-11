# `madmail tasks run`

Parent: [`tasks`](tasks.md)

Run one job now (`prune-old-messages`, `prune-unused-accounts`, …)

## Synopsis

```bash
madmail tasks run [OPTIONS] <TASK>
```

## Options

| Option | Description |
|--------|-------------|
| `--retention` | Override retention (`24h`, `7d`, `720h`); required for `prune-unread-older` without config |
## Examples

```bash
madmail tasks run prune-old-messages
madmail tasks run prune-unread-older --retention 720h
```

## Notes

Task aliases: `prune-messages`/`retention`, `prune-unused`/`unused-accounts`, `purge-read`/`auto-purge-seen`, `purge-unread-older`, `certificate-renew`/`renew-cert`.

## JSON output (`--json`)

```bash
madmail tasks run --json
```

Success stdout:

```json
{"ok": true, "command": "tasks run", "data": { ... }}
```

Schema: [json-output.md](json-output.md#tasks-run).


---
[← `tasks`](tasks.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/tasks.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/tasks.rs)
