# Madmail Relay & Exchanger System

## Overview

Madmail supports **HTTP-based email federation** as a first-class delivery method.
When a user at `a@[1.1.1.1]` sends a message to `b@[2.2.2.2]`, the sending server
tries to deliver the message in the following order:

1. **HTTPS** → `POST https://2.2.2.2/mxdeliv`
2. **HTTP**  → `POST http://2.2.2.2/mxdeliv`  (fallback)
3. **SMTP**  → traditional SMTP on port 25 (final fallback)

An **exchanger** (relay) is an intermediary server that sits between two Madmail
instances when direct delivery is not possible — for example, when servers are
behind NAT, firewalls, or restrictive networks like Iran's.

```
┌──────────────┐  POST /mxdeliv   ┌───────────────┐  POST /mxdeliv   ┌──────────────┐
│  Server S1   │ ───────────────► │   Exchanger   │ ───────────────► │  Server S2   │
│  1.1.1.1     │  X-Mail-From    │  (relay.com)  │  X-Mail-From    │  2.2.2.2     │
│              │  X-Mail-To      │               │  X-Mail-To      │              │
│              │  <RFC 822 body> │               │  <RFC 822 body> │              │
└──────────────┘                  └───────────────┘                  └──────────────┘
```

---

## Wire Format (`/mxdeliv`)

All Madmail HTTP delivery — both direct and via exchangers — uses the same wire format:

```http
POST /mxdeliv HTTP/1.1
X-Mail-From: sender@[1.1.1.1]
X-Mail-To: recipient@[2.2.2.2]
Content-Type: message/rfc822

<raw RFC 822 message: headers + body>
```

| Header | Description |
|--------|-------------|
| `X-Mail-From` | Envelope sender (MAIL FROM) |
| `X-Mail-To` | Envelope recipient (RCPT TO). One header per recipient, or comma-separated |

The body is the **complete** RFC 822 message (headers + body), same as what SMTP DATA would carry.

The receiving server responds:
- `200 OK` — message accepted
- `400 Bad Request` — malformed message (e.g., empty body, missing headers)
- `404 Not Found` — no valid recipients
- `500 Internal Server Error` — storage/delivery failure

---

## Endpoint Override (Endpoint Cache)

The **endpoint cache** is a database-backed lookup table that controls where
outbound messages are delivered. It intercepts the delivery resolution and
redirects traffic to a different host — typically an exchanger.

### How It Works

When Madmail resolves where to send a message for domain `X`:

1. Check the `dns_overrides` table for a matching `lookup_key`
2. If found → use `target_host` instead of `X`
3. If not found and `X` is an IP → use the IP directly
4. If not found and `X` is a domain → fall through to DNS MX resolution

### CLI Commands

```bash
# List all endpoint overrides
maddy endpoint-cache list

# Set an override: route mail for 2.2.2.2 through relay.example.com exchanger
maddy endpoint-cache set 2.2.2.2 relay.example.com "push via relay exchanger"

# Set an override for a domain
maddy endpoint-cache set nine.testrun.org 10.0.0.5 "Route to staging"

# Show a specific entry
maddy endpoint-cache get 2.2.2.2

# Remove an override
maddy endpoint-cache remove 2.2.2.2
```

> **Alias**: `maddy dns-cache` is a backward-compatible alias for `maddy endpoint-cache`.

### Database Schema

The overrides are stored in the `dns_overrides` table in the main application database
(shared with IMAP storage, quotas, etc.):

| Column | Type | Description |
|--------|------|-------------|
| `lookup_key` | TEXT (PK) | Domain name or IP to match (normalized: lowercase, no brackets, no trailing dot) |
| `target_host` | TEXT | Destination host/IP to redirect to |
| `comment` | TEXT | Optional human-readable note |
| `created_at` | TIMESTAMP | Auto-set |
| `updated_at` | TIMESTAMP | Auto-set |

### Config Directive

