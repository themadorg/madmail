# `html-migrate`

Migrate a custom `www_dir` after Go Madmail → madmail-v2:

1. **Go `html/template` → Minijinja** on disk  
2. **Legacy `/qr?data=` → client-side QR** (`setQrCodeImage` + `qrcode.min.js`)

Use this after upgrade if you customized public HTML pages. The same check runs automatically during [`update`](update.md) / [`upgrade`](upgrade.md) (interactive prompt).


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
3. Scans `*.html` under `www_dir` for:
   - Go-style markers (`{{if .Field}}`, `{{if not .Field}}`, `{{.MailDomain}}`, `| cleanDomain`, …)
   - Legacy QR: `/qr?data=…` image assignments (removed endpoint)
4. Also checks `main.js` / `qrcode.min.js` for client-side QR readiness.
5. Scans for **suspicious literal `{%` / `{{`** (e.g. Obtainium URLs with `{%22`) and **warns** (does not rewrite those).
6. If nothing to convert → reports already migrated.
7. If work found → list paths and ask **`[y/N]`** (unless `--yes`).
8. On yes:
   - Rewrite Go templates with the same conversion as the server (`prepare_template`); backup `*.go-template.bak`
   - Rewrite `/qr?data=${encodeURIComponent(…)}` → `setQrCodeImage(…)`
   - Append `setQrCodeImage` to `main.js` if missing (backup `main.js.qr-compat.bak`)
   - Copy embedded `qrcode.min.js` into `www_dir` if missing
   - Inject `<script src="./qrcode.min.js">` (or `/qrcode.min.js`) before `main.js` when needed

### Template conversion examples

| Go (`html/template`) | Minijinja (on disk / after convert) |
|----------------------|-------------------------------------|
| `{{if .RegistrationOpen}}…{{end}}` | `{% if RegistrationOpen %}…{% endif %}` |
| `{{if not .RegistrationOpen}}…{{end}}` | `{% if not RegistrationOpen %}…{% endif %}` |
| `{{.MailDomain \| cleanDomain}}` | `{{ MailDomain \| clean_domain }}` |

### QR conversion example

| Legacy (broken on v2) | Client-side |
|----------------------|-------------|
| `el.src = \`/qr?data=${encodeURIComponent(link)}\`` | `setQrCodeImage(el, link)` |

## Examples

```bash
# Interactive (recommended once after Go → v2)
sudo madmail --config /etc/maddy/maddy.conf html-migrate

# Scripted
sudo madmail --config /etc/maddy/maddy.conf html-migrate --yes
```

## Notes

- Runtime still accepts many Go forms via conversion at render time; migrating on disk is for durable, editor-friendly Minijinja sources.
- QR is **not** fixed at runtime — custom trees that still call `/qr?data=` get a 404 until this migrate (or a manual edit / re-export).
- Non-interactive sessions without `--yes` skip conversion and print how to re-run with `--yes`.
- After migration, `madmail reload` or a service restart picks up the files (live `www_dir` re-reads HTML).
- Literal `{%` / `{{` in URLs must be wrapped in `{% raw %}…{% endraw %}` — see [Customizing HTML pages](../../project/user-guide/17-customizing-html-pages.md).

## JSON output (`--json`)

```bash
madmail html-migrate --json --yes
```

Success stdout includes `action` (`noop_embedded`, `noop_already_migrated`, `skipped_noninteractive`, `declined`, `migrated`) plus `go_style_files`, `qr_legacy_files`, `qr_migrated`, `literal_brace_warnings`, and file lists.

Schema: [json-output.md](json-output.md#html-migrate).


---
[← CLI index](README.md) · [Global flags](global-flags.md) · [html-serve](html-serve.md) · [update](update.md)

[Source: `crates/chatmail/src/ctl/html.rs`, `crates/chatmail-www/src/www_migrate.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/html.rs)
