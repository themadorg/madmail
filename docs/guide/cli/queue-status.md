# `madmail queue status`

Parent: [`queue`](queue.md)

Show outbound queue path, pending entry count, and retry configuration. Default when you run `madmail queue` with no subcommand.

## Synopsis

```bash
madmail queue
madmail queue status
madmail queue status --json
```

## Examples

```bash
madmail queue
madmail --state-dir /var/lib/madmail queue status --json
```

## JSON output (`--json`)

```bash
madmail queue status --json
```

Success stdout:

```json
{
  "ok": true,
  "command": "queue status",
  "data": {
    "path": "/var/lib/madmail/remote_queue",
    "count": 0,
    "max_tries": 3,
    "max_parallelism": 16,
    "initial_retry_secs": 60,
    "retry_time_scale": 1.25,
    "max_delivery_secs": 600,
    "post_init_delay_secs": 10
  }
}
```

Schema: [json-output.md](json-output.md#queue-status).

---
[← `queue`](queue.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/queue_cmd.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/queue_cmd.rs)
