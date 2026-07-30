# `madmail queue list`

Parent: [`queue`](queue.md)

List pending outbound queue entries (metadata only).

## Synopsis

```bash
madmail queue list
madmail queue list --json
```

## Examples

```bash
madmail queue list
```

## JSON output (`--json`)

```bash
madmail queue list --json
```

Success stdout:

```json
{
  "ok": true,
  "command": "queue list",
  "data": {
    "path": "/var/lib/madmail/remote_queue",
    "count": 1,
    "entries": [
      {
        "id": "…",
        "mail_from": "a@example.org",
        "rcpt_to": "b@peer.test",
        "tries_count": 0,
        "queued_at_unix": 1700000000,
        "last_attempt_unix": 0,
        "next_attempt_unix": 1700000000,
        "last_error": null
      }
    ]
  }
}
```

Schema: [json-output.md](json-output.md#queue-list).

---
[← `queue`](queue.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/queue_cmd.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/queue_cmd.rs)
