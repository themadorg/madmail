# `install`: full setup and targeted repair

Use `madmail install` to bootstrap a new relay or repair selected host provisioning
for an existing Unix installation. This is the complete installer CLI reference.
For deployment, DNS and TLS background, see the [native install guide](../install.md).

## Contents

- [Choose the operation](#choose-the-operation)
- [New server with custom paths](#new-server-with-custom-paths)
- [Local bootstrap](#local-bootstrap)
- [Repair only the service account or systemd unit](#repair-only-the-service-account-or-systemd-unit)
- [All install options](#all-install-options)
- [Paths and global flags](#paths-and-global-flags)
- [Preview and JSON output](#preview-and-json-output)
- [Troubleshooting](#troubleshooting)

## Choose the operation

```bash
madmail install --help
madmail install [OPTIONS]
```

| Operation | Selection | Config / database / TLS | Account / permissions | Binary / CLI docs | systemd |
|-----------|-----------|-------------------------|-----------------------|-------------------|---------|
| Full Unix host setup | `--system --simple` with domain or IP | Writes config and seeds DB; runs TLS setup | Creates/repairs unless `--skip-user` | Installs | Writes unit and reloads unless `--skip-systemd` |
| Local bootstrap | `--local --simple` with domain or IP | Writes config and seeds DB; runs TLS setup | No host changes | No installation | No changes |
| Account repair | `--user-only` | Preserves contents | Creates/repairs account and applies permissions | Preserves | No changes |
| Unit repair | `--systemd-only` | Preserves contents | Preserves | Preserves | Replaces unit and reloads |

**Full installation can overwrite an existing config and regenerate self-signed
certificates.** Use repair modes when only host provisioning needs repair.
`--skip-user` and `--skip-systemd` omit individual steps of a full install; they do
not turn it into a repair operation. `--cert-only` stops after TLS setup but can
still create the service account, directories and DKIM keys.

Full bootstrap requires `--simple` or `--non-interactive`; interactive prompts are
not implemented. Repair modes need neither flag. Unix host changes require root;
`--dry-run` does not. No Unix installer mode enables or starts the service.

Without `--system` or `--local`, existing automatic selection remains: config under
`/etc/`, state under `/var/lib/`, or simple setup with default paths selects system
mode. Other custom paths select local mode, **even when invoked as root**. Prefer
explicit mode flags for predictable scripts. Repair flags always imply system mode.

Windows uses `--install-service`, `--start-service` and `--firewall`; Unix system
and repair modes are unsupported there. See [Windows setup](../../../packaging/windows/README.md).
If a flag is unrecognized, check `madmail version` and `madmail install --help`;
use a build containing these features before following its example.

## New server with custom paths

Use a binary named `madmail`. Replace the example domain and email, configure DNS
for the server, and allow inbound TCP 80 for ACME. First preview:

```bash
sudo ./madmail install --simple --system --domain mail.example.org \
  --acme-email admin@example.org --lang en \
  --config-dir /var/xyzzy/etc/madmail \
  --state-dir /var/xyzzy/lib/madmail \
  --binary-path /var/xyzzy/bin/madmail --dry-run
```

Run the same command without `--dry-run` to provision. It creates the service
account, applies permissions, installs the binary and CLI docs, and writes a unit
using these paths. The state directory may be a symlink to a data volume; it is
resolved once. The service account owns the state directory, so install treats
everything in it as untrusted: root never reads, writes, or changes the owner or
mode of a file as root through a link inside it (it only checks whether some
paths exist, which follows links). The database is opened with the service
account's effective user and group ids whenever the service account controls the
state directory, so a link there reaches only what the service account could
reach itself. Files are written under a new temporary name
and renamed into place, so a link or hard link planted at the target is
replaced, not written through. The files written there belong to the service
account; with `--skip-user`, only when the file they replace belongs to the
service account. See [Path safety](#path-safety). Before changing anything,
install refuses a `--state-dir` that is not a directory of its own: a well-known
system directory such as `/var/lib` or `/etc`, any directory directly under `/`
(such as `/data`), a user's home directory (`/home/<name>`), a directory below
`/etc`, `/usr`, `/lib`, `/bin`, `/sbin`, `/boot`, `/dev`, `/proc`, `/sys` or
`/run` that is not named for madmail (such as `/etc/systemd`; `/etc/madmail/data`
is allowed), a path that resolves to one of these (also through a link to a
directory that does not exist yet), an existing root-owned directory that is not
empty and not named for madmail (such as `/var/lib/dpkg`: it may hold another
program's files, and install would give every root-owned directory in it to
the service account; with `--skip-user` nothing is handed over and this rule
does not apply), an existing directory that belongs to another named account
(neither root nor the service account), and an existing directory whose owner
cannot be looked up. Use a directory of its own, such as `/var/lib/madmail`.
To name the owner, install asks the C library and, when it finds no account,
`getent passwd <uid>`, which uses the host's NSS configuration (LDAP, sssd and
so on): the release binaries are linked statically with musl, whose C library
reads only `/etc/passwd`. A lookup error refuses the directory. `getent` cannot
tell a directory service that is down from an account that does not exist, so
make sure such a service is reachable when you install: while it is down, a
directory of an account that only it knows looks like one of a uid without a
name. Install also stops when the service account exists but its groups cannot
be read, or when `id` finds the account and the groups lookup does not. Before
changing anything,
a full install refuses a symlinked or masked unit file (`systemctl unmask` it
first). If the service account already exists with a home elsewhere that still
exists, and the `--state-dir` already exists (for example a pre-created mount
point), it also refuses instead of leaving the data in the old home behind: move
the data if needed, run `--user-only` with the same paths to repoint the account,
then retry. A home that resolves to the same directory as `--state-dir` (for
example a symlinked `--state-dir`) is not "elsewhere". A missing `--state-dir` is
filled by moving the old home there. Then inspect and
start the service:

```bash
sudo systemctl cat madmail
sudo systemctl enable --now madmail
sudo systemctl status madmail
```

For the default layout omit the three path flags. For IP-only self-signed setup,
replace domain/email with `--ip YOUR_PUBLIC_IP --tls-mode self_signed`. For trusted
IP certificates, see [IP ACME setup](../../install-simple-ip-acme.md).

## Path safety

These rules apply to a system install and to `--user-only` repair when the
service account exists (install creates it first unless `--skip-user` is given).
They assume the service account may be compromised and may change its state
directory while install runs.

- Every path install writes into the state directory, and the paths of the
  config file, the TLS files and their directories and the binary, are resolved
  one component at a time. (The systemd unit, the man page and completions go
  into root-owned system directories and are written by path.)
  A directory or link on the way that is owned by the service account (or by
  the previous owner of the state directory), a directory writable by one of the
  service account's groups (primary or supplementary), or a world-writable
  directory without the sticky bit is never followed. When install would write
  through such a path (the config file, a generated certificate or key, the
  binary) or change the config file's owner, it stops with an error. A TLS file
  whose owner install only adjusts is left unchanged instead, and the output
  notes `Skipping ownership of <path>: <reason>`. Only mode bits are checked: a
  POSIX ACL that lets the service account write a directory is caught only when
  that directory's group is one of the service account's groups.
- A TLS path that reaches the state directory (it is below `--state-dir`, or a
  link on its path points into it) is not changed by path. A file there that
  belongs to the service account (or to the previous owner of the state
  directory) is handled by the ownership walk; install writes its own
  certificates and keys there with that owner. A root-owned certificate or key
  you copied into the state directory is left as it is, and the output notes
  `Skipping ownership of <path>: inside the state directory and belongs to uid 0, not to the service account, ...`
  with the steps to run after you check the file (see the ownership walk below).
  For a link there it notes
  `a link inside the state directory; links there are not followed`.
- A TLS file outside the state directory that belongs to the service account is
  given to it again. One that belongs to another account (root, for example) is
  given to the service account only when it has one hard link and, after your
  links are resolved, sits in a directory dedicated to madmail (see the config
  and certs directories below) or in `/etc/letsencrypt/live/<name>/` or
  `/etc/letsencrypt/archive/<name>/`. Otherwise it is left unchanged and the
  output notes
  `Skipping ownership of <path>: <file> belongs to uid <uid> and is not in a directory dedicated to madmail or in /etc/letsencrypt/live or archive, ...`
  with the `chown -h <user>:<group> <file>` command to run if it is your
  certificate or key. Install writes the certificates and keys it generates or
  obtains with the service account as owner.
- A config path given outside `--state-dir` as written that resolves to a path
  inside the state directory is refused, whether a link in the config path
  leads there or `--state-dir` is itself a link to where the config path leads.
  A config path below `--state-dir` as written is allowed.
- `--binary-path` must not be inside `--state-dir` and its directory must not
  resolve into the state directory; install refuses such a path before it
  changes anything. The binary is written as `root:root`, mode `0755`.
- Files install writes into the state directory (DKIM keys, generated
  certificates, the ACME account, a config file under `--state-dir`) belong to
  the service account from the moment they are created, so a run that stops at
  a later step leaves files the service can read.
- The state tree is walked by file descriptor. Links are skipped. A file
  (anything but a directory) is changed only when it belongs to the service
  account or to the previous owner of the state directory (after the account was
  recreated with a new uid); the link count does not matter. A directory is also
  changed when root owns it. Any other file or directory is listed as
  `Ownership of <path> left unchanged: belongs to uid <uid>, not to the service account; ...`
  (with `and has <n> hard links` when it has more than one). A directory listed
  this way is not entered, and the next line says
  `This directory was not entered: its contents were left unchanged and need manual inspection.`
  Each listed file is followed by the steps to give it to the service account:
  stop the service (`systemctl stop <unit>`) and every process of the service
  account: lock and expire the account, so that cron and at, which check the
  account with PAM, start no job for it (`usermod -L -e 1 <user>`); where cron
  or at is installed, remove its jobs (`crontab -r -u <user>`; `atq` lists the
  jobs, `atrm` removes them); kill its processes (`pkill -KILL -u <user>`);
  `ps -u <user>` must then list none. When the file's uid now belongs to
  another account, the steps do the same for that account. When you are done,
  end the expiry again (`usermod -e '' <user>`, and the same for the other
  account); the service unit does not use PAM, so the service starts while its
  account is locked and expired. Do not change the file if it has more than one
  hard link (`stat -c %h <path>`
  prints more than 1), and since one link does not prove that it is madmail's
  file, compare its content with what madmail writes there, or remove it and
  let madmail create it again; then run `chown -h <user>:<group> <path>`. The
  line ends with that command. `chown -h` does not follow a link at the file's
  own name, but the directories on the way to it are looked up again by name,
  which is why no process of those accounts may run, and a check of the
  running processes alone is not enough: a process can ignore `SIGTERM`, and
  cron or at can start a new one for an account whose password is locked.
  Check such a file (it may
  be a hard link to another account's file) and do this only if madmail needs
  it, for example for a database file that a root command created. The same
  steps follow a root-owned TLS file in the state directory and an in-tree file
  install refuses to read, but only when every directory from the state
  directory down to the file belongs to root or the service account and no
  other account can write it; otherwise the output says that the file needs
  manual inspection and prints no command. Mail
  files, which the store hard-links between folders, belong to the service and
  are changed. The state directory itself is changed last, and not at all when
  the walk stops with an error, so an interrupted run can be repeated.
- An existing state directory that belongs to another named account is refused
  before the account is created or changed:
  `it belongs to account <name> (uid <uid>), not to root or to the service account`.
  This happens when the service account was deleted and its old uid was given
  to another account. To keep the data, first stop the service and stop that
  account the same way as the service account above (`usermod -L -e 1 <name>`;
  `crontab -r -u <name>` and `atrm` for its `atq` jobs where cron or at is
  installed; `pkill -KILL -u <name>`; `ps -u <name>` must then list none): it
  owns the directories and files of that uid and could otherwise move them
  while you work. Then give the directory itself to root
  (`chown -h root:root <state dir>`) and run install again: the files and
  directories of that uid in it are then listed and left unchanged for you to
  check. A root-owned state directory that is not named for madmail must be
  empty, so move data in such a directory to one that is, such as
  `/var/lib/madmail`, first. List the files with
  `find <state dir> -xdev -uid <n> -ls`, check them, then hand over directories
  and single-link files with
  `find <state dir> -xdev \( -type d ! -uid 0 ! -uid <n> ! -user <user> -prune \) -o -uid <n> \( -type d -o -type f -links 1 \) -exec chown -h <user>:<group> {} +`.
  The `-prune` part keeps `find` out of a directory of any other account, as
  install's own walk stays out of it; check such a directory yourself. `find
  -exec` and `chown` look up every directory on the way again by name, so the
  service must be stopped and neither account may have a running process.
  Check files with more than one hard link one by one.
- Paths in the output are printed with control characters and bytes that are
  not UTF-8 escaped (`\n`, `\t`, `\xNN`, `\u{NNNN}`), so a file name in the
  state directory cannot add a line to the output. In the commands printed for
  you, such a path is quoted for the shell as `$'...'` with `\xNN` escapes; any
  other path that needs quoting is put in single quotes.
- On Linux, a file inside the state directory is opened with `O_PATH` and then
  reopened through `/proc/self/fd`, so `/proc` must be mounted. Without it,
  install stops with an error that says so.
- The config file ends with mode `0640`: owned by `root:<group>` outside the
  state directory, by the service account inside it. Its owner and mode are set
  after the ownership walk. Outside the state directory, a config file with more
  than one hard link is refused (also when the service account linked it into
  the state directory during the walk); inside it, a config file that belongs to
  another account is refused.
- The config and certs directories are re-owned to `root:<group>` and lose
  their group and other write bits only when they are dedicated to madmail
  (named `madmail`, `chatmail`, either followed by `-` or `_` and a suffix, or the
  binary name, and not a well-known directory such as `/etc` or `/usr/local/etc`;
  a `certs` directory directly inside such a directory also counts). The rule
  applies both to the path as given and to the directory it resolves to, and the
  resolved directory must not be one of the well-known directories (compared by
  device and inode). Any other directory keeps its owner and mode, and the
  output says so (`Leaving owner and mode of <dir> unchanged`).
- ACME certificates are obtained into a private temporary directory and then
  copied into place. When the service account controls the state directory,
  the install language is stored in the database with the service account's
  effective user and group ids, not as root (see the limit in
  `docs/TDD/12-security.md`).

With `--skip-user` and an existing service account, install still writes every
file through these checks; it only skips the account step and the ownership
changes (a rewritten config file keeps the previous file's owner). With
`--skip-user` and no such account, there is no service account to guard against:
files are written by path as root, and the config file is written with mode
`0640` without following a link at its own name. If the account is missing but
the state directory exists and is not owned by root, `--skip-user` is refused.
A local install writes by path as the calling user.

## Local bootstrap

```bash
./madmail install --local --simple --ip 127.0.0.1 \
  --config-dir /tmp/madmail-config --state-dir /tmp/madmail-state
```

This creates application files without host users, ownership changes, binary/docs
installation or systemd. It does not start the server or change generated listener
ports; configure unprivileged ports before running as an ordinary user. See
[local development](../../local-dev.md).

## Repair only the service account or systemd unit

For an existing Unix installation, use one of these mutually exclusive modes:

```bash
sudo ./madmail install --user-only \
  --config-dir /var/xyzzy/etc/madmail --state-dir /var/xyzzy/lib/madmail

sudo ./madmail install --systemd-only \
  --config-dir /var/xyzzy/etc/madmail --state-dir /var/xyzzy/lib/madmail \
  --binary-path /var/xyzzy/bin/madmail
```

Both imply system mode, even with custom paths. Use the same executable name and
paths as the original installation: the executable name determines the service
account, unit name and config filename (`madmail.conf` for `madmail`). These modes
use the supplied installer paths; they do not discover paths from config contents.
No `--simple`, `--non-interactive`, domain, IP or ACME options are needed.

- `--user-only` creates or repairs the account/home/group and applies config, TLS
  and state ownership. Config permissions become `0640`. Run while the service is
  stopped if its account needs repair. An existing account whose home differs is
  repointed at the existing state directory; nothing is moved. Ownership is only
  applied to TLS files at the given paths, so repeat custom `--cert-path` and
  `--key-path` values (the plan output lists the paths it will use). The state
  tree is walked by file descriptor and links in it are never followed or
  chowned through (see [Path safety](#path-safety)).
- `--systemd-only` replaces the unit and runs `systemctl daemon-reload`. It requires
  an existing executable binary and service user/group. Keep local unit changes in
  systemd drop-ins. It does not enable, start or restart the service. Repeat custom
  `--cert-path` and `--key-path` values here too: they decide the unit's
  `ProtectHome=` setting. The unit is written to a temporary file and renamed into
  place; a symlinked or masked unit file is refused rather than written through.

Both require an existing config file and state directory, and preserve the contents
of configuration, database, certificates, DKIM keys and binary. They do not install
CLI documentation. They cannot be combined with `--local`, `--cert-only`, either
skip flag, or Windows service/firewall flags. Unlike full installation, neither
repair mode seeds settings or obtains certificates.

Add `--dry-run` to preview without root or changes; live execution validates existing
files/accounts. Global `--json` reports the mode and planned actions. A failed step
returns nonzero; if daemon-reload fails, the written unit remains for inspection and
retry. These operations are repeatable but are not transactional.


For custom paths, run account repair first if the service identity is missing,
then unit repair. Stop an existing service before changing its account:

```bash
sudo systemctl stop madmail
# Run the appropriate repair command(s) above, then:
sudo systemctl start madmail
sudo systemctl status madmail
```

Skip `stop` if the unit does not exist yet. If you want automatic startup on boot,
run `sudo systemctl enable madmail` after the unit is installed.

## All install options

Defaults below describe Unix unless stated otherwise. Full-bootstrap feature/TLS
options do not configure the server during repair; use only the repair flag,
original paths, optional custom certificate paths, and output/preview options.

| Flag | Short | Argument | Default | Description |
|------|-------|----------|---------|-------------|
| `--simple` | `-s` | — | off | Quick setup: requires `--ip` or `--domain`. Enables chatmail blocks, Shadowsocks in config, TURN, and TLS auto-detection. |
| `--non-interactive` | `-n` | — | off | Non-interactive install for scripts. Requires `--domain` when not using `--simple`. |
| `--domain` | | string | — | Mail primary domain (DNS hostname). With `--simple --domain`: must **not** be an IP — use `--ip` for IP installs. |
| `--hostname` | | string | same as domain | Server hostname (`$(hostname)` in config). SMTP EHLO, TLS SANs, etc. |
| `--ip` | | string | — | Public IP (IPv4 or IPv6). With `--simple --ip`: sets wrapped primary domain `[IP]`, hostname, and `public_ip`. With domain install, omit to auto-detect from DNS (**public IPv4 preferred** over IPv6 when both exist; use this flag to force a specific address). |
| `--config-dir` | | path | `/etc/madmail` | Directory for `madmail.conf` and `certs/`. Overrides default layout; affects systemd unit when paths differ from defaults. |
| `--state-dir` | | path | `/var/lib/<binary>` for `--simple` / `/etc` config | Database, queues, `admin_token`, autocert state. |
| `--tls-mode` | | `autocert` \| `file` \| `self_signed` | auto | Force TLS mode; see [TLS selection](../install.md#tls). |
| `--cert-path` | | path | `{config-dir}/certs/fullchain.pem` | Certificate file for `tls file` / HTTPS |
| `--key-path` | | path | `{config-dir}/certs/privkey.pem` | Private key file |
| `--acme-email` | | email | `admin@<domain>` for DNS autocert | Let's Encrypt account contact |
| `--enable-chatmail` | | — | on with `--simple` or `--domain` | Emit `chatmail { … }` HTTP/HTTPS blocks in config |
| `--enable-ss` | | — | on with `--simple` | Add Shadowsocks `ss_addr` / `ss_password` / `ss_cipher` to chatmail blocks |
| `--turn-off-tls` | | — | off (on for `--simple --ip` without `--auto-ip-cert`) | Disable TLS requirements for chatmail/IMAP/SMTP in generated config |
| `--lang` | | `en` \| `fa` \| `ru` \| `es` | `en` | Website/UI language; seeded into DB as `__LANGUAGE__` |
| `--system` / `--local` | | — | auto | Select Unix system installation or local bootstrap independently of paths. |
| `--dry-run` | | — | off | Print resolved paths and exit before any writes; skips root check |
| `--skip-systemd` | | — | off | Do not write systemd unit or run `daemon-reload` |
| `--skip-user` | | — | off | Skip account creation/repair and config/TLS/state ownership setup; other full-install steps still run |
| `--binary-path` | | path | `/usr/local/bin/<binary>` | Destination for binary copy on system install |
| `--obtain-certificate` | | — | **on** | Issue Let's Encrypt cert for `autocert`, or `file` mode with missing PEMs |
| `--auto-ip-cert` | | — | off | Use Let's Encrypt **IP** certificate with `--simple --ip` |
| `--user-only` | | — | off | Unix: repair only the existing installation's account and permissions; implies system mode. |
| `--systemd-only` | | — | off | Unix: replace only the systemd unit and reload systemd; implies system mode. |
| `--enable-iroh` | | — | off | Add Iroh relay and IMAP discovery settings to generated config. |
| `--no-obtain-certificate` | | — | off | Skip ACME issuance; takes precedence over `--obtain-certificate`. Does not suppress self-signed generation. |
| `--cert-only` | | — | off | Stop after certificate setup; can still create the system account, directories and DKIM keys. |
| `--http-listen` | | address:port | `0.0.0.0:80` | HTTP-01 listener; public ACME validation must reach port 80. |
| `--install-service` | | — | off | Windows: register service after install; notice only on Unix. |
| `--start-service` | | — | off | Windows: register and start service after install; notice only on Unix. |
| `--firewall` | | — | off | Windows: apply mail/HTTP firewall rules after install; notice only on Unix. |
| `--help` | `-h` | — | — | Show options supported by this binary. |


## Paths and global flags

The executable basename determines the account, unit and config filename:
`./madmail` uses `madmail`, `madmail.service` and `madmail.conf`. A versioned download
should be invoked through a `madmail` filename or symlink. `--binary-path` sets the
installed executable location, not the identity used for these names.

| Input | Installer behavior |
|-------|--------------------|
| `--config-dir` | Writes or locates `<binary>.conf` here; default `/etc/madmail`. |
| Install `--state-dir` | Explicit persistent-state path; use it for custom layouts. Default `/var/lib/<binary>` for simple/default system setup. |
| Global `--json` | Print a final JSON envelope after installer progress; see below. |
| Global `--config` / `CHATMAIL_CONFIG` | Does not select the installer output or repair config; use `--config-dir`. |
| Global `--state-dir` / `--libexec` / `CHATMAIL_STATE_DIR` | Can supply fallback state in non-simple/custom-config cases; explicit install `--state-dir` avoids fallback ambiguity. |
| `--binary-path` | Copy target for full system setup; existing executable referenced by unit repair. Default `/usr/local/bin/<binary>`. |
| `--cert-path`, `--key-path` | Override default files under `<config-dir>/certs/`. Both repair modes take the original values: account repair applies ownership to existing files at these paths, and unit repair derives `ProtectHome=` from them. |

System config/state/binary paths must be absolute and contain no control characters.
Repair does not infer custom paths from config contents. Windows defaults are
`%ProgramData%\Madmail\config` and `%ProgramData%\Madmail\data`.

## Preview and JSON output

```bash
./madmail --json install --systemd-only --dry-run \
  --config-dir /var/xyzzy/etc/madmail --state-dir /var/xyzzy/lib/madmail \
  --binary-path /var/xyzzy/bin/madmail
```

Dry-run reports paths, mode and intended operations; it does not prove that live
prerequisites, account commands or daemon-reload will succeed. Full domain bootstrap
can still resolve DNS while planning. Repair performs no DNS or certificate setup.

Install prints human progress followed by a final JSON envelope when `--json` is
selected; **stdout is not a single JSON document**. Scripts must check the exit
status and extract the envelope. See [all install response fields](json-output.md#install).
Failures return nonzero. Steps are not transactional: for example, a unit can be
written successfully before daemon-reload fails.

## Troubleshooting

| Symptom | Explanation and next step |
|---------|---------------------------|
| Custom-path setup skipped user, binary or unit | It selected local mode. For a new bootstrap use `--system`; for existing files use the appropriate repair mode. |
| `repair requires an existing config file and state directory` | Check the executable basename and original `--config-dir` / `--state-dir`. Repair does not bootstrap missing application files. |
| Missing user/group or systemd `217/USER` | Stop the service if present, run `--user-only` with the original paths, then retry. |
| Missing unit or wrong `ExecStart` paths | Ensure the account, config, state and installed binary exist; run `--systemd-only` with all original paths. |
| `... is a symlink (masked?)` | The unit path is a symlink, for example from `systemctl mask`. Full install checks this before any other step; unit repair before writing. Run `systemctl unmask <name>` (or remove the link) and retry. |
| `refusing to use state directory ...: it is a directory below /etc, /usr or another system directory` | The state directory is inside the system's own files. Use a directory of its own, such as `/var/lib/madmail`, or one named for madmail. |
| `refusing to use state directory ...: cannot look up the owner (uid ...)`, `cannot read the groups of account ...` or `the account database disagrees about ...` | The C library or `getent` reported an error, or `id` and the groups lookup disagree about the service account. Fix the lookup and retry; install does not guess. |
| `refusing to use state directory ...: it belongs to root, is not empty and is not named for madmail` | The directory may hold another program's files. Use an empty directory or one named for madmail, such as `/var/lib/madmail`. |
| `... already exists; a full install would leave the data in ... behind` | The account's home differs from an existing `--state-dir`. Move the data into the new state directory if needed, run `--user-only` with the same paths to repoint the account, then rerun the full install. |
| `systemd-only requires an existing executable binary` | Supply the correct installed executable and ensure executable permissions; unit repair does not copy it. |
| `systemctl daemon-reload failed` | Inspect the written unit and systemd availability, fix the cause, then retry. Use local bootstrap in environments without host systemd. |
| Repair succeeds but service is stopped | Start it explicitly. Neither repair mode enables, starts or restarts services. |
| Unit customization disappears | Unit repair replaces the generated unit. Put overrides in systemd drop-ins. |

Metrics remain disabled by default; enable them after setup through
[`monitor enable`](monitor.md#enable-or-disable-monitoring).

[← CLI index](README.md) · [Native install guide](../install.md) · [Docker](../docker.md)

[Implementation](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/install/mod.rs)
