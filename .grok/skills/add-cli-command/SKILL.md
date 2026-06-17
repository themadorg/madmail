---
name: add-cli-command
description: >
  Add a new madmail CLI command end-to-end: clap definition, ctl handler with
  --json output, DB settings keys, dispatch wiring, unit tests, E2E tests,
  tab-completion verification, and operator documentation. Use when the user
  asks to add, implement, or create a new madmail/chatmail subcommand, wire a
  CLI tool, or runs /add-cli-command.
---

# Add CLI Command

Implement a new **`madmail`** operator command following existing patterns in this repo. Production binary: **`madmail`**; dev crate binary: same name (`cargo build -p chatmail` → `target/debug/madmail`).

**Do not ship a command without all four layers:** clap + handler + tests + docs.

---

## 0. Orient — read before coding

Study commands closest to what you are adding:

| Pattern | Read these |
|---------|------------|
| DB toggle (enable/disable/status) | `service_toggle.rs`, `registration.rs`, `cli.rs` `ServiceToggleCommand` |
| DB setting (status/set/reset) | `message_size.rs`, `language.rs`, `proxy.rs` |
| Nested subcommands | `port.rs`, `proxy.rs`, `federation.rs` |
| Destructive + confirmation | `delete_cmd.rs`, `accounts.rs` (`-y` / `--yes`) |
| Read-only status | `status_cmd.rs`, `push.rs` |
| Config-file only | `html.rs`, `version.rs` |

Also read:

| File | Purpose |
|------|---------|
| `crates/chatmail-config/src/cli.rs` | Command tree, global `Args`, parse tests |
| `crates/chatmail/src/ctl/dispatch.rs` | Routing, `not_implemented` list |
| `crates/chatmail/src/ctl/output.rs` | `--json` envelope (`CtlOut`) |
| `crates/chatmail/src/ctl/context.rs` | `CtlContext`, `open_pool()` |
| `crates/chatmail/src/ctl/test_harness.rs` | Unit-test helpers |
| `crates/chatmail/src/ctl/ops_tests.rs` | Dispatch test examples |
| `tests/ctl_cli_e2e.rs` | Subprocess + JSON E2E |
| `docs/TDD/14-cli-tools.md` | Parity matrix |

Check Madmail v1 parity if present: `context/madmail/docs/chatmail/commands.md`.

---

## 1. Pick the implementation pattern

| Type | When to use | Reuse |
|------|-------------|-------|
| **Service toggle** | Single `__FOO_ENABLED__` bool | `service_toggle::run()` — only if label fits `webimap`/`websmtp` pattern; otherwise copy `proxy.rs` enable/disable |
| **Setting group** | `status` / `set` / `reset` on one DB key | `proxy.rs` `setting_set` / `setting_reset` / `setting_status` |
| **Custom handler** | Anything else | New `ctl/<name>.rs` module |

**Naming conventions:**

| Item | Rule | Example |
|------|------|---------|
| CLI name | kebab-case, Madmail parity | `message-size`, `admin-token` |
| Rust module | snake_case | `message_size.rs` |
| Clap variant | PascalCase | `MessageSize` |
| Settings key | `__SCREAMING_SNAKE__` | `__SS_ENABLED__` in `settings_keys.rs` |
| Doc page | kebab-case path | `proxy-cipher-set.md` |
| `CtlOut` command string | space-separated CLI path | `"proxy cipher set"` |

---

## 2. Clap definition (`chatmail-config`)

**File:** `crates/chatmail-config/src/cli.rs`

1. Add variant to `Command` enum with doc comment.
2. Add subcommand enum(s) if nested.
3. Use existing attributes consistently:
   - `#[command(name = "kebab-name")]` when Rust variant differs
   - `#[command(subcommand_required = false)]` + `Option<SubCmd>` for default `status`
   - `visible_aliases = ["alias"]` for short names
   - `#[arg(short, long)] yes: bool` for destructive ops
