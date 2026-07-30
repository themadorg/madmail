# `madmail queue remove`

Parent: [`queue`](queue.md)

Remove one outbound queue entry (meta + body). Alias: `delete`.

## Synopsis

```bash
madmail queue remove <ID> [-y|--yes]
madmail queue remove <ID> -y --json
```

## Options

| Option | Description |
|--------|-------------|
| `-y`, `--yes` | Skip confirmation prompt |

## Examples

```bash
madmail queue remove 6869e998-f3e1-4d4f-8de2-2dd525eed4ab -y
```

## Notes

If confirmation is declined, exit is successful with an aborted JSON envelope when `--json` is set.

## JSON output (`--json`)

Schema: [json-output.md](json-output.md#queue-remove).

---
[← `queue`](queue.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/queue_cmd.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/queue_cmd.rs)
