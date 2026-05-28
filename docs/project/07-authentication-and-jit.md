# 07 — Authentication and Just-In-Time (JIT) Registration

This is one of the most "chatmail-specific" pieces of the system and a frequent source of confusion.

## The Core Idea

**You do not pre-create accounts for normal users.**

Instead:
- A user (or Delta Chat client) presents a username + password.
- If the user does not exist and JIT is enabled → the account is created on the spot (password hashed, Maildir initialized, quota row created).
- If the user exists → normal password verification.

This makes onboarding frictionless while still having real accounts and passwords.

## The Main Entry Point

`chatmail_auth::jit::authenticate(ctx: &AuthContext, username, password)`

File: `crates/chatmail-auth/src/jit.rs`

Steps it performs:

1. **Normalize** the username (PRECIS, lowercase, etc.).
2. **Check blocklist** — immediately reject if blocked.
3. **Validate JIT domain** (if `jit_domain` is set in the session config — usually an IP literal for direct-IP clients).
4. **Look up existing password hash**.
   - If found → verify password. On success → `finish_successful_login`.
   - If not found → proceed to JIT path (if allowed).
5. **JIT path**:
   - Check that JIT or registration_open is enabled in settings.
   - Validate localpart + password policy (`credential_policy`).
   - Hash the password.
   - Create the user row in `passwords` table.
   - Create the Maildir directory tree on disk.
   - Ensure initial quota row.
   - Record first login (for registration token accounting).
6. **First-login accounting**:
   - Some registration tokens are single-use or have max uses.
   - `record_first_login` can decide to retroactively remove an account if the token was invalid after the fact.

## Where `authenticate` Is Called From

- SMTP `AUTH PLAIN` / `AUTH LOGIN` (submission and sometimes inbound in dev)
- IMAP `LOGIN` and `AUTHENTICATE`
- `POST /new` web registration handler (slightly different path that also accepts registration tokens)

All of them end up constructing an `AuthContext` (pool + AppState + primary_domain + jit_domain + credential_policy) and calling the same `authenticate` function.

## JIT Enablement Logic

```rust
async fn jit_enabled(pool: &DbPool) -> Result<bool> {
    if get_bool_setting(..., JIT_REGISTRATION_ENABLED, true) { return true; }
    if get_bool_setting(..., REGISTRATION_OPEN, true) { return true; }
    Ok(false)
}
```

Two separate toggles for historical / policy reasons. Admin UI usually exposes a combined "Allow new registrations" switch.

## Password Storage

- `chatmail_auth::hash` — currently uses a modern hashing algorithm (details in the module; also supports importing existing hashes from other systems).
- Never store or log plaintext passwords.
- The hash is the only thing returned from the DB during login.

## Registration Tokens (optional gating)

`registration_tokens` table + DAO in `chatmail-db`.

- Tokens can be created via admin API / CLI.
- Can have `max_uses`, `expires_at`, `comment`.
- When a user registers via `/new` with a token, the token usage is recorded.
- On first login the system can validate that the token was legitimate; bad tokens can cause the just-created account to be deleted again (`FirstLoginOutcome::AccountRemoved`).

This gives operators a way to do "invite only" without disabling open registration entirely.

## Blocklist

Checked on **every** authentication attempt and also on inbound federation delivery.

- Stored in `blocked_users` table.
- Reasons are recorded (manual, CLI, bulk, etc.).
- Admin can list / add / remove.

A blocked user cannot log in and cannot receive mail.

## First Login Side Effects

`finish_successful_login` calls `record_first_login`.

This is where registration-token consumption and "create the quota row with correct initial values" happens.

It can also decide the account should not actually exist (token abuse) and cleans up the Maildir + password row.

## Relationship to `/new` Web Endpoint

`POST /new` in `chatmail-www/src/handlers.rs`:

- Accepts optional registration token.
- Validates token if provided.
- Creates the password hash row (same as JIT path).
- Initializes Maildir.
- Ensures quota.
- Attaches the token to the account for auditing.
- Does **not** require a successful login — it's a pre-auth registration.

After `/new`, the client still has to do a real IMAP/SMTP login (which will succeed via the normal auth path).

## Common Failure Modes & How to Debug

- "User cannot register" → check `JIT_REGISTRATION_ENABLED` / `REGISTRATION_OPEN` in settings, and that the domain matches `jit_domain` if set.
- "Password rejected after registration" → normalization difference between registration and login, or hashing mismatch.
- "Account disappeared after first login" → registration token validation failed in `record_first_login`.
- "Works in Delta Chat but not via webmail" → different credential policy or domain handling.

Useful queries:
```sql
SELECT * FROM passwords WHERE username LIKE '%alice%';
SELECT * FROM registration_tokens;
SELECT * FROM quotas;
```

## Security Notes

- Constant-time comparison is used for the admin token; password verification uses proper hash comparison.
- No user enumeration via timing on the auth path (the blocklist check is before the DB lookup in some paths, but the design accepts that a blocked user will get a fast reject).
- Password policy is enforced at creation time, not just login.

## Next

With auth and identity understood, the next big pieces are the actual mail protocols and federation.

→ [08-smtp-imap-servers.md](./08-smtp-imap-servers.md)