4. Export new types from `crates/chatmail-config/src/lib.rs` `pub use cli::{...}` when handlers need them.

**Add parse tests** in `cli.rs` `#[cfg(test)]` or `ops_tests.rs`:

```rust
let cli = parse_cli(dir.path(), &["proxy", "cipher", "set", "aes-256-gcm"]);
assert!(matches!(
    cli.command,
    Some(Command::Proxy {
        cmd: Some(ProxyCommand::Cipher {
            cmd: Some(ProxySettingCommand::Set { value })
        })
    }) if value == "aes-256-gcm"
));
```

Test aliases (`pr` → `proxy`) when added.

---

## 3. Settings key (if DB-backed)

**File:** `crates/chatmail-db/src/settings_keys.rs`

```rust
pub const MY_SETTING: &str = "__MY_SETTING__";
```

Only add keys stored in the `settings` table at runtime. Use `set_setting`, `get_setting`, `get_bool_setting`, `delete_setting` from `chatmail_db`.

Wire the key into the server reload path if the running daemon must pick it up (see how existing keys are read in the relevant crate).

---

## 4. Handler module (`chatmail::ctl`)

**Files:**
- `crates/chatmail/src/ctl/<name>.rs` — new module
- `crates/chatmail/src/ctl/mod.rs` — `mod <name>;`
- `crates/chatmail/src/ctl/dispatch.rs` — import + `match` arm + `command_name()` arm

**Handler skeleton:**

```rust
pub async fn my_cmd(args: &Args, cmd: Option<&MyCmdCommand>) -> Result<()> {
    let ctx = CtlContext::from_args(args)?;
    let pool = ctx.open_pool().await?;
    let out = CtlOut::from_args(args, "my-cmd status");

    match cmd {
        None | Some(MyCmdCommand::Status) => { /* ... */ }
        Some(MyCmdCommand::Set { value }) => { /* ... */ }
    }
}
```

**Rules:**
- Never `println!` directly — always `CtlOut`.
- Always branch on `out.is_json()` for status commands with formatted human output.
- Return `Err(ChatmailError::config(...))` for operator errors; `main` prints JSON error to stderr when `--json`.
- Include reload hints in human text and `reload_required: true` in mutation JSON when `madmail reload` is needed.
- Never echo passwords/secrets in JSON (see `proxy password status`).

---

## 5. `--json` output (mandatory)

Every implemented subcommand must support global `--json`. Use `CtlOut`:

| Method | Use for |
|--------|---------|
| `out.emit(data)` | Status / read-only (JSON only prints; human uses `line`/`blank`) |
| `out.done(human, data)` | Simple mutation without `message` field |
| `out.done_msg(human, data, message)` | Mutations — adds `"message"` to envelope |
| `out.aborted()` | User declined `-y` confirmation |

**Success envelope (stdout):**

```json
{"ok": true, "command": "proxy status", "data": { ... }}
{"ok": true, "command": "proxy enable", "message": "Shadowsocks proxy enabled", "data": { "enabled": true, "reload_required": true }}
```

**Error envelope (stderr):**

```json
{"ok": false, "error": "..."}
```

**`command` string is part of the contract** — match it exactly in tests and `json-output.md`. Examples from real handlers:

- `"push status"`, `"proxy cipher set"`, `"webimap"` (toggle mutations use service name)

**Status command pattern:**

```rust
if out.is_json() {
    return out.emit(json!({ "enabled": on, "reload_required": false }));
}
out.blank();
out.line(format!("  Foo: {}", if on { "enabled" } else { "disabled" }));
out.line("  Apply to a running server: madmail reload");
out.blank();
Ok(())
```

**Mutation pattern:**

```rust
set_setting(pool, key, &value).await?;
out.done_msg(
    format!("✅ Setting updated (madmail reload)"),
    json!({ "value": value, "reload_required": true }),
    "Setting updated",
)
```

