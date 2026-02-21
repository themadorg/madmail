# TLS Certificate Configuration

Madmail supports four TLS certificate modes to cover every deployment scenario — from quick local testing to production with automatic Let's Encrypt certificates.

## TLS Modes Overview

| Mode | Challenge | Needs DNS API? | Needs Port 80? | Best For |
|------|-----------|----------------|-----------------|----------|
| **`autocert`** | HTTP-01 | ❌ No | ✅ Yes | Production (recommended) |
| **`acme`** | DNS-01 | ✅ Yes | ❌ No | Production behind firewalls |
| **`file`** | — | — | — | Existing certs (certbot, etc.) |
| **`self_signed`** | — | — | — | Testing, IP-only setups |

## Auto-Detection Logic

When no `--tls-mode` is specified, maddy auto-detects the best mode:

```
1. Existing cert files on disk?     → file
2. Valid DNS domain name?           → autocert (HTTP-01)
3. IP address or localhost?         → self_signed
```

---

## Mode 1: `autocert` (Recommended)

Automatic Let's Encrypt certificates using the **HTTP-01 challenge**. No DNS provider API token needed — just requires port 80 to be open and reachable from the internet.

### How It Works

1. Maddy starts an HTTP server on port 80 for ACME challenge verification
2. On the first HTTPS connection, Let's Encrypt sends a challenge to port 80
3. Maddy responds to the challenge, proving domain ownership
4. Let's Encrypt issues a certificate (valid 90 days, auto-renewed)
5. Non-ACME HTTP requests on port 80 are redirected to HTTPS (302)

### Install Command

```bash
sudo maddy install \
  --domain chat.example.org \
  --hostname chat.example.org \
  --tls-mode autocert \
  --acme-email admin@example.org \
  --enable-chatmail \
  --non-interactive
```

### Generated Config

```
tls {
    loader autocert {
        hostname chat.example.org
        email admin@example.org
        cache_dir /var/lib/maddy/autocert
        agreed
    }
}
```

### Requirements

- **Port 80** must be open and reachable from the internet (for ACME challenges)
- **Port 443** must be open (for HTTPS)
- **DNS A record** must point to the server's public IP
- No firewall, CDN, or reverse proxy blocking port 80

### Certificate Storage

Certificates are cached in `/var/lib/maddy/autocert/` with mode `700` (owner: `maddy`). Certificates persist across restarts and are automatically renewed before expiry.

---

## Mode 2: `acme` (DNS-01 Challenge)

Automatic Let's Encrypt certificates using the **DNS-01 challenge**. Requires a DNS provider API token but does NOT need port 80 open. Best when you can't open port 80 (firewalls, CDNs) or need wildcard certificates.

### Install Command

```bash
sudo maddy install \
  --domain chat.example.org \
  --tls-mode acme \
  --acme-email admin@example.org \
  --acme-dns-provider cloudflare \
  --acme-dns-token "your-api-token" \
  --enable-chatmail \
  --non-interactive
```

### Generated Config

```
tls {
    loader acme {
        hostname chat.example.org
        email admin@example.org
        agreed
        challenge dns-01
        store_path /var/lib/maddy/acme
        dns cloudflare {
            api_token "your-api-token"
        }
    }
}
```

### Supported DNS Providers

- `cloudflare` — Cloudflare API
- `gandi` — Gandi LiveDNS
- `digitalocean` — DigitalOcean DNS
- `vultr` — Vultr DNS
- `hetzner` — Hetzner DNS
- `route53` — AWS Route 53
- `namecheap` — Namecheap DNS

### Certificate Storage

Certificates are stored in `/var/lib/maddy/acme/` with mode `700` (owner: `maddy`).

---

## Mode 3: `file` (User-Provided Certificates)

Use your own certificate files — from certbot, a CA, or any other source.

### Install Command

```bash
sudo maddy install \
  --domain chat.example.org \
  --tls-mode file \
  --cert-path /etc/letsencrypt/live/chat.example.org/fullchain.pem \
  --key-path /etc/letsencrypt/live/chat.example.org/privkey.pem \
  --enable-chatmail \
  --non-interactive
```

### Generated Config

```
tls file /etc/letsencrypt/live/chat.example.org/fullchain.pem /etc/letsencrypt/live/chat.example.org/privkey.pem
```

### File Permissions

```bash
# Cert files must be readable by the maddy user
sudo chown root:maddy /path/to/fullchain.pem /path/to/privkey.pem
sudo chmod 640 /path/to/fullchain.pem /path/to/privkey.pem
```

### Renewing Certificates

When using certbot, set up a post-renewal hook to reload maddy:

```bash
# /etc/letsencrypt/renewal-hooks/deploy/maddy-reload.sh
#!/bin/bash
systemctl reload maddy
```

---

## Mode 4: `self_signed` (Testing / IP-Only)

