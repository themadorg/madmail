# Madmail CLI Commands Reference

Complete reference for all `maddy` command-line interface commands, parameters, and usage examples.

## Global Flags

These flags apply to all commands:

| Flag | Env Var | Default | Description |
|------|---------|---------|-------------|
| `--config PATH` | `MADDY_CONFIG` | `/etc/maddy/maddy.conf` | Configuration file to use |
| `--debug` | — | `false` | Enable debug logging |

---

## `maddy run`

Start the mail server.

```bash
maddy run [options]
```

| Flag | Default | Description |
|------|---------|-------------|
| `--libexec PATH` | `/var/lib/maddy` | Path to the libexec directory |
| `--log TARGET` | `stderr` | Logging target(s) |

### Examples

```bash
# Start with default config
sudo maddy run

# Start with custom config and libexec dir
sudo maddy --config /etc/maddy/maddy.conf run --libexec /var/lib/maddy

# Start with debug logging
sudo maddy --debug run
```

---

## `maddy install`

Install and configure the Madmail server. This is the primary setup command that handles creating system users, generating configuration, setting up TLS certificates, configuring DNS, and installing systemd services.

```bash
maddy install [options]
```

### Installation Modes

| Mode | Flag | Description |
|------|------|-------------|
| **Interactive** | *(default)* | Walks through all configuration options with prompts |
| **Simple** | `--simple, -s` | Minimal questions, sensible defaults, enables chatmail |
| **Non-interactive** | `--non-interactive, -n` | Uses all default values, no prompts |

### All Parameters

#### Core Configuration

| Flag | Default | Description |
|------|---------|-------------|
| `--domain DOMAIN` | — | Primary domain for the mail server (e.g., `example.org`) |
| `--hostname HOSTNAME` | — | MX hostname (e.g., `mx.example.org`) |
| `--ip ADDRESS` | — | Public IP address. In `--simple` mode, also sets domain/hostname |

#### Directory Layout

| Flag | Default | Description |
|------|---------|-------------|
| `--state-dir PATH` | `/var/lib/maddy` | Directory for maddy state files (databases, certs) |
| `--config-dir PATH` | `/etc/maddy` | Directory for configuration files |
| `--libexec-dir PATH` | `/var/lib/maddy` | Directory for runtime files |

#### TLS Configuration

| Flag | Default | Description |
|------|---------|-------------|
| `--tls-mode MODE` | *(auto-detect)* | TLS mode: `autocert`, `acme`, `file`, or `self_signed` |
| `--cert-path PATH` | `/var/lib/maddy/certs/fullchain.pem` | Path to TLS certificate file |
| `--key-path PATH` | `/var/lib/maddy/certs/privkey.pem` | Path to TLS private key file |
| `--generate-certs` | `false` | Generate self-signed TLS certificates |
| `--turn-off-tls` | `false` | Disable TLS verification for clients (useful for self-signed) |

#### ACME (Let's Encrypt) Configuration

| Flag | Default | Description |
|------|---------|-------------|
| `--acme-email EMAIL` | `admin@DOMAIN` | Email for Let's Encrypt registration |
| `--acme-dns-provider NAME` | — | DNS provider for DNS-01 challenge (only for `--tls-mode acme`) |
| `--acme-dns-token TOKEN` | — | API token for the DNS provider (only for `--tls-mode acme`) |

#### Chatmail & Features

| Flag | Default | Description |
|------|---------|-------------|
| `--enable-chatmail` | `false` | Enable chatmail endpoint for automatic user registration |
| `--enable-contact-sharing` | `true` | Enable DeltaChat contact sharing feature |
| `--require-pgp-encryption` | `false` | Require PGP encryption for outbound messages |
| `--allow-secure-join` | `true` | Allow SecureJoin requests without encryption |
| `--pgp-passthrough-senders ADDR...` | — | Sender addresses that bypass PGP requirements |
| `--pgp-passthrough-recipients ADDR...` | — | Recipient addresses that bypass PGP requirements |

#### Shadowsocks Proxy

| Flag | Default | Description |
|------|---------|-------------|
| `--enable-ss` | `false` | Enable Shadowsocks proxy for faster messaging |
| `--ss-password PASS` | *(auto-generated)* | Shadowsocks password |

#### TURN Server (Video Calls)

| Flag | Default | Description |
|------|---------|-------------|
| `--enable-turn` | `true` | Enable TURN relay for video calls |
| `--disable-turn` | `false` | Disable TURN relay |
| `--turn-server HOST` | *(same as hostname)* | TURN server hostname |
| `--turn-secret SECRET` | *(auto-generated)* | TURN shared secret |

