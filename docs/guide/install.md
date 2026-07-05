# Native install guide (`madmail install`)

`madmail install` bootstraps a mail server: writes `madmail.conf`, TLS material, SQLite state, optional systemd unit, and (on system installs) a service user and binary under `/usr/local/bin/`.

The command is **Madmail-compatible**. The binary name (`madmail`, `chatmail`, …) is taken from `argv[0]` and drives config filenames, systemd unit name, and service user name.

## Table of contents

- [Quick start](#quick-start)
- [Install modes](#install-modes)
- [What install creates](#what-install-creates)
- [Default paths](#default-paths)
- [System install vs local paths](#system-install-vs-local-paths)
- [systemd unit behavior](#systemd-unit-behavior)
- [TLS](#tls)
- [All install options](#all-install-options)
- [Global flags](#global-flags)
- [Examples](#examples)
- [After install](#after-install)
- [Related docs](#related-docs)

---

## Quick start

### Production — public IP (self-signed TLS)

```bash
sudo madmail install --simple --ip 203.0.113.50 --lang en
sudo systemctl enable --now madmail
```

### Production — DNS domain (Let's Encrypt)

```bash
sudo madmail install --simple --domain example.org --hostname example.org \
  --acme-email admin@example.org --lang en
sudo systemctl enable --now madmail
```

Port **80** must be free during install when autocert issuance runs (default for valid DNS names).

**Before install (domain):** create an **`A` or `AAAA`** record for the hostname pointing at this machine. See [DNS and Mail Authentication](../project/user-guide/12-dns-mail-auth.md) for the full checklist (MX, SPF, DKIM, DMARC, federation).

### Local / dev — custom paths (no root)

```bash
madmail install --simple --ip 127.0.0.1 \
  --config-dir /tmp/mm --state-dir /tmp/sd --lang en
madmail --config /tmp/mm/madmail.conf run --libexec /tmp/sd
```

### Docker bootstrap (skip host systemd/user)

```bash
madmail install --simple --ip 203.0.113.50 \
  --skip-systemd --skip-user
```

See [Docker deployment guide](docker.md) for volume mounts and container layout.

---

## Install modes

| Mode | Flag | Required inputs | Notes |
|------|------|-----------------|-------|
| **Simple** | `--simple` / `-s` | `--ip` **or** `--domain` | Recommended. Enables chatmail HTTP/HTTPS blocks, Shadowsocks in config, TURN, and sensible TLS defaults. |
| **Non-interactive** | `--non-interactive` / `-n` | `--domain` (no `--simple`) | Script-oriented full install. Interactive prompts are **not implemented** — you must pass `--simple` or `--non-interactive`. |

Without `--simple` or `--non-interactive`, install exits with:

```text
interactive install is not implemented yet; use --non-interactive or --simple
```

---

## What install creates

| Artifact | Default location | Notes |
|----------|------------------|-------|
| Main config | `/etc/madmail/madmail.conf` | Maddy-style config with `chatmail { … }` blocks when enabled |
| TLS certificates | `/etc/madmail/certs/fullchain.pem`, `privkey.pem` | Self-signed, autocert, or existing files (`file` mode) |
| State directory | `/var/lib/madmail/` | `credentials.db`, `messages/`, `remote_queue/`, `autocert/` |
| SQLite credentials DB | `{state_dir}/credentials.db` | Seeded with `__LANGUAGE__` from `--lang` |
| Binary (system install) | `/usr/local/bin/madmail` | Skipped for non-system paths |
| Man page (system install) | `/usr/share/man/man1/<binary>.1` | Embedded in the binary; `mandb` refreshed when available |
| Shell completions (system install) | bash, zsh, fish under `/usr/share/…` | Tab completion for all CLI subcommands |
| systemd unit (system install) | `/etc/systemd/system/madmail.service` | Skipped with `--skip-systemd` or non-system paths |
| Service user (system install) | user/group `madmail` | `useradd -mrU`, home = state dir; skipped with `--skip-user` |

Generated config includes (when applicable):

- SMTP (25), Submission (465/587), IMAP (143/993)
- `chatmail` HTTP (80) and HTTPS (443) registration/UI
- TURN server block (UDP/TCP 3478)
- Optional Shadowsocks settings inside `chatmail` blocks (`--enable-ss` / default on with `--simple`)

---

## Default paths

When you omit `--config-dir` and `--state-dir`:

| Setting | Default |
|---------|---------|
| Config directory | `/etc/madmail` |
| Config file | `/etc/madmail/<binary>.conf` (e.g. `madmail.conf`) |
| Certificate directory | `/etc/madmail/certs/` |
| State directory | `/var/lib/<binary>` (e.g. `/var/lib/madmail`) |
| Runtime directory (in config) | `/run/<binary>` |
| Binary install path | `/usr/local/bin/<binary>` |

`<binary>` is the executable basename (`madmail` when invoked as `madmail`).

---

## System install vs local paths

Install is a **system install** (requires root: service user, binary copy, systemd) when **any** of these is true:

- Config directory is under `/etc/`, or
- State directory is under `/var/lib/`, or
- `--simple` is used **and** you did not override paths away from the defaults above

**Local install** (no root): use explicit non-FHS paths, e.g. `--config-dir /tmp/mm --state-dir /tmp/sd`. Install writes config, certs, and DB only; skips binary install, service user, and systemd.

Root error (system install without `sudo`):

```text
system install requires root (use sudo): installs service user, binary, and systemd unit; config /etc/madmail, state /var/lib/madmail
```

Use `--dry-run` to validate resolved paths without writing files or checking root.

---

## systemd unit behavior

On system installs (unless `--skip-systemd`), install writes `/etc/systemd/system/<binary>.service` and runs `systemctl daemon-reload`.

**Default paths** — unit uses systemd directory shortcuts plus explicit `ExecStart`:

```ini
StateDirectory=madmail
ConfigurationDirectory=madmail
RuntimeDirectory=madmail
LogsDirectory=madmail
WorkingDirectory=/var/lib/madmail
ReadWritePaths=/var/lib/madmail /etc/madmail
ExecStart=/usr/local/bin/madmail --config /etc/madmail/madmail.conf run --libexec /var/lib/madmail
```

**Custom `--config-dir` / `--state-dir`** — unit omits `StateDirectory` / `ConfigurationDirectory` and uses your paths in `WorkingDirectory`, `ReadWritePaths`, and `ExecStart` instead.

---

## TLS

If `--tls-mode` is omitted, install **auto-detects**:

| Condition | Selected mode | Behavior during install |
|-----------|---------------|-------------------------|
| `--auto-ip-cert` with a public IP | `autocert` | Let's Encrypt short-lived IP cert (HTTP-01 on port 80); requires `--acme-email` |
| `--tls-mode` set explicitly | as given | See table below |
| Cert and key files already exist at resolved paths | `file` | Reuses existing PEMs |
| Valid DNS domain as `primary_domain` | `autocert` | Obtains Let's Encrypt cert if `--obtain-certificate` applies (default **on**) |
| IP literal domain (`[203.0.113.50]`) | `self_signed` | Generates self-signed PEMs (unless `--auto-ip-cert`) |

### `--tls-mode` values

| Value | Install behavior |
|-------|------------------|
| `autocert` | Sets `tls_mode autocert` in config; obtains cert via HTTP-01 when `obtain_certificate` is true (default). `acme_email` defaults to `admin@<domain>`. |
| `file` | Sets `tls_mode file`; **requires** existing `--cert-path` and `--key-path` (or defaults under config `certs/`). |
| `self_signed` | Sets `tls_mode self_signed`; generates new self-signed PEMs. |

### TLS-related flags

| Flag | Default | Description |
|------|---------|-------------|
| `--auto-ip-cert` | off | With `--simple --ip`: use Let's Encrypt IP certificate instead of self-signed. Needs a **public** routable IP and `--acme-email` (must be `user@domain`, not `user@IP`). |
| `--acme-email` | empty (auto-filled for DNS autocert) | ACME account email. **Required** with `--auto-ip-cert`. |
| `--obtain-certificate` | **on** (`true`) | When TLS mode is `autocert`, obtain certificate during install (port 80 must be free). There is currently no `--no-obtain-certificate` flag; use `file` / `self_signed` modes to skip ACME issuance. |
| `--turn-off-tls` | off | Sets `turn_off_tls yes` in `chatmail` blocks (plain HTTP registration, insecure IMAP/SMTP auth). Default for `--simple --ip` without `--auto-ip-cert`. |
| `--cert-path` | `{config-dir}/certs/fullchain.pem` | TLS certificate PEM path written into config |
| `--key-path` | `{config-dir}/certs/privkey.pem` | TLS private key PEM path |

---

## All install options

| Flag | Short | Argument | Default | Description |
|------|-------|----------|---------|-------------|
| `--simple` | `-s` | — | off | Quick setup: requires `--ip` or `--domain`. Enables chatmail blocks, Shadowsocks in config, TURN, and TLS auto-detection. |
| `--non-interactive` | `-n` | — | off | Non-interactive install for scripts. Requires `--domain` when not using `--simple`. |
| `--domain` | | string | — | Mail primary domain (DNS hostname). With `--simple --domain`: must **not** be an IP — use `--ip` for IP installs. |
| `--hostname` | | string | same as domain | Server hostname (`$(hostname)` in config). SMTP EHLO, TLS SANs, etc. |
| `--ip` | | string | — | Public IP address. With `--simple --ip`: sets wrapped primary domain `[IP]`, hostname, and `public_ip`. |
| `--config-dir` | | path | `/etc/madmail` | Directory for `madmail.conf` and `certs/`. Overrides default layout; affects systemd unit when paths differ from defaults. |
| `--state-dir` | | path | `/var/lib/<binary>` for `--simple` / `/etc` config | Database, queues, `admin_token`, autocert state. |
| `--tls-mode` | | `autocert` \| `file` \| `self_signed` | auto | Force TLS mode; see [TLS](#tls). |
| `--cert-path` | | path | `{config-dir}/certs/fullchain.pem` | Certificate file for `tls file` / HTTPS |
| `--key-path` | | path | `{config-dir}/certs/privkey.pem` | Private key file |
| `--acme-email` | | email | `admin@<domain>` for DNS autocert | Let's Encrypt account contact |
| `--enable-chatmail` | | — | on with `--simple` or `--domain` | Emit `chatmail { … }` HTTP/HTTPS blocks in config |
| `--enable-ss` | | — | on with `--simple` | Add Shadowsocks `ss_addr` / `ss_password` / `ss_cipher` to chatmail blocks |
| `--turn-off-tls` | | — | off (on for `--simple --ip` without `--auto-ip-cert`) | Disable TLS requirements for chatmail/IMAP/SMTP in generated config |
| `--lang` | | `en` \| `fa` \| `ru` \| `es` | `en` | Website/UI language; seeded into DB as `__LANGUAGE__` |
| `--dry-run` | | — | off | Print resolved paths and exit before any writes; skips root check |
| `--skip-systemd` | | — | off | Do not write systemd unit or run `daemon-reload` |
| `--skip-user` | | — | off | Do not create or adjust the service system user (`useradd` / `usermod`) |
| `--binary-path` | | path | `/usr/local/bin/<binary>` | Destination for binary copy on system install |
| `--obtain-certificate` | | — | **on** | Issue Let's Encrypt cert during install when mode is `autocert` |
| `--auto-ip-cert` | | — | off | Use Let's Encrypt **IP** certificate with `--simple --ip` |

### `--simple` domain vs IP

| Invocation | `primary_domain` | `hostname` | `local_domains` | Default TLS |
|------------|------------------|------------|-----------------|-------------|
| `--simple --ip 203.0.113.50` | `[203.0.113.50]` | `203.0.113.50` | IP + hostname variants | `self_signed` (or `autocert` with `--auto-ip-cert`) |
| `--simple --domain mail.example.org` | `mail.example.org` | `--hostname` or domain | `$(primary_domain)` | `autocert` (DNS) |

`--simple --domain` rejects IP literals; `--simple --ip` requires a valid `IPv4`/`IPv6` address.

---

## Global flags

These appear on every `madmail` subcommand (including `install`):

| Flag | Alias | Env | Default | Used by `install`? |
|------|-------|-----|---------|------------------|
| `--config` | — | `CHATMAIL_CONFIG` | `/etc/madmail/madmail.conf` | **No** — install writes to `--config-dir/<binary>.conf`, not `--config`. `--config` is for other ctl commands and `run`. |
| `--state-dir` | `--libexec` | `CHATMAIL_STATE_DIR` | `/var/lib/madmail` | **Indirectly** — only when install's own `--state-dir` is omitted and install is not `--simple` with `/etc` paths; otherwise install resolves state dir from rules in [Default paths](#default-paths). |

> **Note:** The global `--state-dir` default can appear in `install --help` because of shared CLI parsing. Install path resolution compares against `/var/lib/<binary>` and treats that as the implicit default.

---

## Examples

### Full production IP relay

```bash
sudo madmail install --simple --ip 203.0.113.50 --lang en
sudo systemctl enable --now madmail
madmail admin-token
```

### IP relay with trusted Let's Encrypt IP cert

```bash
sudo madmail install --simple --ip 203.0.113.50 \
  --auto-ip-cert --acme-email admin@example.org --lang en
```

Port 80 must be reachable from the internet for HTTP-01.

### Custom FHS paths (still system install)

```bash
sudo madmail install --simple --domain example.org \
  --config-dir /etc/madmail-custom \
  --state-dir /var/lib/madmail-custom \
  --acme-email admin@example.org
```

systemd `ExecStart` and `ReadWritePaths` use the custom directories; `StateDirectory` shortcuts are omitted.

### Non-interactive domain install (no `--simple`)

```bash
sudo madmail install --non-interactive --domain example.org \
  --hostname example.org --acme-email admin@example.org
```

### Bring your own certificates

```bash
sudo madmail install --simple --domain example.org \
  --tls-mode file \
  --cert-path /etc/madmail/certs/fullchain.pem \
  --key-path /etc/madmail/certs/privkey.pem
```

PEM files must exist before install runs.

### Preview without changes

```bash
madmail install --simple --ip 203.0.113.50 --dry-run
```

### Container / CI (no systemd, no system user)

```bash
madmail install --simple --domain example.org \
  --skip-systemd --skip-user \
  --acme-email admin@example.org
```

### Persian UI

```bash
sudo madmail install --simple --ip 203.0.113.50 --lang fa
```

Supported languages: `en` (English), `fa` (Persian), `ru` (Russian), `es` (Spanish).

---

## Shell completion and man page

On **system install**, `madmail install` copies the embedded manual page and shell completion scripts into standard locations (using the executable basename from `argv[0]`):

| Shell | Path |
|-------|------|
| man | `/usr/share/man/man1/<binary>.1` |
| bash | `/usr/share/bash-completion/completions/<binary>` |
| zsh | `/usr/share/zsh/site-functions/_<binary>` |
| fish | `/usr/share/fish/vendor_completions.d/<binary>.fish` |

After install, use `man madmail` (or `man <binary>`) and press Tab in bash/zsh/fish for subcommand completion.

To install completions manually without a full system install:

```bash
madmail completion bash | sudo tee /usr/share/bash-completion/completions/madmail
madmail completion zsh  | sudo tee /usr/share/zsh/site-functions/_madmail
madmail completion fish | sudo tee /usr/share/fish/vendor_completions.d/madmail.fish
```

Hidden Madmail-compatible helpers (for packagers): `madmail generate-man`, `madmail generate-fish-completion`.

The man page source is [`docs/man/madmail.1.scd`](../man/madmail.1.scd) (scdoc → groff **man** macros, following [man-pages(7)](https://man7.org/linux/man-pages/man7/man-pages.7.html)).
Regenerate with `make man`; the rendered `docs/man/madmail.1` is embedded at build time.

---

## After install

| Task | Command |
|------|---------|
| Start (systemd) | `sudo systemctl enable --now madmail` |
| Start (local paths) | `madmail --config <conf> run --libexec <state-dir>` |
| Logs | `journalctl -u madmail -n 100 --no-pager` |
| Admin API token | `madmail admin-token` |
| Manual page | `man madmail` |
| TLS renewal (autocert) | In-process `renew-certificate` task while server runs |
| Re-issue cert | `madmail certificate get` or `regenerate` |

**Domain deployments:** add **`MX`** if you did not before install; optionally publish SPF, DKIM TXT (`default._domainkey`), and DMARC. Verify federation: `curl -sI https://<hostname>/mxdeliv` (405/400 = reachable). Details: [DNS and Mail Authentication](../project/user-guide/12-dns-mail-auth.md).

For IP/self-signed relays, Delta Chat clients may need to accept self-signed certificates or use `turn_off_tls` (set by default on `--simple --ip` without `--auto-ip-cert`).

---

## Related docs

- [DNS and Mail Authentication](../project/user-guide/12-dns-mail-auth.md) — records, SPF/DKIM/DMARC, federation checks
- [Sending and federation](../project/user-guide/05-sending-receiving-and-federation.md) — how `/mxdeliv` works
- [Docker deployment guide](docker.md) — container layout, volumes, `install` with `--skip-systemd`
- [Install: public IP + Let's Encrypt](../install-simple-ip-acme.md) — `--auto-ip-cert` details
- [CLI command reference](cli/README.md) — one page per `madmail` subcommand
- [CLI JSON output](cli/json-output.md) — `--json` schemas for scripting
- [CLI tools (TDD)](../TDD/14-cli-tools.md) — global flags and ctl overview
- [Configuration (TDD)](../TDD/13-configuration.md) — runtime config after install