Auto-generated self-signed certificates. Best for local testing, development, or IP-based deployments where no domain is available.

### Install Command

```bash
# IP-based setup
sudo maddy install \
  --simple --ip 203.0.113.50 \
  --tls-mode self_signed \
  --turn-off-tls

# Domain-based testing
sudo maddy install \
  --domain test.example.org \
  --tls-mode self_signed \
  --turn-off-tls \
  --non-interactive
```

### Generated Config

```
tls file /var/lib/maddy/certs/fullchain.pem /var/lib/maddy/certs/privkey.pem
```

### Certificate Storage & Persistence

Self-signed certificates are stored in the **state directory** (`/var/lib/maddy/certs/`) with secure permissions:

| File | Owner | Mode | Description |
|------|-------|------|-------------|
| `fullchain.pem` | `maddy:maddy` | `640` | Self-signed certificate |
| `privkey.pem` | `maddy:maddy` | `600` | RSA private key |

On subsequent installs, existing certificates are **reused** (auto-detected as `file` mode), not regenerated. This ensures clients that have previously trusted the self-signed cert continue to work.

### Client Configuration

Clients connecting to self-signed servers will show certificate warnings. For Delta Chat:
- The `--turn-off-tls` flag disables strict TLS verification in the server configuration
- Delta Chat clients support connecting with `--turn-off-tls` enabled

### Upgrading to Production

When you're ready to move to production with a real domain:

```bash
# Delete old self-signed certs
sudo rm /var/lib/maddy/certs/fullchain.pem /var/lib/maddy/certs/privkey.pem

# Re-run install with autocert
sudo maddy install \
  --domain chat.example.org \
  --tls-mode autocert \
  --acme-email admin@example.org \
  --enable-chatmail \
  --non-interactive

sudo systemctl restart maddy
```

---

## Switching Between Modes

### From `self_signed` to `autocert`

```bash
# Remove old self-signed certs so auto-detect doesn't pick them up
sudo rm /var/lib/maddy/certs/fullchain.pem /var/lib/maddy/certs/privkey.pem

# Re-install with autocert
sudo maddy install \
  --domain chat.example.org \
  --tls-mode autocert \
  --acme-email admin@example.org \
  --enable-chatmail \
  --non-interactive

sudo systemctl restart maddy
```

### From `autocert` to `acme` (DNS-01)

```bash
sudo maddy install \
  --domain chat.example.org \
  --tls-mode acme \
  --acme-dns-provider cloudflare \
  --acme-dns-token "your-token" \
  --enable-chatmail \
  --non-interactive

sudo systemctl restart maddy
```

### From any mode to `file`

```bash
sudo maddy install \
  --domain chat.example.org \
  --tls-mode file \
  --cert-path /path/to/fullchain.pem \
  --key-path /path/to/privkey.pem \
  --enable-chatmail \
  --non-interactive

sudo systemctl restart maddy
```

---

## Troubleshooting

### `autocert`: Certificate not obtained

1. **Is port 80 open?**
   ```bash
   sudo ss -tlnp | grep ':80'
   curl http://your-domain/ -v
   ```

2. **Is the DNS A record correct?**
   ```bash
   dig +short your-domain.com A
   # Should return your server's public IP
   ```

3. **Is a firewall blocking port 80?**
   ```bash
   sudo ufw status
   sudo iptables -L -n | grep 80
   ```

4. **Check maddy logs:**
   ```bash
   sudo journalctl -u maddy -f
   ```

### `acme`: DNS challenge failing

1. **Is the DNS API token correct?**
   ```bash
   # Test Cloudflare API
   curl -s -H "Authorization: Bearer YOUR_TOKEN" \
     "https://api.cloudflare.com/client/v4/user/tokens/verify"
   ```

2. **Does the token have DNS edit permissions?**
   - Cloudflare: Zone → DNS → Edit

### `file`: Permission denied

```bash
# Fix permissions
sudo chown root:maddy /path/to/cert.pem /path/to/key.pem
sudo chmod 640 /path/to/cert.pem /path/to/key.pem
```

### `self_signed`: Client can't connect

- Ensure `--turn-off-tls` was used during install
- Check that the generated config has `turn_off_tls yes`
- Delta Chat clients should accept self-signed certs when this is enabled

### Verifying the certificate

```bash
# Check what certificate the server presents
echo | openssl s_client -connect your-domain:443 -servername your-domain 2>&1 | \
  openssl x509 -noout -subject -issuer -dates
```

---

## Security Notes

- **Private keys** are stored with mode `600` (owner-only read)
- **Certificates** are stored with mode `640` (owner + group read)
- **autocert cache** directory has mode `700` (owner-only access)
- All TLS-related files are owned by the `maddy` user/group
- The `autocert` and `acme` loaders only accept certificates for explicitly whitelisted hostnames
- Let's Encrypt certificates are valid for 90 days and renewed automatically ~30 days before expiry
