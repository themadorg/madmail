<p align="center">
  <img src="assets/madmail_logo02_bg_white_bg.png" alt="Madmail logo" width="220">
</p>

<h1 align="center">Madmail-V2</h1>

<p align="center">
  <strong>A rust-based mail relay server for Delta Chat, encrypted, federated, one binary. batteries included.</strong>
</p>

<p align="center">
  <a href="docs/project/user-guide/02-quick-start.md">Quick Setup</a> ·
  <a href="docs/project/user-guide/01-what-is-chatmail.md">Features</a> ·
  <a href="docs/project/user-guide/README.md">Documentation</a> ·
  <a href="docs/project/user-guide/11-deployment-ip-domain-certs.md">Deployment</a>
</p>

Madmail is a **server relay** for the **[Delta Chat](https://delta.chat)** app, users message through the **Chatmail protocol**, while Madmail handles delivery, storage, federation, and real-time services on the server side.

A Rust rewrite of Madmail v1: SMTP, IMAP, encryption enforcement, and real-time relay built in.



## Quick Setup

Download the release binary for your architecture, then install and start the service:

```bash
ARCH=$(uname -m | sed 's/x86_64/amd64/;s/aarch64/arm64/')
curl -fsSL "https://github.com/themadorg/madmail/releases/latest/download/madmail-linux-${ARCH}" \
  -o madmail
chmod +x madmail
```

### With a public IP (no domain)

Trusted TLS via Let's Encrypt IP certificate (~6-day renewal, port 80 required):

```bash
sudo ./madmail install --simple --ip YOUR_IP \
  --auto-ip-cert \
  --acme-email you@example.com \
  --lang en
sudo systemctl enable madmail
sudo systemctl start madmail
```

> Replace `YOUR_IP` with your server's public IPv4 or IPv6 address.

Self-signed TLS (testing / internal — omit `--auto-ip-cert`):

```bash
sudo ./madmail install --simple --ip YOUR_IP --lang en
sudo systemctl enable madmail
sudo systemctl start madmail
```

### With a domain

Standard Let's Encrypt certificate (90-day renewal, DNS must point to your server):

```bash
sudo ./madmail install --simple --domain mail.example.org \
  --acme-email you@example.com \
  --lang en
sudo systemctl enable madmail
sudo systemctl start madmail
```

> Replace `mail.example.org` with your hostname and `you@example.com` with a valid contact email.

More detail:

- [Quick start (full guide)](docs/project/user-guide/02-quick-start.md)
- [Simple IP + ACME install](docs/install-simple-ip-acme.md)
- [IP vs domain deployment](docs/project/user-guide/11-deployment-ip-domain-certs.md)
- [Local development](docs/local-dev.md)


## Documentation

Documentation is organized by audience and purpose.

### For Server Operators and End Users

- **[User & Operator Guide](docs/project/user-guide/README.md)** — Practical, human-friendly documentation covering accounts & registration, privacy model, federation, calls (TURN/Iroh), administration, deployment scenarios, and troubleshooting.

### For Developers and Contributors

- **[Project Documentation](docs/project/README.md)** — Technical tour of architecture, crates, runtime wiring, data flows, build system, and contribution notes.
- **[Technical Design Document (TDD)](docs/TDD/README.md)** — Design specifications for major components.
- **[RFC Reference Library](docs/TDD/RFC/README.md)** — Collection of relevant protocol specifications (SMTP, IMAP, HTTP, TLS, TURN, etc.).

### Quick References

- [Simple IP + ACME Installation](docs/install-simple-ip-acme.md) — IP-based install with Let's Encrypt TLS
- [Local Development Guide](docs/local-dev.md) — Developer setup, build, and testing workflow

Documentation lives in the repository alongside the source code.



## Credits

Madmail v2 stands on many open-source projects.

During **[how we built Madmail v2](docs/how-we-built-it.md)**, dozens of those trees were used as **context** while implementing the Rust server, for behavior parity with Madmail v1, protocol study, client E2E testing, TLS/ACME patterns, and real-time relay integration. What each repository contributed (and related notes) is in **[Context & reference projects](docs/context-references.md)**.

This codebase was also developed with **[Cursor](https://cursor.com)** (coding agent) and **[Gemini 3.1 Pro](https://aistudio.google.com/)** (Google AI Studio) for planning and implementation assistance. See **[AI-assisted development](docs/ai-assisted-development.md)** for how those tools fit into the workflow (more detail to be added there).


## Disclaimer

The product vision, architecture, phase plan, and acceptance criteria were defined and reviewed by **humans**. **Most of the Rust (and related) source in this repository was written with AI assistance** under that direction, not as an unattended dump of generated code, but as an iterative, human-guided process.

**Use at your own risk.** Madmail v2 is AGPL software under active development; run it on production systems only after you have validated it for your threat model and workload.

We always welcome criticism, bug reports, and discussion, please use **[GitHub Discussions](https://github.com/themadorg/madmail/discussions)**.



## Resources

- [GitHub Releases](https://github.com/themadorg/madmail/releases)
- [Telegram Channel](https://t.me/the_madmail)
- [Delta Chat](https://delta.chat)
- [Download Delta Chat Apps](https://delta.chat/en/download)

---

## License

[AGPL-3.0-or-later](LICENCE)