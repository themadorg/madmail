# Docker Deployment Guide

Madmail provides official Docker images via **GitHub Container Registry (GHCR)**.

## Quick Start

```bash
docker pull ghcr.io/themadorg/madmail:latest
```

## Image Tags

| Tag | Description |
|-----|-------------|
| `latest` | Latest stable release from `main` branch |
| `X.Y.Z` | Specific version (e.g. `0.17.0`) |

## Examples

Three ready-to-use deployment examples are provided:

### 01 — Simple IP-based (SQLite)

> Best for: quick testing, single-server setups, low traffic

```bash
cd 01-simple-ip-example
# Edit docker-compose.yml and replace SERVER_IP with your IP
docker compose up -d
```

**Files:**
- [`docker-compose.yml`](01-simple-ip-example/docker-compose.yml) — Single container, SQLite storage
- [`maddy.conf`](01-simple-ip-example/maddy.conf) — Full configuration with comments

**Features:** Auto-generated self-signed TLS certs, DKIM keys, Shadowsocks proxy, Admin API.

---

### 02 — PostgreSQL

> Best for: production, high traffic, multiple concurrent users

```bash
cd 02-postgres-example
# Edit docker-compose.yml and replace SERVER_IP with your IP
docker compose up -d
```

**Files:**
- [`docker-compose.yml`](02-postgres-example/docker-compose.yml) — Madmail + PostgreSQL containers
- [`maddy.conf`](02-postgres-example/maddy.conf) — Configured for Postgres driver

**Features:** No `database is locked` issues, PostgreSQL pub/sub for real-time IMAP updates.

---

### 03 — Domain with Auto-TLS (Let's Encrypt)

> Best for: production with a real domain, automatic certificate management

```bash
cd 03-domain-auto-tls
# Edit docker-compose.yml: set your domain, IP, and ACME email
docker compose up -d
```

**Files:**
- [`docker-compose.yml`](03-domain-auto-tls/docker-compose.yml) — Single container, autocert TLS
- [`maddy.conf`](03-domain-auto-tls/maddy.conf) — Configured for Let's Encrypt HTTP-01

**Features:** Automatic Let's Encrypt certificates (no DNS provider API needed), HTTP→HTTPS redirect, auto-renewal. Port 80 must be open for ACME challenges.

> **Tip:** If you already have certificates (e.g., from certbot), see the comments in `maddy.conf` for how to use `tls file` instead of `autocert`.

---

## Ports

| Port | Protocol | Description |
|------|----------|-------------|
| `25` | SMTP | Incoming/outgoing email |
| `143` | IMAP | Reading email (STARTTLS) |
| `465` | Submission | Outgoing email (TLS/SSL) |
| `587` | Submission | Outgoing email (STARTTLS) |
| `993` | IMAPS | Reading email (TLS/SSL) |
| `443` | HTTPS | Admin API and registration |
| `80` | HTTP | Admin API and registration |
| `1080` | TCP | Shadowsocks proxy |

## Volumes

| Path | Description |
|------|-------------|
| `/data` | State directory — databases, queues, DKIM keys, messages |
| `/data/maddy.conf` | Main configuration file |
| `/data/tls/` | TLS certificates (`fullchain.pem`, `privkey.pem`) |

## Environment Variables

| Variable | Example | Description |
|----------|---------|-------------|
| `MADDY_HOSTNAME` | `mx.example.com` | Server hostname (MX record) |
| `MADDY_DOMAIN` | `example.com` | Primary mail domain |
| `MADDY_PUBLIC_IP` | `203.0.113.1` | Public IP for client configuration |

> **Note:** When using an IP address instead of a domain, wrap it in brackets for `MADDY_DOMAIN`: `[203.0.113.1]`

## TLS Certificates

Place your TLS certificates in the `tls/` directory (relative to `docker-compose.yml`):

```
tls/
├── fullchain.pem
└── privkey.pem
```

On first startup with `auto_create yes`, self-signed certificates will be generated if none exist.

## Building from Source

```bash
# Build the image locally
docker build -t madmail:latest .

# Or with docker compose (development)
docker compose up -d --build
```

## CI/CD

Docker images are automatically built and pushed to GHCR on every push to `main` via [GitHub Actions](../../.github/workflows/pipeline.yml).
