# User & Operator Guide

This section documents running and operating a **chatmail** relay (the Rust **madmail-v2** implementation in this project).

It is written in the same spirit as the original Madmail "chatmail" documentation — clear explanations for normal users and semi-technical operators, focused on what you actually need to know and do.

## Quick Start

- New to chatmail? Start with **[What is Chatmail?](./01-what-is-chatmail.md)**
- Want to run your own server right now? See **[Quick Start: Local Dev & Public Server](./02-quick-start.md)**
- Just want the shortest path to a working public server? Follow **[Install on a Public IP with Let's Encrypt](./../install-simple-ip-acme.md)** (short & practical)

## Main Guides

| Topic | Document | For |
|-------|----------|-----|
| What this server is and why it exists | [01-what-is-chatmail.md](./01-what-is-chatmail.md) | Everyone |
| Getting a server running (local or public) | [02-quick-start.md](./02-quick-start.md) | Operators |
| Accounts, registration (JIT), invites, blocking | [03-accounts-and-registration.md](./03-accounts-and-registration.md) | Operators & power users |
| Privacy model: PGP-only + No-Log explained | [04-privacy-and-security.md](./04-privacy-and-security.md) | Everyone |
| How mail actually moves (local + federation) | [05-sending-receiving-and-federation.md](./05-sending-receiving-and-federation.md) | Semi-technical users |
| Voice & video calls (TURN) + real-time features | [06-calls-and-real-time.md](./06-calls-and-real-time.md) | Users & operators |
| Managing the server (Admin web + CLI) | [07-admin-and-cli.md](./07-admin-and-cli.md) | Operators |
| Quota, retention, maintenance, performance | [08-quota-and-maintenance.md](./08-quota-and-maintenance.md) | Operators |
| Browser access, WebIMAP, sharing contacts | [09-browser-and-web-access.md](./09-browser-and-web-access.md) | End users |
| Common problems & how to fix them | [10-troubleshooting.md](./10-troubleshooting.md) | Everyone |
| IP-only vs Domain, with or without certificates | [11-deployment-ip-domain-certs.md](./11-deployment-ip-domain-certs.md) | Operators (very common question) |
| Advanced / stealth deployment options | [12-advanced-deployment.md](./12-advanced-deployment.md) | Experienced operators |
| Customizing the HTML pages and web UI | [17-customizing-html-pages.md](./17-customizing-html-pages.md) | Operators who want to brand or modify the public site |
| Endpoint Rewrite (push-push / domain redirection) | [15-endpoint-rewrite.md](./15-endpoint-rewrite.md) | Advanced operators |
| Exchangers (push-pull, pull-pull intermediaries) | [16-exchangers.md](./16-exchangers.md) | Advanced operators |

## How to Read These Guides

- **Normal users** (people with Delta Chat accounts on your server): Read 01, 04, 05, 06, 09.
- **Server operators / admins**: Read everything, especially 02, 03, 07, 08, 10, 11.
- **Power users / Delta Chat enthusiasts**: All of them.

## Relationship to Other Documentation

- This **user-guide** series = friendly, practical, "how do I...".
- `docs/project/` (the numbered 01–17 series) = deep technical understanding of the code and architecture (for developers).
- `docs/TDD/` = design decisions (for people who want the "why" at a system level).
- `docs/local-dev.md` and `docs/install-simple-ip-acme.md` = short, task-focused cheat sheets.
- The server also serves HTML documentation at `https://your-server/docs/` (multi-language).

## Contributing

If you run a server and notice something missing, confusing, or that could be explained better, please improve these guides. They are meant to be practical documentation for operators.

Start with the most important file for new people: **[01-what-is-chatmail.md](./01-what-is-chatmail.md)**

---

*This documentation is for humans running or using chatmail servers. It prioritizes clarity and practicality over implementation details.*
