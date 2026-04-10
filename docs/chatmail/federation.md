# Federation

## Overview

Federation is how two or more independent Madmail servers deliver messages to
each other. Instead of relying solely on traditional SMTP (port 25), Madmail
implements an **HTTP-based delivery protocol** using the `/mxdeliv` endpoint.
Every Madmail server exposes this endpoint and every Madmail server knows how to
send to it, making any pair of Madmail instances federation-ready out of the
box.

When HTTP delivery fails, Madmail falls back to standard **SMTP** (port 25)
delivery, ensuring compatibility with the wider email ecosystem.


## Wire Format — The `/mxdeliv` Packet

All inter-server email delivery uses a single HTTP POST request. The envelope
metadata is carried in HTTP headers and the message itself is sent as the
request body.

```
POST /mxdeliv HTTP/1.1
Host: 2.2.2.2
X-Mail-From: alice@1.1.1.1
X-Mail-To: bob@2.2.2.2
X-Mail-To: carol@2.2.2.2
Content-Type: application/octet-stream

<RFC 822 message: headers + body>
```

### Headers

| Header | Required | Cardinality | Description |
|--------|----------|-------------|-------------|
| `X-Mail-From` | Yes | Exactly 1 | Envelope sender (equivalent to SMTP `MAIL FROM`). |
| `X-Mail-To` | Yes | 1 or more | Envelope recipients (equivalent to SMTP `RCPT TO`). Repeat the header once per recipient. |
| `Content-Type` | Recommended | 1 | Always `application/octet-stream`. |

### Body

The body is the **complete RFC 822 message** — all message headers
(`From`, `To`, `Subject`, `DKIM-Signature`, MIME parts, etc.) followed by the
message body. This is the exact same byte stream that would be transmitted
after the SMTP `DATA` command.

### Response Codes

| HTTP Status | Meaning |
|-------------|---------|
| `200 OK` | Message accepted for all recipients. |
| `400 Bad Request` | Missing `X-Mail-To` header or unparseable message headers. |
| `404 Not Found` | No valid recipients (all recipients rejected). |
| `405 Method Not Allowed` | Request method is not `POST`. |
| `413 Request Entity Too Large` | Message body exceeds the server's `max_message_size`. |
| `500 Internal Server Error` | Server-side failure during delivery processing. |


## Endpoint

Every Madmail server registers the `/mxdeliv` handler on the **chatmail HTTP
endpoint** — the same HTTPS listener that serves the web UI, account creation,
and DKIM key publishing. No separate port or service is required.

```
https://<server-address>/mxdeliv
```

For IP-only deployments (common in the Chatmail ecosystem):
```
https://1.1.1.1/mxdeliv
http://1.1.1.1/mxdeliv      ← fallback if HTTPS fails
```


## Delivery Flow

When a user on **Server A** sends a message to a recipient on **Server B**,
the outbound delivery module (`target.remote`) performs the following steps:

```
┌───────────────────┐                           ┌───────────────────┐
│     Server A      │                           │     Server B      │
│  (sender side)    │                           │  (recipient side) │
│                   │   1. POST /mxdeliv        │                   │
│  target.remote ───┼──────────────────────────►│  chatmail endpoint│
│                   │      (HTTPS first)        │   handleReceive   │
│                   │                           │        │          │
│                   │   2. HTTP 200 OK          │        ▼          │
│                   │◄──────────────────────────│  imapsql storage  │
│                   │                           │  (DeliveryTarget) │
└───────────────────┘                           └───────────────────┘
```

### Step-by-Step

1. **Domain extraction** — The sender's Madmail server extracts the domain (or
   IP) from the recipient address (e.g., `bob@2.2.2.2` → `2.2.2.2`).

2. **Endpoint cache check** — If an endpoint override is configured in the
   local database (via `maddy endpoint set`), the resolved address is used
   instead of the raw domain. This enables routing through exchangers or
   testing servers.

3. **HTTPS attempt** — The server POSTs the message to
   `https://<domain>/mxdeliv`. TLS verification is skipped for self-signed
   certificates (common with IP-only Chatmail servers).