**Destructive ops:**

```rust
if !confirm(&format!("Delete {u}?"), yes)? {
    return out.aborted();
}
```

---

## 6. Dispatch wiring

**File:** `crates/chatmail/src/ctl/dispatch.rs`

1. `use super::<module>;`
2. Match arm: `Some(Command::MyCmd { cmd }) => my_cmd::my_cmd(&cli.args, cmd.as_ref()).await`
3. `command_name()` arm for the top-level name
4. Remove command from `not_implemented()` message list once implemented
5. Update `docs/TDD/14-cli-tools.md` status → **done**

For simple toggles, wire directly:

```rust
Some(Command::Webimap(cmd)) => service_toggle::run(
    &cli.args, settings_keys::WEBIMAP_ENABLED, "WebIMAP HTTP API", cmd,
).await,
```

---

## 7. Tests (mandatory)

### A. Clap parse tests

Assert `Cli::try_parse_from` / `parse_cli` produces correct enum shape. Include alias coverage.

**Run:** `cargo test -p chatmail-config` and `cargo test -p chatmail ctl`

### B. Dispatch unit tests (in-process)

**Files:** `crates/chatmail/src/ctl/ops_tests.rs` (settings/port/proxy), `dispatch_tests.rs` (accounts/destructive)

Use `test_harness.rs`:

```rust
#[tokio::test]
async fn dispatch_my_cmd_set_and_reset() {
    let (dir, _args, _db, pool) = setup_ctl_env().await;

    let cli = parse_cli(dir.path(), &["my-cmd", "set", "value"]);
    dispatch(&cli).await.unwrap();
    assert_eq!(
        get_setting(&pool, settings_keys::MY_SETTING).await.unwrap().as_deref(),
        Some("value")
    );

    let cli = parse_cli(dir.path(), &["my-cmd", "reset"]);
    dispatch(&cli).await.unwrap();
    assert!(get_setting(&pool, settings_keys::MY_SETTING).await.unwrap().is_none());
}
```

**Assert:**
- DB state (`get_setting`, `get_bool_setting`, SQL)
- Error messages: `dispatch(&cli).await.unwrap_err().to_string().contains("expected")`
- Preconditions (e.g. `proxy enable` fails when not configured)
- Validation rejects bad input

For config-dependent commands, use `write_ss_test_config()` pattern or write a similar helper.

**Do not** rely on stdout in unit tests — assert DB/filesystem side effects.

### C. `--json` unit or E2E tests

At minimum one test per command family verifying JSON envelope shape:

```rust
let cli = parse_cli_with_config(dir.path(), &config, &["my-cmd", "status", "--json"]);
// Or E2E:
let envelope: Value = serde_json::from_slice(&stdout).unwrap();
assert_eq!(envelope["ok"], true);
assert_eq!(envelope["command"], "my-cmd status");
assert!(envelope["data"]["enabled"].is_boolean());
```

E2E pattern in `tests/ctl_cli_e2e.rs`:

```rust
chatmail()
    .args(state_argv(&state))
    .args(["my-cmd", "status", "--json"])
    .assert().success();
```

Add E2E when the command has subprocess-visible behavior worth guarding (JSON contract, stdout markers).

**Run:** `cargo test -p chatmail-integration --test ctl_cli_e2e --test ctl_ops_e2e`

### D. Tab completion

Completions are auto-generated from clap. After adding commands/aliases:

```bash
cargo build -p chatmail
./target/debug/madmail completion bash | grep my-cmd
cargo test -p chatmail bash_completion
```

Add assertions in `crates/chatmail/src/ctl/docs.rs` when the command is top-level or has a non-obvious alias (see `bash_completion_includes_proxy_subcommand`, `bash_completion_prefix_pr_matches_proxy`).

---

## 8. Documentation (mandatory)

Follow the **`update-cli-docs`** skill (`.grok/skills/update-cli-docs/SKILL.md`) for the full doc workflow.

