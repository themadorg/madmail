# Authentication & JIT Registration (Placeholder)

## JIT (Just-In-Time) Registration
Core Chatmail feature:
- On first successful IMAP or SMTP login, if user does not exist and JIT is enabled → automatically create account + hash password.
- Controlled by `__JIT_REGISTRATION_ENABLED__` setting (falls back to `__REGISTRATION_OPEN__`).

## Flow
1. Normalize username (PRECIS)
2. Lookup in `passwords` table
3. If not found:
   - Check JIT flag
   - If enabled → create user + bcrypt hash → grant access
   - If disabled → reject

## Storage
- Credentials in `passwords` table (username, hash, created_at, etc.)
- Separate from IMAP account provisioning (lazy on first delivery)

## CLI / Admin API
- `registration open/close`
- `jit enable/disable`
- Exposed via Admin API toggles

## Credential length limits (`chatmail` block)

Static directives in `maddy.conf` (see [`13-configuration.md`](13-configuration.md)):

| Directive | Purpose | chatmail-rs default |
|-----------|---------|---------------------|
| `username_length` | Auto-generated localpart size (`/new`) | 8 |
| `password_length` | Auto-generated password size (`/new`) | 16 |
| `min_username_length` | Minimum localpart (JIT + validation) | 8 |
| `max_username_length` | Maximum localpart | 20 |
| `password_min_length` | Minimum password on JIT create | 8 |

- **`POST /new`**: generates `username_length` / `password_length` (clamped as above).
- **JIT (first IMAP/SMTP login)**: rejects accounts when localpart ∉ `[min, max]` or `password` shorter than `password_min_length` (`chatmail-auth::validate_localpart_and_password`). Existing accounts are not re-checked on login.

Madmail example config uses `min_username_length 3`; chatmail-rs defaults to **8** to match typical Chatmail deployments.

## Security
- Bcrypt (or Argon2) for password hashing
- No plaintext passwords ever stored or returned
- Account creation **not** possible via Admin API (intentional)

## Implementation references

Index: [`CONTEXT.md`](CONTEXT.md).

| Concern | madmail | cmrelay | cmdeploy | stalwart |
|---------|---------|---------|----------|----------|
| JIT + password table | [`auth/pass_table/table.go`](../../context/madmail/internal/auth/pass_table/table.go), [`jit_test.go`](../../context/madmail/internal/auth/pass_table/jit_test.go) | [`chatmaild/user.py`](../../context/cmrelay/src/filtermail/python/chatmaild/user.py), [`doveauth.py`](../../context/cmrelay/src/filtermail/python/chatmaild/doveauth.py) | Online JIT: [`test_0_login.py`](../../context/cmdeploy/src/cmdeploy/tests/online/test_0_login.py) | IMAP/SMTP auth in respective `auth.rs` modules |
| SASL | [`auth/sasl.go`](../../context/madmail/internal/auth/sasl.go) | [`doveauth.rs`](../../context/cmrelay/src/filtermail/src/doveauth.rs) | Dovecot auth socket | [`smtp/.../auth.rs`](../../context/stalwart/crates/smtp/src/inbound/auth.rs) |
| `/new` registration | [`endpoint/chatmail/chatmail.go`](../../context/madmail/internal/endpoint/chatmail/chatmail.go) | [`newemail.py`](../../context/cmrelay/src/filtermail/python/chatmaild/newemail.py) | — | — |
| E2E JIT | [`tests/deltachat-test/scenarios/test_11_jit_registration.py`](../../context/madmail/tests/deltachat-test/scenarios/test_11_jit_registration.py) | — | [`test_0_login.py`](../../context/cmdeploy/src/cmdeploy/tests/online/test_0_login.py) | — |

## Related RFCs

Authentication on SMTP/IMAP and username handling. Index: [`RFC/README.md`](RFC/README.md).

| RFC | Topic | Local |
|-----|-------|-------|
| [4616](https://datatracker.ietf.org/doc/html/rfc4616) | SASL PLAIN | [rfc4616.txt](RFC/rfc4616.txt) |
| [4954](https://datatracker.ietf.org/doc/html/rfc4954) | SMTP AUTH | [rfc4954.txt](RFC/rfc4954.txt) |
| [3501](https://datatracker.ietf.org/doc/html/rfc3501) | IMAP LOGIN / AUTHENTICATE | [rfc3501.txt](RFC/rfc3501.txt) |
| [8264](https://datatracker.ietf.org/doc/html/rfc8264) | PRECIS framework | [rfc8264.txt](RFC/rfc8264.txt) |
| [8265](https://datatracker.ietf.org/doc/html/rfc8265) | PRECIS (username normalization) | [rfc8265.txt](RFC/rfc8265.txt) |