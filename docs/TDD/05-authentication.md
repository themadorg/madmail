# Authentication & JIT Registration

**Implementation:** `crates/chatmail-auth` (`jit`, `hash`, `validate`), `chatmail-state::AuthCache`, `jit_flights` (per-user login coalescing), wired from IMAP/SMTP/Web handlers.

**Operator CLI:** [`../guide/cli/registration.md`](../guide/cli/registration.md) · [`registration-tokens.md`](../guide/cli/registration-tokens.md) · [`accounts.md`](../guide/cli/accounts.md) · [`blocklist.md`](../guide/cli/blocklist.md).

## JIT (Just-In-Time) Registration

Core Chatmail feature:

- On first successful IMAP or SMTP login, if user does not exist and JIT is enabled → automatically create account + hash password.
- Controlled by `__JIT_REGISTRATION_ENABLED__` setting (falls back to `__REGISTRATION_OPEN__`).

## Flow

1. Normalize username (PRECIS)
2. **Blocklist check** — in-memory cache (`AuthCache::is_blocked`), hydrated from DB at boot / soft reload
3. Lookup hash in **credentials cache** (`AuthCache::get_hash`)
4. If not found:
   - Acquire per-user **JIT flight mutex** (`AppState::jit_flight`) so concurrent logins coalesce on one DB create
   - Re-check blocklist + cache + JIT flag (cached, no DB round-trip)
   - If enabled → validate lengths → create user + bcrypt hash → grant access
   - If disabled → reject (`535` / auth failed)

## In-memory auth cache (`chatmail-state::AuthCache`)

Madmail `pass_table.credCache` parity. Hot paths (IMAP LOGIN, SMTP AUTH, WebIMAP, routing) must not hit the DB per login when the account is already known.

| Cache partition | Hydrated from | Write-through on |
|-----------------|---------------|------------------|
| Credentials (`username → hash`) | `passwords` table | JIT create, `/new`, admin import, password change |
| Blocklist (`username`) | `blocklist` table | Admin block/unblock, account delete |
| JIT enabled flag | `__JIT_REGISTRATION_ENABLED__` ∨ `__REGISTRATION_OPEN__` | Soft reload (`SIGUSR2`) |

Admin CLI block/unblock updates DB only; running server picks up blocklist on next soft reload unless the change went through Admin API (which updates the cache immediately).

## Storage

- Credentials in `passwords` table (username, hash, created_at, etc.)
- Blocklist in `blocklist` table
- Separate from IMAP account provisioning (lazy on first delivery)

## CLI / Admin API

- `registration open/close`
- `jit enable/disable`
- Exposed via Admin API toggles

## Credential length limits (`chatmail` block)

Static directives in `maddy.conf` (see [`13-configuration.md`](13-configuration.md)):

| Directive | Purpose | madmail-v2 default |
|-----------|---------|---------------------|
| `username_length` | Auto-generated localpart size (`/new`) | 8 |
| `password_length` | Auto-generated password size (`/new`) | 16 |
| `min_username_length` | Minimum localpart (JIT + validation) | 8 |
| `max_username_length` | Maximum localpart | 20 |
| `password_min_length` | Minimum password on JIT create | 8 |

- **`POST /new`**: generates `username_length` / `password_length` (clamped as above). Response includes `email`, `password`, and **`dclogin_url`** (server-built, same shape as `build_dclogin_link`).
- **JIT (first IMAP/SMTP login)**: rejects accounts when localpart ∉ `[min, max]` or `password` shorter than `password_min_length` (`chatmail-auth::validate_localpart_and_password`). Existing accounts are not re-checked on login.

Madmail example config uses `min_username_length 3`; madmail-v2 defaults to **8** to match typical Chatmail deployments.

## dclogin / IP registration

Delta Chat `dclogin:` URLs must include explicit host and TLS hints:

```
dclogin:user@[IP]/?p=…&v=1&ih=IP&ip=993&is=ssl&sh=IP&sp=465&ss=ssl&ic=3
```

- **`ih` / `sh`**: bare connect host (no brackets)
- **`is` / `ss`**: `ssl`, `starttls`, `plain`, or `default` per bound listeners
- **`user@[IP]`** email form for IP-primary domains

