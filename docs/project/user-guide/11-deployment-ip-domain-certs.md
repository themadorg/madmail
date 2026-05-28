# Deployment Scenarios: IP vs Domain, With or Without Certificates

When you set up a chatmail server, you usually face one of these four common situations:

|                          | **With trusted certificate**                  | **Without trusted certificate** (self-signed or none) |
|--------------------------|-----------------------------------------------|-------------------------------------------------------|
| **IP-only server** (no domain name) | Let's Encrypt IP certificate (short-lived)   | Self-signed certificate                               |
| **Domain server** (you own a DNS name) | Standard Let's Encrypt certificate           | Self-signed certificate                               |

This guide explains the practical differences, recommended commands, and trade-offs for each case.

## 1. IP-Only Server + Trusted Certificate (Recommended for most public relays)

This is the most common production setup when you only have a public IP address (no domain name pointing at the server).

### How it works
- You use Let's Encrypt's **IP certificate** feature.
- You get a browser-trusted certificate even without a domain name.
- The certificate is short-lived (roughly 6 days) and is automatically renewed by the server.

### Command

```bash
madmail install --simple --ip 203.0.113.50 \
  --auto-ip-cert \
  --acme-email you@example.com
```

Replace `203.0.113.50` with your **public** IPv4 or IPv6 address.

### Pros
- Users get a green lock in Delta Chat and browsers.
- No need to buy or manage a domain name.
- Very good for "stealth" or temporary relays.

### Cons / Things to know
- Certificates must be renewed frequently (the server handles this automatically via HTTP-01 on port 80).
- You must keep port 80 open for renewal.
- Some older email clients or very strict environments may not like IP-based certificates.

This is the setup described in detail in **[Install on a Public IP with Let's Encrypt](../install-simple-ip-acme.md)**.

## 2. IP-Only Server + No Trusted Certificate (Self-signed)

Use this when you don't care about browser warnings or when you're doing internal/testing deployments.

### During install

Just omit the `--auto-ip-cert` flag:

```bash
madmail install --simple --ip 203.0.113.50
```

The installer will generate a self-signed certificate for the IP address.

### After install

You can generate or replace the self-signed certificate on a running server with:

```bash
madmail certificate self-signed
```

### Pros
- Simplest setup.
- No port 80 requirement.
- Fine for local networks, testing, or trusted environments.

### Cons
- Delta Chat and browsers will show security warnings (users usually have to click "trust" or accept the certificate once).
- Not suitable for public untrusted users.

## 3. Domain Server + Trusted Certificate (Classic & Cleanest)

This is the nicest experience when you own a domain name (`mail.example.org`, `chat.yourname.net`, etc.).

### Recommended command

```bash
madmail install --simple --domain mail.example.org \
  --acme-email you@example.com
```

(You can also use `--hostname` if you want the server to announce a different name.)

### Pros
- Best user experience (green lock everywhere).
- Certificates last 90 days and renew automatically.
- Looks professional.
- Works great with multiple domains on the same server.

### Cons
- You need to control DNS for the domain (or at least be able to point it at your IP).

## 4. Domain Server + No Trusted Certificate (Self-signed)

You have a domain, but for some reason you don't want (or can't get) a real certificate right now.

### During install

```bash
madmail install --simple --domain mail.example.org
```

The installer will fall back to generating a self-signed certificate for the domain.

### Pros
- You can still use a nice domain name in addresses.
- Simple if you're in a hurry.

### Cons
- Same trust warnings as any self-signed setup.
- You lose the main benefit of having a domain name.

**Recommendation**: If you have a domain, almost always get a real certificate. It's not much extra work and dramatically improves the experience.

## How to Add or Change Certificates Later

You are not locked into the choice you made at install time.

### Switch to Let's Encrypt later (IP or domain)

```bash
madmail certificate acme --email you@example.com
# or for IP:
madmail certificate acme --ip 203.0.113.50 --email you@example.com
```

### Switch to self-signed

```bash
madmail certificate self-signed
```

### Provide your own certificate files

```bash
madmail certificate file /path/to/fullchain.pem /path/to/privkey.pem
```

After changing certificates, you usually need to reload or restart the server:

```bash
madmail reload
```

## Quick Decision Guide

- **Public relay for real users, only have an IP** → Use **IP + `--auto-ip-cert`** (case 1).
- **Public relay, you control a domain** → Use **Domain + ACME** (case 3).
- **Internal / test / trusted users only** → Self-signed is fine (cases 2 or 4).
- **Want maximum simplicity right now** → Install with self-signed, add a real cert later when you're ready.

## Important Notes for All Cases

- Port 80 must be reachable from the internet if you want automatic Let's Encrypt (both domain and IP versions).
- The admin web interface and public site (`/new`, docs, etc.) will show security warnings to users unless you have a trusted certificate.
- Even with self-signed certs, Delta Chat can still work well if users accept the certificate once.

## Next Steps

- Detailed IP + trusted cert instructions: [Install on a Public IP with Let's Encrypt](../install-simple-ip-acme.md)
- Local development (self-signed): see the developer workflow guide
- Day-to-day certificate management from the admin interface or CLI: [Admin & CLI](./07-admin-and-cli.md)

Choose the scenario that matches your situation, and don't be afraid to start with self-signed and upgrade to real certificates later — the system is designed to make this easy.
