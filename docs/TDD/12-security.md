# Security Architecture

## Core Security Policies

### 1. PGP-Only Enforcement
**Mandatory** for all user-submitted messages (SMTP submission and IMAP APPEND).

Accepted:
- Properly formed `multipart/encrypted` PGP/MIME messages
- Secure Join handshake messages (`Secure-Join: vc-request`)
- Certain automated bounces (`mailer-daemon@` + `multipart/report`)

Rejected with `523 Encryption Needed`:
- Plain text
- Invalid PGP structure
- Header mismatches (MIME From != Envelope From)

**Implementation**: Deep packet inspection in `pgp_verify` module (see Madmail reference).

### 2. No-Log Policy
When `log off` in config (the default if `log` is omitted):
- Use `tracing` with filter `off` (no protocol/DB chatter)
- No log destinations opened (no stderr fan-out, no log files)
- Only critical boot errors go to stderr via `boot_error` (not tracing)

Operators opt in with the static `log` directive (maddy-compatible):
- `log stderr` / `log on` / `log stderr_ts` — write tracing to stderr
- `log /path/to/file` — append to a file (create parents as needed)
- `log stderr /var/lib/madmail/madmail.log` — both
- `log syslog` — currently maps to stderr (dedicated syslog backend not wired yet)

Debug mode (`debug true` / `yes` / `1` / `enable` / …) **overrides** No-Log: forces `debug` filter level and stderr if no `log` target is set. Logging is not toggled via CLI or admin API.

### 3. Federation Policy Engine
- `ACCEPT` (default) + blocklist rules
- `REJECT` + allowlist rules
- Enforced on **every** inbound SMTP and `/mxdeliv` request
- Enforced on **every** outbound delivery attempt
- In-memory `HashSet` for O(1) checks + `RwLock`

