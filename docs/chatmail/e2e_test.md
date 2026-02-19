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

# Run tests inside local LXC containers (recommended for federation testing)
uv run python3 tests/deltachat-test/main.py --lxc --test-7

# Keep LXC containers alive after test for debugging
uv run python3 tests/deltachat-test/main.py --lxc --test-7 --keep-lxc
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

This is a comprehensive federation test with three parts:

**Part A — Cross-Server Federation:**
- Performs Secure Join between acc1 (server 1) and acc2 (server 2).
- Sends an encrypted message from acc1 → acc2 across server boundaries.
- Verifies the reverse direction: acc2 → acc1.

**Part B — Same-Server Messaging:**
- Creates acc3 on server 2 and Secure Joins with acc2.
- Sends encrypted messages between acc2 ↔ acc3 (both on the same server).

**Part C — Port-Based Federation Analysis:**
- Uses `iptables` on both servers to selectively block ports and test which
  network protocols support cross-server message delivery.
- Tests three scenarios:
  1. **HTTPS Only (443):** Block SMTP (25) + HTTP (80) → test if federation works via HTTPS.
  2. **HTTP Only (80):** Block SMTP (25) + HTTPS (443) → test if federation works via HTTP.
  3. **SMTP Only (25):** Block HTTP (80) + HTTPS (443) → test if federation works via standard SMTP.
- Prints a summary table showing which ports support federation.
- All iptables rules are flushed after testing.

**Delivery Priority:** Madmail attempts delivery in this order:
1. HTTPS POST to `/mxdeliv` (port 443)
2. HTTP POST fallback to `/mxdeliv` (port 80)
3. Standard SMTP delivery (port 25, with MX lookup)

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

### 13. Concurrent Profiles (`test_13_concurrent_profiles.py`)
- Tests multiple user profiles operating concurrently against the same server.
- Verifies isolation and correctness under parallel access.

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

## LXC Testing

The `--lxc` flag creates two isolated Debian 12 (Bookworm) LXC containers, installs Madmail on each, and runs the selected tests against them. This is the recommended way to test federation and cross-server features locally.

```bash
# Full federation test with LXC
uv run python3 tests/deltachat-test/main.py --lxc --test-7

# Keep containers alive for post-test debugging
uv run python3 tests/deltachat-test/main.py --lxc --test-7 --keep-lxc

# After --keep-lxc, you can SSH into the containers:
#   ssh root@<server-ip>    (password: root)
#   journalctl -u maddy.service -f    # live server logs
```

**Resource limits:** Each container is limited to 1GB RAM and 1 CPU core by default (configurable via `LXCManager` constructor arguments).

## Prerequisites

- **Python environment**: The tests use `uv` for dependency management.
- **Delta Chat RPC Server**: The `deltachat-rpc-server` binary must be installed on the system.
- **SSH Access**: For tests like "No Logging" and for administrative commands (Purge), the runner needs SSH access to the remote servers.
- **LXC (Optional)**: If using the `--lxc` flag, `lxc-create`, `lxc-start`, `lxc-ls`, and `lxc-attach` must be available. Requires `sudo` access.
- **iptables (Optional)**: Required for Part C of the Federation test (port-blocking). Installed automatically inside LXC containers if missing.

## Results and Debugging

Test results, including client-side logs and collected server-side logs, are stored in a timestamped directory under `tmp/test_run_YYYYMMDD_HHMMSS/`:

- `client_debug.log`: Detailed logs from the Delta Chat RPC client.
- `server1_debug.log` / `server2_debug.log`: Records from `journalctl` on the mail servers.
- `error.txt`: Contains traceback details if a test fails.

### Debugging Tips

- Use `--keep-lxc` to keep containers alive and inspect server state after a failure.
- Check `journalctl -u maddy.service -f` on the server for real-time debug logs.
- The `RUST_LOG` environment variable is set to `trace` for the RPC client, providing maximum detail in `client_debug.log`.
- Server debug logging is enabled via `debug yes` in the generated config file (`/etc/maddy/maddy.conf`).