Built by `chatmail-config::build_dclogin_link`; www templates use `connectHostForDclogin()` which prefers template fallback over `localhost` / `127.0.0.1` when the page is opened locally.

## Security

- Bcrypt (or Argon2) for password hashing
- No plaintext passwords ever stored or returned
- Account creation **not** possible via Admin API (intentional)

## Unit tests

| Test | Crate | Validates |
|------|-------|-----------|
| `p3_ut03_test_jit_creates_user` | `chatmail-auth` | JIT create |
| `p3_ut04_test_blocked_user_rejected` | `chatmail-auth` | Blocklist cache |
| `p3_ut04_test_jit_disabled_rejects` | `chatmail-auth` | JIT flag cache |
| `jit_coalesces_concurrent_creates_for_same_user` | `chatmail-auth` | Per-user JIT mutex |
| `hydrate_loads_blocklist_and_jit_flag` | `chatmail-state` | Cache hydrate |
| `build_dclogin_link_matches_www_shape` | `chatmail-config` | dclogin URI shape |
| `new_account_returns_dclogin_url_with_ssl_hints` | `chatmail-www` | `POST /new` JSON |

## Implementation references

Index: [`CONTEXT.md`](CONTEXT.md).

| Concern | madmail | cmrelay | cmdeploy | stalwart |
|---------|---------|---------|----------|----------|
| JIT + password table | [`auth/pass_table/table.go`](../../context/madmail/internal/auth/pass_table/table.go), [`jit_test.go`](../../context/madmail/internal/auth/pass_table/jit_test.go) | [`chatmaild/user.py`](../../context/cmrelay/src/filtermail/python/chatmaild/user.py), [`doveauth.py`](../../context/cmrelay/src/filtermail/python/chatmaild/doveauth.py) | Online JIT: [`test_0_login.py`](../../context/cmdeploy/src/cmdeploy/tests/online/test_0_login.py) | IMAP/SMTP auth in respective `auth.rs` modules |
| SASL | [`auth/sasl.go`](../../context/madmail/internal/auth/sasl.go) | [`doveauth.rs`](../../context/cmrelay/src/filtermail/src/doveauth.rs) | Dovecot auth socket | [`smtp/.../auth.rs`](../../context/stalwart/crates/smtp/src/inbound/auth.rs) |
| `/new` registration | [`endpoint/chatmail/chatmail.go`](../../context/madmail/internal/endpoint/chatmail/chatmail.go) | [`newemail.py`](../../context/cmrelay/src/filtermail/python/chatmaild/newemail.py) | — | — |
| E2E JIT | [`tests/deltachat-test/scenarios/test_11_jit_registration.py`](../../context/madmail/tests/deltachat-test/scenarios/test_11_jit_registration.py) | — | [`test_0_login.py`](../../context/cmdeploy/src/cmdeploy/tests/online/test_0_login.py) | — |
| cmping IP dclogin | — | — | — | [`context/cmping/test_cmping_dclogin.py`](../../context/cmping/test_cmping_dclogin.py) |

## Related RFCs

Authentication on SMTP/IMAP and username handling. Index: [`RFC/README.md`](RFC/README.md).

| RFC | Topic | Local |
|-----|-------|-------|
| [4616](https://datatracker.ietf.org/doc/html/rfc4616) | SASL PLAIN | [rfc4616.txt](RFC/rfc4616.txt) |
| [4954](https://datatracker.ietf.org/doc/html/rfc4954) | SMTP AUTH | [rfc4954.txt](RFC/rfc4954.txt) |
| [3501](https://datatracker.ietf.org/doc/html/rfc3501) | IMAP LOGIN / AUTHENTICATE | [rfc3501.txt](RFC/rfc3501.txt) |
| [8264](https://datatracker.ietf.org/doc/html/rfc8264) | PRECIS framework | [rfc8264.txt](RFC/rfc8264.txt) |
| [8265](https://datatracker.ietf.org/doc/html/rfc8265) | PRECIS (username normalization) | [rfc8265.txt](RFC/rfc8265.txt) |