The `endpoint_rewrite` config directive in `target.remote` provides a **global** rewrite
that sends *all* outbound HTTP deliveries to a single relay URL:

```
target.remote outbound_delivery {
    endpoint_rewrite relay.example.com
}
```

This is different from endpoint-cache overrides:
- **`endpoint_rewrite`** → redirects *all* outbound to a single exchanger
- **Endpoint cache** → per-destination overrides (more flexible)

---

## Delivery Flow (Code Path)

The delivery logic lives in [`internal/target/remote/remote.go`](internal/target/remote/remote.go):

```
BodyNonAtomic()
  └─ for each recipient domain:
       └─ tryHTTP(domain)
            ├─ if endpoint_rewrite is set:
            │    └─ POST to rewrite URL → done
            ├─ check endpoint cache for domain override
            │    └─ if found: host = override.target_host
            ├─ HTTPS: POST https://<host>/mxdeliv
            │    └─ if OK → done
            ├─ HTTP: POST http://<host>/mxdeliv
            │    └─ if OK → done
            └─ SMTP fallback:
                 └─ MX lookup → connect port 25 → deliver
```

The receiving endpoint is [`handleReceiveEmail`](internal/endpoint/chatmail/chatmail.go)
which accepts the POST, parses the RFC 822 body, and delivers it to the local storage.

---

## Exchanger Implementations

### 1. `madexchanger` (Go, production)

Full-featured Go exchanger with admin dashboard, database, proxy support, and
rewrite rules. Located in [`exchangers/madexchanger/`](exchangers/madexchanger/).

```bash
cd exchangers/madexchanger
make all          # Build admin-web + Go binary
./madexchanger -config config.yml
```

See [`exchangers/madexchanger/README.md`](exchangers/madexchanger/README.md) for details.

### 2. `simple-request-test` (PHP, lightweight)

Minimal PHP exchanger deployable on any shared hosting with cURL support.
Located in [`exchangers/simple-request-test/`](exchangers/simple-request-test/).

Two entry points:
- **`index.php`** — combined status page + `/mxdeliv` handler (routed via `.htaccess`)
- **`mxdeliv.php`** — standalone `/mxdeliv` handler (deployed to hosting root)

```bash
cd exchangers/simple-request-test

# Deploy the mxdeliv handler to the exchanger host
make deploy-mxdeliv

# Deploy the status page
make deploy-ir-test

# Check the exchanger push log
make log-mxdeliv
```

The PHP exchanger flow:
1. Receives `POST /mxdeliv` with `X-Mail-From`, `X-Mail-To`, and RFC 822 body
2. Extracts destination IP/domain from the recipient address (strips `[]` brackets)
3. Pushes via `curl` to `https://<destination>/mxdeliv`
4. Logs to `madexchanger-push.log` (one directory above `public_html`)

### 3. `madexchanger-php` (PHP, full)

Full PHP exchanger with config file, migration support, and admin page.
Located in [`exchangers/madexchanger-php/`](exchangers/madexchanger-php/).

---

## Example Production Setup

### Servers

| Server | IP | Role |
|--------|------|------|
| S1 | `1.1.1.1` | Madmail instance |
| S2 | `2.2.2.2` | Madmail instance |
| Exchanger | `relay.example.com` | PHP exchanger (shared hosting) |

### Endpoint Cache Configuration

On **S1** (`1.1.1.1`):
```bash
maddy endpoint-cache set 2.2.2.2 relay.example.com "push via relay exchanger"
```

On **S2** (`2.2.2.2`):
```bash
maddy endpoint-cache set 1.1.1.1 relay.example.com "push via relay exchanger"
```

### Message Flow Example

When `a@[1.1.1.1]` sends to `b@[2.2.2.2]`:

