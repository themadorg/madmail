# Version manager (`/opt/madmail`)

**Status:** design plan (not implemented)  
**Related code today:**

| Area | Location | Current behavior |
|------|----------|------------------|
| Install binary | `crates/chatmail/src/ctl/install/system.rs` | Copies live exe → `/usr/local/bin/madmail` |
| systemd unit | `crates/chatmail/src/ctl/install/systemd.rs` | `ExecStart=/usr/local/bin/madmail …` |
| Signed upgrade | `crates/chatmail/src/upgrade.rs` | In-place replace of `current_exe`; sibling `*.prev` only |
| Deploy scripts | `scripts/deploy.sh`, `scripts/deploy.defaults.sh` | `REMOTE_BIN=/usr/local/bin/madmail` |
| Version stamp | `.version`, workspace `Cargo.toml` | e.g. `2.20.0` |

**Operator CLI (planned):** [`../guide/cli/`](../guide/cli/README.md) — new `versions` / extend `upgrade` (see below).  
**Parity matrix:** [`14-cli-tools.md`](14-cli-tools.md).

---

## Problem

Upgrades keep **one** previous binary (`/usr/local/bin/madmail.prev`). That is enough for emergency rollback of the last step, but operators cannot:

- Keep a **history** of N known-good releases on disk
- Switch between versions without re-downloading
- Inspect **which** build is live vs archived
- Avoid filling `/usr/local/bin` with ad-hoc folders of old bins

Putting archives under `/usr/local/bin/` is the wrong place: that directory is for **PATH commands**, not version trees.

---

## Goals

1. Store versioned madmail binaries under **`/opt/madmail`** (FHS-friendly app root).
2. Keep **`/usr/local/bin/madmail`** as a stable PATH entry (symlink → active version).
3. On every successful **install** / **upgrade**, archive the previous binary with a **version id**.
4. Support **list**, **use** (switch), **prune**, and **rollback** without re-fetch.
5. Preserve existing safety: Ed25519 signature check, host preflight (`version`), systemd stop/start, automatic rollback if post-install smoke fails.
6. **Signature on every activation path:** any install/upgrade that writes a binary into the version tree **must** verify Ed25519 first; `versions use` **must** re-verify the on-disk binary before flipping the active symlink. Inventory commands (`list`, `current`, `path`, `prune`, `remove`) do **not** need a signature check.
7. **One-shot “get latest from GitHub”:** `madmail update latest` downloads the current release asset from GitHub Releases, then runs the **same** URL-upgrade security pipeline (TLS, size caps, archive rules, **mandatory Ed25519 signature check**, host preflight, version-tree install, smoke/rollback) as an explicit download URL. **GitHub does not replace signature verification** — unsigned or bad-signed assets must always abort.
8. Migrate smoothly from the current single-file install.

### Non-goals (v1)

- Multi-arch side-by-side on one host (one primary arch per install tree).
- Storing full release tarballs / Windows artifacts under `/opt` (Linux server binary only).
- Replacing GitHub Releases / `publish.sh` as the distribution channel.
- Per-user non-root installs (upgrade/install remain root-owned system tools).

---

## Layout

```text
/opt/madmail/
  current -> versions/2.20.0          # symlink (optional convenience)
  versions/
    2.19.0/
      madmail                        # executable (mode 0755)
      meta.json                      # optional install metadata
    2.20.0/
      madmail
      meta.json
  state/                             # reserved; do NOT put maildir here
                                     # (runtime state stays under InstallConfig.state_dir,
                                     #  typically /var/lib/madmail)

/usr/local/bin/madmail -> /opt/madmail/versions/2.20.0/madmail
# or -> /opt/madmail/current/madmail
```

### Version directory naming

| Rule | Detail |
|------|--------|
| Primary id | Semver from binary (`madmail version` / embedded package version), e.g. `2.20.0` |
| Collision | Same semver rebuilt → use `2.20.0+YYYYMMDDHHMMSS` or `2.20.0+git.<shortsha>` if available |
| Invalid chars | Reject path separators; only `[0-9A-Za-z._+-]` |
| Active pointer | Atomic symlink replace for `current` and `/usr/local/bin/madmail` |

### `meta.json` (optional but recommended)

