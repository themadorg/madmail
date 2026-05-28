# Madmail-V2

**The mail relay for Delta Chat, encrypted, federated, one binary.**

Madmail is a **server relay** for the **[Delta Chat](https://delta.chat)** app, users message through the **Chatmail protocol**, while Madmail handles delivery, storage, federation, and real-time services on the server side.

A Rust rewrite of Madmail v1: SMTP, IMAP, encryption enforcement, and real-time relay built in.

[Quick Setup](docs/project/user-guide/02-quick-start.md) · [Features](docs/project/user-guide/01-what-is-chatmail.md) · [Documentation](docs/project/user-guide/README.md) · [Deployment](docs/project/user-guide/11-deployment-ip-domain-certs.md)

---

---

## Quick Setup

### With a public IP (no domain)

Trusted TLS via Let's Encrypt IP certificate (~6-day renewal, port 80 required):

```bash
curl -fsSL https://github.com/themadorg/madmailv2/releases/latest/download/madmail-linux-$(uname -m | sed 's/x86_64/amd64/;s/aarch64/arm64/') -o madmail && chmod +x madmail && sudo ./madmail install --simple --ip YOUR_IP --auto-ip-cert --acme-email you@example.com --lang en && sudo systemctl enable madmail && sudo systemctl start madmail
```

> Replace `YOUR_IP` with your server's public IPv4 or IPv6 address.

Self-signed TLS (testing / internal — omit `--auto-ip-cert`):

```bash
curl -fsSL https://github.com/themadorg/madmailv2/releases/latest/download/madmail-linux-$(uname -m | sed 's/x86_64/amd64/;s/aarch64/arm64/') -o madmail && chmod +x madmail && sudo ./madmail install --simple --ip YOUR_IP --lang en && sudo systemctl enable madmail && sudo systemctl start madmail
```

### With a domain

Standard Let's Encrypt certificate (90-day renewal, DNS must point to your server):

```bash
curl -fsSL https://github.com/themadorg/madmailv2/releases/latest/download/madmail-linux-$(uname -m | sed 's/x86_64/amd64/;s/aarch64/arm64/') -o madmail && chmod +x madmail && sudo ./madmail install --simple --domain mail.example.org --acme-email you@example.com --lang en && sudo systemctl enable madmail && sudo systemctl start madmail
```

> Replace `mail.example.org` with your hostname and `you@example.com` with a valid contact email.

More detail: 

- [Simple IP + ACME install](docs/install-simple-ip-acme.md) 
- [IP vs domain deployment](docs/project/user-guide/11-deployment-ip-domain-certs.md) 
- [Local development](docs/local-dev.md)

---

## Documentation

Madmail includes comprehensive documentation organized by audience and purpose.

### For Server Operators and End Users

- **[User & Operator Guide](docs/project/user-guide/README.md)** — Practical, human-friendly documentation covering accounts & registration, privacy model, federation, calls (TURN/Iroh), administration, deployment scenarios, and troubleshooting.

### For Developers and Contributors

- **[Project Documentation](docs/project/README.md)** — A complete step-by-step technical tour of the architecture, crate structure, runtime wiring, data flows, build system, and contribution guidelines.
- **[Technical Design Document (TDD)](docs/TDD/README.md)** — Authoritative, in-depth design specifications for every major component of the system.
- **[RFC Reference Library](docs/TDD/RFC/README.md)** — Collection of relevant protocol specifications (SMTP, IMAP, HTTP, TLS, TURN, etc.).

### Quick References

- [Simple IP + ACME Installation](docs/install-simple-ip-acme.md) — Shortest path to a production relay with trusted TLS
- [Local Development Guide](docs/local-dev.md) — Developer setup, build, and testing workflow

All documentation is maintained alongside the source code and kept up to date with the current implementation.

---

## Credits

Madmail v2 stands on many open-source projects.

During **[how we built Madmail v2](docs/how-we-built-it.md)**, dozens of those trees were used as **context** while implementing the Rust server, for behavior parity with Madmail v1, protocol study, client E2E testing, TLS/ACME patterns, and real-time relay integration. What each repository contributed (and related notes) is in **[Context & reference projects](docs/context-references.md)**.

This codebase was also developed with **[Cursor](https://cursor.com)** (coding agent) and **[Gemini 3.1 Pro](https://aistudio.google.com/)** (Google AI Studio) for planning and implementation assistance. See **[AI-assisted development](docs/ai-assisted-development.md)** for how those tools fit into the workflow (more detail to be added there).

### Disclaimer

The product vision, architecture, phase plan, and acceptance criteria were defined and reviewed by **humans**. **Most of the Rust (and related) source in this repository was written with AI assistance** under that direction, not as an unattended dump of generated code, but as an iterative, human-guided process.

**Use at your own risk.** Madmail v2 is AGPL software under active development; run it on production systems only after you have validated it for your threat model and workload.

We always welcome criticism, bug reports, and discussion, please use **[GitHub Discussions](https://github.com/themadorg/madmailv2/discussions)**.

---

## Resources

- [GitHub Releases](https://github.com/themadorg/madmailv2/releases)
- [Telegram Channel](https://t.me/the_madmail)
- [Delta Chat](https://delta.chat)
- [Download Delta Chat Apps](https://delta.chat/en/download)

---

## License

[AGPL-3.0-or-later](LICENCE)