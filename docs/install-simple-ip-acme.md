# Install a public-IP relay with Let's Encrypt (short-lived IP certificate)

Use this flow when the mail server is reached by **public IP** (typical Delta Chat / Chatmail relay) and you want a **browser-trusted TLS certificate** from Let's Encrypt, without owning a DNS name for the host.

## Command

Run as **root** on the server (or via `sudo`):

```bash
sudo madmail install --simple --ip 203.0.113.50 --auto-ip-cert \
  --acme-email you@example.com
```

Replace:

| Placeholder | Meaning |
|-------------|---------|
| `203.0.113.50` | Example only ([RFC 5737](https://datatracker.ietf.org/doc/html/rfc5737)); use your server's **public** IPv4 or IPv6 |
| `you@example.com` | Contact email for the ACME account (must be `user@domain`, **not** `user@IP`) |

`--simple` implies non-interactive install; you do not need `--non-interactive` unless you drop `--simple`.

### Optional flags

| Flag | Purpose |
|------|---------|
| `--enable-chatmail` | Turn on Chatmail blocks in the generated config |
| `--enable-ss` | Include Shadowsocks proxy blocks |
| `--skip-systemd` | Config + certs only, no `madmail.service` |
| `--skip-user` | Do not create the `madmail` system user |
| `--dry-run` | Print planned paths without writing files |
| `--no-obtain-certificate` | Skip issuance during install (unusual with `--auto-ip-cert`) |
| `--lang` | Website / UI language for the public pages and docs (`en`, `fa`, `ru`, `es`). Seeds the `LANGUAGE` setting. |

## What this does

1. **System layout** — Installs `madmail` to `/usr/local/bin/madmail`, writes `/etc/madmail/madmail.conf`, state under `/var/lib/madmail`, enables a `madmail.service` systemd unit (unless `--skip-systemd`).
2. **TLS mode** — Sets `autocert` (not self-signed). With `--auto-ip-cert`, `turn_off_tls` stays **off** so IMAP/SMTP submission use TLS with the issued cert.
3. **Certificate** — Obtains a Let's Encrypt **short-lived IP certificate** (profile `shortlived`, ~6-day lifetime) with **HTTP-01** on `0.0.0.0:80`.
4. **Storage** — Writes PEMs to:
   - `/etc/madmail/certs/fullchain.pem`
   - `/etc/madmail/certs/privkey.pem`  
   ACME account key: `/var/lib/madmail/autocert/account.key.pem`

Implementation: `instant-acme` + HTTP-01 solver in `crates/chatmail-acme` (`obtain_ip.rs`). DNS hostname certificates still use `lers` via `madmail certificate get` when `primary_domain` is a real DNS name.

## Prerequisites

- **Root** on the target host.
- **Public routable IP** — Private LAN addresses (e.g. `10.x`, `192.168.x`) are rejected for IP certs.
- **Port 80 free** during install — Nothing else (nginx, apache, an already-running `madmail`, certbot standalone) may bind HTTP-01. Stop conflicting services first:

  ```bash
  sudo systemctl stop madmail nginx apache2 2>/dev/null || true
  ```

- **Valid ACME email** — Let's Encrypt requires a normal mailbox domain in `--acme-email`. Example: `ops@yourcompany.org`. Addresses like `admin@203.0.113.50` are rejected.
- **Outbound HTTPS** — Server must reach Let's Encrypt (production directory).

## Compared to plain `--simple --ip`

| | `--simple --ip` only | `--simple --ip --auto-ip-cert` |
|--|----------------------|--------------------------------|
| TLS | Self-signed PEM in `certs/` | Let's Encrypt IP cert (HTTP-01) |
| `turn_off_tls` | Often on for IP installs | Off (TLS enabled with real cert) |
| Delta Chat clients | Must accept self-signed / `turn_off_tls` | Standard TLS trust (public CA) |
| Port 80 during install | Not required | **Required** (HTTP-01) |

## After install

```bash
sudo systemctl enable --now madmail
sudo journalctl -u madmail -n 50 --no-pager
sudo madmail admin-token
```

Verify HTTPS (admin API or IMAPS) presents a certificate whose SAN includes your IP.

## Renewal

IP certificates are **short-lived** (~6 days). Renew before expiry (default threshold: **4 days** remaining):

```bash
sudo systemctl stop madmail   # free port 80 if madmail holds it
sudo madmail certificate get
sudo systemctl start madmail
```

Automate with cron or a systemd timer (daily is safe):

```cron
0 3 * * * root systemctl stop madmail; /usr/local/bin/madmail certificate get; systemctl start madmail
```

`certificate get` is idempotent: it skips issuance if the cert is still valid beyond the renewal window.

Force re-issue:

```bash
sudo madmail certificate regenerate
```

## Troubleshooting

### HTTP-01 / `order not ready` / `Invalid`

- Port **80** not reachable from the internet (firewall, cloud security group).
- Another process still listening on `:80`.
- Challenge cleaned up too early — use a current `madmail` build; retry after `systemctl stop madmail`.

### `--acme-email is required with --auto-ip-cert`

Pass `--acme-email` with a real domain mailbox.

### `--auto-ip-cert ignored: … is not a public IP`

The address is private, loopback, or documentation space. Use the machine's **public** IP as seen from clients.

### `Text file busy` when installing over `/usr/local/bin/madmail`

Stop the service before replacing the binary, or install to a new path and swap:

```bash
sudo systemctl stop madmail
sudo cp ./target/release/madmail /usr/local/bin/madmail
```

### Wrong systemd unit (`madmail-new.service`, CHDIR errors)

Install using the final binary name (`madmail`), not a temporary name like `madmail-new`. Remove stale units or run `madmail uninstall` (see uninstall docs for cleaning `madmail*.service` units).

## Related documentation

- [TDD/19-certificates.md](TDD/19-certificates.md) — TLS modes, `certificate` CLI, DNS autocert
- [TDD/14-cli-tools.md](TDD/14-cli-tools.md) — Full `install` flag list
- [local-dev.md](local-dev.md) — Development setup (not for production IP relays)
