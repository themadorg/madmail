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
using these paths. Then inspect and start the service:

```bash
sudo systemctl cat madmail
sudo systemctl enable --now madmail
sudo systemctl status madmail
```

For the default layout omit the three path flags. For IP-only self-signed setup,
replace domain/email with `--ip YOUR_PUBLIC_IP --tls-mode self_signed`. For trusted
IP certificates, see [IP ACME setup](../../install-simple-ip-acme.md).

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
  stopped if its account needs repair. Custom TLS locations can be supplied using
  `--cert-path` and `--key-path`.
- `--systemd-only` replaces the unit and runs `systemctl daemon-reload`. It requires
  an existing executable binary and service user/group. Keep local unit changes in
  systemd drop-ins. It does not enable, start or restart the service.

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
| `--cert-path`, `--key-path` | Override default files under `<config-dir>/certs/`; account repair applies ownership to existing files at these paths. |

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
| `systemd-only requires an existing executable binary` | Supply the correct installed executable and ensure executable permissions; unit repair does not copy it. |
| `systemctl daemon-reload failed` | Inspect the written unit and systemd availability, fix the cause, then retry. Use local bootstrap in environments without host systemd. |
| Repair succeeds but service is stopped | Start it explicitly. Neither repair mode enables, starts or restarts services. |
| Unit customization disappears | Unit repair replaces the generated unit. Put overrides in systemd drop-ins. |

Metrics remain disabled by default; enable them after setup through
[`monitor enable`](monitor.md#enable-or-disable-monitoring).

[← CLI index](README.md) · [Native install guide](../install.md) · [Docker](../docker.md)

[Implementation](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/install/mod.rs)
