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
        *   The system queries the registration status (dynamically stored in the `settings` table).
        *   If `__REGISTRATION_OPEN__` is `true`:
            *   A new user entry is created immediately.
            *   The provided password is hashed and stored.
            *   The login attempt is granted.
        *   If `__REGISTRATION_OPEN__` is `false`:
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

### Mailbox Provisioning
When a user is auto-registered during login, their IMAP mailbox is lazily provisioned on the first access or first mail delivery. 
- **Delivery Hook**: `internal/storage/imapsql/delivery.go` also verifies registration status to determine if a non-existent recipient should be auto-created during incoming mail delivery.

## Management CLI
Registration status can be toggled without restarting the server:
```bash
maddy creds registration open   # Sets __REGISTRATION_OPEN__ to true
maddy creds registration close  # Sets __REGISTRATION_OPEN__ to false
```