4. **HTTP fallback** — If HTTPS fails (connection refused, timeout, TLS
   handshake error), the server retries over plain HTTP:
   `http://<domain>/mxdeliv`.

5. **SMTP fallback** — If both HTTP attempts fail, the server falls back to
   traditional SMTP delivery on port 25 using MX record lookup.

6. **Recipient processing** — On the receiving side, the `/mxdeliv` handler:
   - Parses `X-Mail-From` and `X-Mail-To` headers.
   - Reads and parses the RFC 822 body.
   - Creates a delivery transaction via the `DeliveryTarget` interface.
   - Adds each recipient via `AddRcpt` (validating they exist or
     auto-provisioning if `auto_create` is enabled).
   - Commits the message into the recipient's IMAP mailbox.


## DKIM Verification for Federation

Madmail signs all outgoing messages with **DKIM** and publishes the public key
over HTTPS for verification by receiving servers that cannot perform DNS
lookups (common for IP-only deployments).

### Key Publishing Endpoint

```
GET /.well-known/_domainkey/<selector>
```

Default:
```
GET https://1.1.1.1/.well-known/_domainkey/default
```

Response (plain text):
```
v=DKIM1; k=rsa; p=MIIBIjANBgkqhkiG9w0BAQE...
```

This follows the convention from the [chatmail relay project](https://github.com/chatmail/relay/issues/843),
where `filtermail-rs` verifies DKIM signatures using an HTTPS fallback when
DNS lookup is not available.

See [DKIM Signing and Key Publishing](dkim.md) for full details.


## Relaying via Madexchanger

For complex network topologies (NAT, firewalls, censored networks),
**Madexchanger** can sit between two Madmail servers as a transparent relay
proxy. It speaks the exact same `/mxdeliv` protocol on both sides.

```
┌──────────────┐   POST /mxdeliv    ┌────────────────┐   POST /mxdeliv    ┌──────────────┐
│   Server A   │ ────────────────► │  Madexchanger   │ ────────────────► │   Server B   │
│              │   X-Mail-From     │  (relay proxy)  │   X-Mail-From     │              │
│              │   X-Mail-To       │                 │   X-Mail-To       │              │
│              │   <RFC 822 body>  │                 │   <RFC 822 body>  │              │
└──────────────┘                    └────────────────┘                    └──────────────┘
```

### How to Route Through an Exchanger

1. On **Server A**, configure an endpoint override so that outbound deliveries
   destined for Server B are redirected to the exchanger:

   ```bash
   maddy endpoint set <server-b-domain> https://<exchanger-ip>:8443/mxdeliv
   ```

   Or configure a global redirect via `endpoint_rewrite` in `maddy.conf`:

   ```
   target.remote outbound_delivery {
       endpoint_rewrite https://<exchanger-ip>:8443
   }
   ```

2. The **exchanger** receives the message and forwards it to Server B
   using the recipient's domain (dynamic routing) or a fixed downstream URL.

3. **Server B** receives the message on `/mxdeliv` as if it came directly
   from Server A — the exchanger is transparent.

### Exchanger Routing Modes

| Mode | Behavior |
|------|----------|
| **Dynamic** (default) | Extracts domain from `X-Mail-To`, delivers to `https://<domain>/mxdeliv` (HTTPS first, HTTP fallback). |
| **Static** | All messages go to a fixed `downstream_url` regardless of recipient domain. |

### Exchanger Features

- **Incoming allow list** — Accept all messages or only those matching specific sender/recipient/domain patterns.
- **Outgoing allow list** — Restrict which destination servers the exchanger is allowed to deliver to.
- **Routing rules** — Redirect messages matching specific patterns to override destinations.
- **Outbound proxies** — Route outbound delivery through SOCKS5 or HTTP(S) proxies.
- **Admin dashboard** — Embedded web UI for monitoring relay traffic and managing rules.

See the [Madexchanger documentation](../../madexchanger/README.md) for full
configuration details.


## Federation Ports

Madmail federation can work over three different transport layers. The
[E2E federation test](e2e_test.md) (`test_07_federation.py`) verifies each
independently:

| Port | Protocol | Role |
|------|----------|------|
| **443** | HTTPS | Primary HTTP delivery via `/mxdeliv` (encrypted). |
| **80** | HTTP | Fallback HTTP delivery via `/mxdeliv` (unencrypted). |
| **25** | SMTP | Standard SMTP delivery (last resort fallback). |

### Priority Order

```
1. HTTPS (443)  →  POST https://<domain>/mxdeliv
2. HTTP  (80)   →  POST http://<domain>/mxdeliv    (only if HTTPS fails)
3. SMTP  (25)   →  Traditional MX-based delivery   (only if both HTTP fail)
```

> **Note:** For two Madmail servers to federate, at least one of these
> transports must be reachable. The HTTPS path is strongly preferred because it
> works with IP-only deployments and does not require DNS MX records.


## TLS and Self-Signed Certificates

Madmail's HTTP delivery client **skips TLS certificate verification** by
default (`InsecureSkipVerify: true`). This is intentional:

- Many Chatmail servers are deployed on bare IP addresses without domain names.
- Self-signed certificates are common and expected in this ecosystem.
- Message integrity and authenticity are guaranteed by **DKIM signatures** and
  **end-to-end PGP encryption**, not by the transport layer TLS certificate.

This matches the approach taken by the
[chatmail relay project](https://github.com/chatmail/relay/issues/842).


## Configuration Reference

### Receiving Side (automatic)

No configuration is needed. The `/mxdeliv` endpoint is always active on the
chatmail HTTP listener. Messages are delivered through the standard
`DeliveryTarget` pipeline, which means all policies apply:

- **TLS required** — Only HTTPS connections are accepted. Plain HTTP
  requests to `/mxdeliv` are rejected with `403 Forbidden`.
- **Domain validation** — Recipients must belong to this server. Emails
  addressed to users at other domains (e.g., `user@2.2.2.2` on a server
  configured for `1.1.1.1`) are rejected with `404 Not Found`.
- **Admin protection** — Delivery to system addresses (`admin`, `root`,
  `postmaster`, `mailer-daemon`, `abuse`, `hostmaster`, `webmaster`) is
  blocked via federation to prevent probing and abuse.
- **Silent drop for non-existent users** — If a recipient address has
  the correct domain but the user account does not exist, the server
  responds `200 OK` to the sender but silently discards the message.
  This prevents user enumeration by remote servers.
- **PGP enforcement** — Unencrypted messages are rejected (`523 5.7.1`).
- **Auto-provisioning** — If `auto_create` is enabled, accounts are created
  on first delivery.

### Sending Side (`maddy.conf`)

```hcl
target.remote outbound_delivery {
    # Optional: redirect all outbound HTTP deliveries to a relay
    endpoint_rewrite https://exchanger.example.com

    # Optional: per-domain overrides (managed via CLI)
    # maddy endpoint set 2.2.2.2 https://relay.example.com/mxdeliv

    # TLS and timeouts
    tls_client { }
    connect_timeout 5m
    command_timeout 5m
}
```

### Endpoint Cache Overrides (CLI)

```bash
# Add a per-domain override
maddy endpoint set 2.2.2.2 https://exchanger.example.com:8443/mxdeliv

# List current overrides
maddy endpoint list

# Remove an override
maddy endpoint remove 2.2.2.2
```


## Quick Verification

Test federation between two Madmail servers using `curl`:

```bash
# From any machine, simulate a delivery to Server B
curl -X POST https://2.2.2.2/mxdeliv \
  -k \
  -H "X-Mail-From: alice@1.1.1.1" \
  -H "X-Mail-To: bob@2.2.2.2" \
  -H "Content-Type: application/octet-stream" \
  -d 'From: alice@1.1.1.1
To: bob@2.2.2.2
Subject: Federation Test
Date: Sat, 14 Mar 2026 10:00:00 +0000
Message-ID: <test-federation@1.1.1.1>

Hello from Server A!'
```

A `200 OK` response confirms that the `/mxdeliv` endpoint is reachable and
the message was accepted.

For comprehensive automated testing, use the
[E2E test suite](e2e_test.md) (`test_07_federation.py`).
