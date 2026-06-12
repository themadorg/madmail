# TLS certificates

Operator reference: [`context/madmail/docs/chatmail/certificate.md`](../../context/madmail/docs/chatmail/certificate.md).  
Tutorials: [`only-chatmail-domain-setup-auto-cert.md`](../../context/madmail/docs/tutorials/only-chatmail-domain-setup-auto-cert.md), [`only-chatmail-domain-setup-no-auto-cert.md`](../../context/madmail/docs/tutorials/only-chatmail-domain-setup-no-auto-cert.md).

Implementation: `crates/chatmail-acme/` ([lers](https://github.com/akrantz01/lers) for ACME HTTP-01), `crates/chatmail-tls/` (load PEM at runtime).

**Operator guide:** [`../guide/cli/certificate.md`](../guide/cli/certificate.md) · [`certificate-autocert.md`](../guide/cli/certificate-autocert.md) · install: [`../guide/cli/install.md`](../guide/cli/install.md).

## Difference from Madmail (maddy) autocert loader

Madmail can embed an **maddy `autocert` loader** in `maddy.conf` that obtains certs on first TLS connection. **madmail-v2** always uses:

```
tls file /var/lib/madmail/certs/fullchain.pem /var/lib/madmail/certs/privkey.pem
```

Let's Encrypt is obtained **out-of-band** via CLI (`madmail certificate get`) or during `madmail install --tls-mode autocert`. The server only loads PEM files.

## TLS modes (`install` / auto-detect)

| Mode | CLI | Behaviour |
|------|-----|-----------|
| `autocert` | `--tls-mode autocert --acme-email admin@domain` | HTTP-01 via lers (DNS name) → writes `certs/*.pem`, account key in `autocert/account.key.pem` |
| `autocert` (IP) | `--simple --ip PUBLIC_IP --auto-ip-cert --acme-email user@domain` | HTTP-01 via instant-acme, Let's Encrypt **shortlived** profile (~6-day IP cert) — see [`../install-simple-ip-acme.md`](../install-simple-ip-acme.md) |
| `file` | `--tls-mode file --cert-path … --key-path …` | Use existing PEMs (certbot, Caddy, etc.) |
| `self_signed` | `--tls-mode self_signed` or `--simple --ip` (without `--auto-ip-cert`) | rcgen self-signed in `certs/` (IP SANs via DNS names for bracket domains) |

Auto-detect (no `--tls-mode`):

1. Existing `fullchain.pem` + `privkey.pem` → `file`
2. Valid DNS `primary_domain` → `autocert`
3. Otherwise → `self_signed`

## Storage layout

| Path | Purpose |
|------|---------|
| `{state_dir}/certs/fullchain.pem` | Certificate chain (mode `640`) |
| `{state_dir}/certs/privkey.pem` | Private key (mode `600`) |
| `{state_dir}/autocert/account.key.pem` | ACME account key (lers renewals) |

## CLI

### `madmail certificate autocert`

- [`certificate-autocert-enable.md`](../guide/cli/certificate-autocert-enable.md) — writes `tls_mode autocert` + `acme_email` to config via `chatmail-config::update_config_autocert`
- [`certificate-autocert-status.md`](../guide/cli/certificate-autocert-status.md) — shows mode, contact email, renewal eligibility
- Enables the in-process daily renewal loop ([21-scheduled-maintenance.md](21-scheduled-maintenance.md)) when server runs

### `madmail certificate get`

- Reads `primary_domain` from `maddy.conf` (or `--domain`)
- HTTP-01 on `--http-listen` (default `0.0.0.0:80`) — **port 80 must be free**
- Skips issuance if cert valid ≥30 days unless `--force`
- `--staging` for Let's Encrypt staging

### `madmail certificate regenerate`

- Same as get but **always** issues a new certificate

### `madmail install`

Madmail-compatible flags (non-interactive / simple):

```bash
# IP relay + Let's Encrypt short-lived IP certificate (production-friendly)
sudo madmail install --simple --ip 203.0.113.50 --auto-ip-cert \
  --acme-email ops@example.org

# IP / testing (self-signed, no port 80)
sudo madmail install --simple --ip 203.0.113.50

# Domain + Let's Encrypt (stop anything on port 80 first)
sudo madmail install \
  --domain example.com \
  --hostname example.com \
  --ip 203.0.113.50 \
  --tls-mode autocert \
  --acme-email admin@example.com \
  --enable-chatmail \
  --non-interactive

# Domain + existing certs (e.g. certbot)
sudo madmail install \
  --domain example.com \
  --tls-mode file \
  --cert-path /etc/letsencrypt/live/example.com/fullchain.pem \
  --key-path /etc/letsencrypt/live/example.com/privkey.pem \
  --enable-chatmail \
  --non-interactive
```

Writes `/etc/madmail/madmail.conf` (or `{binary}.conf`), creates state dirs, optional systemd unit (`--skip-systemd` to omit).

## Renewal

### In-process (`tls_mode autocert`)

When `maddy.conf` sets `tls_mode autocert`, `chatmail run` starts a **daily** renewal loop via `chatmail-tasks::spawn_maintenance_scheduler`:

1. `SupervisorCertRenewer` (`chatmail/src/supervisor/cert_renew.rs`) checks PEM expiry.
2. Renews when fewer than **30 days** remain (DNS) or **4 days** (IP / short-lived profile).
3. Stops the plain HTTP listener on port 80, runs HTTP-01 via `chatmail-acme`, reloads TLS listeners.
4. Manual trigger: `madmail tasks run renew-certificate` (aliases: `renew-cert`, `certificate-renew`).

Requires port 80 to be available during issuance (same as CLI `certificate get`).

### External (cron / systemd timer)

- **Cron/systemd timer:** `madmail certificate get` (idempotent)
- **IP certificates (`--auto-ip-cert`):** ~6-day lifetime; renew when fewer than 4 days remain — run `certificate get` daily (free port 80 during issuance). See [`../install-simple-ip-acme.md`](../install-simple-ip-acme.md).
- **After renew:** `systemctl reload madmail` or `madmail reload`

## DNS-01 (`acme` mode)

Not implemented in madmail-v2 v1 (Madmail supports Cloudflare etc.). Use `file` mode with external certbot DNS plugin, or HTTP-01 `autocert`.

## Related

- [`../install-simple-ip-acme.md`](../install-simple-ip-acme.md) — operator guide: `--simple --ip --auto-ip-cert`
- [`13-configuration.md`](13-configuration.md) — `tls file` parsing
- [`14-cli-tools.md`](14-cli-tools.md) — command parity
- [`15-deployment.md`](15-deployment.md) — production checklist (when written)

## Related RFCs

TLS certificates and ACME issuance. Offline copies: [`RFC/README.md`](RFC/README.md). Regenerate: [`RFC/download-rfcs.sh`](RFC/download-rfcs.sh).

| RFC | Topic | Local file |
|-----|-------|------------|
| [8446](https://datatracker.ietf.org/doc/html/rfc8446) | TLS 1.3 (server certificates) | [rfc8446.txt](RFC/rfc8446.txt) |
| [8555](https://datatracker.ietf.org/doc/html/rfc8555) | ACME protocol (`chatmail-acme` / lers) | [rfc8555.txt](RFC/rfc8555.txt) |
| [8314](https://datatracker.ietf.org/doc/html/rfc8314) | TLS for mail submission | [rfc8314.txt](RFC/rfc8314.txt) |
| [9110](https://datatracker.ietf.org/doc/html/rfc9110) | HTTP-01 challenge (port 80) | [rfc9110.txt](RFC/rfc9110.txt) |
