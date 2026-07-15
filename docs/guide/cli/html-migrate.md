# `html-migrate`

Convert a custom `www_dir` from **Go `html/template`** syntax to **Minijinja** (madmail-v2) on disk.

Use this after a Go Madmail → madmail-v2 upgrade if you customized public HTML pages. The same check runs automatically during [`update`](update.md) / [`upgrade`](upgrade.md) (interactive prompt).


## Synopsis

```bash
madmail html-migrate
madmail html-migrate --yes
```

## Global flags

| Flag | Alias | Environment | Default | Description |
|------|-------|-------------|---------|-------------|
| `--config` | — | `CHATMAIL_CONFIG` | `/etc/madmail/madmail.conf` (or `./data/chatmail.toml` when present) | Path to the server config file |
| `--state-dir` | `--libexec` | `CHATMAIL_STATE_DIR` | `/var/lib/madmail` (or `./data` when it contains state) | Persistent state directory |


## Arguments / flags

| Flag | Description |
|------|-------------|
| `-y`, `--yes` | Apply conversion without prompting (for scripts / non-interactive upgrades) |

## Behavior

1. Reads `www_dir` from the config (`chatmail { www_dir … }` or `www_dir` in TOML).
2. If **no** custom `www_dir` (embedded site) → nothing to do.
3. Scans `*.html` under `www_dir` for Go-style markers (`{{if .Field}}`, `{{if not .Field}}`, `{{.MailDomain}}`, `| cleanDomain`, …).
4. If none found → already Minijinja-style (or no templates).
5. If found → list sample paths and ask **`[y/N]`** (unless `--yes`).
6. On yes: rewrite files in place using the same conversion as the server (`prepare_template`), writing a sibling `*.go-template.bak` backup for each changed file.

Converted examples:

| Go (`html/template`) | Minijinja (on disk / after convert) |
|----------------------|-------------------------------------|
| `{{if .RegistrationOpen}}…{{end}}` | `{% if RegistrationOpen %}…{% endif %}` |
| `{{if not .RegistrationOpen}}…{{end}}` | `{% if not RegistrationOpen %}…{% endif %}` |
| `{{.MailDomain \| cleanDomain}}` | `{{ MailDomain \| clean_domain }}` |

## Examples

```bash
# Interactive (recommended once after Go → v2)
sudo madmail --config /etc/maddy/maddy.conf html-migrate

# Scripted
sudo madmail --config /etc/maddy/maddy.conf html-migrate --yes
```

## Notes

- Runtime still accepts many Go forms via conversion at render time; migrating on disk is for durable, editor-friendly Minijinja sources.
- Non-interactive sessions without `--yes` skip conversion and print how to re-run with `--yes`.
- After migration, `madmail reload` or a service restart picks up the files (live `www_dir` re-reads HTML).

## JSON output (`--json`)

```bash
madmail html-migrate --json --yes
```

Success stdout includes `action` (`noop_embedded`, `noop_already_migrated`, `skipped_noninteractive`, `declined`, `migrated`) and file lists.

Schema: [json-output.md](json-output.md#html-migrate).


---
[← CLI index](README.md) · [Global flags](global-flags.md) · [html-serve](html-serve.md) · [update](update.md)

[Source: `crates/chatmail/src/ctl/html.rs`, `crates/chatmail-www/src/www_migrate.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/html.rs)
