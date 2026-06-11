# `madmail tasks list`

Parent: [`tasks`](tasks.md)

List available maintenance jobs and config-driven schedule

## Synopsis

```bash
madmail tasks list [OPTIONS]
```

## JSON output (`--json`)

```bash
madmail tasks list --json
```

Success stdout:

```json
{"ok": true, "command": "tasks list", "data": { ... }}
```

Schema: [json-output.md](json-output.md#tasks-list).


---
[← `tasks`](tasks.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/tasks.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/tasks.rs)
