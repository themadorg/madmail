# DKIM Signing and Key Publishing

## Overview

Madmail signs all outgoing messages with **DKIM** (DomainKeys Identified Mail) and publishes the public verification key via **HTTPS**. This allows receiving mail servers to verify message authenticity.

## How It Works

### Outgoing Messages (Signing)

When a user sends a message through the Submission endpoint (port 465/587), the DKIM modifier (`internal/modify/dkim/`) signs the message:

1. The modifier loads the private key from `{state_dir}/dkim_keys/{domain}_{selector}.key`.
2. It signs the message headers and body using **RSA-2048** (default) or **Ed25519**.
3. The `DKIM-Signature` header is added to the outgoing message.
4. The selector used is `default` (configurable in `maddy.conf`).

### Key Generation

DKIM keys are generated automatically:
- **During installation**: `maddy install` generates a new RSA-2048 key pair.
- **On first boot**: If no key exists, the DKIM modifier generates one on startup.

Two files are created in `{state_dir}/dkim_keys/`:
- `{domain}_{selector}.key`: Private key (PEM format, mode 0600).
- `{domain}_{selector}.dns`: Public key record (plain text RDATA).

### Public Key Publishing via HTTPS

For federation with chatmail relays that operate without DNS, Madmail publishes the DKIM public key via HTTPS at:

```
https://<domain>/.well-known/_domainkey/<selector>
```

For the default configuration, this becomes:
```
https://example.com/.well-known/_domainkey/default
```

The response is plain text containing the DKIM DNS TXT record RDATA:
```
v=DKIM1; k=rsa; p=MIIBIjANBgkqhkiG9w0BAQE...
```

This follows the convention established by the [chatmail relay project](https://github.com/chatmail/relay/issues/843), where [filtermail-rs](https://github.com/chatmail/filtermail/pull/35) verifies DKIM signatures using an HTTPS fallback when DNS lookup fails.

## Security

### What Is Exposed

The HTTPS endpoint serves **only the public key** — the same data that would be published in a DNS TXT record. This is public information by design.

### Input Validation

The selector parameter is validated with multiple layers:
1. **Canonicalization**: `filepath.Base()` strips any path components.
2. **Character whitelist**: Only `[a-zA-Z0-9._-]` are accepted (per RFC 6376).
3. **Length limit**: Maximum 64 characters.

### No-Log Policy

Consistent with the [No Log Policy](nolog.md), the handler:
- Logs only the selector name, never the resolved file path.
- All log output respects the global `log off` / `__LOG_DISABLED__` setting.

## Configuration

In `maddy.conf`, DKIM signing is configured in the Submission pipeline:

```hcl
submission tls://0.0.0.0:465 tcp://0.0.0.0:587 {
    # ...
    default_destination {
        modify {
            dkim $(primary_domain) $(local_domains) default
        }
        deliver_to &remote_queue
    }
}
```

The three arguments to `dkim` are:
1. **Domain(s)**: The domain(s) to sign for.
2. **Selector**: The DKIM selector (typically `default`).

The HTTPS endpoint requires no additional configuration — it reads from the same key files that the signing module uses.

## Verification

After deployment, verify the endpoint:

```bash
# Check that the DKIM key is published
curl https://your-domain/.well-known/_domainkey/default
```

Expected output:
```
v=DKIM1; k=rsa; p=MIIBIjANBg...
```

## Federation Context

This feature is part of the federation requirements between madmail and chatmail relays:

1. **Chatmail relays accept self-signed TLS** ([relay#842](https://github.com/chatmail/relay/issues/842)) — for IP-only servers.
2. **filtermail-rs verifies DKIM via HTTPS** ([filtermail#35](https://github.com/chatmail/filtermail/pull/35)) — the receiving side.
3. **Relays publish DKIM via HTTPS** ([relay#843](https://github.com/chatmail/relay/issues/843)) — the chatmail relay side.
4. **Madmail publishes DKIM via HTTPS** — this document, the madmail side.
