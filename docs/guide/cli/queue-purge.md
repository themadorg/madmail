# `madmail queue purge`

Parent: [`queue`](queue.md)

Remove **all** outbound queue entries. Alias: `clear`.

## Synopsis

```bash
madmail queue purge [-y|--yes]
madmail queue purge -y --json
```

## Options

| Option | Description |
|--------|-------------|
| `-y`, `--yes` | Skip confirmation prompt |

## Examples

```bash
madmail queue purge -y
```

## Notes

- Only clears `{state_dir}/remote_queue` (or configured `target.queue` location).
- Does **not** delete user maildir messages (use [`tasks`](tasks.md) / admin storage purge for that).
- Same effect as admin API action `purge_queue`.

## JSON output (`--json`)

Schema: [json-output.md](json-output.md#queue-purge).

---
[← `queue`](queue.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/queue_cmd.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/queue_cmd.rs)