```json
{
  "version": "2.20.0",
  "installed_at": "2026-08-03T12:00:00Z",
  "source": "upgrade",
  "source_url": "https://github.com/themadorg/madmail/releases/download/v2.20.0/madmail-linux-amd64",
  "sha256": "…",
  "variant": "linux-amd64",
  "signature_ok": true
}
```

---

## Runtime and PATH contract

| Consumer | Resolution |
|----------|------------|
| Shell / admin | `/usr/local/bin/madmail` on `PATH` |
| systemd | Prefer **stable path** in unit file: either keep `ExecStart=/usr/local/bin/madmail` (symlink) **or** `ExecStart=/opt/madmail/current/madmail` |
| `madmail upgrade` | Resolves **real** path via `current_exe` + canonicalize; installs new version **next to** version tree, then flips symlink |
| `*.prev` | Deprecated after migration; keep one generation for compatibility or map to “previous version dir” |

**Recommendation:** keep `ExecStart=/usr/local/bin/madmail` so existing units and `REMOTE_BIN` defaults keep working; only the symlink target changes.

---

## Commands (planned CLI)

Surface either as top-level group or under `upgrade` — prefer a dedicated group for clarity:

```text
madmail versions list
madmail versions current
madmail versions use <version>
madmail versions prune [--keep N] [--yes]
madmail versions remove <version>   # refuse if active
madmail versions path [version]     # print filesystem path
```

### Behavior sketch

| Command | Behavior |
|---------|----------|
| `list` | Enumerate `/opt/madmail/versions/*`; mark active; show size, mtime, meta |
| `current` | Print active version id + resolved path |
| `use V` | **Must** re-verify Ed25519 signature on `versions/V/madmail`, then preflight `version`, stop services, flip symlinks, start services; on smoke failure flip back. Refuse switch if signature fails (treat as tampered / unsigned archive). |
| `prune --keep N` | Delete oldest non-active versions beyond N (default **N=5**). No signature check (does not execute or activate). |
| `remove V` | Error if V is active or only remaining version. No signature check. |
| `list` / `current` / `path` | Read-only; **no** signature check required. |

**Signature policy (activation vs inventory):**

| Operation | Ed25519 signature |
|-----------|-------------------|
| Install new binary into the version tree (`install` / `upgrade` from path or URL) | **Always required** before write/activate (same as today — never skip) |
| **`madmail update latest`** (and `upgrade latest`) | **Always required** after download/extract, **before** writing into `/opt/madmail/versions/` or stopping services. Same `verify_signature` as `perform_upgrade`. **Never skip** because the source is “official GitHub”. Fail closed: unsigned / invalid signature → abort, leave active version unchanged. |
| `versions use <V>` (activate an already-archived binary) | **Always required** on that on-disk file before stop/symlink flip |
| `versions list`, `current`, `path`, `prune`, `remove` | Not required (no execution / no new trust boundary) |

**JSON:** all commands support `--json` for automation (same style as other ctl commands).

### Upgrade / update UX

```text
madmail upgrade <path-or-url>     # install into versions/<new>, keep previous under versions/
madmail upgrade --rollback        # optional alias → versions use <previous>
madmail update <path-or-url>      # existing alias of upgrade (keep)
madmail update latest             # NEW: fetch latest GitHub release asset, then same secure pipeline
```

#### `madmail update latest` (required)

Resolve the **latest** published madmail binary from **GitHub Releases** and install it through the version manager — without the operator pasting a full asset URL.

| Item | Spec |
|------|------|
| Intent | “Give me the newest signed release for this host” |
| Source | GitHub only: `https://github.com/themadorg/madmail/releases` (not arbitrary mirrors) |
| Default asset | Prefer host-appropriate Linux server asset, e.g. `madmail-linux-amd64.tar.gz` (or musl / arch variant if detection already exists); document exact name table in operator CLI docs |
| Resolution strategy (v1) | Prefer stable redirect URL: `…/releases/latest/download/<asset>` (same family as existing test fixture URL). Optional later: GitHub Releases API for tag/metadata before download. |
| Entry point | Clap: `update` subcommand accepts either a path/URL **or** the keyword `latest` (not a free-form path). `upgrade latest` should behave the same for parity. |
| After resolve | Call the **identical** code path as `handle_update_url` → versioned `perform_upgrade` (no bypass of checks). |