#### Iroh Relay (Webxdc)

| Flag | Default | Description |
|------|---------|-------------|
| `--enable-iroh` | `false` | Enable Iroh relay for Webxdc realtime |
| `--iroh-port PORT` | `3340` | Port for the Iroh relay |

#### Message Limits

| Flag | Default | Description |
|------|---------|-------------|
| `--max-message-size SIZE` | `32M` | Maximum message size (e.g., `32M`, `100M`) |

#### System Integration

| Flag | Default | Description |
|------|---------|-------------|
| `--binary-path PATH` | `/usr/local/bin/maddy` | Where to install the maddy binary |
| `--systemd-path PATH` | `/etc/systemd/system` | Directory for systemd service files |
| `--maddy-user USER` | `maddy` | System user to run maddy as |
| `--maddy-group GROUP` | `maddy` | System group to run maddy as |
| `--skip-user` | `false` | Skip creation of system user and group |
| `--skip-systemd` | `false` | Skip systemd service file installation |

#### Misc

| Flag | Default | Description |
|------|---------|-------------|
| `--dry-run` | `false` | Show what would be done without making changes |
| `--skip-dns` | `false` | Skip interactive DNS configuration |
| `--skip-sync` | `false` | Disable SQLite synchronous mode (unsafe) |
| `--cloudflare` | `true` | Add Cloudflare-specific DNS record notes |
| `--debug, -d` | `false` | Enable debug logging in generated config |

---

### Install Use Cases

#### 1. Quick IP-Based Setup (Testing / Local)

The fastest way to get a running instance. Uses self-signed certificates and an IP address as the domain:

```bash
sudo maddy install --simple --ip 203.0.113.50
```

This will:
- Generate self-signed TLS certificates
- Set domain to `[203.0.113.50]`
- Enable chatmail (automatic user registration)
- Enable contact sharing
- Enable TURN for video calls
- Skip all interactive prompts

#### 2. Production Domain with Let's Encrypt (Recommended)

Full production setup with automatic certificate management via HTTP-01:

```bash
sudo maddy install \
  --domain chat.example.org \
  --tls-mode autocert \
  --acme-email admin@example.org \
  --enable-chatmail \
  --enable-ss
```

This will:
- Obtain and auto-renew Let's Encrypt certificates via HTTP-01 challenge
- Port 80 is used for ACME challenges and HTTP→HTTPS redirect
- No DNS provider API token needed
- Enable chatmail for user registration
- Enable Shadowsocks proxy

#### 2b. Production with DNS-01 Challenge (Behind Firewall)

When port 80 is blocked, use DNS-01 challenge instead:

```bash
sudo maddy install \
  --domain chat.example.org \
  --tls-mode acme \
  --acme-email admin@example.org \
  --acme-dns-provider cloudflare \
  --acme-dns-token "your-cloudflare-api-token" \
  --enable-chatmail \
  --enable-ss
```

This will:
- Configure Let's Encrypt via DNS-01 challenge
- Requires DNS provider API token
- Enable chatmail for user registration
- Enable Shadowsocks proxy

#### 3. Production Domain with Existing Certificates

If you already have certificates (e.g., from certbot):

```bash
sudo maddy install \
  --domain chat.example.org \
  --tls-mode file \
  --cert-path /etc/letsencrypt/live/chat.example.org/fullchain.pem \
  --key-path /etc/letsencrypt/live/chat.example.org/privkey.pem \
  --enable-chatmail
```

#### 4. Fully Non-Interactive with All Defaults

```bash
sudo maddy install --non-interactive --domain example.org
```

Uses all defaults: standard ports, self-signed certs (if no certs found), no chatmail.

#### 5. Simple Setup with Shadowsocks and TURN

For restricted network environments:

```bash
sudo maddy install \
  --simple \
  --ip 203.0.113.50 \
  --enable-ss \
  --enable-turn \
  --turn-off-tls
```

#### 6. Simple Setup with Debug Logging

```bash
sudo maddy install --simple --ip 203.0.113.50 --debug
```

#### 7. Dry Run (Preview Changes)

See what would be done without making any changes:

```bash
sudo maddy install --simple --ip 203.0.113.50 --dry-run
```

#### 8. Custom Ports and Directories

```bash
sudo maddy install \
  --domain example.org \
  --state-dir /opt/maddy/state \
  --config-dir /opt/maddy/config \
  --binary-path /opt/maddy/bin/maddy \
  --maddy-user mailserver \
  --maddy-group mailserver
```

#### 9. Interactive Installation

