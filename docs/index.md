# Documentation Index

Welcome to the Madmail documentation. This page serves as a central hub for all available guides and references.

## 🚀 Getting Started
- **[Chatmail Setup Guide](./chatmail-setup.md)** - Comprehensive guide to setting up a Chatmail server using the Maddy fork (manual & Docker).
- **[Standard Setup Tutorial](./tutorials/setting-up.md)** - Step-by-step practical guide for personal mail server installation.
- **[Building from Source](./tutorials/building-from-source.md)** - System dependencies and build instructions for manual compilation.
- **[Docker Deployment](./docker.md)** - Official Docker image usage, ports, and configuration details.

## 💬 Chatmail Specifics
- **[Authentication Specification](./chatmail/authentication.md)** - Details on "just-in-time" auto-registration and credential lookup logic.
- **[E2E Test Suite](./chatmail/e2e_test.md)** - Overview of automated tests using Delta Chat to verify server behavior.
- **[No Log Policy](./chatmail/nolog.md)** - Privacy enforcement via dynamic logging toggles and `NopOutput` backends.
- **[PGP-Only Email Policy](./chatmail/only_pgp_mails.md)** - In-depth look at PGP/MIME verification and message rejection criteria.
- **[Settings Database](./chatmail/settings_db.md)** - Dynamic configuration storage for flags like registration and logging.
- **[VoIP & TURN Integration](./chatmail/turn.md)** - Technical details on integrated TURN server and IMAP metadata discovery.

## 🛠 Operation & Configuration
- **[Upgrading](./upgrading.md)** - Best practices and manual migration steps for incompatible version changes.
- **[Multiple Domains](./multiple-domains.md)** - Configuring account isolation vs. shared namespaces across domains.
- **[Outbound Security](./seclevels.md)** - Understanding MX authentication and TLS enforcement policies.
- **[F.A.Q.](./faq.md)** - Common issues, resource usage, and comparisons with other mail servers.
- **[Release Process](../RELEASES.md)** - Information for maintainers on tags, GoReleaser, and GitHub Actions.
- **[Binary Verification](./binary-verification.md)** - SHA256 hashes for all releases and verification instructions.
- **[Signature Verification](./signature.md)** - Technical details on Ed25519 digital signatures and `maddy upgrade` mechanism.

## 📚 Advanced Tutorials
- **[Remote MX Forwarding](./tutorials/alias-to-remote.md)** - How to (and why you shouldn't) forward messages to remote servers.
- **[PAM Authentication](./tutorials/pam.md)** - Using the host's PAM infrastructure for user authentication.

## 💻 Internals & References
- **[Followed Specifications](./internals/specifications.md)** - List of RFCs and standards implemented by maddy.
- **[Implementation Quirks](./internals/quirks.md)** - Documented deviations from standards or unusual behaviors.
- **[SQLite Optimization](./internals/sqlite.md)** - WAL mode, auto-vacuuming, and performance notes for the SQLite backend.
- **[Unicode Support](./internals/unicode.md)** - Internal UTF-8 handling, internationalized domains, and PRECIS profiles.
- **[Development Guide](./DEVELOPMENT.md)** - Common developer tasks (tidy, lint, test) and make targets.
- **[Hacking Madmail](../HACKING.md)** - Design goals, module architecture, and core philosophy.
- **[Detailed Contribution Guide](./contributing.md)** - Branching strategy, PR workflow, and AI responsibility.
- **[Style Guide](./STYLEGUIDE.md)** - Lightweight checklist for documentation voice, tone, and formatting.
- **[AI Disclosure](./ai-disclosure.md)** - Transparency regarding AI-assisted development and our security model.
