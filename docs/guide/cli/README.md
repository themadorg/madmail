# Madmail CLI reference

The `madmail` binary is a single executable that both **runs the mail server** and provides **operator tools** (Madmail/maddy compatible). In development builds the crate binary may be named `chatmail`; production installs use `madmail`.

```bash
madmail --help
madmail <command> --help
```

## Global flags

Every subcommand accepts:

| Flag | Alias | Env | Default |
|------|-------|-----|---------|
| `--config` | — | `CHATMAIL_CONFIG` | `/etc/madmail/madmail.conf` |
| `--state-dir` | `--libexec` | `CHATMAIL_STATE_DIR` | `/var/lib/madmail` |
| `--json` | — | — | off (machine-readable stdout) |

Details: [global-flags.md](global-flags.md) · [JSON schemas](json-output.md)

## Command aliases

| Alias | Canonical command |
|-------|-------------------|
| [`dns-cache`](dns-cache.md) | [`endpoint-cache`](endpoint-cache.md) |
| [`reg-tokens`](reg-tokens.md) | [`registration-tokens`](registration-tokens.md) |
| [`tokens`](tokens.md) | [`registration-tokens`](registration-tokens.md) |
| [`update`](update.md) | [`upgrade`](upgrade.md) |
| [`ban-list`](ban-list.md) | [`accounts ban-list`](accounts-ban-list.md) |
| [`create-user`](create-user.md) | [`accounts create-random`](accounts-create-random.md) |
| [`pr`](pr.md) | [`proxy`](proxy.md) |

Port service aliases: `submission_tls` → `submission-tls`, `imap_tls` → `imap-tls`, `ss` → `shadowsocks`.

## Server lifecycle

### [`run`](run.md)


### [`install`](install.md)


### [`uninstall`](uninstall.md)


### [`upgrade`](upgrade.md)


### [`update`](update.md)


### [`reload`](reload.md)


### [`completion`](completion.md)


### [`version`](version.md)


### [`status`](status.md)

## Admin & access

### [`admin-token`](admin-token.md)


### [`admin-web`](admin-web.md)

- [`disable`](admin-web-disable.md)
- [`enable`](admin-web-enable.md)
- [`path`](admin-web-path.md)
- [`status`](admin-web-status.md)

### [`certificate`](certificate.md)

- [`autocert`](certificate-autocert.md)
- [`autocert enable`](certificate-autocert-enable.md)
- [`autocert status`](certificate-autocert-status.md)
- [`get`](certificate-get.md)
- [`regenerate`](certificate-regenerate.md)
- [`status`](certificate-status.md)

## Accounts & registration

### [`accounts`](accounts.md)

- [`ban`](accounts-ban.md)
- [`ban-list`](accounts-ban-list.md)
- [`create`](accounts-create.md)
- [`create-random`](accounts-create-random.md)
- [`delete`](accounts-delete.md)
- [`delete-all`](accounts-delete-all.md)
- [`export`](accounts-export.md)
- [`import`](accounts-import.md)
- [`info`](accounts-info.md)
- [`status`](accounts-status.md)
- [`unban`](accounts-unban.md)

### [`ban-list`](ban-list.md)


### [`blocklist`](blocklist.md)

- [`add`](blocklist-add.md)
- [`list`](blocklist-list.md)
- [`remove`](blocklist-remove.md)

### [`create-user`](create-user.md)


### [`delete`](delete.md)


### [`registration`](registration.md)

- [`close`](registration-close.md)
- [`open`](registration-open.md)
- [`status`](registration-status.md)

### [`registration-tokens`](registration-tokens.md)

- [`create`](registration-tokens-create.md)
- [`delete`](registration-tokens-delete.md)
- [`list`](registration-tokens-list.md)
- [`status`](registration-tokens-status.md)

### [`creds`](creds.md) *(planned)*

## Policy & delivery

### [`federation`](federation.md)

- [`allow`](federation-allow.md)
- [`block`](federation-block.md)
- [`dismiss`](federation-dismiss.md)
- [`dismiss-flush`](federation-dismiss-flush.md)
- [`dismiss-list`](federation-dismiss-list.md)
- [`flush`](federation-flush.md)
- [`list`](federation-list.md)
- [`policy`](federation-policy.md)
- [`remove`](federation-remove.md)
- [`status`](federation-status.md)
- [`undismiss`](federation-undismiss.md)

