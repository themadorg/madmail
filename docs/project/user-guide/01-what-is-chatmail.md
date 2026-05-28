# What is Chatmail?

**Chatmail** is the name of the protocol for doing secure, private chat using email.

Delta Chat uses this protocol. Instead of building yet another closed messenger, Delta Chat turns normal email (IMAP + SMTP) into a fast, encrypted messaging experience — but it works best when both sides use a proper **chatmail relay**.

## Features


|                         |                                                                 |
| ----------------------- | --------------------------------------------------------------- |
| **SMTP + IMAP**         | Submission, incoming, and retrieval — full stack in one process |
| **PGP-only policy**     | Unencrypted mail rejected at the server                         |
| **JIT registration**    | No sign-up flow; accounts created on first auth                 |
| **HTTP federation**     | `/mxdeliv` for fast inter-server delivery, SMTP as fallback     |
| **No-Log mode**         | Strip message metadata when strict privacy is required          |
| **Admin API + Web UI**  | JSON-RPC API and embedded Svelte dashboard                      |
| **WebIMAP**             | REST + WebSocket for web and desktop clients                    |
| **TURN / Iroh**         | Audio/video calls and WebXDC relay, no extra services           |
| **Shadowsocks**         | Stealth deployment in restricted networks                       |
| **ACME / TLS**          | Automatic certificate management                                |
| **Quota & blocklist**   | Per-account limits and federation policy (ACCEPT/REJECT)        |
| **SQLite + PostgreSQL** | Both backends supported; live config reload without restart     |




### Chatmail is the Protocol, madmail is a Server

- **Chatmail** = the rules and design (PGP-only, Just-in-Time accounts, federation between relays, built-in support for calls, strong privacy defaults, etc.).
- **madmail** = one implementation of a chatmail relay server (there is a Go version and the Rust version in this project).

In other words: madmail is a **chatmail relay server**. It is not a general-purpose email server — it is specialized for the way Delta Chat works.

Chatmail relays are intentionally "dumb" — they keep very little long-term state about users. Accounts exist mainly to receive encrypted messages and to allow sending. There is no social graph, no permanent user profiles, and data is cleaned up regularly.

Delta Chat is now a **relay-based** system. While it can technically work with any email server, it delivers the best experience (instant push, reliable calls, good metadata protection, easy onboarding) when users are on a proper chatmail relay.

Other ways to run a chatmail relay exist (for example setups based on cmdeploy with Dovecot + Postfix), but madmail is currently the most popular and complete implementation.

## The Big Ideas Behind Chatmail Relays

These principles define how a proper chatmail relay should behave. They apply to madmail (both the Go and Rust versions) as well as other chatmail-compatible setups.

### 1. Automatic & Temporary Accounts (Just-in-Time Registration)

You usually don’t have to ask an admin to create an account for you.

When you first connect with Delta Chat using a username and password, the relay creates the account for you automatically — if the relay allows new registrations. There is also a simple `/new` web endpoint that can create an account on the fly (often with randomly generated credentials).

Important: Accounts on a chatmail relay are **temporary**. Both accounts and the messages stored in them are regularly deleted after some time (this is normal and by design). A relay account is not a permanent personal mailbox — it is mainly a place to receive encrypted messages and a way to send messages from.

### 2. Encryption by Default (PGP-Only)

Almost every message that travels through a chatmail relay must be encrypted.

- Normal plain-text email is rejected.
- Only properly encrypted messages (or a few special “handshake” messages used by Delta Chat to set up secure chats) are accepted.

This is not because the relay operators are mean — it is the foundation of the privacy and security model. If the relay never sees readable mail, it cannot accidentally leak it or be forced to hand it over.

### 3. Simple Operation

A chatmail relay is usually designed to be as simple as possible to run. The madmail implementations (both Go and Rust) are single binaries that include everything you need (mail protocols, federation, TURN for calls, admin interface, etc.). Other setups (such as cmdeploy with Postfix + Dovecot) also aim for simplicity through automation.

In all cases the goal is the same: one service to manage instead of many moving parts.

### 4. Federation

Different chatmail relays can talk to each other.

If Alice is on `alice@chat.example.org` and Bob is on `bob@chat.example.net`, they can still have an encrypted chat. The relays handle moving the messages between them.

The core way relays exchange mail is using normal email (SMTP). Some implementations (including madmail) also support a faster direct method between relays as an optimization, with normal email as a reliable fallback.

You are not locked into one big company’s servers.

### 5. Strong Privacy Defaults

Chatmail relays are designed with “No-Log” and minimal data collection in mind.

Many operators run them with logging turned off completely. The relay is not interested in reading your messages (it usually can’t anyway because they are encrypted).

## Who Runs Chatmail Servers?

- Individuals who want to host their friends and family
- Small organizations and communities
- People in countries or networks where centralized messengers are risky or blocked
- Anyone who wants a simple, auditable, self-hosted alternative for secure messaging

## How Is This Different from a Normal Email Server?


| Normal Email Server                       | Chatmail Relay (e.g. madmail)                                 |
| ----------------------------------------- | ------------------------------------------------------------- |
| Accounts usually created manually         | Accounts created automatically (and are temporary)            |
| Long-term personal mailboxes              | Temporary storage for encrypted messages + sending capability |
| Accepts almost any email                  | Rejects unencrypted mail by default                           |
| Complex to set up and maintain            | Designed to be simple to run                                  |
| Federation is slow and unreliable         | Normal email, with optional faster methods between relays     |
| Calls and real-time features are separate | TURN for calls is built-in and discovered automatically       |
| Lots of logs and metadata                 | Strong No-Log and privacy options                             |


## What You Experience as a User

From the point of view of someone using Delta Chat on a chatmail relay, it mostly “just works”:

- You enter an email address and a password (or use a `/new` link that can create one on the fly).
- The app sets up everything.
- You can chat with people on the same relay or other chatmail relays.
- Voice and video calls usually work even if you are behind a difficult internet connection.
- Everything is end-to-end encrypted.

Because accounts and messages on a relay are cleaned up after a while, a chatmail address is not a permanent personal inbox in the traditional sense — it is a temporary place to receive and send encrypted messages.

Because Delta Chat is now a relay-based system, the quality of your experience depends heavily on the quality of the chatmail relay you are using.

You rarely have to think about the relay at all — which is exactly the point.

## Next Steps

- Want to try it? See the [Quick Start guide](./02-quick-start.md)
- Running your own chatmail relay? Read the practical setup instructions
- Curious about the privacy guarantees? Read the [Privacy & Security](./04-privacy-and-security.md) guide

Chatmail is the protocol that makes modern, private messaging over email possible. madmail (and other implementations) are the relay servers that power it.