Simply run without flags to be guided through all options:

```bash
sudo maddy install
```

The interactive mode will prompt for:
- Domain and hostname
- TLS mode (autocert / acme / file / self_signed)
- Network ports
- Authentication settings
- Chatmail configuration
- Shadowsocks, TURN, PGP settings
- DNS provider configuration (if acme mode selected)

#### 10. Minimal Server (No Chatmail, No Extras)

Traditional mail server without chatmail features:

```bash
sudo maddy install \
  --domain example.org \
  --tls-mode file \
  --cert-path /etc/ssl/mail/cert.pem \
  --key-path /etc/ssl/mail/key.pem
```

#### 11. With Iroh Relay (Webxdc Realtime)

```bash
sudo maddy install \
  --simple \
  --ip 203.0.113.50 \
  --enable-iroh \
  --iroh-port 3340
```

#### 12. Production Behind Cloudflare (DNS-01)

When behind Cloudflare proxy, port 80 is not directly accessible, so use DNS-01:

```bash
sudo maddy install \
  --domain chat.example.org \
  --tls-mode acme \
  --acme-email admin@example.org \
  --acme-dns-provider cloudflare \
  --acme-dns-token "cf-api-token" \
  --enable-chatmail \
  --enable-ss \
  --enable-turn \
  --cloudflare
```

The `--cloudflare` flag adds reminders to disable Cloudflare's proxy for DNS records that need direct access (MX, SMTP).

#### 13. Production with Autocert (Simplest)

The simplest production setup — just needs a domain and port 80:

```bash
sudo maddy install \
  --domain chat.example.org \
  --tls-mode autocert \
  --acme-email admin@example.org \
  --enable-chatmail \
  --non-interactive
```

No DNS provider API token, no manual certs — just domain + email.

---

## `maddy uninstall`

Remove the maddy installation from the system.

```bash
maddy uninstall [options]
```

| Flag | Default | Description |
|------|---------|-------------|
| `--keep-data` | `false` | Keep state directory (databases, certificates) |
| `--keep-config` | `false` | Keep configuration files |
| `--yes, -y` | `false` | Skip confirmation prompts |
| `--dry-run` | `false` | Show what would be removed without removing |

### Examples

```bash
# Full removal with confirmation
sudo maddy uninstall

# Remove everything except data
sudo maddy uninstall --keep-data

# Force removal without prompts
sudo maddy uninstall -y
```

---

## `maddy upgrade`

Upgrade the maddy binary. Verifies the Ed25519 signature before replacing the current executable. Automatically handles stopping and restarting the maddy systemd service.

```bash
maddy upgrade PATH_OR_URL
```

### Examples

```bash
# Upgrade from a local file
sudo maddy upgrade /tmp/maddy-new

# Upgrade from a URL (downloads, verifies, replaces)
sudo maddy upgrade https://example.com/maddy-v2.0.0

# Alias: 'update' works the same way
sudo maddy update https://example.com/maddy-v2.0.0
```

---

## `maddy status`

Show server status including active connections, registered users, uptime, and email server tracking.

```bash
maddy status [options]
```

| Flag | Default | Description |
|------|---------|-------------|
| `--details, -d` | `false` | Show per-port connection breakdown |

### Examples

```bash
# Basic status
maddy --config /etc/maddy/maddy.conf status

# Detailed per-port breakdown
maddy --config /etc/maddy/maddy.conf status --details
```

### Output

```
IMAP            connections: 42     unique IPs: 28
TURN            relays: 3
Shadowsocks     connections: 15     unique IPs: 12

Registered users:   156
Boot time:          2026-02-21 10:30:00 (up 6h 19m 15s)

Email servers seen (since last restart):
  Connection IPs:   45
  Domain servers:   23
  IP servers:       8
```

---

## `maddy admin-token`

Display the admin API credentials for this server.

```bash
maddy admin-token [options]
```

| Flag | Env Var | Default | Description |
|------|---------|---------|-------------|
| `--state-dir PATH` | `MADDY_STATE_DIR` | `/var/lib/maddy` | Path to the state directory |
| `--raw` | — | `false` | Print only the raw token (for scripts) |

### Examples

```bash
# Show token with pretty formatting
maddy admin-token

# Get raw token for use in scripts
TOKEN=$(maddy admin-token --raw)

# Use in a curl command
curl -X POST https://your-server/api/admin \
  -H 'Content-Type: application/json' \
  -d "{\"method\":\"GET\",\"resource\":\"/admin/status\",\"headers\":{\"Authorization\":\"Bearer $TOKEN\"}}"
```

