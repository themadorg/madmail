# Chatmail-Only Setup Tutorial (Automatic Certificates)

This tutorial guide you through setting up a **Chatmail-only** Madmail server with **automatic Let's Encrypt certificates**. This setup is perfect for private Delta Chat instances that do not require federation with the global email network.

In this example, we will use:
- **Primary Domain**: `example.com`
- **Public IP**: `1.1.1.1`
- **Email Format**: `a@example.com`

---

## Step 0: Download the Binary

First, download the latest Madmail binary for your architecture and make it executable:

```bash
curl -fsSL https://github.com/themadorg/madmail/releases/latest/download/madmail-linux-$(uname -m | sed 's/x86_64/amd64/;s/aarch64/arm64/') -o madmail
chmod +x madmail
```

---

## Prerequisites

1.  **A Linux Server**: With a public IP address (`1.1.1.1`).
2.  **DNS Control**: Ability to add an A record for `example.com`.
3.  **Open Ports**: Ports 80 and 443 must be reachable for automatic certificate issuance.

---

## Step 1: DNS Setup (Minimal)

For a Chatmail-only setup, you only need an **A record**. **MX and SPF records are not required.**

### A Record
| Type | Host | Value |
|------|------|-------|
| A    | @    | 1.1.1.1 |

---

## Step 2: Installation

Run the `install` command with `autocert` enabled. Note that we include `--enable-chatmail`.

```bash
sudo ./madmail install \
  --domain example.com \
  --hostname example.com \
  --ip 1.1.1.1 \
  --tls-mode autocert \
  --acme-email admin@example.com \
  --enable-chatmail \
  --non-interactive
```

---

## Step 3: DKIM Configuration (Recommended)

To help Delta Chat clients verify the server identity, add the generated DKIM public key to your DNS.

Find the value:
```bash
cat /var/lib/madmail/dkim_keys/example.com_default.dns
```
Add it as a TXT record for `default._domainkey.example.com`.

---

## Step 4: Verify and Start

Start the mail server:

```bash
sudo systemctl enable --now madmail
```

Monitor the logs to ensure the certificate is obtained successfully:

```bash
sudo journalctl -u madmail -f
```
