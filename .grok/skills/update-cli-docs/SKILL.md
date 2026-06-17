---
name: update-cli-docs
description: >
  Update the Madmail CLI operator reference in docs/guide/cli/ after adding,
  changing, or removing madmail subcommands. Also covers the man page
  (docs/man/madmail.1.scd), shell tab completion verification (clap-generated),
  JSON schemas, TDD parity matrix, and landing-site search index. Use when the
  user asks to update CLI docs, man page, completions, document a new madmail
  command, sync docs/cli with clap, or runs /update-cli-docs.
---

# Update CLI Documentation

Keep **`docs/guide/cli/`** in sync with the `madmail` CLI (clap definitions + `ctl/` handlers). This is the primary operator reference; it is published at `/docs/guide/cli/` on the landing site.

> **Path note:** People often say "docs/cli" — the canonical directory is `docs/guide/cli/`.

## 1. Orient — source of truth

Read these **before** editing docs:

| Layer | Path | What it defines |
|-------|------|-----------------|
| Command tree | `crates/chatmail-config/src/cli.rs` | Subcommands, aliases, global flags |
| Install / TLS CLI | `crates/chatmail-config/src/install_cli.rs` | `install`, `certificate` flags |
| Dispatch | `crates/chatmail/src/ctl/dispatch.rs` | Which commands are implemented vs `not_implemented` |
| Handler | `crates/chatmail/src/ctl/<module>.rs` | Behavior, prompts, JSON `data` fields |
| JSON envelope | `crates/chatmail/src/ctl/output.rs` | `--json` success/error format |
| Parity matrix | `docs/TDD/14-cli-tools.md` | Implementation status, `ctl/` module map |
| Madmail v1 reference | `context/madmail/docs/chatmail/commands.md` | Behavior parity (if present) |
| Man page source | `docs/man/madmail.1.scd` | Hand-written overview (synopsis, command list, options) |
| Man page render | `docs/man/madmail.1` | Generated roff, embedded at compile time |
| Tab completion | `crates/chatmail/src/ctl/docs.rs` | Auto-generated from clap via `clap_complete` |

## 2. Verify live CLI

Build and inspect help output; docs must match clap, not guesswork:

```bash
cargo build -p chatmail
./target/debug/chatmail --help
./target/debug/chatmail <command> --help
./target/debug/chatmail <command> <subcommand> --help
```

Production binary name is **`madmail`**; dev builds use **`chatmail`**. Write examples as `madmail …` everywhere in docs.

For JSON shapes, read the handler's `CtlOut::emit` / `done` calls or run with `--json` against a dev server when practical.

## 3. File layout

```
docs/guide/cli/
├── README.md              # Master index (sections + links)
├── global-flags.md        # --config, --state-dir, --json
├── json-output.md         # Per-command JSON schemas (anchors)
├── <command>.md           # Parent command (e.g. accounts.md, push.md)
├── <command>-<sub>.md     # Leaf subcommand (e.g. accounts-ban.md)
└── <alias>.md             # Thin alias page (e.g. ban-list.md → accounts ban-list)
```

**Naming:** kebab-case filenames mirror the CLI path: `madmail federation dismiss-list` → `federation-dismiss-list.md`.

## 4. Page templates

### Parent command page (`<command>.md`)

Use for commands with subcommands or rich overview. Include:

- `# \`<command>\`` title (backtick-wrapped command name)
- One-line description
- `## Synopsis` with fenced `bash` block
- `## Global flags` — copy table from [global-flags.md](global-flags.md) (parent pages only)
- `## Subcommands` table
- `## Examples`
- `## Notes` (behavior caveats, `madmail reload` reminders)
- `## Subcommand pages` bullet list linking to leaf pages
- `## JSON output` section → link to `json-output.md#<anchor>`
- Footer (see §5)

### Leaf subcommand page (`<command>-<sub>.md`)

Shorter; omit global flags table unless the subcommand is standalone.

```markdown
# `madmail <command> <sub>`

Parent: [`<command>`](<command>.md)

<one-line description>

## Synopsis

\`\`\`bash
madmail <command> <sub> [OPTIONS] <ARGS>
\`\`\`

## Options

| Option | Description |
|--------|-------------|
| `-y`, `--yes` | Skip confirmation prompt |

## Examples

\`\`\`bash
madmail <command> <sub> …
\`\`\`

## Notes

<behavior details from handler>

## JSON output (`--json`)

\`\`\`bash
madmail <command> <sub> --json
\`\`\`

Success stdout:

\`\`\`json
{"ok": true, "command": "<command> <sub>", "data": { ... }}
\`\`\`

Schema: [json-output.md](json-output.md#<anchor>).

---
[← `<command>`](<command>.md) · [CLI index](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/src/ctl/<module>.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/<module>.rs)
```

