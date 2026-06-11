# `madmail tasks run-all`

Parent: [`tasks`](tasks.md)

Run all jobs enabled by `storage.imapsql` retention settings in config

## Synopsis

```bash
madmail tasks run-all [OPTIONS]
```

## JSON output (`--json`)

```bash
madmail tasks run all --json
```

Success stdout:

```json
{"ok": true, "command": "tasks run all", "data": { ... }}
```

Schema: [json-output.md](json-output.md#tasks-run-all).


---
[← `tasks`](tasks.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/tasks.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/tasks.rs)
