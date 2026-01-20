# Chatmail Authentication Specification

## Overview
Chatmail implements a "just-in-time" registration pattern. Unlike traditional mail servers that require manual account provisioning, Chatmail allows accounts to be created automatically during the first successful authentication attempt (IMAP or SMTP Submission).

## Authentication Logic
The core logic resides in the `auth.pass_table` module (`internal/auth/pass_table/table.go`).

### Scenario: User Login (IMAP/SMTP)
1.  **Normalization**: The username is normalized (case-folded and cleaned) using PRECIS.
2.  **Credential Lookup**: The system attempts to find the user in the `passwords` table.
3.  **Branching Logic**:
    *   **User Exists**: Standard password verification is performed (Bcrypt hash comparison).
    *   **User Does Not Exist**:
        *   The system queries the JIT registration status (`__JIT_REGISTRATION_ENABLED__`).
        *   If `__JIT_REGISTRATION_ENABLED__` is `true`:
            *   A new user entry is created immediately.
            *   The provided password is hashed and stored.
            *   The login attempt is granted.
        *   If `__JIT_REGISTRATION_ENABLED__` is `false`:
            *   The login attempt is rejected with `Invalid Credentials`.

## Technical Implementation Details

### Module Architecture
- **Auth Provider**: `internal/auth/pass_table`
- **Storage Backend**: `internal/storage/imapsql`
- **Flag Management**: `internal/table/sql_table`

### Dynamic Configuration (Settings Table)
System-wide flags are decoupled from user credentials to ensure performance and prevent schema conflicts:
- **Table Name**: `settings` (or as configured in `maddy.conf` via `settings_table`).
- **Key-Value Pair**: `__REGISTRATION_OPEN__` -> `"true"`/`"false"`.
- **Precedence**: If the key is missing from the table, the system falls back to the static `auto_create` value defined in `maddy.conf`.

The JIT registration flag (`__JIT_REGISTRATION_ENABLED__`) controls automatic account creation. If not explicitly set, it defaults to the value of `__REGISTRATION_OPEN__`.

### Mailbox Provisioning
When a user is auto-registered during login, their IMAP mailbox is lazily provisioned on the first access or first mail delivery. 
- **Delivery Hook**: `internal/storage/imapsql/delivery.go` also verifies JIT registration status to determine if a non-existent recipient should be auto-created during incoming mail delivery.

## JIT vs API Registration

Chatmail supports two primary ways of creating accounts:

1.  **JIT (Just-In-Time)**: Triggered by a login attempt. 
    - **Pros**: Zero friction for users.
    - **Cons**: Can lead to "address squatting" or accidental account creation if the password doesn't match a intended policy.
2.  **API-Based (`/new`)**: Triggered by a POST request to the web endpoint.
    - **Pros**: Allows the server to control username generation and ensure uniqueness before the client attempts to login. 
    - **Controlled Flow**: Even if `__JIT_REGISTRATION_ENABLED__` is `false`, the `/new` API will still work as long as `__REGISTRATION_OPEN__` is `true`. This allows operators to disable automatic creation on login while still allowing new users to sign up via the official web landing page.

## Management CLI
Registration status can be toggled without restarting the server:
```bash
maddy creds registration open   # Sets __REGISTRATION_OPEN__ to true
maddy creds registration close  # Sets __REGISTRATION_OPEN__ to false
```

JIT registration can be managed independently:
```bash
maddy creds jit enable   # Enable automatic account creation
maddy creds jit disable  # Disable automatic account creation
maddy creds jit status   # Show current JIT registration status
```