**Security pipeline for `update latest` (must match URL upgrade — no weaker path):**

```text
1. resolve asset URL under github.com/themadorg/madmail/releases/… only
2. HTTPS download (default TLS verify; --accept-unsafe-https only if operator opts in — same as today)
3. reject unsupported archive types (only .tar.gz / .tgz or raw signed binary — same check_supported_url_archive rules)
4. enforce MAX_DOWNLOAD_SIZE (100 MiB today) on response body and on extracted member
5. extract madmail from tar.gz when needed (owner-only temp; path safety)
6. **verify Ed25519 signature on candidate (MANDATORY — refuse unsigned / invalid; do not proceed to install)**
7. host preflight: candidate `version` (captures version id; aborts on exec/loader failure)
8. install into /opt/madmail/versions/<VERSION>/, flip symlinks, smoke, rollback on failure
9. optional prune --keep N after success
```

| Rule | Detail |
|------|--------|
| **Signature** | **Always required** for `update latest`. Call the same `verify_signature` used by `perform_upgrade` / URL upgrades. Order: download → (extract) → **sig check** → preflight → version-tree install. Never skip for GitHub. |
| Binary / download size | Same caps as URL update (`MAX_DOWNLOAD_SIZE` on download stream and archive member) |
| Host version preflight | Same as `perform_upgrade` (after signature passes) |
| Already on latest | If active version id equals candidate’s version **and** content hash matches installed file, exit 0 with clear message (no service bounce). If same semver but different hash, install collision policy (`2.20.0+timestamp` or refuse — see version dir naming). Still run signature check on the downloaded candidate before deciding (do not trust GitHub alone). |
| Offline / GitHub down | Fail with actionable error; do not fall back to unsigned cache |
| `--json` | Report resolved URL (or tag), version id, previous version, **signature_ok**, success/rollback |

**Not in scope for `latest` alone:** pinning an older GitHub tag (use explicit URL or `versions use` for local archive). Optional later: `update v2.20.0` tag syntax.

---

## Upgrade algorithm (target)

Replace the current “overwrite file + `*.prev`” path in `perform_upgrade` with versioned install:

```text
1. verify Ed25519 signature on candidate
2. preflight: candidate `version` (captures VERSION string)
3. require root
4. ensure /opt/madmail/versions exists (mode root-owned 0755)
5. VERSION = parse from preflight output (fallback: meta / timestamp)
6. DEST = /opt/madmail/versions/VERSION/
   - if DEST exists and same content hash → skip copy or refuse
   - else install candidate → DEST/madmail (atomic write via temp + rename)
7. write meta.json
8. PREV = resolve current symlink target (or legacy /usr/local/bin/madmail file)
9. stop systemd units
10. atomic symlink update:
    - /opt/madmail/current → versions/VERSION
    - /usr/local/bin/madmail → versions/VERSION/madmail
11. smoke preflight on resolved path
12. on failure: restore previous symlink targets, start services, error
13. on success: start services; optional prune --keep N
14. post-upgrade hooks (www migrate, docs refresh) unchanged
```

### Migration from legacy install

| On-disk state | Action on first versioned upgrade/install |
|---------------|-------------------------------------------|
| Regular file `/usr/local/bin/madmail` | Copy into `versions/<oldver>/`, then convert PATH entry to symlink |
| `/usr/local/bin/madmail.prev` | Import as `versions/<prevver>/` if version parseable; else `versions/prev-import-<ts>/` |
| Already symlink into `/opt/madmail/…` | No-op migrate; continue versioned flow |
| Unit still points at old path | Symlink keeps unit valid; optional `systemctl daemon-reload` only if unit text changes |

---

## Install path changes

`InstallConfig.binary_path` today defaults to `/usr/local/bin/madmail`.

| Phase | Install behavior |
|-------|------------------|
| v1 | Install into `/opt/madmail/versions/<ver>/madmail`, create `/usr/local/bin/madmail` symlink, leave `binary_path` as the **symlink path** for unit generation |
| later | Optional flag `--binary-root /opt/madmail` for custom roots (test VMs, multi-instance) |

Dry-run (`install --dry-run`) must print both the version dir and the symlink plan.

---

## Deploy / ops

