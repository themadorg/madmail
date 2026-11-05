# Configuration precedence and deployment safeguards

Issue: #217; supersedes conflicting precedence statements in TDD 13 and project 06.

## Contract

For each operational setting, the first explicitly supplied value wins:
**environment → explicit file setting → database → module directive → built-in
default** (explicit sources are defined below). Explicit startup flags
select paths or override their documented startup-only values. Database-backed
CLI commands are mutations, not startup overrides. `false`, zero (where valid),
and empty lists are explicit values, not absence.

A shared registry defines setting names, database keys, validation, and environment
names (`CHATMAIL_<UPPERCASE_SETTING>`). Both TOML top-level fields and a `.conf`
`settings { name value }` block support registry settings. Unknown TOML keys and
unknown `settings` directives fail with actionable errors.

**Only explicit sources protect a setting:** a `CHATMAIL_<NAME>` variable,
`CHATMAIL_{SMTP,IMAP,HTTP}_ADDR` (which protects that service's `*_port` and
`*_local_only`), a `settings { }` entry, or a top-level TOML key that is a registry
name. Module-specific directives (listener lines, `alpn_imap` / `alpn_smtp`,
`hostname`, `imap { turn_* iroh_relay_url }`, `turn { realm secret relay_* }`,
`chatmail { ss_* language admin_path admin_web_path allow_inbound_remote_rcpt }`,
`storage.imapsql { retention appendlimit }`, `max_message_size`) and TOML legacy
fields (`smtp_listen`, `turn_enable`, `retention`, ...) remain file defaults that a
database value overrides, as before. `madmail install` writes listener, TURN, proxy
and Iroh directives, so this rule keeps a fresh install's CLI and panel settings
writable. A port setting never implies a bind mode: `*_local_only` is protected only
when set explicitly or through `CHATMAIL_*_ADDR`.

Effective value order: protected value → database → module directive → built-in
default. The SMTP identity is the exception it was before: it uses a protected
`smtp_hostname` or the `hostname` directive, never a database row.

Each DB connection carries an immutable policy snapshot. Runtime setting reads
(single, batch and list) resolve protected values before consulting persisted rows.
Raw storage access is explicitly named and reserved for migration/inspection.
Writes and resets reject protected keys before modifying storage. Multi-setting
operations preflight every affected key before any write.

The admin API exposes effective source and protection metadata without secret
values. Mutations return a 409 configuration_locked error in the existing API
response envelope. CLI mutations exit nonzero. UI controls display their source
and disable editing when protected. Authentication remains required.

## Lifetime and offline commands

File/environment changes require a process restart. Database changes retain their
existing immediate/reload/restart semantics. HTTP route reload resolves admin_path
through the same setting accessor as status and CLI URLs. Failed route validation
must not move the UI to an unusable API path.

The server publishes source-only protection metadata in its state directory using
atomic replacement. CLI commands also honor this server snapshot, including when
the CLI does not inherit systemd environment variables. The snapshot remains
conservative after shutdown: remove a deployment lock by changing its environment
or file source and restarting the service. It contains no values or credentials.
A CLI with no published snapshot uses its locally loaded configuration. This is an
operator safeguard, not a defense against an administrator editing the DB directly.

At process startup, emit one stderr warning per protected setting naming only the
setting and controlling source. systemd captures stderr in its journal. These are
operator configuration diagnostics, an explicit exception to No-Log's suppression
of routine traffic logs; no mail data, credentials or environment values are logged.

## Migration

Existing configurations keep their behaviour: module directives, including lines
older templates wrote uncommented (`retention 24h`, `appendlimit 100M`,
`max_message_size 100M`, `language <code>`, `turn_enable yes`), stay below database
values and lock nothing, so an upgrade does not silently freeze CLI/panel settings.
The legacy `retention` line never drove purging and still only labels the info page.
Protection is opt-in through `settings { }`, top-level TOML registry keys, or the
environment; while protected, existing DB rows remain intact but inactive, and
removing the source and restarting exposes them again. Enabling admin-web must not
persist a default path. Admin path writes (CLI and API) validate the value, its
character set, and overlap with the effective other path before any write. Retention duration alone does not enable
purging: message_retention_enabled must explicitly be true from some source.

## Verification plan

- Registry: every supported key, environment mapping, invalid bool/port/duration,
  path normalization, secrets excluded from Debug/metadata/error messages.
- Parsers: TOML/.conf parity, false vs absent, unknown settings, correct block
  placement, relay min/max validation and retention enablement; listener lines,
  `alpn_*` and every directive the install template writes (current and older
  templates) produce no protection.
- Storage: SQLite and PostgreSQL-compatible access; single/batch/list resolution;
  file/env conflicts; guarded set/reset; independent pools and cloned snapshots.
- CLI: nonzero protected writes, no partial compound writes, service snapshot with
  a different environment, offline behavior, restart replacing stale protections;
  on a generated config, `port … set/local/reset`, `shared-port`, `proxy password`
  / `cipher`, `message-size` and `language` succeed; status output separates a
  protected value from a stored DB override.
