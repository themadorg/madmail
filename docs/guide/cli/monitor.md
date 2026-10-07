# `monitor`

Live connection and message-throughput metrics from a running server, one row per sample. The endpoint uses OpenMetrics/Prometheus and is disabled by default in new installations.


## Synopsis

```bash
madmail monitor [-n|--interval <SECS>] [-c|--count <N>] [--addr <HOST:PORT>]
madmail monitor status
madmail monitor enable [--ip <IP>] [--port <PORT>] [--username <USER>] [--password <PASSWORD> | --clear-password]
madmail monitor disable
```

## Global flags

| Flag | Alias | Environment | Default | Description |
|------|-------|-------------|---------|-------------|
| `--config` | — | `CHATMAIL_CONFIG` | `/etc/madmail/madmail.conf` (or `./data/chatmail.toml` when present) | Path to the server config file |
| `--state-dir` | `--libexec` | `CHATMAIL_STATE_DIR` | `/var/lib/madmail` (or `./data` when it contains state) | Persistent state directory (`credentials.db`, maildirs, `admin_token`, …) |


## Options

| Flag | Description |
|------|-------------|
| `-n`, `--interval <SECS>` | Seconds between samples (default `2`) |
| `-c`, `--count <N>` | Stop after `N` samples (default: run until interrupted) |
| `--addr <HOST:PORT>` | Scrape this address instead of the config's `openmetrics` listener |
| `--username <USER>` | HTTP Basic username for scraping (default: config, or `metrics`) |
| `--password <PASSWORD>` | HTTP Basic password for scraping (default: config); also accepts `MADMAIL_MONITOR_PASSWORD` |

## Enable or disable monitoring

| Command | Effect |
|---------|--------|
| [`madmail monitor status`](monitor-status.md) | Show whether the endpoint is enabled in the saved config and its listen address. |
| [`madmail monitor enable`](monitor-enable.md) | Add `127.0.0.1:9749` when no listener is configured. Preserve an existing listener. |
| [`madmail monitor disable`](monitor-disable.md) | Disable the configured metrics endpoint. |

Enable and disable update the file selected by `--config`. They support `.conf` and `.toml` files and require write access to that file. Repeating a command when the setting already matches leaves the file unchanged.

For a standard Linux installation using systemd:

```bash
madmail monitor status
sudo madmail monitor enable
sudo systemctl restart madmail
madmail monitor --count 1
```

To disable it:

```bash
sudo madmail monitor disable
sudo systemctl restart madmail
```

For a custom config, use the same path used by your server:

```bash
madmail --config /path/to/madmail.conf monitor enable
madmail --config /path/to/madmail.conf monitor status
```

Restart that server process after changing the configuration. `madmail reload` does not reread this static listener setting. `status` reports the saved configuration; use `madmail monitor --count 1` to check whether the running endpoint responds.

Existing installations with metrics already configured remain enabled until you disable them.

## Enable manually in configuration

Add this block to your `.conf` file, then restart the server:

```text
openmetrics tcp://127.0.0.1:9749 {
}
```

For `.toml`, add the setting at the top level, before any table headers:

```toml
openmetrics_listen = "127.0.0.1:9749"
```

To disable manually, remove the block or setting and restart the server.

The `/metrics` endpoint is unauthenticated unless a password is configured. Keep unauthenticated metrics on loopback. `madmail monitor` rewrites wildcard bind addresses (`0.0.0.0`, `[::]`) to loopback when scraping.

## Listen address and HTTP password

Choose the listen IP and port when enabling metrics. Either option can be supplied independently; unspecified settings keep their configured values (or default to `127.0.0.1:9749`). IPv4 and IPv6 are supported. Ports must be between 1 and 65535.

```bash
sudo madmail monitor enable --ip 127.0.0.1 --port 9750 --username metrics --password 'your-password'
sudo systemctl restart madmail
madmail monitor --count 1
```

For IPv6, use `--ip ::1`; the saved address is `[::1]:9750`. Changing address or authentication requires a server restart.

The route uses HTTP Basic authentication. Without `--username`, the default username is `metrics`; existing credentials are preserved when not overridden. `madmail monitor` automatically uses credentials from its selected config. For an explicit endpoint:

```bash
madmail monitor --addr 127.0.0.1:9750 --username metrics --password 'your-password' --count 1
```

Set `MADMAIL_MONITOR_PASSWORD` to supply a password through the environment instead of a command argument. `monitor status` reports whether authentication is required and never prints the password. Missing or incorrect HTTP credentials receive `401 Unauthorized`.

To remove password protection while keeping metrics enabled:

```bash
sudo madmail monitor enable --clear-password
sudo systemctl restart madmail
```

For manual `.conf` configuration:

```text
openmetrics tcp://127.0.0.1:9750 {
    username metrics
    password "your-password"
}
```

Equivalent top-level TOML settings:

```toml
openmetrics_listen = "127.0.0.1:9750"
openmetrics_username = "metrics"
openmetrics_password = "your-password"
```

Passwords are stored in the server config; restrict access to that file. The listener serves plain HTTP, so Basic authentication does not encrypt credentials. For remote scraping, use a TLS reverse proxy or a trusted private network. A Prometheus scrape can use `basic_auth` with the configured username and password.

## Example

```bash
madmail monitor
madmail monitor --interval 1 --count 10
madmail monitor --count 1 --json
```

```
Scraping http://127.0.0.1:9749/metrics every 2s
  imap   smtp submission   total     msg/s aborted/s   queue
     4      2          0       6         -         -       3
     4      2          0       6     19.88      0.00       3
```

## Displayed metrics

- **Connections** are `maddy_conns_active` per module; `total` sums every module.
- **msg/s** is the change in `maddy_smtp_smtp_completed_transactions` (summed across modules) divided by the wall-clock time between two scrapes. The first row has no previous sample, so it shows `-`.
- A counter that goes backwards means the server restarted; that interval reports `0`.

## JSON output (`--json`)

```bash
madmail monitor --count 1 --json
```

One success envelope per sample, so without `--count` the output is newline-delimited JSON:

```json
{"ok": true, "command": "monitor", "data": { ... }}
```

Configuration commands also support JSON:

```bash
madmail --json monitor status
```

```json
{"ok": true, "command": "monitor", "data": {"enabled": false, "listen": null, "authentication_required": false, "username": "metrics", "changed": false, "restart_required": false}}
```

`enabled` and `listen` describe the saved configuration. `changed` and `restart_required` are true when an enable/disable command changes the file. A false value does not verify that the running server has applied an earlier change.

Schema: [json-output.md](json-output.md#monitor).


---
[← CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/monitor.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/monitor.rs)
