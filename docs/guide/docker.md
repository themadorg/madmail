# Docker deployment guide

Madmail publishes official container images to **GitHub Container Registry (GHCR)**. Images are built automatically on every push to `main` and tagged with `latest` and the release version (for example `2.3.1`).

## Table of contents

- [Pull the image](#pull-the-image)
- [What is in the image](#what-is-in-the-image)
- [Quick start (IP address)](#quick-start-ip-address)
- [Quick start (domain name)](#quick-start-domain-name)
  - [DNS records](#1-dns-records)
  - [Bootstrap with `install`](#2-bootstrap-with-install-recommended)
  - [Run the container](#3-run-the-container)
  - [Verify](#4-verify)
  - [Docker Compose (domain)](#docker-compose-domain)
- [Domain and hostname](#domain-and-hostname)
  - [Hostname vs mail domain](#hostname-vs-mail-domain)
  - [DNS checklist](#dns-checklist-domain-production)
  - [Registration and mail URLs](#registration-and-mail-urls-domain)
  - [Changing the domain later](#changing-the-domain-later)
  - [Delta Chat registration](#delta-chat-registration)
  - [Admin API token](#operator-access-admin-api-token)
  - [Admin web dashboard](#admin-web-dashboard-disabled-by-default)
- [TLS](#tls)
  - [Self-signed](#self-signed-testing--ip-relay)
  - [Let's Encrypt (public IP)](#lets-encrypt--public-ip-trusted-short-lived)
  - [Let's Encrypt (DNS domain)](#lets-encrypt--dns-domain-trusted-classic)
  - [Bring your own certificates](#bring-your-own-certificates-tls_mode-file)
  - [TLS quick reference](#tls-quick-reference)
- [Ports](#ports)
- [TURN (calls)](#turn-calls)
- [Volumes and layout](#volumes-and-layout)
- [Environment variables](#environment-variables-bundled-config-only)
- [Custom configuration](#custom-configuration)
- [Migrate from systemd](#migrate-from-a-native-systemd-install)
- [Docker Compose example](#docker-compose-example)
- [Offline install](#offline-install-save--load)
- [Operations](#operations)
- [Build from source](#build-from-source)
- [Related docs](#related-docs)

## Pull the image

```bash
docker pull ghcr.io/themadorg/madmail:latest
```

To pin a specific release:

```bash
docker pull ghcr.io/themadorg/madmail:2.3.1
```

| Tag | Description |
|-----|-------------|
| `latest` | Most recent release from `main` |
| `X.Y.Z` | Exact semver from [releases](https://github.com/themadorg/madmail/releases) |

GHCR packages are public; no login is required to pull.

## What is in the image

The image is a minimal Alpine runtime with:

- `/bin/madmail` — mail server binary (admin web SPA embedded at build time; **disabled by default** until `admin-web enable`)
- `/bin/iroh-relay` — WebXDC realtime relay helper
- `/etc/madmail/madmail.conf` — default SQLite config (see `assets/madmail.conf.docker` in the repo)
- `/var/lib/madmail` — state (SQLite DBs, queues, `admin_token`, DKIM keys)
- `/etc/madmail/certs/` — TLS PEM files (when using the bundled config)
- `/run/madmail` — runtime sockets and PID files

Default entrypoint (same layout as a native `madmail install`):

```text
/bin/madmail --config /etc/madmail/madmail.conf run --libexec /var/lib/madmail
```

CLI commands run inside the container (`docker exec madmail madmail …`) use the same paths automatically — you do not need to repeat `--config` or `--libexec`.

## Quick start (IP address)

Replace `203.0.113.50` with your public IP. Bootstrap with `install` (self-signed TLS, HTTPS registration on port 443), then run the container — see [Self-signed (testing / IP relay)](#self-signed-testing--ip-relay) for details.

```bash
mkdir -p /var/lib/madmail /etc/madmail /run/madmail

docker run --rm \
  --cap-add NET_BIND_SERVICE \
  -p 80:80 \
  -v /var/lib/madmail:/var/lib/madmail \
  -v /etc/madmail:/etc/madmail \
  ghcr.io/themadorg/madmail:latest \
  install --simple --ip 203.0.113.50 --skip-systemd --skip-user

docker run -d \
  --name madmail \
  --restart unless-stopped \
  --cap-add NET_BIND_SERVICE \
  -p 25:25 -p 80:80 -p 443:443 \
  -p 143:143 -p 465:465 -p 587:587 -p 993:993 \
  -p 3478:3478/udp -p 49152-65535:49152-65535/udp \
  -v /var/lib/madmail:/var/lib/madmail \
  -v /etc/madmail:/etc/madmail:ro \
  -v /run/madmail:/run/madmail \
  ghcr.io/themadorg/madmail:latest
```

Verify registration page: `curl -skI https://203.0.113.50/`

You do **not** need to create TLS files with `openssl` first — `install --simple --ip` generates a self-signed certificate pair automatically.

## Quick start (domain name)

Use this when you own a DNS name (for example `example.org`) and want trusted HTTPS for Delta Chat registration, IMAP, and SMTP.

### 1. DNS records

Point this at your Docker host’s **public IP** before starting (replace names with yours):

| Type | Name | Value | Purpose |
|------|------|-------|---------|
| `A` / `AAAA` | `example.org` | `203.0.113.50` | Server hostname (TLS, HTTPS, SMTP banner) |

Notes:

- **Hostname** and **mail domain** are both `example.org` here — one DNS name for TLS, registration, and user addresses (`alice@example.org`).
- You can use a split layout (e.g. `mail.example.org` + `example.org`) instead; see [Hostname vs mail domain](#hostname-vs-mail-domain).
- Port **80** must reach the container for Let's Encrypt HTTP-01 (issue and renewal).

### 2. Bootstrap with `install` (recommended)

`madmail install` writes `/etc/madmail/madmail.conf`, places certs in `/etc/madmail/certs/`, and uses state under `/var/lib/madmail/`. For Docker, skip systemd and the system user:

```bash
mkdir -p /var/lib/madmail /etc/madmail

docker run --rm \
  --cap-add NET_BIND_SERVICE \
  -p 80:80 \
  -v /var/lib/madmail:/var/lib/madmail \
  -v /etc/madmail:/etc/madmail \
  ghcr.io/themadorg/madmail:latest \
  install --simple --domain example.org \
    --hostname example.org \
    --ip 203.0.113.50 \
    --acme-email admin@example.org \
    --skip-systemd --skip-user
```

For a valid DNS domain, install defaults to **`tls_mode autocert`** and obtains a Let's Encrypt certificate during setup (port 80 must be free). Omit `--acme-email` to default to `admin@example.org`.

Self-signed instead of Let's Encrypt (testing only):

```bash
docker run --rm \
  --cap-add NET_BIND_SERVICE \
  -v /var/lib/madmail:/var/lib/madmail \
  -v /etc/madmail:/etc/madmail \
  ghcr.io/themadorg/madmail:latest \
  install --simple --domain example.org \
    --hostname example.org \
    --ip 203.0.113.50 \
    --tls-mode self_signed \
    --no-obtain-certificate \
    --skip-systemd --skip-user
```

### 3. Run the container

```bash
docker run -d \
  --name madmail \
  --restart unless-stopped \
  --cap-add NET_BIND_SERVICE \
  -p 25:25 \
  -p 80:80 \
  -p 443:443 \
  -p 143:143 \
  -p 465:465 \
  -p 587:587 \
  -p 993:993 \
  -p 3478:3478/udp \
  -p 49152-65535:49152-65535/udp \
  -v /var/lib/madmail:/var/lib/madmail \
  -v /etc/madmail:/etc/madmail:ro \
  -v /run/madmail:/run/madmail \
  ghcr.io/themadorg/madmail:latest
```

### 4. Verify

```bash
docker ps
curl -sI https://example.org/
docker exec madmail madmail admin-token
docker exec madmail madmail certificate status
```

Users register at **`https://example.org/`** — see [Delta Chat registration](#delta-chat-registration).

### Docker Compose (domain)

```yaml
services:
  madmail:
    image: ghcr.io/themadorg/madmail:latest
    container_name: madmail
    restart: unless-stopped
    cap_add:
      - NET_BIND_SERVICE
    ports:
      - "25:25"
      - "80:80"
      - "443:443"
      - "143:143"
      - "465:465"
      - "587:587"
      - "993:993"
      - "3478:3478/udp"
      - "49152-65535:49152-65535/udp"
    volumes:
      - /var/lib/madmail:/var/lib/madmail
      - /etc/madmail:/etc/madmail:ro
      - /run/madmail:/run/madmail
```

Run `madmail install …` once on the host (or in a one-off container) before `docker compose up -d`.

## Domain and hostname

Madmail separates three ideas that often confuse newcomers:

| Concept | Config variable | Example | Used for |
|---------|-----------------|---------|----------|
| **Hostname** | `$(hostname)` | `example.org` | SMTP/IMAP identity, TLS certificate name, server banner |
| **Primary / mail domain** | `$(primary_domain)` | `example.org` | User addresses (`user@example.org`), JIT registration, DKIM |
| **Public IP** | `$(public_ip)` | `203.0.113.50` | Client setup hints, TURN, some chatmail blocks |

In the **bundled** `madmail.conf.docker`, set these via environment variables:

| Env var | Domain example | IP example |
|---------|----------------|------------|
| `MADDY_HOSTNAME` | `example.org` | `203.0.113.50` |
| `MADDY_DOMAIN` | `example.org` | `[203.0.113.50]` |

After `madmail install`, the same values live in `/etc/madmail/madmail.conf` as `$(hostname)`, `$(primary_domain)`, and `$(public_ip)`.

### Hostname vs mail domain

Common layouts:

| Layout | Hostname | Mail domain | Typical use |
|--------|----------|-------------|-------------|
| **Split** | `mail.example.org` | `example.org` | Classic — MX points at `mail.` subdomain |
| **Single host** | `example.org` | `example.org` | Small relay — one DNS name for everything (used in this guide) |
| **IP relay** | `203.0.113.50` | `[203.0.113.50]` | No DNS name; bracketed IP as domain |

The `chatmail { … }` block in your config sets `mail_domain`, `mx_domain`, and `web_domain` — install sets these from `primary_domain` and `hostname` automatically.

### DNS checklist (domain production)

Before going live, confirm:

1. **`A` / `AAAA`** — hostname resolves to the Docker host’s public IP.
2. **`MX`** — mail domain points at the hostname (priority `10` or lower is fine).
3. **Port 25** — reachable from the internet if you want inbound federation from other mail servers (many cloud providers block it; check your host).
4. **Ports 80 and 443** — mapped to the container for HTTPS registration and ACME renewal.
5. **Reverse DNS (PTR)** — optional but helps deliverability; set with your VPS provider to match the hostname.

**Optional mail-auth records (SMTP hygiene, not required for chatmail HTTP federation):**

| Type | Name | Example |
|------|------|---------|
| `TXT` (SPF) | `@` | `v=spf1 mx a -all` |
| `TXT` (DKIM) | `default._domainkey` | public key for selector `default` (keys generated under `/var/lib/madmail/`) |
| `TXT` (DMARC) | `_dmarc` | `v=DMARC1; p=none; rua=mailto:admin@example.org` |

`madmail install --domain` obtains TLS and creates DKIM signing keys; it does not publish these TXT records. Full operator guide (native and Docker): [DNS and Mail Authentication](../project/user-guide/12-dns-mail-auth.md).

### Registration and mail URLs (domain)

| Service | URL / connection |
|---------|------------------|
| Delta Chat signup (HTTPS) | `https://example.org/` |
| IMAP | `example.org:993` (TLS) or `:143` (STARTTLS) |
| SMTP submission | `example.org:587` (STARTTLS) or `:465` (TLS) |
| Admin API | `https://example.org/api/admin` (path may vary) |

The public page is for **end users** (QR registration). The operator **admin web UI** stays disabled until `admin-web enable` (see above).

### Changing the domain later

Domains are defined in the config file and settings database. To move to a new name:

1. Update DNS (`A`, `AAAA`, `MX`) to the new host.
2. Edit `madmail.conf` (`primary_domain`, `hostname`, `chatmail` blocks) or re-run `install`.
3. Re-issue TLS: `madmail certificate regenerate` (Let's Encrypt) or replace PEM files.
4. Restart the container: `docker restart madmail`.

See also: [Deployment: IP vs domain](../project/user-guide/11-deployment-ip-domain-certs.md).

### Delta Chat registration

Madmail is a **Delta Chat** mail server. End users open the public HTTPS page to create an account — they do **not** use the operator admin dashboard for signup.

After `madmail install`, registration is served on the default HTTPS port (no port number in the URL):

```text
https://203.0.113.50/
https://example.org/
```

On that page users can:

- **Scan the QR code** with Delta Chat on another device
- **Tap the setup button** to open Delta Chat on the same device
- **Copy the DCLOGIN link** and paste it into Delta Chat to start chatting

Delta Chat may warn about the self-signed certificate on IP relays — accept it once or use `turn_off_tls` for lab setups.

### Operator access: admin API token

On first start the server writes a random admin API token to `/var/lib/madmail/admin_token` (inside the container). Operators use this token for the Admin API and the optional admin web dashboard.

Show the token (prints URL, token, and a login QR for [admin.madmail.chat](https://admin.madmail.chat)):

```bash
docker exec madmail madmail admin-token
```

Raw token only (for scripts):

```bash
docker exec madmail madmail admin-token --raw
```

### Admin web dashboard (disabled by default)

The embedded admin web UI is **off by default**. Public visitors only see the Delta Chat registration page; `/admin` returns 404 until you enable it.

Check status:

```bash
docker exec madmail madmail admin-web status
```

Enable (served at `/admin` by default):

```bash
docker exec madmail madmail admin-web enable
docker restart madmail
```

Then open `https://203.0.113.50/admin/` (or your HTTPS URL and custom path) and log in with the token from `admin-token`.

Use a non-default path on a public server (recommended):

```bash
docker exec madmail madmail admin-web path /admin-secret-path
docker restart madmail
```

Disable again:

```bash
docker exec madmail madmail admin-web disable
docker restart madmail
```

`admin-web` writes settings to the database; the running server picks them up after a container restart (`docker restart madmail`).

Alternatively, use the hosted panel at [admin.madmail.chat](https://admin.madmail.chat) with your server API URL and token — no need to enable the embedded UI.

## TLS

All TLS listeners (IMAPS, submission, HTTPS registration, etc.) read PEM files from paths in your config. The bundled Docker config uses:

```text
tls file /etc/madmail/certs/fullchain.pem /etc/madmail/certs/privkey.pem
```

### Self-signed (testing / IP relay)

**No manual `openssl` required.** Madmail writes `fullchain.pem` and `privkey.pem` when they are not present yet:

- **During `madmail install`** when you use IP simple install **without** `--auto-ip-cert` (the default for `--simple --ip`).
- **On first server start** only if `tls_mode self_signed` is already set in the config and the PEM paths are missing (the bundled `madmail.conf.docker` uses `tls file` without `tls_mode self_signed` — use `install` or set `tls_mode self_signed` yourself).

If `/etc/madmail/certs/` already contains PEM files (for example from a prior autocert install), `install` reuses them. Delete the old files first when you want a fresh self-signed pair:

```bash
rm -f /etc/madmail/certs/fullchain.pem /etc/madmail/certs/privkey.pem
```

Example — bootstrap config and certs with install, then run the long-lived container:

```bash
mkdir -p /var/lib/madmail /etc/madmail

docker run --rm \
  --cap-add NET_BIND_SERVICE \
  -p 80:80 \
  -v /var/lib/madmail:/var/lib/madmail \
  -v /etc/madmail:/etc/madmail \
  ghcr.io/themadorg/madmail:latest \
  install --simple --ip 203.0.113.50 --skip-systemd --skip-user

docker run -d --name madmail --restart unless-stopped \
  --cap-add NET_BIND_SERVICE \
  -p 25:25 -p 80:80 -p 443:443 -p 143:143 -p 465:465 -p 587:587 -p 993:993 \
  -p 3478:3478/udp -p 49152-65535:49152-65535/udp \
  -v /var/lib/madmail:/var/lib/madmail \
  -v /etc/madmail:/etc/madmail:ro \
  -v /run/madmail:/run/madmail \
  ghcr.io/themadorg/madmail:latest
```

Self-signed certs show browser / Delta Chat trust warnings; users accept the certificate once or use `turn_off_tls` for lab setups.

To replace self-signed PEMs, remove the old files and run `install --simple --ip …` again (without `--auto-ip-cert`), or delete the PEMs and restart when `tls_mode self_signed` is set so madmail can recreate them.

Check what is on disk:

```bash
docker exec madmail madmail certificate status
```

### Let's Encrypt — public IP (trusted, short-lived)

For a **public IP** with a browser-trusted certificate (~6-day Let's Encrypt IP profile, auto-renewed while the server runs):

```bash
docker run --rm \
  --cap-add NET_BIND_SERVICE \
  -p 80:80 \
  -v /var/lib/madmail:/var/lib/madmail \
  -v /etc/madmail:/etc/madmail \
  ghcr.io/themadorg/madmail:latest \
  install --simple --ip 203.0.113.50 --auto-ip-cert \
    --acme-email ops@example.org --skip-systemd --skip-user
```

Port **80** must be free during issuance and renewal. `--acme-email` must be a normal mailbox (`user@domain`), not `user@IP`.

Details: [Install on a public IP with Let's Encrypt](../install-simple-ip-acme.md).

### Let's Encrypt — DNS domain (trusted, classic)

Preferred for domain deployments — see [Quick start (domain name)](#quick-start-domain-name). Summary:

1. Create DNS records (`A`/`AAAA`, `MX`) for your hostname and mail domain.
2. Bootstrap with install (autocert is the default for valid DNS names):

```bash
docker run --rm --cap-add NET_BIND_SERVICE -p 80:80 \
  -v /var/lib/madmail:/var/lib/madmail -v /etc/madmail:/etc/madmail \
  ghcr.io/themadorg/madmail:latest \
  install --simple --domain example.org --hostname example.org \
    --ip 203.0.113.50 --acme-email admin@example.org \
    --skip-systemd --skip-user
```

3. On an **already running** container, enable autocert and obtain a cert:

```bash
docker exec madmail madmail \
  certificate autocert enable --email admin@example.org --obtain
docker restart madmail
```

With `tls_mode autocert`, madmail renews certificates automatically while the container is running (daily task; needs port `80` for HTTP-01). DNS certificates last ~90 days; renewal starts when ~30 days remain.

Obtain or force-renew manually:

```bash
docker exec madmail madmail certificate get
docker exec madmail madmail certificate regenerate
```

### Bring your own certificates (`tls_mode file`)

If you already have PEM files (certbot, Caddy, another reverse proxy):

```bash
mkdir -p /etc/madmail/certs
cp /path/to/fullchain.pem /etc/madmail/certs/
cp /path/to/privkey.pem   /etc/madmail/certs/
```

Set `tls_mode file` in the config and ensure `tls file` points at those paths (the bundled config already does). Restart the container after replacing files.

### TLS quick reference

| Goal | Approach | Port 80 needed? |
|------|----------|-----------------|
| Quick IP lab | `tls_mode self_signed` or `install --simple --ip` (no `--auto-ip-cert`) | No |
| Trusted IP relay | `install --simple --ip --auto-ip-cert` | Yes (issue + renew) |
| Trusted domain | `tls_mode autocert` + `certificate get` / install | Yes (issue + renew) |
| External PEMs | `tls_mode file` + mount certs | No |

## Ports

| Port | Service |
|------|---------|
| `25` | SMTP (inbound / outbound) |
| `143` | IMAP (STARTTLS) |
| `465` | Submission (implicit TLS) |
| `587` | Submission (STARTTLS) |
| `993` | IMAPS |
| `80` / `443` | HTTP / HTTPS — Delta Chat registration page, chatmail API, Admin API |
| `3478` / **udp** | TURN/STUN control (Delta Chat voice/video calls) |
| `49152–65535` / **udp** | TURN relay media (default range; configurable in admin/config) |

The **admin web dashboard** is optional and disabled until `admin-web enable`. When enabled it is mounted at `/admin` (or a custom path). The **Admin API** is always available at `/api/admin` when `admin_token` is not set to `disabled`.

Production configs often also expose `80` / `443` (HTTPS registration) and `8388` (Shadowsocks). Map whatever your config file listens on, for example:

```bash
-p 80:80 -p 443:443 -p 8388:8388 -p 3478:3478/udp -p 49152-65535:49152-65535/udp
```

## TURN (calls)

Delta Chat voice/video calls use the embedded TURN server in `madmail`. It needs **UDP** on two port groups (defaults shown):

| Purpose | Ports | Notes |
|---------|-------|-------|
| TURN/STUN control | `3478` | Configurable via `turn_port` / admin `__TURN_PORT__` |
| TURN relay (media) | `49152–65535` (default) | Configurable via `relay_port_min` / `relay_port_max` or admin Services page |

`madmail install` enables TURN and sets `turn_server` / `relay_ip` from your `--ip` or `--domain`. In Docker, clients must see the **host public IP** in IMAP metadata — not a container-only address. After install, confirm `turn_server` and `relay_ip` in `/etc/madmail/madmail.conf` match your VPS public IP.

### Firewall (host and cloud)

Open the same UDP ranges on the **Docker host** and in your cloud security group:

```bash
# ufw example
ufw allow 3478/udp comment 'madmail TURN/STUN'
ufw allow 49152:65535/udp comment 'madmail TURN relay'
```

If you customized `turn_port`, open that port instead of `3478`. More examples (firewalld, nftables): [Delta Chat calls — network exposure](../TDD/20-deltachat-calls.md#network-exposure-firewall).

### Docker Compose (bridged network)

Add TURN ports alongside mail/HTTPS (adjust `3478` if you changed `turn_port`; use your configured relay range instead of `49152-65535` when narrowed):

```yaml
services:
  madmail:
    image: ghcr.io/themadorg/madmail:latest
    ports:
      - "3478:3478/udp"
      - "49152-65535:49152-65535/udp"
```

Mapping the full relay range works but is heavy. For production calls, prefer **`network_mode: host`** so TURN binds directly on the host (no `ports:` entries needed for TURN; host firewall rules still apply):

```yaml
services:
  madmail:
    image: ghcr.io/themadorg/madmail:latest
    network_mode: host
    cap_add:
      - NET_BIND_SERVICE
    volumes:
      - /var/lib/madmail:/var/lib/madmail
      - /etc/madmail:/etc/madmail:ro
      - /run/madmail:/run/madmail
```

Enable TURN in the admin UI or ensure `turn_enable yes` and `__TURN_ENABLED__` are on. Troubleshooting: [Delta Chat calls (TDD)](../TDD/20-deltachat-calls.md).

### Verify TURN after building the image

From the repo root (requires Docker and Go for `context/relay-ping`):

```bash
make test-docker-turn-e2e
```

This builds a local image, runs `install --simple --ip 127.0.0.1`, maps UDP `3478` and a narrow relay range (`55000–55010` by default), runs `relay-ping` connectivity, and checks that a TURN allocation lands in that relay range. See `scripts/docker-turn-e2e.sh` for env overrides (`DOCKER_TURN_BUILD=0`, `DOCKER_TURN_SKIP_ALLOCATE=1`, `DOCKER_TURN_RELAY_MIN` / `MAX`).

## Volumes and layout

The image uses the same paths as a native systemd install so you can bind-mount host directories directly:

| Container path | Host bind mount | Purpose |
|----------------|-----------------|---------|
| `/var/lib/madmail` | `/var/lib/madmail` | State — SQLite DBs, queues, messages, DKIM keys, `admin_token` |
| `/etc/madmail` | `/etc/madmail` | Config (`madmail.conf`, `aliases`), TLS certs under `certs/` |
| `/run/madmail` | `/run/madmail` | Runtime sockets and PID files |

Typical `docker run` volume flags:

```bash
-v /var/lib/madmail:/var/lib/madmail \
-v /etc/madmail:/etc/madmail \
-v /run/madmail:/run/madmail
```

Mounting `/etc/madmail` replaces the config baked into the image — run `madmail install` before the first long-lived container (see [Quick start (IP address)](#quick-start-ip-address)).

## Environment variables (bundled config only)

If you use the image’s bundled `madmail.conf.docker` without `install`, set:

| Variable | Example | Used for |
|----------|---------|----------|
| `MADDY_HOSTNAME` | `example.org` or `203.0.113.50` | `$(hostname)` |
| `MADDY_DOMAIN` | `example.org` or `[203.0.113.50]` | `$(primary_domain)` — bracket the IP when the mail domain is an address |

After `madmail install`, hostname and domain are written into `/etc/madmail/madmail.conf` instead.

## Custom configuration

To use your own config (domain, autocert, Postgres, Shadowsocks, etc.), place `madmail.conf` under `/etc/madmail` on the host and bind-mount the three paths. The default image entrypoint already points at `/etc/madmail/madmail.conf` and `/var/lib/madmail`:

```bash
cp assets/madmail.conf.docker /etc/madmail/madmail.conf
# edit /etc/madmail/madmail.conf
docker run -d --name madmail --restart unless-stopped \
  --cap-add NET_BIND_SERVICE \
  -p 25:25 -p 80:80 -p 443:443 -p 143:143 -p 465:465 -p 587:587 -p 993:993 -p 8388:8388 \
  -p 3478:3478/udp -p 49152-65535:49152-65535/udp \
  -v /var/lib/madmail:/var/lib/madmail \
  -v /etc/madmail:/etc/madmail:ro \
  -v /run/madmail:/run/madmail \
  ghcr.io/themadorg/madmail:latest
```

Use `--user 999:988` only if host paths are owned by the `madmail` system user from a prior native install.

## Migrate from a native (systemd) install

1. **Install Docker** on the host (if needed).
2. **Stop and disable** the systemd service:

   ```bash
   systemctl stop madmail.service madmail-cert-renew.timer
   systemctl disable madmail.service madmail-cert-renew.timer
   ```

3. **Remove the host binary** (keep `/var/lib/madmail` and `/etc/madmail`):

   ```bash
   rm -f /usr/local/bin/madmail
   ```

4. **Pull and run** with the same volume mounts so existing databases and certs are reused (see [Custom configuration](#custom-configuration)).

5. **Verify:**

   ```bash
   docker ps
   docker logs madmail
   curl -sI http://127.0.0.1/
   ```

## Docker Compose example

```yaml
services:
  madmail:
    image: ghcr.io/themadorg/madmail:latest
    container_name: madmail
    restart: unless-stopped
    cap_add:
      - NET_BIND_SERVICE
    ports:
      - "25:25"
      - "80:80"
      - "443:443"
      - "143:143"
      - "465:465"
      - "587:587"
      - "993:993"
      - "3478:3478/udp"
      - "49152-65535:49152-65535/udp"
    volumes:
      - /var/lib/madmail:/var/lib/madmail
      - /etc/madmail:/etc/madmail
      - /run/madmail:/run/madmail
```

Run `madmail install …` once before `docker compose up -d` (same as [Quick start (domain name)](#quick-start-domain-name)). For TURN-only details see [TURN (calls)](#turn-calls).

## Offline install (save / load)

On a machine with network access:

```bash
docker pull ghcr.io/themadorg/madmail:latest
docker save ghcr.io/themadorg/madmail:latest | gzip -1 > madmail-latest.tar.gz
scp madmail-latest.tar.gz root@your-server:/root/
```

On the target server:

```bash
gunzip -c /root/madmail-latest.tar.gz | docker load
docker run ... ghcr.io/themadorg/madmail:latest
```

## Operations

| Task | Command |
|------|---------|
| Logs | `docker logs -f madmail` |
| Restart | `docker restart madmail` |
| Shell | `docker exec -it madmail sh` |
| Admin token | `docker exec madmail madmail admin-token` |
| Admin web on/off | `docker exec madmail madmail admin-web enable` then `docker restart madmail` |
| Upgrade image | `docker pull ghcr.io/themadorg/madmail:latest && docker stop madmail && docker rm madmail` then `docker run` again with the same volumes |

## Build from source

To build the image locally instead of pulling from GHCR:

```bash
git clone --recurse-submodules https://github.com/themadorg/madmail.git
cd madmail
docker build -t ghcr.io/themadorg/madmail:local .
```

The Dockerfile uses three stages: admin-web (Bun), Rust compile (Alpine), and Alpine runtime. See `Dockerfile` in the repository root.

## Related docs

- [Delta Chat calls — TURN ports, firewall, Docker](../TDD/20-deltachat-calls.md)
- [Deployment scenarios (IP, domain, certs)](../project/user-guide/11-deployment-ip-domain-certs.md)
- [TLS certificates (TDD)](../TDD/19-certificates.md)
- [Install: public IP + Let's Encrypt](../install-simple-ip-acme.md)
- [Configuration](../project/06-configuration-system.md)
- [Admin UI and CLI](../project/user-guide/07-admin-and-cli.md)