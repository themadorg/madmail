# `webmail-cors`

Manage browser CORS origins for WebIMAP, WebSMTP, and `POST /new` (`__WEBMAIL_CORS_ORIGINS__`).

Alias: `webmail-dev` (same command).

## Synopsis

```bash
madmail webmail-cors <status|set|add|remove|reset|enable>
```

## Subcommands

| Subcommand | Description |
|------------|-------------|
| `status` | Show origins list and WebIMAP/WebSMTP status (default) |
| `set ORIGINS` | Replace full list (comma/newline separated; `*` = any) |
| `add ORIGIN` | Append one origin |
| `remove ORIGIN` | Remove one origin |
| `reset` | Clear all origins |
| `enable ORIGIN` | Enable WebIMAP + WebSMTP and allow CORS from ORIGIN |

## Examples

```bash
# Local Vite dev app
madmail webmail-cors enable http://127.0.0.1:5173

madmail webmail-cors status
madmail webmail-cors add http://localhost:5173
madmail webmail-cors set "http://127.0.0.1:5173,http://localhost:5173"
```

Changes take effect on the next HTTP request (no restart required).

---
[← CLI index](README.md) · [Global flags](global-flags.md)