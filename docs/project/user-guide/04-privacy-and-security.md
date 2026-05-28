# Privacy and Security Model

Chatmail servers are built with a very specific idea of privacy.

The core rule is simple:

> The server should have as little ability to read or leak user data as possible.

Everything else (PGP-only enforcement, No-Log mode, minimal logging, how federation works, etc.) flows from this principle.

## Why “PGP-Only”?

On a normal email server, the server operator (or anyone who compromises the server) can read almost every message that passes through it.

On a chatmail server this is deliberately made very hard.

### What the server accepts

- Properly encrypted PGP/MIME messages (the normal case for Delta Chat)
- A small number of special “Secure-Join” handshake messages that Delta Chat uses to establish secure contacts
- Certain automatic bounce messages from mail servers

### What the server rejects

- Plain text email
- HTML email that is not properly encrypted
- Almost anything that would let the server (or an attacker) see the actual content

When the server rejects a message because it is not encrypted, it returns a clear but privacy-preserving error. The sender’s client usually shows a helpful message to the user.

**Why do this?** If the server never sees readable mail, it cannot accidentally log it, be legally compelled to hand it over, or be hacked and have the data stolen.

This is not a “nice to have” feature — it is the foundation of the trust model.

## No-Log Mode

Many chatmail operators run their servers with logging completely disabled (“No-Log”).

When this mode is active:

- The server writes almost nothing to disk about who connected, what they did, or what messages were exchanged.
- Even routine operational logs are suppressed.
- If someone later obtains the server’s disk (legally or illegally), there is very little historical data to find.

This is controlled by a simple setting. It can be changed at runtime through the admin interface.

No-Log is one of the reasons some people prefer chatmail servers over traditional ones in high-risk environments.

## What Data Does the Server Actually Store?

For a typical user the server stores:

- Username (normalized)
- Password hash (never the password)
- How much storage the user is using
- When the account was created and last used (for maintenance / dormant account cleanup)
- Which registration token was used (if any)
- Basic counters (how many messages sent/received) — these can be disabled or minimized

It does **not** store:

- The content of your messages (those live on disk as encrypted files in the user’s Maildir)
- Detailed “who emailed whom” graphs
- IP addresses or connection logs (when No-Log is enabled)
- Unencrypted copies of anything

## Federation and Privacy

When two people on different chatmail servers chat, their messages have to travel between the two servers.

Chatmail uses a modern, fast method (`/mxdeliv`) by preference, with normal email as a fallback.

Even in this case:
- The sending server has already enforced the PGP-only rule.
- The receiving server will also enforce it.
- The message content itself is still encrypted to the recipient’s key.

The servers in the middle are mostly just moving opaque encrypted blobs.

## Calls (Voice & Video) and TURN

Delta Chat supports voice and video calls using WebRTC.

For these calls to work when people are behind routers or firewalls, a relay (TURN server) is often needed.

A chatmail server can run its own TURN server. The credentials for that TURN server are given to the user’s Delta Chat client automatically through a special IMAP mechanism (METADATA).

Important privacy points:
- The TURN credentials are temporary and tied to the user’s account.
- The same operator who hosts the encrypted mail is also providing the media relay.
- No third-party TURN provider needs to see who is calling whom.

## What About the Admin?

The person running the server (the admin) has significant power:

- They can block or delete accounts.
- They can see basic usage statistics.
- They can (in some configurations) read server logs.

However, even the admin normally cannot read the actual content of users’ messages because those messages are encrypted with keys the admin does not have.

This is why the “PGP-only” rule is so important: it protects users from a malicious or compromised admin as well as from outside attackers.

## Common Questions

**“Can the server operator read my chats?”**

Normally no — the messages are encrypted. The main thing the operator can see is that a certain account exists and is using a certain amount of storage.

**“What if the server is seized or hacked?”**

With No-Log enabled and proper encryption, there is very little useful data for an attacker to find. The encrypted message files are useless without the recipients’ private keys.

**“Is this as private as Signal or WhatsApp?”**

Different trade-offs. Signal has more metadata protection in some areas. Chatmail gives you the ability to run your own server, choose who you trust, and benefit from email federation. Many people consider “I run the server” or “my friend runs the server” a stronger privacy guarantee than trusting a large company.

## Summary

Chatmail’s privacy model is based on three pillars:

1. The server is intentionally blind to message content (PGP-only enforcement).
2. The server can be configured to remember almost nothing about what happened (No-Log).
3. Operators and users both benefit from the same strong defaults.

This is why many people consider a well-run chatmail server one of the more trustworthy places to have their private conversations.

## Next

- How accounts are actually created and managed: [Accounts & Registration](./03-accounts-and-registration.md)
- Running and administering the server day to day: [Admin & CLI](./07-admin-and-cli.md)
