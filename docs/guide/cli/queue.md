# `queue`

Outbound delivery queue management — federation / remote SMTP retries stored under `{state_dir}/remote_queue` (or `target.queue` `location`).

This is **not** IMAP maildir storage. For purging user mailboxes / seen messages, use [`tasks`](tasks.md) or the admin API `/admin/queue` storage purge actions.

## Synopsis

```bash
madmail queue
madmail queue status
madmail queue list
madmail queue show <ID>
madmail queue remove <ID> [-y]
madmail queue purge [-y]
```

## Global flags

| Flag | Alias | Environment | Default | Description |
|------|-------|-------------|---------|-------------|
| `--config` | — | `CHATMAIL_CONFIG` | `/etc/madmail/madmail.conf` (or `./data/chatmail.toml` when present) | Path to the server config file |
| `--state-dir` | `--libexec` | `CHATMAIL_STATE_DIR` | `/var/lib/madmail` (or `./data` when it contains state) | Persistent state directory |
| `--json` | — | — | off | Machine-readable JSON on stdout |

## Subcommands

| Subcommand | Description |
|------------|-------------|
| [`status`](queue-status.md) | Path, pending count, retry config (default) |
| [`list`](queue-list.md) | List pending entries (meta) |
| [`show`](queue-show.md) | Show one entry by id |
| [`remove`](queue-remove.md) | Delete one entry (`-y` skips confirm) |
| [`purge`](queue-purge.md) | Delete all entries (`-y` skips confirm); alias `clear` |

## Examples

```bash
# Summary
madmail queue
madmail queue status --json

# Inspect stuck deliveries
madmail queue list
madmail queue show 6869e998-f3e1-4d4f-8de2-2dd525eed4ab

# Drop one entry or the whole queue
madmail queue remove 6869e998-f3e1-4d4f-8de2-2dd525eed4ab -y
madmail queue purge -y
```

## Notes

- Entries are `{id}.meta` + `{id}.body` (bodies may be hard-linked across recipients).
- A running `madmail run` worker drains the same directory; purging only affects **pending** retries, not already-delivered mail.
- No `madmail reload` is required after purge/remove (filesystem change is immediate).

## Subcommand pages

- [queue status](queue-status.md)
- [queue list](queue-list.md)
- [queue show](queue-show.md)
- [queue remove](queue-remove.md)
- [queue purge](queue-purge.md)

## JSON output (`--json`)

See [json-output.md](json-output.md#queue-status) and related anchors.

---
[← CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/queue_cmd.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/queue_cmd.rs)