### Alias page (`<alias>.md`)

Point to canonical command; keep for discoverability and README alias table:

```markdown
# `<alias>`

Alias for [`<canonical>`](<canonical>.md).

\`\`\`bash
madmail <alias> …
\`\`\`

Same as `madmail <canonical> …`.

---
[← CLI index](README.md)
```

## 5. Footer conventions

Every page ends with:

```
---
[<nav links>](README.md) · [Global flags](global-flags.md)

[Source: `crates/chatmail/...`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/...)
```

- **Parent pages:** `[← CLI index](README.md) · [Global flags](global-flags.md)`
- **Leaf pages:** `[← \`parent\`](parent.md) · [CLI index](README.md) · [Global flags](global-flags.md)`
- **Source link:** point to the `ctl/` module that implements the command (or `cli.rs` for top-level-only commands).

## 6. Checklist by change type

### New subcommand (implemented)

- [ ] Create leaf page `<parent>-<sub>.md` from template
- [ ] Update parent `<parent>.md`: subcommands table, subcommand pages list, examples
- [ ] Add `### \`<parent> <sub>\`` section in `json-output.md` with real `data` fields
- [ ] Add link under correct section in `README.md`
- [ ] Update `docs/TDD/14-cli-tools.md` command index row (status → **done**, guide link, `ctl/` module)
- [ ] If alias: add alias page + row in README "Command aliases" table
- [ ] If DB-backed toggle: note "run `madmail reload`" in Notes
- [ ] If new **top-level** command or alias: update `docs/man/madmail.1.scd` DESCRIPTION section → `make man`
- [ ] Verify tab completion includes the command (see §13)

### Changed flags / behavior

- [ ] Update affected leaf + parent pages (Synopsis, Options, Examples, Notes)
- [ ] Update matching `json-output.md` section if `data` shape changed
- [ ] Re-run `--help` and spot-check against handler
- [ ] If global flags or top-level synopsis changed: update `docs/man/madmail.1.scd` OPTIONS/SYNOPSIS → `make man`

### New top-level command