**Minimum checklist for a new implemented command:**

- [ ] `docs/guide/cli/<command>.md` parent page
- [ ] Leaf pages for each subcommand (`<command>-<sub>.md`)
- [ ] `docs/guide/cli/json-output.md` sections with real `data` fields from handler
- [ ] `docs/guide/cli/README.md` entry in correct category section
- [ ] `docs/TDD/14-cli-tools.md` → **done**, module name, settings keys
- [ ] If top-level command or alias: `docs/man/madmail.1.scd` → `make man`
- [ ] `cd landing && bun run docs:tree` (search index)
- [ ] Cross-link related docs (e.g. feature TDD) when central to the topic

**Verify help matches docs:**

```bash
cargo build -p chatmail
./target/debug/madmail <command> --help
./target/debug/madmail <command> <sub> --help
```

Never invent flags not present in clap.

---

## 9. Final verification

Run before considering the command done:

```bash
cargo build -p chatmail
./target/debug/madmail <command> --help

# Unit tests
cargo test -p chatmail ctl
cargo test -p chatmail-config

# E2E (if added)
cargo test -p chatmail-integration --test ctl_cli_e2e --test ctl_ops_e2e

# Completion
cargo test -p chatmail bash_completion

# Quality gate (repo standard)
make lint
```

Manually spot-check `--json` on at least one status and one mutation subcommand.

---

## 10. Common pitfalls

| Pitfall | Fix |
|---------|-----|
| Parses but `not implemented` | Forgot `dispatch.rs` match arm |
| Compile error | Forgot `mod.rs` registration |
| `--json` prints human text | Used `println!` instead of `CtlOut` |
| `--json` prints nothing on status | Missing `out.is_json()` branch |
| JSON contract broken | Wrong `CtlOut::from_args` command string |
| E2E hangs | Missing `-y` on destructive command |
| Secrets in JSON | Omit values; return `source` / `db_override` only |
| Docs out of sync | Re-run `--help`, update `json-output.md` anchors |
| `not_implemented` list stale | Update message in `dispatch.rs` |
| New toggle via `service_toggle` | Hardcoded `"webimap"`/`"websmtp"` labels — write custom handler if needed |

---

## 11. File checklist

| Step | Path |
|------|------|
| Clap | `crates/chatmail-config/src/cli.rs` |
| Re-exports | `crates/chatmail-config/src/lib.rs` |
| Handler | `crates/chatmail/src/ctl/<name>.rs` |
| Module | `crates/chatmail/src/ctl/mod.rs` |
| Dispatch | `crates/chatmail/src/ctl/dispatch.rs` |
| Settings | `crates/chatmail-db/src/settings_keys.rs` |
| Unit tests | `crates/chatmail/src/ctl/ops_tests.rs`, `dispatch_tests.rs` |
| E2E tests | `tests/ctl_cli_e2e.rs`, `tests/ctl_ops_e2e.rs` |
| Completion tests | `crates/chatmail/src/ctl/docs.rs` |
| CLI guide | `docs/guide/cli/*.md` |
| JSON schemas | `docs/guide/cli/json-output.md` |
| Parity | `docs/TDD/14-cli-tools.md` |
| Man page | `docs/man/madmail.1.scd` (top-level only) |

---

## 12. Quick workflow

```
1. Read similar existing command (table in §0)
2. Add clap variant + parse tests
3. Add settings key (if DB-backed)
4. Implement ctl/<name>.rs with --json on every subcommand
5. Wire dispatch.rs + mod.rs; update not_implemented list
6. Add ops_tests (DB assertions) + JSON test; E2E if warranted
7. Add completion test for top-level / alias
8. Run update-cli-docs workflow (pages, json-output, README, TDD, man, landing)
9. make lint && cargo test -p chatmail ctl
```

For doc-only details (page templates, anchors, man page rules), use **`/update-cli-docs`**.