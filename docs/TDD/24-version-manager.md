# Version manager (platform install root)

**Status:** partially implemented (layout helpers, CLI `versions`, `update latest` URL resolve, upgrade archives into version tree; install-path migration / service-polish still open)  
**Related code today:**

| Area | Location | Current behavior |
|------|----------|------------------|
| Install binary | `crates/chatmail/src/ctl/install/system.rs` | Unix: copies live exe → `/usr/local/bin/madmail`; Windows: install path under ProgramData-style layout |
| Service | `systemd.rs` (Unix); Windows service install | Unix: `ExecStart=/usr/local/bin/madmail …`; Windows: service ImagePath to installed exe |
| Signed upgrade | `crates/chatmail/src/upgrade.rs` | In-place replace of `current_exe`; sibling `*.prev` only |
| Deploy scripts | `scripts/deploy.sh`, `scripts/deploy.defaults.sh` | Linux: `REMOTE_BIN=/usr/local/bin/madmail` |
| Install roots | `ctl/install/mod.rs` | Windows config/state: `%ProgramData%\Madmail\{config,data}` via `windows_madmail_root()` — **no `/opt`** |
| Version stamp | `.version`, workspace `Cargo.toml` | e.g. `2.20.0` |

**Operator CLI (planned):** [`../guide/cli/`](../guide/cli/README.md) — new `versions` / extend `upgrade` (see below).  
**Parity matrix:** [`14-cli-tools.md`](14-cli-tools.md).

---

## Problem

Upgrades keep **one** previous binary (`/usr/local/bin/madmail.prev`). That is enough for emergency rollback of the last step, but operators cannot:

- Keep a **history** of N known-good releases on disk
- Switch between versions without re-downloading
- Inspect **which** build is live vs archived
- Avoid filling PATH directories (`/usr/local/bin`, or a Windows “bin” folder) with ad-hoc copies of old bins

Putting archives next to the PATH command is the wrong place: PATH is for the **stable launcher**, not the version tree. **Windows has no `/opt`** — use a Windows-native install root instead (below).

---

## Goals

1. Store versioned madmail binaries under a **platform install root** (not the PATH directory alone):
   - **Linux/Unix:** `/opt/madmail` (FHS add-on app root)
   - **Windows:** `%ProgramFiles%\Madmail` (default; no `/opt`)
2. Keep a **stable PATH / service entry** that points at the active version (symlink, junction, or small launcher — platform-appropriate).
3. On every successful **install** / **upgrade**, archive the previous binary with a **version id**.
4. Support **list**, **use** (switch), **prune**, and **rollback** without re-fetch.
5. Preserve existing safety: Ed25519 signature check, host preflight (`version`), service stop/start (systemd or Windows service), automatic rollback if post-install smoke fails.
6. **Signature on every activation path:** any install/upgrade that writes a binary into the version tree **must** verify Ed25519 first; `versions use` **must** re-verify the on-disk binary before flipping the active pointer. Inventory commands (`list`, `current`, `path`, `prune`, `remove`) do **not** need a signature check.
7. **One-shot “get latest from GitHub”:** `madmail update latest` downloads the current **host-matching** release asset (Linux or Windows), then runs the **same** URL-upgrade security pipeline (TLS, size caps, archive rules, **mandatory Ed25519 signature check**, host preflight, version-tree install, smoke/rollback). **GitHub does not replace signature verification** — unsigned or bad-signed assets must always abort.
8. Migrate smoothly from the current single-file install on **both** Unix and Windows.
9. **Same CLI semantics** on Windows and Unix (`versions list|use|…`, `update latest`); only roots and service integration differ.

### Non-goals (v1)

- Multi-arch side-by-side on one host (one primary arch per install tree).
- Storing full multi-platform release tarballs in the version tree (store **this host’s** extracted binary only: `madmail` or `madmail.exe`).
- Replacing GitHub Releases / `publish.sh` as the distribution channel.
- Per-user non-admin installs as the default (system install remains elevated: root / Administrator). Optional later: per-user root under `%LOCALAPPDATA%\Madmail`.

---

## Layout

Paths are resolved by a single helper, e.g. `install_root()` → env override or platform default. **Never hardcode `/opt` on Windows.**

### Platform defaults