```
S1 (1.1.1.1)
  ↓ endpoint cache: 2.2.2.2 → relay.example.com
  ↓ POST https://relay.example.com/mxdeliv
      X-Mail-From: a@[1.1.1.1]
      X-Mail-To:   b@[2.2.2.2]

relay.example.com (PHP exchanger)
  ↓ extracts destination: 2.2.2.2
  ↓ POST https://2.2.2.2/mxdeliv
      X-Mail-From: a@[1.1.1.1]
      X-Mail-To:   b@[2.2.2.2]

S2 (2.2.2.2)
  ↓ handleReceiveEmail
  ↓ delivers to b's local mailbox
```

---

## Deploying Changes

### Update Madmail Servers

```bash
# From the madmail repo root:
make push     # Builds, signs, and deploys to both servers
```

This runs `build.sh`, signs the binary, uploads via `scp`, and restarts
the `maddy.service` on each remote via `maddy upgrade`.

### Update PHP Exchanger

```bash
cd exchangers/simple-request-test
make deploy-mxdeliv   # Upload mxdeliv.php to exchanger host via FTP
```

### Upload Binary to Exchanger (for download)

```bash
curl -u '<ftp_user>:<password>' -T build/maddy "ftp://<exchanger_host>/public_html/maddy"
```

---

## Monitoring & Debugging

### Server Logs

```bash
# Tail live logs on S1
ssh root@<S1_IP> "journalctl -u maddy.service -f"

# Tail live logs on S2
ssh root@<S2_IP> "journalctl -u maddy.service -f"

# Filter for federation events
ssh root@<S1_IP> "journalctl -u maddy.service --since '1 hour ago' | grep -i 'federation\|mxdeliv\|endpoint'"
```

### Key Log Messages

| Log Message | Meaning |
|-------------|---------|
| `endpoint cache hit` | Endpoint override resolved from DB |
| `endpoint cache override for HTTP delivery` | Using override target for delivery |
| `Attempting HTTP POST` | Sending message to URL |
| `[federation] delivery OK` | HTTP delivery succeeded (sender side) |
| `[federation] HTTP delivery failed, falling back to SMTP` | HTTP failed, trying SMTP |
| `HTTP delivery request received` | Inbound message received on `/mxdeliv` |
| `[federation] received via https` | Message successfully stored (receiver side) |

### Exchanger Logs

```bash
# Read the PHP exchanger push log
cd exchangers/simple-request-test
make log-mxdeliv

# Or directly:
curl -s -u '<ftp_user>:<password>' 'ftp://<exchanger_host>/madexchanger-push.log' | tail -30
```

### Health Check

```bash
# Check if the exchanger is responding
curl https://<exchanger_host>/mxdeliv
# Should return JSON: {"status":"ok","mode":"push",...}
```

---

## Troubleshooting

### Messages sent but not received

1. **Check sender logs** — look for `[federation] delivery OK` to confirm the
   sender reached the exchanger
2. **Check exchanger logs** — look for `Received:` entries to confirm the
   exchanger got the message, and `OK →` / `FAIL →` for push results
3. **Check receiver logs** — look for `HTTP delivery request received` to
   confirm the destination server got the push

Common issues:
- **Exchanger returns 200 but doesn't log** — caching layer on hosting
  (CDN, Varnish, LiteSpeed cache) may cache POST responses. Disable caching
  for `/mxdeliv` in hosting control panel.
- **Push returns HTTP 400** — the message body is malformed or empty.
  The receiver's `handleReceiveEmail` returns 400 for unparseable RFC 822.
- **Push returns HTTP 404** — recipient doesn't exist on the destination server.
- **HTTPS fails, HTTP also fails, SMTP fails** — all three transport methods
  exhausted. Check network connectivity to destination.

### Endpoint cache not working

```bash
# Verify the override exists
maddy endpoint-cache list

# Check if the server is using it (look for "endpoint cache hit" in debug logs)
journalctl -u maddy.service | grep "endpoint cache"
```

The endpoint cache auto-discovers the database from the `local_mailboxes`
storage module. If it fails to initialize, remote delivery falls back to
direct resolution (no overrides).
