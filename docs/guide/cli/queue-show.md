# `madmail queue show`

Parent: [`queue`](queue.md)

Show one outbound queue entry by id.

## Synopsis

```bash
madmail queue show <ID>
madmail queue show <ID> --json
```

## Arguments

| Arg | Description |
|-----|-------------|
| `ID` | Queue entry id (filename without `.meta`) |

## Examples

```bash
madmail queue show 6869e998-f3e1-4d4f-8de2-2dd525eed4ab
```

## Notes

Errors if the entry does not exist or the meta file is unreadable.

## JSON output (`--json`)

Schema: [json-output.md](json-output.md#queue-show).

---
[← `queue`](queue.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/queue_cmd.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/queue_cmd.rs)
