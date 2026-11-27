# `madmail admin-api`

Show, set, or reset the path where the admin API is mounted. The default is `/api/admin`.

A running server remounts HTTP routes from the path that is mounted now, then from the built-in path if that request fails. A refused remount keeps the saved path. The next start also reads the saved path. A `settings { admin_path ... }` entry, a top-level TOML `admin_path`, or `CHATMAIL_ADMIN_PATH` protects the value and this command will not change it.

Parent: [CLI reference](README.md)

## Subcommands

- [`path`](admin-api-path.md) — show, set, or reset the path