| Knob | Today | After |
|------|-------|--------|
| `REMOTE_BIN` | `/usr/local/bin/madmail` | Unchanged (symlink) |
| Unsigned deploy | `install -m 755` overwrites file | Prefer `madmail upgrade` or a small helper that writes into `/opt/madmail/versions/…` |
| Signed deploy | `madmail upgrade` | Same command; version manager is internal |
| Disk | N/A | Document prune policy; default keep 5 |

Local dev / CI: version manager is **production-install** concern; unit tests use a temp root via env, e.g. `MADMAIL_OPT_ROOT=/tmp/madmail-opt-test`.

---

## Configuration

| Mechanism | Key / env | Default |
|-----------|-----------|---------|
| Env (test/override) | `MADMAIL_OPT_ROOT` | `/opt/madmail` |
| Keep count | setting or flag `--keep` | `5` |
| Config file | optional later in `madmail.conf` | not required for v1 |

Do **not** store mailboxes or `chatmail.db` under `/opt/madmail`; state remains `/var/lib/madmail` (or configured `state_dir`).

---

## Security

| Concern | Mitigation |
|---------|------------|
| Untrusted binaries entering versions/ | **Always** Ed25519-verify the candidate before writing into `/opt/madmail/versions/` on install/upgrade (same as today; never skip) |
| `update latest` / URL download | Same size caps (`MAX_DOWNLOAD_SIZE`), archive allow-list, TLS defaults, then **mandatory Ed25519** + preflight; GitHub “latest” is **not** a trust shortcut and **never** waives signature verification |
| Tampered archived binary activated via `use` | **Always** re-run Ed25519 verify on `versions/V/madmail` before stop/symlink flip; refuse if verify fails |
| Inventory without crypto cost | `list` / `current` / `path` / `prune` / `remove` do not verify signatures |
| Symlink attacks | Create dirs as root; refuse to follow unexpected symlinks when writing DEST |
| PATH hijack | `/usr/local/bin/madmail` owned root:root, mode 755 symlink |
| Prune deletes active | Refuse remove/prune of active target |
| Rollback window | Keep at least previous version until next successful upgrade + prune |

### Activation order for `versions use V`

```text
1. resolve path = versions/V/madmail (reject missing / not a regular file)
2. verify_signature(path)  — hard fail if false (do not stop services)
3. preflight path version
4. stop systemd units
5. atomic symlink flip (current + /usr/local/bin/madmail)
6. smoke preflight on resolved path
7. on failure: restore previous symlink targets, start services, error
8. on success: start services
```

---

## Testing plan

| Layer | Cases |
|-------|--------|
| Unit | Version id sanitization; symlink atomic update helpers; meta parse; keep-N selection |
| Unit (upgrade) | Extend `upgrade.rs` tests: install root via `MADMAIL_OPT_ROOT` temp dir; no real systemd; signature always required on install |
| Unit (`use`) | Signed archive → switch OK; unsigned/tampered on-disk binary → refuse before service stop; no sig check on `list` |
| Unit (`update latest`) | Resolves expected GitHub latest URL for arch; rejects non-GitHub resolution; oversize download aborted; **unsigned / bad-signature payload fails after download and never flips symlink or stops services**; already-latest short-circuit still only after successful sig check on candidate |
| Integration | Legacy file → versioned tree migration; `use` switches active only after sig + preflight; failed smoke restores previous |
| E2E / manual | VM or docker: install → upgrade twice → `versions list` → `use` older → service healthy; optional `update latest` against real/mock GitHub |
| Deploy script | `REMOTE_BIN` still works when symlink |

---

## Implementation plan (PRs)

### PR1 — Layout helpers + docs

- Add module e.g. `crates/chatmail/src/ctl/versions.rs` (or `version_manager.rs`): paths, ensure dirs, read/write meta, list, resolve active.
- Env `MADMAIL_OPT_ROOT` for tests.
- This TDD file + stub operator page under `docs/guide/cli/versions.md` (can land with PR3).
- Unit tests for pure path logic.

### PR2 — Install writes version tree

- `install_binary`: copy into `/opt/madmail/versions/<ver>/madmail`, point `/usr/local/bin/madmail` symlink.
- systemd unit generation unchanged (`ExecStart` still symlink path).
- Migration: if existing real file at binary_path, import into versions first.

### PR3 — Upgrade uses version tree