### 4. TLS & Certificate Modes
Support all four modes from Madmail:
- `autocert` (Let's Encrypt HTTP-01)
- `acme` (DNS-01)
- `file` (user provided)
- `self_signed`

Use `rustls` + `rustls-acme` or `instant-acme` crates.

### 5. Admin API Security
- Shared secret Bearer token
- Constant-time comparison
- Rate limiting (10 attempts/min/IP)
- Always HTTP 200 + status in body
- Request size limit (1MB)
- No sensitive data leakage

### 6. Rate Limiting & DoS Protection
- Per-IP auth failure rate limit on Admin API
- Connection limits per service
- Early size checks on message submission

### 7. Quota Enforcement
Checked on every delivery and IMAP quota command.
In-memory cache with write-through updates.

### 8. Operator Webhooks

`/admin/services/webhooks` uses the existing authenticated admin RPC boundary.
Notifications contain account/quota metadata only; never passwords, message content
or invitation identifiers. Stored signing secrets are excluded from API responses,
generic settings, public pages and logs. Receivers must verify HMAC-SHA256 over the
raw request body and apply their own replay protection.

HTTPS uses normal certificate verification; HTTP is restricted to loopback testing.
Redirects and environment proxies are disabled. Bounded in-process queues, timeouts,
retries and per-user quota deduplication isolate mail paths from receiver failures.
The feature is disabled by default and follows the existing No-Log policy.
See [operator webhook design and tests](25-operator-webhooks.md).

## Cross-Protocol Attacks (ALPACA)

Every TLS listener presents the same certificate, because there is one hostname.
That removes the certificate-separation defence against [ALPACA](https://alpaca-attack.com/)
(USENIX Security 2021): an attacker redirects a victim's HTTPS connection to a
different TLS service that presents an acceptable certificate for the same name,
then abuses that service's parser. TLS does not bind a connection to an intended
port, so a lax mail listener stays a valid substitute server for the web origin
however strict the HTTPS port is.

Strict ALPN is therefore the only TLS-layer defence available, and RFC 9325
(BCP 195) §3.8 makes it the recommendation. Each listener advertises exactly the
protocols it will speak, so rustls answers a client whose offers do not overlap
with a fatal `no_application_protocol`:

| Listener | Advertised ALPN |
|----------|-----------------|
| IMAP implicit TLS (993) | `imap` |
| Submission implicit TLS (465) | `smtp` |
| HTTPS sharing mail (443, `alpn_imap` / `alpn_smtp`) | enabled mail tokens + `http/1.1` |
| HTTPS alone (443) | none |
| STARTTLS upgrades (143 / 587), inbound SMTP (25) | none |

Built by `load_mail_tls_configs` in `crates/chatmail/src/shared_listener.rs`.

On the shared HTTPS port, ALPN is also checked **before** the SNI hostname. That
ordering is what makes hostname routing safe: SNI names a host rather than a
protocol and a browser sets it from its URL bar, so consulting SNI first would send
a visitor to `https://imap.example.org/` into the IMAP parser with no attacker
involved. Browsers always offer ALPN in order to negotiate HTTP/2, so they are
decided before the hostname is ever read. Tests: `p12_it07_alpn_outranks_sni`,
`p12_ut20_configured_sni_replaces_the_prefix_default`.

Two limits are deliberate. STARTTLS ports advertise nothing because the client
negotiates TLS after a plaintext greeting and has no ClientHello to offer ALPN in.
And a client that sends no ALPN extension is never rejected — rustls only fails
when the client offered ALPN and nothing overlapped — which is what keeps
Thunderbird, Apple Mail and Delta Chat on the standard ports working. That also
means an ALPN-less client remains unattributable, so on the shared HTTPS port such
a connection is served as HTTPS rather than guessed into a mail parser.

Tests: `p12_it04_alpn_less_client_still_reaches_imap`,
`p12_it05_browser_alpn_refused_on_imap_port`,
`p12_it02_unoffered_alpn_is_refused`,
`p12_ut17_dedicated_mail_ports_get_their_own_alpn`.

## Threat Model Considerations
- User enumeration prevention (silent drop on non-existent users during federation)
- Timing attack prevention (constant-time token compare)
- Configuration injection prevention (sanitize values written to config)
- Self-signed cert acceptance only for federation (not for submission)

### Installer and uninstaller running as root

System install, `install --user-only` and `uninstall` run as root next to the
service account. The threat model assumes that account is compromised: it owns the
state directory and can create, rename (including `renameat2(RENAME_EXCHANGE)`
loops), link and hard-link anything there while root runs, and
`fs.protected_hardlinks` may be 0.

- Install and both repair modes refuse, before `useradd` or `usermod`, a state
  directory that is not madmail's own: a well-known system directory (as given,
  after links, or by device and inode), a direct child of `/`, a user's home
  directory, or a directory below `/etc`, `/usr`, `/lib`, `/bin`, `/sbin`,
  `/boot`, `/dev`, `/proc`, `/sys`, `/run` (or `/lib32`, `/lib64`, `/libx32`)
  none of whose components below the top-level one is named for madmail (as
  given or after links, also when a dangling link names a directory install
  would create; `/etc/madmail/data` passes, `/etc/systemd` does not), an
  existing directory of root or of a uid without an account name that is not
  empty and whose physical path (after links) is not named for madmail (such as
  `/var/lib/dpkg`, `/var/spool/cron`, a link `/srv/madmail` to one of them, or a
  deleted package account's `/var/lib/oldapp`; the walk would give every
  root-owned directory in it to the service; not with `--skip-user`, which has
  no walk), or an existing directory that belongs to another named account or
  whose owner cannot be looked up. Such a directory is taken when it holds only
  a root-owned `lost+found` or carries `.madmail-state` as a regular file with
  one link that belongs to root (`fstatat(AT_SYMLINK_NOFOLLOW)` on the opened
  directory). `SecureInstall::open` creates that file (`O_CREAT | O_EXCL |
  O_NOFOLLOW` through the directory descriptor) before anything else in a state
  directory it creates, or in a root-owned one it accepts (not with
  `--skip-user`); the walk leaves it root's. Only root can create a root-owned
  file, and the link count rules out a hard link to one, so a file the service
  account made never counts. `SecureInstall::open` makes these checks on
  the directory it opens first and refuses to go on when the directory it then
  keeps open is a different one. Install
  gives the state directory and what is in it to the service account, so such a
  directory would hand system or another account's files to it.
- Install resolves every path it writes into the state directory, and the paths
  of the config file, the TLS files and their directories and the binary, one
  component at a time (`ctl/secure_fs.rs`) and does not follow a directory or link
  owned by the service account (current or previous uid), a directory writable by
  one of its groups (primary or supplementary), or a world-writable directory
  without the sticky bit. Inside the state directory, recognized by device and
  inode, every component is opened with `O_NOFOLLOW`, and files are written by
  create-new-then-rename with `fchmod` / `fchown` on the new descriptor. Ownership
  and modes of those files and directories are changed through the opened
  descriptor. The unit file and the man page and completions are written by path
  into root-owned system directories. A `--binary-path` inside the state
  directory, or whose directory resolves into it, is refused, and so is a config
  path outside the state directory that resolves into it.
- Files install writes inside the state directory get the service account as
  owner on the new file's descriptor, before they are renamed into place.
- The ownership walk of the state tree (`ctl/install/chown_walk.rs`) checks and
  changes each entry through a descriptor for that entry and skips links. A file
  is given to the service account only when it already belongs to the service uid
  or the previous service uid; a directory also when root owns it. A directory of
  another account is not changed or descended into. The link count is not used: a
  hard link to another account's file becomes that inode's only link once the
  original name is replaced by a rename. So a file root owns, for example the
  config file root just wrote outside the tree and the service linked into it,
  is never handed over. A state directory whose owner's uid now names another
  account is refused before the walk, and so is one whose owner lookup reports
  an error. The lookup asks `getpwuid_r` and, when that finds no entry,
  `getent passwd -- <uid>` (`/usr/bin/getent`, then `/bin/getent`, then
  `PATH`): the release binaries are linked statically with musl, whose
  `getpwuid_r` reads only `/etc/passwd` and never asks NSS modules (LDAP, sssd,
  winbind, systemd-userdb), while `getent` uses the host's NSS configuration.
  The entry `getent` prints must be the one asked for (field 0 equal to the
  name, or field 2 to the uid). Only a NULL result or `ENOENT` from
  `getpwuid_r`, and exit status 2 from `getent` (or no `getent` installed when
  `/etc/nsswitch.conf` is missing or lists only `files` for passwd and group,
  as on BusyBox hosts, which have no NSS modules), count as "no account"; any
  other result refuses the directory. Residual: `getent` exits with 2 both for a missing entry and when
  the NSS service is down, so while an LDAP or sssd server is unreachable, a
  directory of an account only it knows looks like one of the previous service
  uid and its files of that uid are given to the service. Install also stops
  when the service account exists but its groups cannot be read
  (`getpwnam_r`, then `getent passwd -- <name>`, then `id -G`), or when `id` finds
  the account and that lookup does not, since a group it does not know of could
  write a directory it would trust. The state directory itself is changed last,
  and not when the walk fails.
- A TLS file of another account outside the state directory is given to the
  service account only in a directory dedicated to madmail or in the Let's
  Encrypt `live`/`archive` layout, and only with one hard link. Otherwise the
  note prints `chown -h`, so a file replaced by a link before the operator runs
  it does not take the change.
- Paths are printed escaped and commands are printed shell-quoted, so a file name
  the service account chose cannot add lines to the output or to a command the
  operator copies. The printed steps for a file stop the service and every
  process of its account, and of the account that now has the file's uid when
  one has, before `chown -h`, because `chown` resolves the directories on the way
  again by name; they end with the command. Each account is locked and expired
  (`usermod -L -e 1970-01-02`; an account that exists only in a directory
  service must be locked there, as `usermod` cannot change it), so cron and at,
  which check the account with PAM, start no new job for it, its cron and at
  jobs are removed, its systemd user manager is stopped
  (`loginctl terminate-user`), and its processes are killed with `SIGKILL`,
  which they cannot ignore; a check of `ps` alone would miss a job started
  after it. Account names are shell-quoted in the commands and escaped in the
  text. For the account that now has the file's uid, the steps say to note its
  expiry first (`chage -l`) and afterwards to give it back and to unlock the
  account if it was not locked before (`usermod -U`). The steps say to end the
  service account's expiry afterwards; the
  unit has no `PAMName=`, so systemd starts the service while it is expired. They say not to change a file with
  more than one hard link, and that one link does not prove the file is
  madmail's. A file inside the state directory that is reached through a
  directory another account owns or can write gets no command, only a note
  that it needs manual inspection.
- The config file's owner and mode are set after the walk, so they are final.
- The ACME client writes only into a root-owned 0700 staging directory.
- Directories outside the state directory are re-owned or re-moded only when they
  are dedicated to madmail by name, both as given and as resolved, and are not a
  well-known system directory by device and inode (`ctl/host_paths.rs`).
- Uninstall applies the same naming rule to every directory it would remove,
  never removes a directory that is or contains a kept path (also through links),
  uses a config file's `state_dir` only when that file and its path pass the
  root-only trust rule below and the file has one hard link, and removes a path
  only after checking every directory and link on the way to it under the same
  rule, without following the last component. A path whose way fails the check,
  or whose removal fails, is left in place and reported, and uninstall goes on. Directories are removed
  through descriptors (`openat` with `O_NOFOLLOW`, `unlinkat`) from the checked
  parent down. A config path inside a state directory goes with that directory.
- The root-only trust rule (`Trust::root_only`, used by uninstall and for the
  ACME staging directory) needs root as owner, and refuses a directory writable
  by a group of the service, one writable by any other group except root without
  the sticky bit, and one world-writable without the sticky bit. For uninstall
  the groups of the service are those of every service account found (the
  `User=` values of madmail and chatmail units, the accounts named after those
  units, the primary binary name, `madmail` and `chatmail`; the config check and
  the removal use the same list, `uninstall::trust_accounts`), the group of the config
  file's directory and the groups of the state directories; when an account
  exists but its groups cannot be read, the config file's `state_dir` is not
  used and the allowance below is turned off. Two directories are allowed to be group-writable by another system group:
  `/var/log` and `/var/mail`, matched by device and inode
  (`secure_fs::WELL_KNOWN_GROUP_DIRS`), and only when that group is not a group
  of the service. Ubuntu ships `/var/log` as `root:syslog` 0775 and Debian ships
  `/var/mail` as `root:mail` 2775; a member of such a group could otherwise
  rename madmail's directory there between the check and the removal, which
  the allowance accepts for these two directories only.

Known limits:

- Trust is decided from owner and mode bits only. A POSIX ACL that grants the
  service account write access to a directory owned by root, with a group that is
  not one of its groups, is not detected.
- The install language is written to the SQLite database by the installer process
  with the service account's effective uid and gid (`setegid` / `seteuid`, groups
  reduced to the service group); the saved set-user-ID stays 0. That keeps root
  from opening or creating a file through a link the account planted, but a
  database crafted to exploit a memory-safety bug in SQLite could run code that
  calls `seteuid(0)`. The seed runs as root, without that change, only when the
  state directory is not controlled by the service account (not owned by its
  current or previous uid, not writable by its groups, and not world-writable
  without the sticky bit).