| Role | Linux / Unix | Windows |
|------|----------------|---------|
| Install / version root | `/opt/madmail` | `%ProgramFiles%\Madmail` (e.g. `C:\Program Files\Madmail`) |
| Version dirs | `{root}/versions/<ver>/madmail` | `{root}\versions\<ver>\madmail.exe` |
| Optional `current` pointer | `{root}/current` → `versions/<ver>` (symlink) | `{root}\current` → `versions\<ver>` (**directory junction** or symlink; both need appropriate privileges) |
| Stable PATH / service binary | `/usr/local/bin/madmail` → active exe (symlink) | Prefer `{root}\bin\madmail.exe` → active exe, **or** re-point Windows service `ImagePath` to active path; add `{root}\bin` to machine PATH if desired |
| Runtime state (mail, DB) | `/var/lib/madmail` (or `state_dir`) | `%ProgramData%\Madmail\data` (existing `windows_madmail_root()` — **not** under Program Files version tree) |
| Config | `/etc/…` / configured path | `%ProgramData%\Madmail\config` |
| Env override (tests + custom) | `MADMAIL_INSTALL_ROOT` (alias: `MADMAIL_OPT_ROOT` on Unix for backward naming) | Same `MADMAIL_INSTALL_ROOT` (e.g. `C:\Temp\madmail-install-test`) |

### Linux tree

```text
/opt/madmail/
  current -> versions/2.20.0          # symlink (optional convenience)
  versions/
    2.19.0/
      madmail                        # executable (mode 0755)
      meta.json
    2.20.0/
      madmail
      meta.json

/usr/local/bin/madmail -> /opt/madmail/versions/2.20.0/madmail
```

### Windows tree

```text
%ProgramFiles%\Madmail\
  current → versions\2.20.0          # junction or symlink (optional)
  versions\
    2.19.0\
      madmail.exe
      meta.json
    2.20.0\
      madmail.exe
      meta.json
  bin\
    madmail.exe → ..\versions\2.20.0\madmail.exe   # stable entry (symlink/junction/hardlink)

# Runtime state stays separate:
%ProgramData%\Madmail\config\
%ProgramData%\Madmail\data\
```

**Windows notes:**

| Topic | Detail |
|-------|--------|
| No `/opt` | Do not create or document `C:\opt\madmail` as the default. |
| Symlinks | Creating symlinks often requires Administrator or Developer Mode; **directory junctions** (`mklink /J`) or **replace-file** of a stable `bin\madmail.exe` copy are acceptable if symlink fails — document chosen strategy in implementation. Prefer **atomic replace** of the active pointer. |
| Locked exe | Windows may lock the running `madmail.exe`; upgrade must stop the Windows service (and any other holders) before replace, same as stop-systemd-before-replace on Linux. |
| Binary name | Always `madmail.exe` in version dirs; CLI still invoked as `madmail` when on PATH. |
| `update latest` asset | Host-matching Windows asset (e.g. `madmail-windows-amd64.zip` / `.tar.gz` / signed exe — follow whatever `publish.sh` ships); same size + signature rules as Linux. |
| Archive formats | If Windows releases use `.zip`, either allow `.zip` **only for Windows builds** in the download helper or ship `.tar.gz` for both; document in PR that implements `update latest`. |

### Version directory naming