---

## User Management

### `maddy creds`

Manage user authentication credentials.

```bash
maddy creds SUBCOMMAND [options] [args]
```

| Subcommand | Args | Description |
|-----------|------|-------------|
| `list` | — | List all credential entries |
| `create` | `USERNAME` | Create credentials for a user |
| `remove` | `USERNAME` | Remove credentials for a user |
| `password` | `USERNAME` | Change password for a user |

| Flag | Env Var | Default | Description |
|------|---------|---------|-------------|
| `--cfg-block NAME` | `MADDY_CFGBLOCK` | `local_authdb` | Auth module config block |
| `--password PASS` | — | *(prompt)* | Password (otherwise prompted interactively) |
| `--hash ALGO` | — | `bcrypt` | Password hash algorithm |
| `--bcrypt-cost N` | — | `10` | Bcrypt cost factor |

### Examples

```bash
# Create a user
sudo maddy --config /etc/maddy/maddy.conf creds create user@example.org

# List all users
sudo maddy --config /etc/maddy/maddy.conf creds list

# Change password
sudo maddy --config /etc/maddy/maddy.conf creds password user@example.org

# Remove credentials
sudo maddy --config /etc/maddy/maddy.conf creds remove user@example.org
```

### `maddy imap-acct`

Manage IMAP storage accounts.

```bash
maddy imap-acct SUBCOMMAND [options] [args]
```

| Subcommand | Args | Description |
|-----------|------|-------------|
| `list` | — | List all IMAP accounts |
| `create` | `USERNAME` | Create an IMAP account |
| `remove` | `USERNAME` | Remove an IMAP account and all mailboxes |

### Examples

```bash
# Create IMAP account (after creating credentials)
sudo maddy --config /etc/maddy/maddy.conf imap-acct create user@example.org

# List all IMAP accounts
sudo maddy --config /etc/maddy/maddy.conf imap-acct list
```

### `maddy delete`

Fully delete a user account: credentials, IMAP storage, quota, and block from re-registration.

```bash
maddy delete [options] USERNAME
```

| Flag | Default | Description |
|------|---------|-------------|
| `--yes, -y` | `false` | Skip confirmation |
| `--auth-block NAME` | `local_authdb` | Auth module config block |
| `--storage-block NAME` | `local_mailboxes` | Storage module config block |
| `--reason TEXT` | `account deleted via CLI` | Reason for blocking |

### Examples

```bash
# Delete with confirmation prompt
sudo maddy --config /etc/maddy/maddy.conf delete user@example.org

# Delete without confirmation
sudo maddy --config /etc/maddy/maddy.conf delete -y user@example.org
```

---

## `maddy blocklist`

Manage the blocklist of usernames prevented from re-registration.

```bash
maddy blocklist SUBCOMMAND [options] [args]
```

| Subcommand | Args | Description |
|-----------|------|-------------|
| `list` | — | List all blocked users |
| `add` | `USERNAME [REASON]` | Block a username |
| `remove` | `USERNAME` | Unblock a username |

### Examples

```bash
# List blocked users
sudo maddy --config /etc/maddy/maddy.conf blocklist list

# Block a user
sudo maddy --config /etc/maddy/maddy.conf blocklist add user@example.org "spam"

# Unblock a user
sudo maddy --config /etc/maddy/maddy.conf blocklist remove user@example.org
```

---

## `maddy sharing`

Manage DeltaChat contact sharing links. These create short URLs on the chatmail web interface that redirect to DeltaChat contact invitations.

```bash
maddy sharing SUBCOMMAND [options] [args]
```

| Subcommand | Args | Description |
|-----------|------|-------------|
| `list` | — | List all contact share links |
| `create` | `SLUG URL [NAME]` | Create a new share link |
| `reserve` | `SLUG` | Reserve a slug without a link |
| `remove` / `delete` | `SLUG` | Remove a share link |
| `edit` | `SLUG NEW_URL [NEW_NAME]` | Edit an existing share link |

### Examples

```bash
# Create a contact sharing link
maddy --config /etc/maddy/maddy.conf sharing create myname "https://i.delta.chat/#ABCDEF..." "My Name"

# List all links
maddy --config /etc/maddy/maddy.conf sharing list

# Reserve a slug for later
maddy --config /etc/maddy/maddy.conf sharing reserve vip

# Edit a link
maddy --config /etc/maddy/maddy.conf sharing edit myname "https://i.delta.chat/#NEWKEY..." "New Name"

# Delete a link
maddy --config /etc/maddy/maddy.conf sharing delete myname
```

---

