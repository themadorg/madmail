---
title: "IMAP, SMTP and HTTPS on port 443: how Madmail routes the traffic"
date: 2026-10-12
author: KamyarAT
language: en
translation: imap-smtp-port-443
description: "How Madmail serves IMAP, SMTP submission, and the website on port 443, and how the listener chooses a protocol."
---

If a network blocks ports 993 and 465, Delta Chat can lose access to its mail server even while ordinary websites work fine. The server may be healthy, but the client can't reach it.

We added shared-port support to [Madmail](/) to give clients another way to connect. IMAP and SMTP submission can use port 443 alongside the website and Chatmail's HTTP endpoints. A browser gets the website; a mail connection reaches the appropriate mail handler.

Each connection still carries its own application protocol over TLS. The mail traffic isn't wrapped in HTTP. The work happens in the listener, which has to decide what kind of connection it has accepted.

## TLS already gives us a protocol selector

With three protocols on the same hostname and port, the destination address alone doesn't tell the server where to send a connection. [ALPN](https://datatracker.ietf.org/doc/html/rfc7301), short for Application-Layer Protocol Negotiation, supplies that information during the TLS handshake. The client offers protocol identifiers, and the server selects one it supports.

On Madmail's shared listener, the result maps to a handler:

| Negotiated ALPN | Handler |
| --- | --- |
| `imap` | IMAP |
| `smtp` | SMTP submission |
| `http/1.1` | HTTPS |

Madmail terminates TLS and hands the connection to that handler inside the Rust process. Its existing protocol handlers do the rest, so the feature doesn't require a separate gateway.

Delta Chat core already sends `imap` when connecting to IMAP on a port other than 993, and `smtp` when connecting to submission on a port other than 465. On the standard ports, it sends no ALPN. That meant the shared-port feature could use the client's existing behavior without a change to Delta Chat core.

The web side stays on HTTP/1.1. The shared listener doesn't advertise `h2`, so enabling mail on port 443 doesn't also enable HTTP/2.

## Handling clients that don't send ALPN

Some mail clients don't offer ALPN. For those connections, Madmail checks the hostname sent through TLS Server Name Indication, or SNI.

Operators can configure mail-specific hostnames. The default naming rules recognize `imap.` for IMAP and `smtp.` or `submission.` for SMTP submission. A client using one of those names can reach the corresponding handler without sending an ALPN identifier. The TLS certificate must still cover the hostname the client uses.

If the hostname doesn't identify the protocol either, Madmail briefly inspects the first decrypted application bytes. It can recognize an HTTP request, an early SMTP `EHLO`, or a pipelined IMAP command.

That last fallback has a limit: IMAP and SMTP normally expect the server to send a greeting first. A client waiting for that greeting gives the listener nothing to inspect. If the listener can't identify the connection, it defaults to HTTPS. Clients that need a mail greeting should use ALPN or a recognized mail hostname.

The routing order is therefore ALPN first, SNI only when no ALPN was offered, then the first application bytes if the connection is still unidentified.

## Why ALPN takes priority

A connection that negotiates `http/1.1` goes to the HTTPS handler even if its hostname begins with `imap.`. The hostname mustn't override the protocol agreed during the handshake.

This rule matters when services share certificates. The [ALPACA research](https://alpaca-attack.com/) describes attacks where a valid TLS connection intended for one application ends up at another. A successful TLS handshake alone doesn't guarantee that the right application protocol is on the other end.

Madmail advertises only the ALPN identifiers appropriate to each listener. The dedicated implicit-TLS IMAP listener advertises `imap`; the dedicated submission listener advertises `smtp`. The shared listener advertises `http/1.1` plus whichever mail protocols are enabled there.

If a client offers ALPN identifiers with no overlap, the handshake fails. Madmail doesn't accept the connection and then guess a different protocol. Clients offering no ALPN can still use the traditional paths described above.

These rules reduce protocol confusion. They don't cover every cross-protocol threat, but they make the routing decision explicit and testable.

## Enabling it on a relay

IMAP and SMTP submission can be enabled on the shared port independently. To check the current state and enable both:

```bash
madmail shared-port status
madmail shared-port enable both
madmail reload
```

The configuration must already include a `chatmail tls://...` listener block for that port. The toggle enables mail routing on a shared listener; it can't turn a plain HTTPS listener into one. The [operator guide](/docs/guide/cli/shared-port) covers the listener configuration and status checks.

CLI changes take effect after a soft reload. The admin API can apply changes to new connections in-process, without restarting the server or rebinding the socket. The dedicated mail ports remain available too.

Autoconfiguration follows the enabled protocols: disable IMAP on 443, for example, and Madmail stops advertising that endpoint for IMAP. `madmail alpn` remains an alias for the CLI command, though `shared-port` also covers the SNI and application-byte fallbacks.

## Checking the three paths

On a configured instance, you can check each route directly. Replace `mail.example.org` with your hostname:

```bash
# IMAP on the shared port
openssl s_client -connect mail.example.org:443 \
  -servername mail.example.org -alpn imap -quiet

# SMTP submission on the shared port
openssl s_client -connect mail.example.org:443 \
  -servername mail.example.org -alpn smtp -quiet

# HTTPS on the same port
curl --http1.1 -I https://mail.example.org/
```

The first two connections should receive the appropriate mail protocol greeting. The third should return an HTTP response from the website.

Those checks establish that the listener is routing connections. To test whether it helps on a particular network, also try authenticated mail sessions and a real Delta Chat client. A connection completing its TLS handshake doesn't tell you whether messages can be sent and received.

The repository includes a [network-condition test harness](https://github.com/themadorg/madmail/tree/main/tests/highdpi-smoke) covering reachability, TLS, authenticated protocol use and Delta Chat behavior. Comparing the affected network with an unrestricted one helps narrow down where a failure occurs.

## Where port 443 helps

Shared-port mail is useful when a network blocks the usual mail ports but allows these connections through on 443. It doesn't disguise IMAP or SMTP as web traffic. A filtering system that inspects ALPN or TLS fingerprints can still identify or block the connections.

It also changes only the client-to-relay connection. Delivery between relays has its own configuration, including DNS, incoming mail and outbound delivery. Chatmail's HTTP federation can use HTTPS, but putting client IMAP and submission on 443 doesn't move all server-to-server traffic there.

If you try this on a network where the standard ports are blocked, we'd like to hear how it behaves. Reports that include the client version, the protocol tested, and whether the same setup works on another network help us investigate.

You can inspect the [listener implementation](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/shared_listener.rs), see the [changes introduced in v2.30](https://github.com/themadorg/madmail/compare/v2.29.1...v2.30.0), or open an [issue](https://github.com/themadorg/madmail/issues).
