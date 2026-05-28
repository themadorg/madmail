# Phase 1 (B1) — Sprint backlog

One markdown file per implementation step. Each step links to [TDD](../../TDD/README.md) sections and [RFC](../../TDD/RFC/README.md) references where they apply.

## Steps

| Step | File | Summary |
|------|------|---------|
| P1-S01 | [P1-S01-workspace-init.md](P1-S01-workspace-init.md) | Cargo workspace + pinned deps |
| P1-S02 | [P1-S02-ci-pipeline.md](P1-S02-ci-pipeline.md) | GitHub Actions (fmt, clippy, test) |
| P1-S03 | [P1-S03-shared-errors.md](P1-S03-shared-errors.md) | `chatmail-types` / `ChatmailError` |
| P1-S04 | [P1-S04-cli-args.md](P1-S04-cli-args.md) | `clap` global flags |
| P1-S05 | [P1-S05-static-config.md](P1-S05-static-config.md) | TOML + Madmail `.conf` parsing |
| P1-S06 | [P1-S06-sqlite-setup.md](P1-S06-sqlite-setup.md) | SQLx pool + `init_db` |
| P1-S07 | [P1-S07-base-schema.md](P1-S07-base-schema.md) | Embedded migration SQL |
| P1-S08 | [P1-S08-apply-migrations.md](P1-S08-apply-migrations.md) | Migrate on boot, WAL, FK |
| P1-S09 | [P1-S09-settings-crud.md](P1-S09-settings-crud.md) | Settings DAO |
| P1-S10 | [P1-S10-bool-settings.md](P1-S10-bool-settings.md) | `get_bool_setting` |
| P1-S11 | [P1-S11-tracing-nolog.md](P1-S11-tracing-nolog.md) | Reloadable `tracing` / No-Log |
| P1-S12 | [P1-S12-boot-sequence.md](P1-S12-boot-sequence.md) | `main.rs` lifecycle |
| P1-S13 | [P1-S13-admin-token.md](P1-S13-admin-token.md) | `admin_token` file (0600) |
| P1-S14 | [P1-S14-integration-test.md](P1-S14-integration-test.md) | `tests/boot_test.rs` |

## Tests (unit + integration)

| Test ID | Step | Crate / module |
|---------|------|----------------|
| P1-UT01 | P1-S04 | `chatmail-config::cli` |
| P1-UT02 | P1-S05 | `chatmail-config::parse` |
| P1-UT03 | P1-S08 | `chatmail-db::lib` |
| P1-UT04 | P1-S09 | `chatmail-db::settings` |
| P1-UT05 | P1-S10 | `chatmail-db::settings` |
| P1-UT06 | P1-S09 | `chatmail-db::settings` |
| P1-UT07 | P1-S13 | `chatmail::admin` |
| P1-UT08 | P1-S11 | `chatmail::logging` |
| P1-IT01 | P1-S14 | `tests/boot_test.rs` |

Run all Phase 1 tests:

```bash
cargo test --workspace
# 22 unit tests + 1 integration test (as of Phase 1 completion)
```

## Design references

- **TDD index**: [`docs/TDD/README.md`](../../TDD/README.md)
- **Phase 1 dev guide**: [`docs/Phase1.md`](../../Phase1.md)
- **Local run**: [`docs/local-dev.md`](../../local-dev.md)
- **Full phase overview** (archived summary): [phase-1-implementation-plan.md](phase-1-implementation-plan.md)

## Module mapping

| Path | Purpose | Madmail analogue |
|------|---------|------------------|
| `Cargo.toml` (root) | Workspace, dependency pins | `go.mod` |
| `crates/chatmail-types` | `ChatmailError`, shared types | `framework/exterrors` |
| `crates/chatmail-config` | CLI + static config | `framework/config/`, `cmd/maddy/main.go` |
| `crates/chatmail-db` | SQLite pool, migrations, settings DAO | `internal/db/gormsqlite`, `internal/table/sql_table.go` |
| `crates/chatmail-db/migrations/` | Embedded schema | GORM `AutoMigrate` |
| `crates/chatmail/src/logging.rs` | No-Log / `tracing` reload | `docs/chatmail/nolog.md` |
| `crates/chatmail/src/admin.rs` | Admin bearer token file | `chatmail.go` `ensureAdminToken` |
| `crates/chatmail/src/main.rs` | Boot sequence | `maddy.go` `Run()` |