## `maddy endpoint-cache`

Manage endpoint override cache. Allows redirecting outbound mail delivery to specific hosts without modifying system DNS. Useful for testing, migrations, and federation with non-DNS-reachable servers.

Alias: `maddy dns-cache`

```bash
maddy endpoint-cache SUBCOMMAND [options] [args]
```

| Subcommand | Args | Description |
|-----------|------|-------------|
| `list` | — | List all endpoint override entries |
| `set` | `LOOKUP_KEY TARGET_HOST [COMMENT]` | Create or update an override |
| `get` | `LOOKUP_KEY` | Show a specific override entry |
| `remove` / `delete` | `LOOKUP_KEY` | Remove an override entry |

### Examples

```bash
# Route mail for a domain to a specific server
maddy --config /etc/maddy/maddy.conf endpoint-cache set nine.testrun.org 10.0.0.5 "Route to staging"

# Redirect IP-based delivery
maddy --config /etc/maddy/maddy.conf endpoint-cache set 1.1.1.1 2.2.2.2 "Redirect"

# List all overrides
maddy --config /etc/maddy/maddy.conf endpoint-cache list

# Remove an override
maddy --config /etc/maddy/maddy.conf endpoint-cache remove nine.testrun.org
```

---

## HTML Customization

### `maddy html-export`

Export the embedded HTML templates to a directory for customization.

```bash
maddy html-export DEST_DIR
```

### `maddy html-serve`

Configure maddy to serve HTML from an external directory (or revert to embedded files).

```bash
maddy --config /etc/maddy/maddy.conf html-serve WWW_DIR
```

Use `embedded` as the directory to revert to built-in templates:

```bash
maddy --config /etc/maddy/maddy.conf html-serve embedded
```

### Examples

```bash
# Export templates for editing
maddy html-export /tmp/maddy-templates

# Edit templates...
vim /tmp/maddy-templates/index.html

# Deploy custom templates
sudo cp -r /tmp/maddy-templates /var/lib/maddy/www
sudo chown -R maddy:maddy /var/lib/maddy/www
maddy --config /etc/maddy/maddy.conf html-serve /var/lib/maddy/www
sudo systemctl restart maddy

# Revert to built-in templates
maddy --config /etc/maddy/maddy.conf html-serve embedded
sudo systemctl restart maddy
```

---

## `maddy version`

Print version and build information.

```bash
maddy version
```

---

## `maddy hash`

Compute password hashes for use in configuration or credential management.

```bash
maddy hash [options]
```

| Flag | Default | Description |
|------|---------|-------------|
| `--password PASS` | *(prompt)* | Password to hash |
| `--hash ALGO` | `bcrypt` | Hash algorithm |
| `--bcrypt-cost N` | `10` | Bcrypt cost factor |

---

## Common Workflows

### First-Time Setup (Quick Start)

```bash
# 1. Install with IP address
sudo maddy install --simple --ip YOUR_IP

# 2. Start the service
sudo systemctl enable --now maddy

# 3. Get admin token
maddy admin-token

# 4. Check status
maddy --config /etc/maddy/maddy.conf status
```

### First-Time Setup (Production)

```bash
# 1. Install with domain and autocert (Let's Encrypt HTTP-01)
sudo maddy install \
  --domain chat.example.org \
  --tls-mode autocert \
  --acme-email admin@example.org \
  --enable-chatmail \
  --enable-ss

# 2. Configure DNS records (shown by installer)

# 3. Start the service
sudo systemctl enable --now maddy

# 4. Verify
maddy --config /etc/maddy/maddy.conf status --details
```

### Manual User Management

```bash
CONFIG="--config /etc/maddy/maddy.conf"

# Create a user
sudo maddy $CONFIG creds create user@example.org
sudo maddy $CONFIG imap-acct create user@example.org

# Delete a user (full cleanup + block)
sudo maddy $CONFIG delete user@example.org

# Unblock a previously deleted user
sudo maddy $CONFIG blocklist remove user@example.org
```

### Upgrade Workflow

```bash
# Download and upgrade (signature-verified)
sudo maddy upgrade https://your-server/maddy

# Check the new version
maddy version

# Verify service is running
maddy --config /etc/maddy/maddy.conf status
```

---

## See Also

- [TLS Certificate Configuration](certificate.md) — Detailed TLS mode documentation
- [Admin API](admin_api.md) — REST API for server management
- [TURN Server](turn.md) — Video call relay configuration
- [Iroh Relay](iroh.md) — Webxdc realtime relay
- [DNS Cache](dns_cache.md) — Endpoint override details