- Change `perform_upgrade` to install-to-versions + flip symlink (algorithm above).
- Map rollback to previous version dir (keep `*.prev` import for one release if needed).
- CLI: `madmail versions list|current|use|prune|remove|path`.
- `versions use`: call same `verify_signature` as `perform_upgrade` **before** stop/preflight-activate; fail closed if unsigned/bad signature.
- Wire clap + `--json`; update parity matrix in `14-cli-tools.md`.

### PR4 — `update latest` (GitHub)

- Clap: `madmail update latest` and `madmail upgrade latest` resolve GitHub Releases latest asset for this host.
- Implement resolver → call existing `handle_update_url` / shared download helper → **`perform_upgrade` (or shared install that always calls `verify_signature`)** — **no** separate install path that skips size/sig/preflight.
- Hard requirement: signature failure aborts with the same class of error as a bad local upgrade (`INVALID SIGNATURE` / equivalent); active binary and services untouched.
- Unit tests with mock HTTP for size limit + **signature failure must be covered**; fixture URL shape matching `…/releases/latest/download/…`.
- Operator docs: preferred asset names, signature always verified, `--accept-unsafe-https` note (TLS only — **does not** skip Ed25519), “already latest” behavior.

### PR5 — Ops polish

- Default prune after upgrade (`--keep 5`, disable with `--keep 0` or `--no-prune`).
- `deploy.sh` notes / optional call path; document `madmail update latest` for signed remote updates.
- `upgrade-safety.sh` / smoke scripts assert version dirs when present.
- CHANGELOG + install guide blurb.

---

## Acceptance criteria

- [ ] Fresh `madmail install` produces `/opt/madmail/versions/<v>/madmail` and `/usr/local/bin/madmail` → that file.
- [ ] `madmail upgrade` adds a new version directory without deleting the previous one (until prune).
- [ ] Failed post-install smoke restores previous active version and services recover (same guarantees as today).
- [ ] `madmail versions list` / `use` work with `--json`.
- [ ] Install/upgrade **never** places a binary under `versions/` without a successful Ed25519 check.
- [ ] `versions use` **always** re-verifies signature; fails before stopping services if verify fails; `list`/`path`/etc. do not require it.
- [ ] `madmail update latest` fetches from GitHub Releases latest for this host and applies **full** URL-upgrade security (size cap, archive rules, **mandatory Ed25519 signature**, preflight, smoke/rollback) into the version tree.
- [ ] `madmail update latest` **never** activates or archives a binary that failed `verify_signature`; GitHub origin does not waive the check.
- [ ] Existing systemd units with `ExecStart=/usr/local/bin/madmail` need no edit for v1.
- [ ] Tests pass with `MADMAIL_OPT_ROOT` under tmp; no requirement to write real `/opt` in CI.

---

## Open questions

1. **Exact CLI name:** `versions` vs `version` vs `bin`?
2. **Keep count default:** 5 vs 3 vs unlimited until explicit prune?
3. **Should `current` symlink under `/opt` be required**, or only `/usr/local/bin/madmail`?
4. **Import policy for `madmail.prev`:** always import, or only when version string is recoverable?
5. **Multi-instance hosts:** one `/opt/madmail` vs `/opt/madmail-<instance>` (out of scope unless needed).
6. **`update latest` asset selection:** hardcode `madmail-linux-amd64.tar.gz` vs auto-detect arch/musl (prefer auto if reliable; document fallback).
7. **`update latest` vs Releases API:** redirect URL only (v1) vs API for tag display / multi-asset pick.

---

## References

- Current upgrade safety (issue #114 notes in `upgrade.rs`): preflight before stop; backup; post smoke; rollback.
- URL download path in `upgrade.rs`: `handle_update_url`, `MAX_DOWNLOAD_SIZE` (100 MiB), `check_supported_url_archive`, Ed25519 after extract.
- FHS: `/opt/<package>` for add-on application software; `/usr/local/bin` for local admin commands on PATH.
- Sibling TDD: [`14-cli-tools.md`](14-cli-tools.md), [`13-configuration.md`](13-configuration.md), [`12-security.md`](12-security.md).
- Deploy defaults: `scripts/deploy.defaults.sh` (`REMOTE_BIN`).
- Example latest asset shape: `https://github.com/themadorg/madmail/releases/latest/download/madmail-linux-amd64.tar.gz`.
