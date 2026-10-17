# Configuration precedence and deployment safeguards

Issue: #217; supersedes conflicting precedence statements in TDD 13 and project 06.

## Contract

For each operational setting, the first explicitly supplied value wins:
**environment → file → database → built-in default**. Explicit startup flags
select paths or override their documented startup-only values. Database-backed
CLI commands are mutations, not startup overrides. `false`, zero (where valid),
and empty lists are explicit values, not absence.

A shared registry defines setting names, database keys, validation, and environment
names (`CHATMAIL_<UPPERCASE_SETTING>`). Both TOML top-level fields and a `.conf`
`settings { name value }` block support registry settings. Existing module-specific
`.conf` directives remain supported. Unknown TOML keys and unknown `settings`
directives fail with actionable errors. Legacy module blocks remain compatible.

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

Previously DB values often won. Explicit file values now protect those settings;
existing DB rows remain intact but inactive. Removing the higher-priority source
and restarting exposes the stored value again. Generated templates should comment
optional values where operators are expected to use CLI/UI management. Enabling
admin-web must not persist a default path. Retention duration alone does not enable
purging: message_retention_enabled must explicitly be true from some source.

## Verification plan

- Registry: every supported key, environment mapping, invalid bool/port/duration,
  path normalization, secrets excluded from Debug/metadata/error messages.
- Parsers: TOML/.conf parity, aliases, false vs absent, unknown settings, correct
  block placement, relay min/max validation and retention enablement.
- Storage: SQLite and PostgreSQL-compatible access; single/batch/list resolution;
  file/env conflicts; guarded set/reset; independent pools and cloned snapshots.
- CLI: nonzero protected writes, no partial compound writes, service snapshot with
  a different environment, offline behavior, restart replacing stale protections.
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