| Rule | Detail |
|------|--------|
| Primary id | Semver from binary (`madmail version` / embedded package version), e.g. `2.20.0` |
| Collision | Same semver rebuilt → use `2.20.0+YYYYMMDDHHMMSS` or `2.20.0+git.<shortsha>` if available |
| Invalid chars | Reject path separators (`/`, `\`) and Windows-reserved names; only `[0-9A-Za-z._+-]` |
| Active pointer | Atomic update of `current` + stable PATH entry (symlink/junction/replace) |

### `meta.json` (optional but recommended)

```json
{
  "version": "2.20.0",
  "installed_at": "2026-08-03T12:00:00Z",
  "source": "upgrade",
  "source_url": "https://github.com/themadorg/madmail/releases/download/v2.20.0/madmail-linux-amd64",
  "sha256": "…",
  "variant": "linux-amd64",
  "os": "linux",
  "signature_ok": true
}
```

On Windows, `variant` / `os` reflect the Windows asset (e.g. `"windows-amd64"`, `"windows"`).

---

## Runtime and PATH contract

| Consumer | Linux / Unix | Windows |
|----------|--------------|---------|
| Shell / admin | `/usr/local/bin/madmail` on `PATH` | `{install_root}\bin\madmail.exe` on machine PATH (or service-only if PATH not updated) |
| Service | Prefer stable `ExecStart=/usr/local/bin/madmail` (symlink) | Prefer stable service `ImagePath` → `{install_root}\bin\madmail.exe` (or `current\madmail.exe`) so version flips do not require rewriting the service definition when possible |
| `madmail upgrade` | Resolve real path via `current_exe` + canonicalize; install into version tree; flip active pointer | Same algorithm; stop Windows service before replace if file is locked |
| `*.prev` | Deprecated after migration | Same |

**Recommendation (Linux):** keep `ExecStart=/usr/local/bin/madmail` so existing units and `REMOTE_BIN` defaults keep working; only the symlink target changes.

**Recommendation (Windows):** keep one stable path for the service and PATH; never scatter versioned exes on the user PATH.

---

## Commands (planned CLI)

Surface either as top-level group or under `upgrade` — prefer a dedicated group for clarity:

```text
madmail versions list [--remote]
madmail versions current
madmail versions use <version>
madmail versions prune [--keep N] [--yes]
madmail versions remove <version>   # refuse if active
madmail versions path [version]     # print filesystem path
```

### Behavior sketch

| Command | Behavior |
|---------|----------|
| `list` | Enumerate **local** `{install_root}/versions/*` (Unix `/opt/madmail`, Windows `%ProgramFiles%\Madmail`); mark active; show size, mtime, meta |
| `list --remote` | Query **GitHub Releases** (same project as `update latest`) and list remote version tags/assets available for this host; mark which are already installed locally and which is active; indicate remote `latest` |
| `current` | Print active version id + resolved path |
| `use V` | **Must** re-verify Ed25519 signature on `versions/V/madmail`, then preflight `version`, stop services, flip symlinks, start services; on smoke failure flip back. Refuse switch if signature fails (treat as tampered / unsigned archive). |
| `prune --keep N` | Delete oldest non-active versions beyond N (default **N=5**). **Always requires `--yes`** (including with `--json`; never treat JSON as implicit confirm). No signature check (does not execute or activate). |
| `remove V` | Error if V is active or only remaining version. **Always requires `--yes`** (including with `--json`). No signature check. |
| `list` / `current` / `path` | Read-only; **no** binary signature check required (metadata only; `list --remote` does not download binaries) |

#### `versions list --remote`

| Item | Spec |
|------|------|
| Intent | “What releases exist upstream, and how do they compare to what I have installed?” |
| Source | GitHub only: `themadorg/madmail` Releases (HTTPS; default TLS verify; honor `--accept-unsafe-https` if the parent CLI already exposes it for network ops) |
| Default (no flag) | **Local only** — no network |
| With `--remote` | Fetch release **metadata** (tags, published_at, asset names/sizes for host-matching assets). **Do not** download full binaries; **no** Ed25519 check on list (nothing to verify until install/`update`) |
| Compare | Annotate each remote entry: `installed` / `active` / `available` (not on disk) / `latest` |
| Local rows | Still show local-only versions (e.g. custom builds) that are not on GitHub, clearly labeled `local-only` |
| Failure | Network/API error → non-zero exit with clear message; do not pretend local list is remote |
| Rate limits | Prefer GitHub Releases API with modest page size; document unauthenticated limit; optional later: `GH_TOKEN` / `GITHUB_TOKEN` for higher limits |
| `--json` | Array of objects with at least: `version`, `source` (`local` \| `remote` \| `both`), `active`, `installed`, `remote_latest`, optional `published_at`, `asset`, `asset_size` |

Example human output (illustrative):

```text
VERSION     SOURCE   ACTIVE  NOTES
2.20.1      both     *       remote latest; installed
2.20.0      both             installed
2.19.0      local            local-only (not on GitHub)
2.18.0      remote           available (not installed)
```

**Signature policy (activation vs inventory):**

| Operation | Ed25519 signature |
|-----------|-------------------|
| Install new binary into the version tree (`install` / `upgrade` from path or URL) | **Always required** before write/activate (same as today — never skip) |
| **`madmail update latest`** (and `upgrade latest`) | **Always required** after download/extract, **before** writing into `{install_root}/versions/` or stopping services. Same `verify_signature` as `perform_upgrade`. **Never skip** because the source is “official GitHub”. Fail closed: unsigned / invalid signature → abort, leave active version unchanged. |
| `versions use <V>` (activate an already-archived binary) | **Always required** on that on-disk file before stop/symlink flip |
| `versions list` (local), `list --remote`, `current`, `path`, `prune`, `remove` | Not required — inventory/metadata only; remote list does **not** download or activate binaries |

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
| Default asset | Prefer **host OS + arch** asset: e.g. Linux `madmail-linux-amd64.tar.gz` (or musl); Windows `madmail-windows-amd64…` as published; document exact name table in operator CLI docs |
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
8. install into `{install_root}/versions/<VERSION>/` (`madmail` or `madmail.exe`), flip active pointer, smoke, rollback on failure
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

Prefer versioned install over “overwrite file + `*.prev`”. After the first successful versioned upgrade the stable PATH entry is a symlink into `versions/<id>/`; **never** `canonicalize` that path and write into `versions/<old>/` (that clobbers archive history while signatures still verify).

```text
1. verify Ed25519 signature on candidate
2. preflight: candidate `version` (captures VERSION string)
3. require root
4. ensure `{install_root}/versions` exists (Unix: root-owned 0755; Windows: Administrators / appropriate ACLs)
5. VERSION = parse from preflight output (fallback: meta / timestamp)
6. DEST = `{install_root}/versions/VERSION/`
   - install via install_candidate only (atomic write via temp + rename; refuse symlink dest)
7. write meta.json
8. PREV = resolve current active pointer (or legacy single-file install path)
9. stop services (systemd **or** Windows service)
10. atomic active-pointer update only (no in-place replace of any path under versions/):
    - `{install_root}/current` → `versions/VERSION`
    - stable PATH entry → active binary (Unix: rename temp symlink over existing; Windows: remove+rename)
11. smoke preflight on versions/VERSION binary (regular file)
12. on failure: restore previous active pointer (report honestly if restore fails), start services, error
13. on success: start services; optional prune --keep N
14. post-upgrade hooks (www migrate, docs refresh) unchanged where applicable
15. if install root is not writable: legacy single-file replace of a path **outside** versions/; refuse to clobber version-tree targets
```

### Migration from legacy install

| On-disk state | Action on first versioned upgrade/install |
|---------------|-------------------------------------------|
| Regular file `/usr/local/bin/madmail` (Unix) | Copy into `versions/<oldver>/`, then convert PATH entry to symlink |
| Single installed `madmail.exe` (Windows) | Copy into `versions\<oldver>\madmail.exe`, establish stable `bin\` or service path |
| `*.prev` sibling | Import as `versions/<prevver>/` if version parseable; else `versions/prev-import-<ts>/` |
| Already under platform install root version tree | No-op migrate; continue versioned flow |
| Service still points at old path | Prefer updating only the stable entry target; rewrite service ImagePath/unit only if needed |

---

## Install path changes

`InstallConfig.binary_path` today defaults to `/usr/local/bin/madmail` on Unix; Windows uses its install defaults.

| Phase | Install behavior |
|-------|------------------|
| v1 (Unix) | Install into `/opt/madmail/versions/<ver>/madmail`, create `/usr/local/bin/madmail` symlink; `binary_path` stays the **stable** path for unit generation |
| v1 (Windows) | Install into `%ProgramFiles%\Madmail\versions\<ver>\madmail.exe`, stable entry under `%ProgramFiles%\Madmail\bin\madmail.exe` (or equivalent); service uses stable path |
| later | Optional flag `--install-root <path>` / `MADMAIL_INSTALL_ROOT` for custom roots (test VMs, multi-instance); **do not** require `/opt` on Windows |

Dry-run (`install --dry-run`) must print platform-resolved version dir and stable-entry plan.

---

## Deploy / ops

| Knob | Today | After |
|------|-------|--------|
| `REMOTE_BIN` | `/usr/local/bin/madmail` | Unchanged (symlink) |
| Unsigned deploy | overwrites single binary | Prefer `madmail upgrade` / helper into `{install_root}/versions/…` |
| Signed deploy | `madmail upgrade` | Same command; version manager is internal |
| Disk | N/A | Document prune policy; default keep 5 |
| Linux servers | `REMOTE_BIN` symlink path | Unchanged |
| Windows hosts | N/A in `deploy.sh` today | Document manual/service path; version tree under Program Files |

Local dev / CI: version manager is a **production-install** concern; unit tests use a temp root via env, e.g. `MADMAIL_INSTALL_ROOT=/tmp/madmail-install-test` (Unix) or a temp directory on Windows CI. Accept `MADMAIL_OPT_ROOT` as a **deprecated alias** for the same override on Unix only.

---

## Configuration

| Mechanism | Key / env | Default (Unix) | Default (Windows) |
|-----------|-----------|----------------|-------------------|
| Env (test/override) | `MADMAIL_INSTALL_ROOT` | `/opt/madmail` | `%ProgramFiles%\Madmail` |
| Env (alias) | `MADMAIL_OPT_ROOT` | same as install root if set | ignored or maps to install root (document one behavior) |
| Keep count | setting or flag `--keep` | `5` | `5` |
| Config file | optional later in conf | not required for v1 | not required for v1 |

Do **not** store mailboxes or `chatmail.db` under the install/version root. State remains:

- Unix: `/var/lib/madmail` (or configured `state_dir`)
- Windows: `%ProgramData%\Madmail\data` (existing layout)

---

## Security

| Concern | Mitigation |
|---------|------------|
| Untrusted binaries entering versions/ | **Always** Ed25519-verify the candidate before writing into `{install_root}/versions/` on install/upgrade (same as today; never skip) |
| `update latest` / URL download | Same size caps (`MAX_DOWNLOAD_SIZE`), archive allow-list, TLS defaults, then **mandatory Ed25519** + preflight; GitHub “latest” is **not** a trust shortcut and **never** waives signature verification |
| Tampered archived binary activated via `use` | **Always** re-run Ed25519 verify on `versions/V/madmail` before stop/symlink flip; refuse if verify fails |
| Inventory without crypto cost | `list` / `list --remote` / `current` / `path` / `prune` / `remove` do not verify binary signatures (remote list is metadata only; installing still requires sig) |
| Symlink attacks | Create dirs as root; refuse to follow unexpected symlinks when writing DEST |
| PATH hijack | Unix: `/usr/local/bin/madmail` root:root 755 symlink; Windows: ACL so only Administrators write install root / `bin` |
| Prune deletes active | Refuse remove/prune of active target |
| Rollback window | Keep at least previous version until next successful upgrade + prune |

### Activation order for `versions use V`

```text
1. resolve path = versions/V/madmail[.exe] (reject missing / not a regular file)
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
| Unit | Version id sanitization; path helpers for Unix **and** Windows roots; active-pointer update helpers; meta parse; keep-N selection |
| Unit (upgrade) | Extend `upgrade.rs` tests: install root via `MADMAIL_INSTALL_ROOT` temp dir; no real systemd/service; signature always required on install; `#[cfg(windows)]` path/exe naming |
| Unit (`use`) | Signed archive → switch OK; unsigned/tampered on-disk binary → refuse before service stop; no sig check on `list` |
| Unit (`list --remote`) | Mock GitHub API: merge remote tags with local tree; mark active/installed/latest; offline/error path; local-only rows preserved |
| Unit (`update latest`) | Resolves expected GitHub latest URL for arch; rejects non-GitHub resolution; oversize download aborted; **unsigned / bad-signature payload fails after download and never flips symlink or stops services**; already-latest short-circuit still only after successful sig check on candidate |
| Integration | Legacy file → versioned tree migration; `use` switches active only after sig + preflight; failed smoke restores previous |
| E2E / manual | VM or docker: install → upgrade twice → `versions list` → `use` older → service healthy; optional `update latest` against real/mock GitHub |
| Deploy script | `REMOTE_BIN` still works when symlink |

---

## Implementation plan (PRs)

### PR1 — Layout helpers + docs

- Add module e.g. `crates/chatmail/src/ctl/versions.rs` (or `version_manager.rs`): **platform `install_root()`**, paths, ensure dirs, read/write meta, list, resolve active.
- Env `MADMAIL_INSTALL_ROOT` (and Unix alias `MADMAIL_OPT_ROOT`) for tests.
- Defaults: Unix `/opt/madmail`; Windows `%ProgramFiles%\Madmail` — **never** `/opt` on Windows.
- This TDD file + stub operator page under `docs/guide/cli/versions.md` (can land with PR3).
- Unit tests for pure path logic on both `cfg(unix)` and `cfg(windows)`.

### PR2 — Install writes version tree

- `install_binary`: copy into `{install_root}/versions/<ver>/madmail[.exe]`, update stable PATH entry.
- systemd unit generation unchanged on Unix (`ExecStart` still stable path); Windows service keeps stable ImagePath when possible.
- Migration: if existing real file at binary_path, import into versions first.

### PR3 — Upgrade uses version tree

- Change `perform_upgrade` to install-to-versions + flip symlink (algorithm above).
- Map rollback to previous version dir (keep `*.prev` import for one release if needed).
- CLI: `madmail versions list [--remote]|current|use|prune|remove|path`.
- `versions list --remote`: GitHub Releases metadata client; merge with local `{install_root}/versions`; no binary download; filter or label assets by OS.
- `versions use`: call same `verify_signature` as `perform_upgrade` **before** stop/preflight-activate; fail closed if unsigned/bad signature.
- Wire clap + `--json`; update parity matrix in `14-cli-tools.md`.

### PR4 — `update latest` (GitHub)

- Clap: `madmail update latest` and `madmail upgrade latest` resolve GitHub Releases latest asset for **this OS/arch** (Linux and Windows assets).
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

- [ ] Fresh install produces versioned binary under platform root: Unix `/opt/madmail/versions/<v>/madmail` + `/usr/local/bin/madmail` pointer; Windows `%ProgramFiles%\Madmail\versions\<v>\madmail.exe` + stable `bin` (or service) entry — **not** under `/opt` on Windows.
- [ ] `madmail upgrade` adds a new version directory without deleting the previous one (until prune).
- [ ] Failed post-install smoke restores previous active version and services recover (same guarantees as today).
- [ ] `madmail versions list` / `use` work with `--json`.
- [ ] `madmail versions list --remote` shows GitHub releases plus local install state (active/installed/available/latest); default `list` stays local-only and offline.
- [ ] Install/upgrade **never** places a binary under `versions/` without a successful Ed25519 check.
- [ ] `versions use` **always** re-verifies signature; fails before stopping services if verify fails; `list`/`path`/etc. do not require it.
- [ ] `madmail update latest` fetches from GitHub Releases latest for this host OS/arch and applies **full** URL-upgrade security (size cap, archive rules, **mandatory Ed25519 signature**, preflight, smoke/rollback) into the version tree.
- [ ] `madmail update latest` **never** activates or archives a binary that failed `verify_signature`; GitHub origin does not waive the check.
- [ ] Existing systemd units with `ExecStart=/usr/local/bin/madmail` need no edit for v1 (Unix).
- [ ] Windows service can keep a stable ImagePath across version flips when using `bin\` or `current` pointer.
- [ ] Tests pass with `MADMAIL_INSTALL_ROOT` under tmp; no requirement to write real `/opt` or real `Program Files` in CI.

---

## Open questions

1. **Exact CLI name:** `versions` vs `version` vs `bin`?
2. **Keep count default:** 5 vs 3 vs unlimited until explicit prune?
3. **Should `current` under install root be required**, or only the stable PATH entry (`/usr/local/bin/madmail` / `bin\madmail.exe`)?
4. **Import policy for `madmail.prev`:** always import, or only when version string is recoverable?
5. **Multi-instance hosts:** one install root vs `Madmail-<instance>` (out of scope unless needed).
6. **`update latest` asset selection:** auto-detect OS/arch (required) vs hardcode; Linux musl vs gnu; Windows asset naming from `publish.sh`.
7. **`update latest` vs Releases API:** redirect URL only (v1) vs API for tag display / multi-asset pick. Note: `versions list --remote` **needs** the Releases API (or equivalent) even if `update latest` uses the redirect URL.
8. **`list --remote` depth:** all releases vs last N (e.g. 20) vs only latest; default suggest last **20** non-prerelease unless `--all`.
9. **Windows active pointer:** symlink vs junction vs replace-file for `bin\madmail.exe` when symlinks are restricted.
10. **Windows default root:** `%ProgramFiles%\Madmail` vs `%ProgramData%\Madmail\bin` tree (prefer Program Files for exes; keep state in ProgramData).

---

## References

- Current upgrade safety (issue #114 notes in `upgrade.rs`): preflight before stop; backup; post smoke; rollback.
- URL download path in `upgrade.rs`: `handle_update_url`, `MAX_DOWNLOAD_SIZE` (100 MiB), `check_supported_url_archive`, Ed25519 after extract.
- FHS (Unix): `/opt/<package>` for add-on application software; `/usr/local/bin` for local admin commands on PATH.
- Windows: `%ProgramFiles%` for application binaries; `%ProgramData%\Madmail` for config/state (`windows_madmail_root()` in install code). **No `/opt` on Windows.**
- Sibling TDD: [`14-cli-tools.md`](14-cli-tools.md), [`13-configuration.md`](13-configuration.md), [`12-security.md`](12-security.md).
- Deploy defaults: `scripts/deploy.defaults.sh` (`REMOTE_BIN`) — Linux-oriented; Windows uses service/PATH docs.
- Example latest asset shapes:
  - Linux: `https://github.com/themadorg/madmail/releases/latest/download/madmail-linux-amd64.tar.gz`
  - Windows: publish-defined name under the same `…/releases/latest/download/` prefix.
