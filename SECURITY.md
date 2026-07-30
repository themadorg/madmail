# Security Policy

Madmail is a federated mail / Chatmail relay. Security issues can affect operator hosts, user accounts, and message integrity. We take reports seriously.

## Supported versions

We prioritize fixes for the **latest release** on the default branch (`main`) and the most recent tagged release on [GitHub Releases](https://github.com/themadorg/madmail/releases).

Older release lines may receive fixes at maintainer discretion when the issue is severe and a patch is practical.

## Reporting a vulnerability

**Please do not file public GitHub issues for undisclosed security vulnerabilities.**

### Preferred: private GitHub Security Advisories

1. Open a **private vulnerability report** on this repository:  
   <https://github.com/themadorg/madmail/security/advisories/new>
2. Include as much of the following as you can:
   - Affected version(s) or commit
   - Component (e.g. SMTP submission, IMAP, admin API, PGP gate, federation `/mxdeliv`, upgrade path)
   - Reproduction steps or a minimal proof of concept
   - Impact (confidentiality, integrity, availability, multi-tenant abuse)
   - Whether you plan to request a CVE

If private advisories are unavailable for any reason, use a GitHub Security contact via the repository **Security** tab, or contact the maintainers through the organization channels listed on <https://github.com/themadorg>.

## What to expect

| Stage | Target |
|-------|--------|
| Initial acknowledgement | **Within 14 days** of a clear report |
| Triage / severity | After we can reproduce or validate the issue |
| Fix / advisory | Coordinated with the reporter when practical |
| Public disclosure | Prefer after a fixed release is available, or on an agreed timeline |

We may ask for more detail or a safer repro. You may request anonymity; otherwise we are happy to credit reporters in the advisory or release notes.

## Scope (examples)

In scope:

- Authentication / authorization bypasses
- Cross-user data access
- Encryption-policy bypasses (e.g. unencrypted mail accepted as encrypted)
- Open-relay or abuse-amplification issues on public listeners
- Remote code execution, path traversal, or injection in server components
- Failures in signed update / upgrade verification
- Sensitive information leaks via logs, errors, or admin APIs

Out of scope (unless they lead to a server compromise):

- Issues only in third-party clients (e.g. Delta Chat) without a Madmail defect
- Denial of service that requires unrealistic resource limits without a concrete bug
- Reports that require physical access or full operator collusion on a correctly configured host
- Social engineering of operators

## Secure configuration notes

Operators should follow the published guides:

- [Privacy and security model](docs/project/user-guide/04-privacy-and-security.md)
- [Quick start](docs/project/user-guide/02-quick-start.md)
- [DNS and mail auth](docs/project/user-guide/12-dns-mail-auth.md)

Keep the server updated (signed upgrade path is supported by the `madmail` binary). Prefer TLS, restrict admin exposure, and keep registration policy appropriate for your threat model.

## Non-security bugs

Use the public issue tracker: <https://github.com/themadorg/madmail/issues>

## Thanks

Responsible disclosure keeps operators and users safer. We appreciate the research community and operators who report issues carefully.
