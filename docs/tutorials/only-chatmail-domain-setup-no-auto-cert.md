# Chatmail-Only Setup Tutorial (Manual Certificates)

This tutorial guide you through setting up a **Chatmail-only** Madmail server. This setup is optimized for private Delta Chat usage where you don't need federation with external mail servers (Gmail, etc.), and thus don't require MX or SPF records.

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
3.  **TLS Certificates**: You must have your certificate and private key files ready on the server.
    - Example path: `/etc/ssl/live/example.com/fullchain.pem`
    - Example path: `/etc/ssl/live/example.com/privkey.pem`

---

## Step 1: Installation

Run the `install` command. Since this is a Chatmail-only setup, we focus on the `--enable-chatmail` flag.

```bash
sudo ./madmail install \
  --domain example.com \
  --hostname example.com \
  --ip 1.1.1.1 \
  --tls-mode file \
  --cert-path /etc/ssl/live/example.com/fullchain.pem \
  --key-path /etc/ssl/live/example.com/privkey.pem \
  --enable-chatmail \
  --non-interactive
```

---

## Step 2: Permissions

Ensure the `madmail` group can read your certificate files:

```bash
sudo chown root:madmail /etc/ssl/live/example.com/fullchain.pem /etc/ssl/live/example.com/privkey.pem
sudo chmod 640 /etc/ssl/live/example.com/fullchain.pem /etc/ssl/live/example.com/privkey.pem
```

---

## Step 3: DNS Configuration (Minimal)

For a Chatmail-only setup, you only need the **A record** for the registration and messaging endpoint. **MX and SPF records are not required.**

### A Record
| Type | Host | Value |
|------|------|-------|
| A    | @    | 1.1.1.1 |

### DKIM Record (Recommended)
While not strictly required for internal delivery, adding a DKIM record helps Delta Chat clients verify the server.

Find the record value:
```bash
cat /var/lib/madmail/dkim_keys/example.com_default.dns
```
Add it as a TXT record for `default._domainkey.example.com`.

---

## Step 4: Verify and Start

Start the service:

```bash
sudo systemctl enable --now madmail
```

Check the logs:
```bash
sudo journalctl -u madmail -f
```
