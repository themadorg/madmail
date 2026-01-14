# Madmail - Maddy Chatmail Server
> Optimized all-in-one mail server for instant, secure messaging

This is a specialized fork of [Maddy Mail Server](https://github.com/foxcpp/maddy) optimized specifically for **chatmail** deployments. It provides a single binary solution for running secure, encrypted-only email servers designed for Delta Chat and similar messaging applications.

## What is Chatmail?

Chatmail servers are email servers optimized for secure messaging rather than traditional email. They prioritize:
- **Instant account creation** without personal information
- **Encryption-only messaging** to ensure privacy
- **Automatic cleanup** to minimize data retention
- **Low maintenance** for easy deployment

## Key Features

### ✅ Implemented
- **Passwordless onboarding**: Users can create accounts instantly via QR codes
- **Encrypted messages only (inbound & outbound)**: Strict filtering of unencrypted emails
- **Stale account cleanup**: Automatic removal of inactive addresses after a configurable period
- **Delta Chat integration**: Native support for Delta Chat account creation and QR provisioning
- **Web interface**: Modern and sleek account creation page with dark mode support
- **HTML Customization**: Export and serve custom HTML/CSS files for the web interface
- **Admin CLI**: Manage share links, reserve slugs, and manage accounts from the terminal
- **Single binary deployment**: Everything needed in one lightweight executable

## Releases & Downloads

Pre-built release artifacts for common platforms are published on the repository's GitHub Releases page. Each release includes signed archives for the following targets (when available):
- linux (amd64, arm64)
- macOS (amd64, arm64)
- windows (amd64, arm64)

To download the latest release, visit: https://github.com/themadorg/madmail/releases and pick the artifact matching your OS/architecture. Artifacts are packaged as tar.gz (Linux/macOS) or zip (Windows) and include a `maddy` binary and the default `maddy.conf`.

If you prefer to build locally, see the "Building from source" tutorial in the docs (it also documents how to use the releases and how to embed version information): docs/tutorials/building-from-source.md

## Configuration Differences from Standard Maddy
This chatmail-optimized distribution includes several key enhancements:

1. **Integrated Admin CLI**: Powerful terminal commands for managing contact share links (`maddy sharing`) and web interface customization (`maddy html-export/serve`).
2. **Advanced Encryption Enforcement**: Automatic blocking of both inbound and outbound unencrypted messages to maintain a high-security posture.
3. **Dynamic Web Interface**: A built-in HTTP/HTTPS registration endpoint with the ability to serve custom HTML from an external directory.
4. **Maintenance Automation**: Native support for cleaning up stale accounts and implementing strict message retention policies.
5. **Delta Chat Native**: Direct provisioning of accounts via QR codes, fully compatible with the Delta Chat ecosystem.

## Architecture

```
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│   Web Interface │    │   SMTP/IMAP      │    │   Delta Chat    │
│   (QR Codes)    │◄──►│   Mail Server    │◄──►│   Clients       │
└─────────────────┘    └──────────────────┘    └─────────────────┘
         │                       │                       │
         └───────────────────────┼───────────────────────┘
                                 │
                    ┌──────────────────┐
                    │   SQLite Storage │
                    │   (Accounts &    │
                    │    Messages)     │
                    └──────────────────┘
```

## Contributing

This project maintains compatibility with the upstream Maddy project while adding chatmail-specific optimizations. Contributions should:

1. Maintain backward compatibility with standard Maddy configurations
2. Follow the chatmail specification and best practices
3. Include tests for new chatmail-specific features
4. Update documentation for any user-facing changes

## Upstream Compatibility

This fork periodically syncs with the upstream Maddy project to incorporate security updates and improvements. Chatmail-specific features are implemented as optional modules that don't interfere with standard Maddy functionality.

## License

This project inherits the GPL-3.0 license from the upstream Maddy Mail Server project.


## Community & Support
For the latest updates, news, and community support, join our Telegram channel:
👉 [**Madmail Telegram Channel**](https://t.me/the_madmail)

---

*For traditional email server needs, consider using the upstream [Maddy Mail Server](https://github.com/foxcpp/maddy) project.*
