# `monitor status`

Show the saved metrics endpoint configuration.

## Synopsis

```bash
madmail monitor status
```

Reports whether metrics are enabled and, when enabled, the listen address. This reads the file selected by `--config`; it does not probe the running server. Use `madmail monitor --count 1` to check connectivity.

Both `.conf` and `.toml` configurations are supported. See [global flags](global-flags.md) for `--config` and `--json`.

## Examples

```bash
madmail monitor status
madmail --json monitor status
```

## JSON output

Returns `enabled`, `listen`, `authentication_required`, `username`, `changed`, and `restart_required` in `data`. See the [JSON schema](json-output.md#monitor).

---
[← Monitor](monitor.md) · [CLI index](README.md)