### [`endpoint-cache`](endpoint-cache.md)

- [`get`](endpoint-cache-get.md)
- [`list`](endpoint-cache-list.md)
- [`remove`](endpoint-cache-remove.md)
- [`set`](endpoint-cache-set.md)

### [`sharing`](sharing.md)

- [`create`](sharing-create.md)
- [`edit`](sharing-edit.md)
- [`list`](sharing-list.md)
- [`remove`](sharing-remove.md)
- [`reserve`](sharing-reserve.md)

### [`queue`](queue.md) *(planned)*


### [`exchanger`](exchanger.md) *(planned)*


### [`submission-access`](submission-access.md) *(planned)*

## Services & limits

### [`port`](port.md)

- [`status`](port-status.md)
- [`smtp`](port-smtp.md)
- [`submission`](port-submission.md)
- [`submission-tls`](port-submission-tls.md)
- [`imap`](port-imap.md)
- [`imap-tls`](port-imap-tls.md)
- [`turn`](port-turn.md)
- [`sasl`](port-sasl.md)
- [`iroh`](port-iroh.md)
- [`shadowsocks`](port-shadowsocks.md)
- [`http`](port-http.md)
- [`https`](port-https.md)

### [`message-size`](message-size.md)

- [`reset`](message-size-reset.md)
- [`set`](message-size-set.md)
- [`status`](message-size-status.md)

### [`webimap`](webimap.md)

- [`disable`](webimap-disable.md)
- [`enable`](webimap-enable.md)
- [`status`](webimap-status.md)

### [`websmtp`](websmtp.md)

- [`disable`](websmtp-disable.md)
- [`enable`](websmtp-enable.md)
- [`status`](websmtp-status.md)

### [`push`](push.md)

- [`auto`](push-auto.md)
- [`off`](push-off.md)
- [`on`](push-on.md)
- [`status`](push-status.md)

### [`proxy`](proxy.md)

Alias: [`pr`](pr.md).

- [`status`](proxy-status.md)
- [`enable`](proxy-enable.md)
- [`disable`](proxy-disable.md)
- [`cipher`](proxy-cipher.md)
- [`cipher status`](proxy-cipher-status.md)
- [`cipher set`](proxy-cipher-set.md)
- [`cipher reset`](proxy-cipher-reset.md)
- [`password`](proxy-password.md)
- [`password status`](proxy-password-status.md)
- [`password set`](proxy-password-set.md)
- [`password reset`](proxy-password-reset.md)

### [`language`](language.md)

- [`reset`](language-reset.md)
- [`set`](language-set.md)
- [`status`](language-status.md)

### [`tasks`](tasks.md)

- [`list`](tasks-list.md)
- [`run`](tasks-run.md)
- [`run-all`](tasks-run-all.md)

## Web content

### [`html-export`](html-export.md)


### [`html-serve`](html-serve.md)


### [`html-migrate`](html-migrate.md)

## IMAP tooling

### [`imap-acct`](imap-acct.md) *(planned)*


### [`imap-mboxes`](imap-mboxes.md) *(planned)*


### [`imap-msgs`](imap-msgs.md) *(planned)*

## Utilities

### [`hash`](hash.md) *(planned)*


### [`migrate-pgp-config`](migrate-pgp-config.md) *(planned)*

## Typical workflow

1. [install](install.md) the server
2. `systemctl enable --now madmail` or `madmail run`
3. [admin-token](admin-token.md) to log into the admin UI
4. Change settings via CLI or web admin
5. [reload](reload.md) to apply DB-backed changes
6. [upgrade](upgrade.md) for signed binary updates

## Related docs

- [Native install guide](../install.md)
- [Docker guide](../docker.md)
- [CLI tools (TDD)](../../TDD/14-cli-tools.md) — implementation parity, `ctl/` module map, Madmail Go references
- [TDD index](../../TDD/README.md) — architecture and crate map

[Source: `crates/chatmail-config/src/cli.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail-config/src/cli.rs)