- With `--skip-user` and an existing account, writes still go through the
  descriptor checks; without an account there is no service to defend against, and
  install refuses a state directory that exists and is not owned by root. A local
  install writes by path as the caller.

See [CLI tools](14-cli-tools.md) for the details.

## Rust-Specific Security
- Prefer crates with good audit history (`rustls`, `tokio`, `axum`, `sqlx`)
- Use `zeroize` for sensitive data in memory where appropriate
- Avoid `unwrap()` in production paths
- Structured logging with `tracing` (can be completely disabled)

## Implementation references

Index: [`CONTEXT.md`](CONTEXT.md).

| Concern | madmail | cmrelay | cmdeploy | stalwart |
|---------|---------|---------|----------|----------|
| PGP-only enforcement | [`pgp_verify/pgp_verify.go`](../../context/madmail/internal/pgp_verify/pgp_verify.go), tests in [`pgp_verify/`](../../context/madmail/internal/pgp_verify/) | [`openpgp.rs`](../../context/cmrelay/src/filtermail/src/openpgp.rs) | — | — |
| Submission check | [`endpoint/smtp/submission.go`](../../context/madmail/internal/endpoint/smtp/submission.go) | [`inbound.rs`](../../context/cmrelay/src/filtermail/src/inbound.rs) | — | — |
| `/mxdeliv` security | [`mxdeliv_security.go`](../../context/madmail/internal/endpoint/chatmail/mxdeliv_security.go) | [`mxdeliv.rs`](../../context/cmrelay/src/filtermail/src/mxdeliv.rs) | — | — |
| Federation policy | [`federationtracker/policy.go`](../../context/madmail/internal/federationtracker/policy.go) | — | — | — |
| No-Log | [`docs/chatmail/nolog.md`](../../context/madmail/docs/chatmail/nolog.md) | — | — | [`crates/trc/`](../../context/stalwart/crates/trc/) (tracing design) |
| TLS / ACME | [`internal/tls/`](../../context/madmail/internal/tls/) | [`manager/internal/install/tls.go`](../../context/cmrelay/src/manager/internal/install/tls.go) | [`acmetool/`](../../context/cmdeploy/src/cmdeploy/acmetool/) | TLS in server crates + install |
| E2E security tests | [`test_02_unencrypted_rejection.py`](../../context/madmail/tests/deltachat-test/scenarios/test_02_unencrypted_rejection.py), [`test_08_no_logging.py`](../../context/madmail/tests/deltachat-test/scenarios/test_08_no_logging.py), [`test_22_mxdeliv_security.py`](../../context/madmail/tests/deltachat-test/scenarios/test_22_mxdeliv_security.py) | — | — | — |

