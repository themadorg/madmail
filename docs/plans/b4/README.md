# Phase 4 — SMTP & PGP gate

## Goal

Submission/inbound SMTP + enforce_encryption.

## TDD index

- [docs/TDD/README.md](../../TDD/README.md)
- [RFC library](../../TDD/RFC/README.md)

## Steps

| Step | File | Summary |
|------|------|---------|
| P4-S01 | [P4-S01-pgp-crate-init.md](P4-S01-pgp-crate-init.md) | pgp crate init |
| P4-S02 | [P4-S02-enforce-encryption.md](P4-S02-enforce-encryption.md) | enforce encryption |
| P4-S03 | [P4-S03-pgp-mime-check.md](P4-S03-pgp-mime-check.md) | pgp mime check |
| P4-S04 | [P4-S04-securejoin-bypass.md](P4-S04-securejoin-bypass.md) | securejoin bypass |
| P4-S05 | [P4-S05-smtp-crate-init.md](P4-S05-smtp-crate-init.md) | smtp crate init |
| P4-S06 | [P4-S06-smtp-tls.md](P4-S06-smtp-tls.md) | smtp tls |
| P4-S07 | [P4-S07-smtp-session-ehlo.md](P4-S07-smtp-session-ehlo.md) | smtp session ehlo |
| P4-S08 | [P4-S08-smtp-auth-plain.md](P4-S08-smtp-auth-plain.md) | smtp auth plain |
| P4-S09 | [P4-S09-smtp-mail-rcpt.md](P4-S09-smtp-mail-rcpt.md) | smtp mail rcpt |
| P4-S10 | [P4-S10-smtp-data.md](P4-S10-smtp-data.md) | smtp data |
| P4-S11 | [P4-S11-smtp-pgp-gate.md](P4-S11-smtp-pgp-gate.md) | smtp pgp gate |
| P4-S12 | [P4-S12-smtp-quota.md](P4-S12-smtp-quota.md) | smtp quota |
| P4-S13 | [P4-S13-smtp-deliver.md](P4-S13-smtp-deliver.md) | smtp deliver |

## Tests

See step files for P*-UT* / E2E mappings.
