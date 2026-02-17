# Delta Chat E2E Test Suite for Madmail

This document describes the end-to-end (E2E) test suite for the Madmail server, which uses the Delta Chat RPC client to simulate real user interactions and verify server behavior.

## Running the Tests

### End-to-End Tests
The core E2E suite uses Delta Chat to verify real-world behavior:

```bash
make test
```

This command executes the test runner using `uv`:
`uv run python3 tests/deltachat-test/main.py`

### Unit Tests
For internal Go logic, you can run the unit tests:

```bash
make test-unit
```

### Selective E2E Testing

You can run specific tests or all tests using command-line arguments:

```bash
# Run all tests (default)
uv run python3 tests/deltachat-test/main.py --all

# Run specific tests
uv run python3 tests/deltachat-test/main.py --test-1 --test-3
```

## Test Scenarios

The suite consists of several scenarios located in `tests/deltachat-test/scenarios/`:

### 1. Account Creation (`test_01_account_creation.py`)
- Creates random test accounts on the specified mail servers.
- Uses the `dclogin` format to explicitly set IMAP/SMTP hosts and bypass DNS lookups.
- Verifies that the account can be successfully configured and start I/O.

### 2. Unencrypted Message Rejection (`test_02_unencrypted_rejection.py`)
- Attempts to send a plain-text (unencrypted) email via SMTP.
- Verifies that the Madmail server correctly rejects it with a **523 "Encryption Needed"** error code.
- This ensures the server enforces a "PGP-only" policy.

### 3. Secure Join (`test_03_secure_join.py`)
- Performs a Secure Join handshake between two accounts.
- Verifies that both parties become "verified contacts" and establish a secure, PGP-encrypted channel.

### 4. P2P Encrypted Message
- Sends an encrypted peer-to-peer message between two verified accounts.
- Verifies successful delivery and decryption on the receiver's end.

### 5. Group Creation & Message (`test_05_group_message.py`)
- Creates a new multi-user group chat.
- Adds another verified contact to the group.
- Verifies that group messages are correctly distributed and received by members.

### 6. File Transfer (`test_06_file_transfer.py`)
- Generates a random 1MB file.
- Sends the file through Delta Chat.
- Verifies the integrity of the received file by comparing its **SHA256 hash** with the original.

### 7. Federation (`test_07_federation.py`)
- Verifies cross-server messaging between accounts on different Madmail instances.
- Verifies same-server messaging between two accounts on the same instance.
- Ensures that PGP encryption and delivery work across server boundaries.

### 8. No Logging Test (`test_08_no_logging.py`)
- This test verifies the "privacy-first" nature of the server.
- It automatically disables logging on the remote servers via SSH.
- Sends a large volume of messages (30+) across P2P, Group, and Federation scenarios.
- Checks `journalctl` to ensure that no (or minimal) logs were generated during these operations.
- Automatically re-enables logging after completion.

### 9. Big File Transfer (`test_09_send_bigfile.py`)
- Stress tests the SMTP/IMAP pipeline with multiple large file attachments.
- Verifies that `appendlimit` and `max_message_size` are correctly enforced and handled.

### 10. Binary Signature & Upgrade (`test_10_upgrade_mechanism.py`)
- Verifies the integrity of the binary upgrade mechanism.
- Tests both successful signed upgrades and rejection of unsigned/tampered binaries.
- Simulates updates from both local files and remote URLs.

### 11. JIT Registration (`test_11_jit_registration.py`)
- Verifies "Just-In-Time" account creation.
- An account is automatically created the first time it receives an email or when a user tries to log in, without prior manual registration.

### 12. SMTP/IMAP IDLE Test (`test_12_smtp_imap_idle.py`)
- Verifies the responsiveness of the IMAP IDLE implementation.
- Tests receiving messages in real-time without polling.
- Ensures SMTP delivery triggers IDLE notifications correctly.

### 14. Message Purging (`test_14_purge_messages.py`)
- Verifies administrative commands for purging user data.
- Tests `purge-read` (removes messages marked as seen).
- Tests `purge-all` (completely wipes an account's mailbox).
- Verifies storage reclaimed via server-side statistics.

### 15. Iroh Discovery (`test_15_iroh_discovery.py`)
- Verifies that the server correctly advertises the Iroh Relay URL via IMAP METADATA.
- Ensures the client can fetch and parse the relay address for P2P connection establishment.

### 16. WebXDC Realtime P2P (`test_16_webxdc_realtime.py`)
- Verifies end-to-end real-time P2P communication between two WebXDC instances.
- Coordinates the Iroh handshake via the integrated Iroh Relay.
- Verifies that high-frequency data packets are delivered with low latency outside the standard IMAP/SMTP flow.

### 17. Admin API (`test_17_admin_api.py`)
- Extracts the `admin_token` from the remote server's config via SSH.
- Verifies authentication: missing tokens, wrong tokens, and correct tokens.
- Tests the `/admin/status` endpoint for user count and uptime data.
- Tests the `/admin/storage` endpoint for disk usage and state dir info.
- Toggles registration open/closed via `/admin/registration` and verifies state.
- Toggles TURN service via `/admin/services/turn` and verifies state.
- Lists accounts via `/admin/accounts` and verifies count.
- Gets storage quota stats via `/admin/quota`.
- Creates a disposable account, verifies it in the listing, deletes it via the API, and confirms removal.
- Verifies method validation (405 on invalid methods).
- Tests DNS override CRUD via `/admin/dns` (create, verify, delete, confirm).

## Prerequisites

- **Python environment**: The tests use `uv` for dependency management.
- **Delta Chat RPC Server**: The `deltachat-rpc-server` binary must be installed on the system.
- **SSH Access**: For tests like "No Logging" and for administrative commands (Purge), the runner needs SSH access to the remote servers.
- **LXC (Optional)**: If using the `--lxc` flag, the runner will automatically create isolated containers to run the tests.

## Results and Debugging

Test results, including client-side logs and collected server-side logs, are stored in a timestamped directory under `tmp/test_run_YYYYMMDD_HHMMSS/`:

- `client_debug.log`: Detailed logs from the Delta Chat RPC client.
- `server1_debug.log` / `server2_debug.log`: Records from `journalctl` on the mail servers.
- `error.txt`: Contains traceback details if a test fails.
