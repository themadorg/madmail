# `monitor disable`

Disable the metrics endpoint in the server configuration.

## Synopsis

```bash
madmail monitor disable
```

This command disables the listener in the file selected by `--config` and needs write permission. Restart the server afterward to stop serving metrics. Repeating the command when already disabled leaves the file unchanged.

Both `.conf` and `.toml` configurations are supported. See [global flags](global-flags.md) for `--config` and `--json`.

## Examples

```bash
sudo madmail monitor disable
sudo systemctl restart madmail
madmail monitor status
```

The restart example assumes the standard Linux systemd service. For other deployments, restart the server process using your service manager.

## JSON output

Returns `enabled`, `listen`, `authentication_required`, `username`, `changed`, and `restart_required` in `data`. See the [JSON schema](json-output.md#monitor).

---
[← Monitor](monitor.md) · [CLI index](README.md)
