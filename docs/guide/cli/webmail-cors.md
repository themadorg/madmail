# `webmail-cors`

Manage browser CORS origins for WebIMAP, WebSMTP, and `POST /new` (`__WEBMAIL_CORS_ORIGINS__`).

Alias: `webmail-dev` (same command).

## Synopsis

```bash
madmail webmail-cors <status|enable|disable|set|add|remove|reset>
```

Alias: `madmail webmail-dev` (same command).

## Subcommands

| Subcommand | Description |
|------------|-------------|
| `status` | Show browser access, WebIMAP/WebSMTP, and optional CORS whitelist (default) |
| `enable [ORIGIN]` | Turn on WebIMAP + WebSMTP. Without ORIGIN, the server reflects each request `Origin` (no `*`) |
| `disable` | Turn off WebIMAP + WebSMTP (browser access off) |
| `set ORIGINS` | Replace full whitelist (comma/newline separated; `*` = any) |
| `add ORIGIN` | Append one origin to the whitelist |
| `remove ORIGIN` | Remove one origin from the whitelist |
| `reset` | Clear the whitelist |

## Examples

```bash
# Enable browser access (reflect request Origin — recommended)
madmail webmail-cors enable

# Legacy: enable and add a specific origin to the whitelist
madmail webmail-cors enable http://127.0.0.1:5173

madmail webmail-cors status
madmail webmail-cors disable
madmail webmail-cors add http://localhost:5173
madmail webmail-cors set "http://127.0.0.1:5173,http://localhost:5173"
```

Changes take effect on the next HTTP request (no restart required).

---
[← CLI index](README.md) · [Global flags](global-flags.md)