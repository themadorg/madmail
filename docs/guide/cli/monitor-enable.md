# `monitor enable`

Enable the metrics endpoint in the server configuration.

## Synopsis

```bash
madmail monitor enable [--ip <IP>] [--port <PORT>] [--username <USER>] [--password <PASSWORD> | --clear-password]
```

Metrics are disabled by default in new installations. This command adds a loopback listener at `127.0.0.1:9749` when none exists and preserves an existing listener. It updates the file selected by `--config` and needs write permission. Restart the server afterward; `madmail reload` does not reread this static setting.

Both `.conf` and `.toml` configurations are supported. See [global flags](global-flags.md) for `--config` and `--json`.

## Options

| Option | Effect |
|--------|--------|
| `--ip <IP>` | Set the listen IP (IPv4 or IPv6). Default: existing value, or `127.0.0.1`. |
| `--port <PORT>` | Set the listen port (1–65535). Default: existing value, or `9749`. |
| `--username <USER>` | Set the HTTP Basic username. Default: existing value, or `metrics`. |
| `--password <PASSWORD>` | Set HTTP Basic password protection; also accepts `MADMAIL_MONITOR_PASSWORD`. |
| `--clear-password` | Remove password protection; conflicts with `--password`. |

Settings not supplied remain unchanged. See [authentication and address examples](monitor.md#listen-address-and-http-password).

## Examples

```bash
sudo madmail monitor enable
sudo systemctl restart madmail
madmail monitor --count 1
```

The restart example assumes the standard Linux systemd service. For other deployments, restart the server process using your service manager.

## JSON output

Returns `enabled`, `listen`, `authentication_required`, `username`, `changed`, and `restart_required` in `data`. See the [JSON schema](json-output.md#monitor).

---
[← Monitor](monitor.md) · [CLI index](README.md)