- API: authenticated source metadata, 409 conflicts, unchanged DB after rejection,
  custom admin path remount and old path behavior, auth at the new endpoint.
- Runtime: admin-web reset/enable/file path regression, TURN ranges and retention
  reaching consumers, invalid configuration rejected before listener activation.
- UI: disabled controls, source explanations, effective path after save/reset,
  normal editing when no deployment lock exists.
- Documentation: update TDD 13/21, project 06/10/11, operator and CLI guides;
  distinguish historical plans from current contracts.
- Required checks: fmt, workspace clippy with warnings denied, workspace tests,
  admin-web type check/tests/build; record environment-dependent failures honestly.

## Implementation verification (2026-10-16)

- `cargo fmt --all -- --check`: passed.
- `cargo clippy --workspace --all-targets --offline -- -D warnings`: passed.
- `cargo test --workspace --offline --no-fail-fast -- --test-threads=1`:
  1,080 passed, zero failed, three ignored. Serial execution isolates the suite's
  existing process-environment mutations. Release-signing tests used an isolated
  Python environment with `cryptography`; integration tests had local networking.
- Admin web: Svelte type check reported zero errors/warnings; six Bun tests passed;
  the Vite static production build passed.
- Parent and UI-submodule `git diff --check`: passed.

Regression coverage includes TOML/block-format parity, environment precedence,
explicit false, invalid settings and overlapping paths, secret-free metadata,
SQLite single/batch/list resolution, rejected writes/resets preserving stored
values, compound CLI/API safeguards, CLI service snapshots, retention consumption,
and HTTP requests to a custom admin endpoint with authentication and lock errors.

These checks do not certify a production deployment. Live PostgreSQL tests require
`MADMAIL_TEST_PG_DSN`; actual systemd journal capture was not run. Docker
verification is recorded below. UI validation covers helpers, types and compilation, not browser automation.
The startup diagnostics use stderr for systemd capture. Existing documentation
links into optional `context/` reference checkouts require those checkouts; new
local configuration-guide links resolve in this repository.

### Docker verification follow-up

`bash tests/docker-test.sh` passed against a newly built shipping image:

- Workspace: 1,080 passed, zero failed, three normally ignored tests.
- Landing-site tests passed.
- Fresh install, HTTPS and embedded admin dashboard passed.
- Metrics enable/disable/restart, authentication and CLI sampling passed.
- Multi-domain registration, IMAP/SMTP login and domain isolation passed.
- SMTP TLS/STARTTLS delivery, IMAP retrieval, wrong-password and plaintext
  rejection, and message persistence after restart passed.
- The separately invoked ignored `docker_turn_live_allocate` test passed:
  IMAP TURN metadata and actual allocation within the configured relay range.

An additional isolated-container smoke check passed for ENV > file > DB, protected
retention, API set/reset conflicts, CLI snapshot enforcement with the relevant ENV
removed, source-only startup warnings, custom authenticated routes, and restart.
This was an ad hoc check rather than browser automation. Its harness refreshed
Docker's dynamically assigned host port after restart. Temporary containers and
volumes were removed. Logs: `target/docker-tests/run-3JRtooEI/` (generated, untracked).

### TOML / .conf / ENV matrix

`python3 tests/configuration-sources-docker.py` passed for both formats against the
shipping image. The tests use the exact operator-guide snippets. Each format passed
seven phases: editable DB fallback; file precedence and protected writes/resets;
explicit file false; ENV precedence and false; ENV-only locks with an independently
invoked CLI and restart; removal of protections restoring original DB values and
editing; and rejection of invalid ENV/file booleans, unknown keys, and reversed TURN
ranges. The checked values include admin web enable/path, API path, retention and
TURN relay bounds. This reusable script supersedes the earlier ad hoc source check.

## Review follow-up (#219)

The first implementation derived protection from module directives. On a fresh
install the generated listener lines and `alpn_*` locked `smtp_port`,
`smtp_local_only`, `imap_port`, `https_port`, `shared_port_*`, and the TURN, proxy
and Iroh lines locked `turn_port`, `ss_password`, `ss_cipher` and more; an older
template's `retention 24h` overrode the database retention. Protection now comes
only from the explicit sources above. Regression tests:
`ctl::install::tests::generated_config_keeps_cli_settings_writable` (renders the
install template, also with the older uncommented lines, and runs the CLI against
it), `settings_policy::tests::*` (listener/alpn/legacy directives, `settings` block,
`CHATMAIL_*_PORT` versus `CHATMAIL_*_ADDR`, TURN range, admin path overlap and
character set), `parse::policy_tests::toml_listen_and_legacy_fields_are_defaults_registry_keys_lock`,
admin `protected_setting_without_row_is_not_reported_as_set` and
`admin_web_path_overlap_uses_file_api_path`, CLI `rejected_paths_write_nothing`,
`proxy_settings_protected_only_by_explicit_settings_block`, and the shared-port
status test. The Docker installer suite also runs `madmail port smtp set 2525`
after a real install.