## Related RFCs

PGP policy, MIME, TLS, and certificate automation. Index: [`RFC/README.md`](RFC/README.md).

| RFC | Topic | Local |
|-----|-------|-------|
| [3156](https://datatracker.ietf.org/doc/html/rfc3156) | PGP/MIME (`multipart/encrypted`) | [rfc3156.txt](RFC/rfc3156.txt) |
| [9580](https://datatracker.ietf.org/doc/html/rfc9580) | OpenPGP message format | [rfc9580.txt](RFC/rfc9580.txt) |
| [4880](https://datatracker.ietf.org/doc/html/rfc4880) | OpenPGP (legacy reference) | [rfc4880.txt](RFC/rfc4880.txt) |
| [2045](https://datatracker.ietf.org/doc/html/rfc2045)–[2049](https://datatracker.ietf.org/doc/html/rfc2049) | MIME structure | [rfc2045.txt](RFC/rfc2045.txt) … [rfc2049.txt](RFC/rfc2049.txt) |
| [5321](https://datatracker.ietf.org/doc/html/rfc5321) | SMTP error semantics (`523`, `554`) | [rfc5321.txt](RFC/rfc5321.txt) |
| [5322](https://datatracker.ietf.org/doc/html/rfc5322) | Header / envelope matching | [rfc5322.txt](RFC/rfc5322.txt) |
| [8446](https://datatracker.ietf.org/doc/html/rfc8446) | TLS 1.3 | [rfc8446.txt](RFC/rfc8446.txt) |
| [7301](https://datatracker.ietf.org/doc/html/rfc7301) | TLS ALPN extension | — |
| [9325](https://datatracker.ietf.org/doc/html/rfc9325) | TLS BCP 195 — strict ALPN/SNI vs ALPACA | — |
| [8555](https://datatracker.ietf.org/doc/html/rfc8555) | ACME (Let's Encrypt / DNS-01) | [rfc8555.txt](RFC/rfc8555.txt) |

## Raw IMAP/SMTP WebSocket transport

See [the bridge TDD](26-websocket-mail-bridge.md) and [operator/client usage](../guide/cli/websocket-bridge.md). The bridge is opt-in, TLS-only, and reuses native authenticated mail sessions. Coverage lives in `tests/websocket_bridge_e2e.rs`; run the isolated Docker scenario with `bash tests/websocket-bridge-docker.sh`.