- [ ] Add `Command` variant in `cli.rs` (user's code change — doc follows)
- [ ] Create `<command>.md` + leaf pages as needed
- [ ] Wire into `README.md` under the right category section
- [ ] Add `json-output.md` sections
- [ ] Add row to `docs/TDD/14-cli-tools.md` (both command index and category matrix)
- [ ] Add dispatch entry in `dispatch.rs` (implementation)
- [ ] Add entry to `docs/man/madmail.1.scd` DESCRIPTION (correct category subsection)
- [ ] Run `make man` and commit `docs/man/madmail.1`
- [ ] Verify tab completion (see §13); add/extend test in `docs.rs` if alias completion is non-obvious

### Planned / not implemented

- [ ] Page may exist with `*(planned)*` or `*(defer)*` in README — do **not** invent flags
- [ ] Match `not_implemented()` list in `dispatch.rs`
- [ ] Status in `14-cli-tools.md`: **planned** or **defer**

### Removed command

- [ ] Delete or mark deprecated in page + README
- [ ] Remove `json-output.md` section
- [ ] Update `14-cli-tools.md`

## 7. README.md index structure

`README.md` groups commands into fixed sections — place new entries in the right group:

1. Server lifecycle (`run`, `install`, `upgrade`, `reload`, …)
2. Admin & access (`admin-token`, `admin-web`, `certificate`)
3. Accounts & registration
4. Policy & delivery (`federation`, `endpoint-cache`, `sharing`, …)
5. Services & limits (`port`, `message-size`, `webimap`, `push`, `tasks`, …)
6. Web content (`html-export`, `html-serve`)
7. IMAP tooling
8. Utilities

Mark unimplemented entries: `### [\`creds\`](creds.md) *(planned)*`

Also maintain the **Command aliases** table at the top when adding `visible_aliases`.

## 8. json-output.md anchors

Anchor IDs are GitHub-style slugs of the heading text:

| Heading | Anchor |
|---------|--------|
| `### accounts create` | `#accounts-create` |
| `### admin-web enable` / `disable` / `path` | `#admin-web-enabledisablepath` |

Leaf pages link: `json-output.md#accounts-create`. When adding a section, place it under the matching category heading (Server lifecycle, Admin & access, …).

Document:
- Full success envelope example with realistic `data`
- Special cases from `json-output.md` "Exceptions" table (`admin-token --raw`, `create-user --json-only`, etc.)

## 9. Cross-references

Update links in other docs **only when the command is central to that topic**:

| Doc | When to touch |
|-----|---------------|
| `docs/TDD/14-cli-tools.md` | Always — parity matrix |
| `docs/TDD/<feature>.md` | Feature mentions CLI (e.g. `23-push-notifications.md` → `push.md`) |
| `docs/project/user-guide/07-admin-and-cli.md` | High-level overview only — not per-flag reference |
| `docs/man/madmail.1.scd` | New top-level commands, aliases, global OPTIONS/SYNOPSIS changes |
| `docs/guide/cli/completion.md` | Only if completion install paths or hidden helpers change |

Do not duplicate the full CLI reference into the user guide; link to `docs/guide/cli/README.md` instead.

## 10. Landing site search index

After adding or renaming doc files, regenerate the documentation tree (picks up new `docs/guide/cli/*.md` automatically):

```bash
cd landing && bun run docs:tree
```

This updates `landing/src/lib/assets/documentation.json` and `search-index.json`. Commit those if the landing site is deployed from repo assets.

## 11. Quality bar

- **Examples must be copy-pasteable** — real flag names from `--help`
- **No invented flags** — if not in clap, don't document it
- **Consistent binary name** — `madmail` in prose; mention `chatmail` only for dev builds
- **Reload reminder** — DB-backed settings (`push`, `webimap`, `admin-web`, `port`, …) need `madmail reload`
- **Destructive ops** — document `-y` / `--yes` and confirmation behavior
- **Username expansion** — note `@` domain expansion when handler does it
- Match existing tone: concise tables, short Notes, no filler

## 12. Man page (`docs/man/madmail.1.scd`)

The man page is a **hand-written summary**, not generated from clap. The rendered `docs/man/madmail.1` is committed and **embedded at compile time** (`crates/chatmail/src/ctl/docs.rs`).

Update `.scd` when:

- Adding or removing a **top-level** command (or documenting a new alias like `pr` → `proxy`)
- Changing **global flags** (`OPTIONS` section) or common **SYNOPSIS** forms
- Changing install/completion behavior described in `NOTES` or `EXAMPLES`

Do **not** duplicate every subcommand flag in the man page — that detail lives in `docs/guide/cli/`. The man page lists command families and one-line descriptions (see existing `DESCRIPTION` subsections).

Regenerate after editing:

```bash
make man          # requires scdoc; writes docs/man/madmail.1
make man-lint     # groff smoke test (optional)
```

Commit both `madmail.1.scd` and `madmail.1`. On system install, `madmail install` and `madmail upgrade` install the embedded page to `/usr/share/man/man1/<binary>.1`.

## 13. Tab completion (shell TAB)

Completions are **auto-generated from clap** — there are no hand-edited completion script files in the repo.

| What | How |
|------|-----|
| Generator | `clap_complete` in `crates/chatmail/src/ctl/docs.rs` |
| Operator command | `madmail completion bash\|zsh\|fish` |
| System install | `madmail install` writes to FHS paths (see `completion.md`) |
| Tests | `docs.rs` tests (e.g. `bash_completion_includes_proxy_subcommand`) |

After changing `cli.rs` (new commands, aliases, flag renames):

1. Rebuild: `cargo build -p chatmail`
2. Spot-check: `./target/debug/chatmail completion bash | grep <command>`
3. For new **aliases** (e.g. `pr` → `proxy`): confirm prefix completion works; add a test in `docs.rs` if non-trivial (see `bash_completion_prefix_pr_matches_proxy`)
4. Run: `cargo test -p chatmail bash_completion`

No separate "completion doc" update is needed unless install paths or hidden helpers (`generate-man`, `generate-fish-completion`) change — then edit `docs/guide/cli/completion.md`.

## 14. Quick workflow

```
1. Read cli.rs + ctl/<module>.rs (+ dispatch.rs for status)
2. Run --help on changed commands
3. Edit/create docs/guide/cli/<pages>.md
4. Update json-output.md + README.md
5. Update docs/TDD/14-cli-tools.md
6. Update docs/man/madmail.1.scd (top-level commands / global flags) → make man
7. Verify tab completion: completion bash + cargo test bash_completion
8. cd landing && bun run docs:tree
9. Spot-check links: README → leaf → json-output anchor → source link
```