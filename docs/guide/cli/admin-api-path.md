# `madmail admin-api path`

Configuration precedence is **environment → explicit file setting → database → defaults**. A `CHATMAIL_ADMIN_PATH` variable, a `.conf` `settings { }` entry, or a top-level TOML setting protects its value from this command.

Parent: [`admin-api`](admin-api.md)

Show, set, or reset the admin API path. Setting or resetting asks a running server to remount HTTP routes. The request is sent to the path that is mounted now, so the route can move. A path that already accepts POST, such as `/new`, `/share`, or `/mxdeliv`, is rejected and is not saved.

## Synopsis

```bash
madmail admin-api path [OPTIONS] [PATH]
```

## Options

| Option | Description |
|--------|-------------|
| `--reset` | Delete the database override and return to the file or default path |

## Examples

```bash
madmail admin-api path
madmail admin-api path /api/secret
madmail admin-api path --reset
```

## JSON output (`--json`)

```bash
madmail admin-api path --json
```

Success stdout for a read:

```json
{
  "ok": true,
  "command": "admin-api path",
  "data": {
    "path": "/api/admin",
    "api_url": "https://your-server/api/admin"
  }
}
```

Success stdout for a change that a running API refused:

```json
{
  "ok": true,
  "command": "admin-api path",
  "message": "Admin API path set to /api/secret",
  "data": {
    "path": "/api/secret",
    "api_url": "https://your-server/api/secret",
    "reset": false,
    "applied": false,
    "restart_required": true,
    "diagnostic": "https://your-server/api/admin: admin API: reload failed"
  }
}
```

`applied` is true only when a running server accepted the remount. Otherwise the database change is kept and `restart_required` is true. `diagnostic` is omitted when no candidate returned an API error. An error from the mounted URL does not skip the built-in URL, and the command still exits 0. An unreachable API URL is not reported as a stopped server. `--json` writes one JSON object and nothing else to stdout.
