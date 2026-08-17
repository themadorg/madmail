# DNS and Mail Authentication Records

This guide answers the questions operators ask after `madmail install --domain`: which DNS records are required, what the installer sets up automatically, and whether SPF, DKIM, or DMARC affect federation with other chatmail servers.

For TLS-only deployment choices (IP vs domain, autocert vs self-signed), see [Deployment Scenarios](./11-deployment-ip-domain-certs.md).

## What `install --domain` does automatically

`madmail install --simple --domain mail.example.org` (with DNS already pointing at your server) typically:

1. **Obtains a Let's Encrypt certificate** — HTTP-01 on port 80; needs an **`A` or `AAAA`** record for the hostname before install.
2. **Writes server identity** — `primary_domain`, `hostname`, and `chatmail { … }` blocks in `madmail.conf`.
3. **Generates DKIM signing keys** — `{state_dir}/dkim/default.private` (0600) and `default.txt` (the TXT value). Selector **`default`**. The same key signs **federation** `/mxdeliv` and SMTP fallback, not only a theoretical SMTP modify pipeline.

It does **not** create SPF, DKIM, or DMARC records in your DNS zone. You add those in your registrar or DNS panel if you want them.

## DNS records: required vs optional

Replace `mail.example.org` with your hostname and `example.org` with your mail domain (they are often the same).

### Required for a domain deployment

| Type | Name | Value | Purpose |
|------|------|-------|---------|
| `A` / `AAAA` | hostname (`mail.example.org` or `example.org`) | your server's public IP | TLS (ACME), HTTPS federation (`/mxdeliv`), IMAP/SMTP, registration page |
| (ports) | — | **80**, **443** open to the internet | ACME renewal; chatmail federation and clients |

Set the **`A`/`AAAA` record before install** when using Let's Encrypt. Port **80** must be free during install and renewal.

### Recommended for production

| Type | Name | Value | Purpose |
|------|------|-------|---------|
| `MX` | mail domain (`example.org`) | `10 mail.example.org.` | SMTP **fallback** federation; classic inbound mail routing |

### Optional (SMTP hygiene and deliverability)

These matter when you rely on **normal SMTP** (port 25) to or from non-chatmail hosts. They are **not** required for HTTP federation between chatmail relays.

| Type | Name | Example | Purpose |
|------|------|---------|---------|
| `TXT` (SPF) | `@` (mail domain) | `v=spf1 mx a -all` | Authorize your server to send mail for the domain (tighten to your IP if you prefer) |
| `TXT` (DKIM) | `default._domainkey` | `v=DKIM1; k=rsa; p=…` | Publish the public key matching madmail's selector `default` (see below) |
| `TXT` (DMARC) | `_dmarc` | `v=DMARC1; p=none; rua=mailto:admin@example.org` | Start with `p=none`; tighten policy later if needed |
| `PTR` | (at your VPS provider) | hostname matching forward DNS | Helps SMTP reputation; optional for chatmail HTTP federation |

### IP-only relays

If you install with `--simple --ip`, you do not need MX or mail-auth TXT records. HTTP federation can work using the bracketed IP domain (see [TDD — Federation](../../TDD/07-federation.md)).

## Publishing DKIM DNS

The generated config signs outbound mail with:

```text
dkim $(primary_domain) $(local_domains) default
```

Selector name: **`default`**. Keys live in your state directory (e.g. `/var/lib/madmail/`).

To publish DKIM:

1. Copy `{state_dir}/dkim/default.txt` (created at install or on first outbound send).
2. Add a **`TXT`** record at `default._domainkey.example.org` with that single string (`v=DKIM1; k=rsa; p=…`, no line breaks).

Existing installs generate the key on first federated send (or restart after upgrade). Publish the TXT before expecting cmdeploy peers to accept mail.

## Federation vs SMTP authentication

Chatmail servers (madmail, cmdeploy/Postfix+Dovecot, and others) prefer **HTTP federation**:

```text
POST https://<recipient-domain>/mxdeliv
```

Fallback order: HTTPS → HTTP → SMTP (port 25).

On the `/mxdeliv` path:

- **PGP encryption** is enforced on Madmail inbound.
- **Outbound** Madmail adds a DKIM signature (selector `default`) so **cmdeploy** peers do not return `400` / `554 5.7.1 No DKIM signature found`.
- **TLS certificate trust between relays is not required** (self-signed certs are normal).
- Inbound `dkim` / `spf` / `dmarc` checks in `madmail.conf` apply to mail arriving on **SMTP port 25**, not to Madmail `/mxdeliv`.

Missing **DKIM TXT** can still fail **cmdeploy verification** after a signature is present. Publish `default._domainkey`. SPF/DMARC remain optional for chatmail-to-chatmail HTTP.

More detail: [Sending, Receiving, and Federation](./05-sending-receiving-and-federation.md) and [Troubleshooting](./10-troubleshooting.md).

## Verify your setup

From your laptop or another host on the internet:

```bash
# Forward DNS
dig +short mail.example.org A
dig +short example.org MX

# Federation endpoint reachable (405 or 400 is fine; timeout/refused is not)
curl -sI https://mail.example.org/mxdeliv
```

On the server:

```bash
madmail status
madmail federation list
```

Use the admin web UI **Federation** section for per-peer success rate, latency, and queue depth.

## Replacing a cmdeploy (Postfix + Dovecot) server

**Yes** — madmail v2 is an alternative chatmail relay implementation. It speaks the same protocol (JIT accounts, PGP-only, IMAP/SMTP, `/mxdeliv`, TURN metadata).

**No** — it is not an in-place upgrade of an existing cmdeploy VM:

- Deploy madmail on a new host (or the same host after removing the old stack).
- Point **`A`/`AAAA` and `MX`** at the new server.
- Users re-register (chatmail accounts are temporary by design).
- There is no supported import of Dovecot maildirs or cmdeploy account databases.

After cutover, confirm federation to a known peer (e.g. another public chatmail host) using the checks above.

See also [What is Chatmail?](./01-what-is-chatmail.md) for how madmail relates to other relay stacks.

## Related docs

- [Quick Start](./02-quick-start.md) — install commands
- [Native install guide](../../guide/install.md) — full `madmail install` reference
- [Docker DNS checklist](../../guide/docker.md#dns-checklist-domain-production) — same records, container layout
- [Federation (technical)](../../TDD/07-federation.md) — wire protocol and delivery order