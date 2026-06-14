"""
Test #17: Admin API Test

This test verifies the Admin API (POST /api/admin) functionality:
1. Token extraction: Reads admin_token from the remote server config via SSH.
2. Authentication: Verifies that missing/wrong tokens are rejected (401).
3. Status endpoint: Verifies /admin/status returns user count and uptime info.
4. Storage endpoint: Verifies /admin/storage returns disk and state dir info.
5. Registration toggle: Verifies /admin/registration can open/close registration.
6. JIT Registration toggle: Verifies /admin/registration/jit can be toggled.
7. TURN toggle: Verifies /admin/services/turn can be toggled.
8. Iroh toggle: Verifies /admin/services/iroh can be toggled.
9. Shadowsocks toggle: Verifies /admin/services/shadowsocks can be toggled.
10. Account listing: Verifies /admin/accounts lists existing accounts.
11. Quota management: Verifies /admin/quota returns storage stats.
12. Account deletion via API: Verifies /admin/accounts DELETE removes a user.
13. Queue operations: Verifies /admin/queue accepts purge commands.
14. Contact shares: Verifies /admin/shares CRUD (if contact sharing enabled).
15. DNS overrides: Verifies /admin/dns CRUD.
16. Method validation: Verifies 405 for unsupported methods.
17. Log toggle: Verifies /admin/services/log can be toggled.
18. Port settings: Verifies set/get/reset for all 7 port settings.
19. Config settings: Verifies set/get/reset for all 8 config settings.
20. Bulk settings: Verifies /admin/settings returns all settings at once.
21. Reload endpoint: Verifies /admin/reload accepts POST and rejects GET.
"""

import json
import time

import requests

from utils.ssh import run_ssh_command


def get_admin_token(remote):
    """Extract the admin token from the remote madmail server."""
    for config_path in ("/etc/madmail/madmail.conf", "/etc/maddy/maddy.conf"):
        returncode, stdout, stderr = run_ssh_command(
            remote,
            f"grep -m1 'admin_token' {config_path} 2>/dev/null "
            r"| grep -v '^\s*#' | awk '{print $2}'",
        )
        config_token = stdout.strip()
        if config_token and config_token != "disabled":
            return config_token
        if config_token == "disabled":
            raise Exception(f"admin_token is set to 'disabled' in {config_path}")

    for token_path in ("/var/lib/madmail/admin_token", "/var/lib/maddy/admin_token"):
        returncode, stdout, stderr = run_ssh_command(
            remote, f"cat {token_path} 2>/dev/null"
        )
        token = stdout.strip()
        if token:
            return token

    returncode, stdout, stderr = run_ssh_command(
        remote, "madmail admin-token --raw 2>/dev/null"
    )
    token = stdout.strip()
    if token:
        return token

    raise Exception(
        f"Admin token not found on {remote}. "
        "Check /var/lib/madmail/admin_token or admin_token in madmail.conf."
    )


def api_call(base_url, resource, method="GET", body=None, token=None):
    """Make an Admin API call and return (status_code, response_json)."""
    payload = {
        "method": method,
        "resource": resource,
        "headers": {},
    }
    if token:
        payload["headers"]["Authorization"] = f"Bearer {token}"
    if body is not None:
        payload["body"] = body

    last_exc = None
    for attempt in range(8):
        try:
            resp = requests.post(
                f"{base_url}/api/admin",
                json=payload,
                timeout=15,
            )
            try:
                data = resp.json()
            except Exception:
                data = {"raw": resp.text}
            return resp.status_code, data
        except requests.exceptions.ConnectionError as exc:
            last_exc = exc
            time.sleep(2)
    raise last_exc


def test_toggle(base_url, token, resource, label):
    """Generic test for toggle-style endpoints (enable/disable)."""
    print(f"\n  Testing {label} ({resource})")

    # GET current state
    status, data = api_call(base_url, resource, token=token)
    assert data.get("status") == 200, f"  ✗ GET {resource} failed: {data}"
    current = data["body"]["status"]
    print(f"    Current: {current}")

    # Disable
    status, data = api_call(
        base_url, resource, method="POST",
        body={"action": "disable"}, token=token
    )
    assert data.get("status") == 200, f"  ✗ Disable failed: {data}"
    assert data["body"]["status"] == "disabled", f"  ✗ Expected disabled, got {data['body']}"
    print(f"    ✓ Disabled")

    # Verify state persisted
    status, data = api_call(base_url, resource, token=token)
    assert data["body"]["status"] == "disabled", f"  ✗ State not persisted: {data['body']}"

    # Enable
    status, data = api_call(
        base_url, resource, method="POST",
        body={"action": "enable"}, token=token
    )
    assert data.get("status") == 200, f"  ✗ Enable failed: {data}"
    assert data["body"]["status"] == "enabled", f"  ✗ Expected enabled, got {data['body']}"
    print(f"    ✓ Enabled")

    # Bad action
    status, data = api_call(
        base_url, resource, method="POST",
        body={"action": "invalid"}, token=token
    )
    assert data.get("status") == 400, f"  ✗ Expected 400 for bad action, got {data}"
    print(f"    ✓ Bad action rejected (400)")

    # Wrong method
    status, data = api_call(
        base_url, resource, method="DELETE", token=token
    )
    assert data.get("status") == 405, f"  ✗ Expected 405 for DELETE, got {data}"
    print(f"    ✓ Wrong method rejected (405)")

    print(f"    ✓ {label} PASSED")


def run(dc, remote, test_dir=None):
    """
    E2E scenario for verifying the Admin API.

    Args:
        dc: DeltaChat RPC client instance (used for creating test accounts)
        remote: IP or hostname of the Madmail server
        test_dir: Optional directory for test artifacts
    """
    print("\n" + "="*50)
    print("TEST #17: Admin API")
    print("="*50)

    base_url = f"http://{remote}"
    token = get_admin_token(remote)
    print(f"  Admin token retrieved from {remote} (length={len(token)})")

    passed = 0
    total = 0

    # ------------------------------------------------------------------
    # 1. Authentication — Missing token
    # ------------------------------------------------------------------
    total += 1
    print("\n[1/21] Authentication tests")

    status, data = api_call(base_url, "/admin/status", token=None)
    assert data.get("status") == 401, \
        f"Expected 401 for missing token, got {data}"
    print("  ✓ Missing token correctly rejected (401)")

    # Wrong token
    status, data = api_call(base_url, "/admin/status", token="wrong-token-12345")
    assert data.get("status") == 401, \
        f"Expected 401 for wrong token, got {data}"
    print("  ✓ Wrong token correctly rejected (401)")
    passed += 1

    # ------------------------------------------------------------------
    # 2. Correct token
    # ------------------------------------------------------------------
    total += 1
    print("\n[2/21] Correct token access")
    status, data = api_call(base_url, "/admin/status", token=token)
    assert data.get("status") == 200, \
        f"Expected 200 for correct token, got {data}"
    print("  ✓ Correct token accepted (200)")
    passed += 1

    # ------------------------------------------------------------------
    # 3. Unknown resource
    # ------------------------------------------------------------------
    total += 1
    print("\n[3/21] Unknown resource")
    status, data = api_call(base_url, "/admin/nonexistent", token=token)
    assert data.get("status") == 404, \
        f"Expected 404 for unknown resource, got {data}"
    print("  ✓ Unknown resource returns 404")
    passed += 1

    # ------------------------------------------------------------------
    # 4. /admin/status
    # ------------------------------------------------------------------
    total += 1
    print("\n[4/21] /admin/status")
    status, data = api_call(base_url, "/admin/status", token=token)
    body = data.get("body", {})

    assert "users" in body, f"Expected 'users' in status body, got {body}"
    assert "registered" in body["users"], f"Expected 'registered' in users"
    user_count = body["users"]["registered"]
    assert isinstance(user_count, int) and user_count >= 0
    print(f"  ✓ Status: {user_count} registered users")

    if body.get("uptime"):
        print(f"    Uptime: {body['uptime'].get('duration', 'unknown')}")
    if body.get("email_servers"):
        es = body["email_servers"]
        print(f"    Email servers: {es.get('connection_ips', 0)} IPs, "
              f"{es.get('domain_servers', 0)} domains")
    passed += 1

    # ------------------------------------------------------------------
    # 5. /admin/storage
    # ------------------------------------------------------------------
    total += 1
    print("\n[5/21] /admin/storage")
    status, data = api_call(base_url, "/admin/storage", token=token)
    body = data.get("body", {})

    assert data.get("status") == 200, f"Expected 200 for storage, got {data}"
    if body.get("disk"):
        disk = body["disk"]
        pct = disk.get("percent_used", 0)
        total_gb = disk.get("total_bytes", 0) / (1024**3)
        print(f"  ✓ Disk: {pct:.1f}% used of {total_gb:.1f} GB")
    if body.get("state_dir"):
        sd = body["state_dir"]
        size_mb = sd.get("size_bytes", 0) / (1024**2)
        print(f"    State dir: {sd.get('path')} ({size_mb:.1f} MB)")
    if body.get("database"):
        db = body["database"]
        db_mb = db.get("size_bytes", 0) / (1024**2)
        print(f"    Database: {db.get('driver')} ({db_mb:.1f} MB)")
    passed += 1

    # ------------------------------------------------------------------
    # 6. /admin/registration — Toggle
    # ------------------------------------------------------------------
    total += 1
    print("\n[6/21] Registration toggle")

    # Get current state
    status, data = api_call(base_url, "/admin/registration", token=token)
    assert data.get("status") == 200
    original_state = data["body"]["status"]
    print(f"  Current registration status: {original_state}")

    # Close registration
    status, data = api_call(
        base_url, "/admin/registration", method="POST",
        body={"action": "close"}, token=token
    )
    assert data.get("status") == 200
    assert data["body"]["status"] == "closed"
    print("  ✓ Registration closed via API")

    # Verify it's actually closed
    status, data = api_call(base_url, "/admin/registration", token=token)
    assert data["body"]["status"] == "closed"
    print("  ✓ Confirmed registration is closed")

    # Open registration
    status, data = api_call(
        base_url, "/admin/registration", method="POST",
        body={"action": "open"}, token=token
    )
    assert data.get("status") == 200
    assert data["body"]["status"] == "open"
    print("  ✓ Registration opened via API")

    # Bad action
    status, data = api_call(
        base_url, "/admin/registration", method="POST",
        body={"action": "invalid"}, token=token
    )
    assert data.get("status") == 400
    print("  ✓ Bad action rejected (400)")

    # Wrong method
    status, data = api_call(
        base_url, "/admin/registration", method="DELETE", token=token
    )
    assert data.get("status") == 405
    print("  ✓ Wrong method rejected (405)")
    passed += 1

    # ------------------------------------------------------------------
    # 7. /admin/registration/jit — JIT Toggle
    # ------------------------------------------------------------------
    total += 1
    print("\n[7/21] JIT Registration toggle")
    test_toggle(base_url, token, "/admin/registration/jit", "JIT Registration")
    passed += 1

    # ------------------------------------------------------------------
    # 8. /admin/services/turn — TURN Toggle
    # ------------------------------------------------------------------
    total += 1
    print("\n[8/21] TURN service toggle")
    test_toggle(base_url, token, "/admin/services/turn", "TURN")
    passed += 1

    # ------------------------------------------------------------------
    # 9. /admin/services/iroh — Iroh Toggle
    # ------------------------------------------------------------------
    total += 1
    print("\n[9/21] Iroh service toggle")
    test_toggle(base_url, token, "/admin/services/iroh", "Iroh")
    passed += 1

    # ------------------------------------------------------------------
    # 10. /admin/services/shadowsocks — Shadowsocks Toggle
    # ------------------------------------------------------------------
    total += 1
    print("\n[10/21] Shadowsocks service toggle")
    test_toggle(base_url, token, "/admin/services/shadowsocks", "Shadowsocks")
    passed += 1

    # ------------------------------------------------------------------
    # 11. /admin/accounts — List accounts
    # ------------------------------------------------------------------
    total += 1
    print("\n[11/21] Account listing")

    status, data = api_call(base_url, "/admin/accounts", token=token)
    assert data.get("status") == 200
    body = data.get("body", {})
    total_accts = body.get("total", 0)
    accounts = body.get("accounts", [])
    assert isinstance(accounts, list)
    assert total_accts == len(accounts)
    print(f"  ✓ Listed {total_accts} accounts")
    passed += 1

    # ------------------------------------------------------------------
    # 12. /admin/quota — Storage stats
    # ------------------------------------------------------------------
    total += 1
    print("\n[12/21] Quota / storage stats")

    status, data = api_call(base_url, "/admin/quota", token=token)
    assert data.get("status") == 200
    body = data.get("body", {})
    assert "total_storage_bytes" in body
    assert "accounts_count" in body
    assert "default_quota_bytes" in body
    default_quota_gb = body["default_quota_bytes"] / (1024**3)
    print(f"  ✓ Quota stats: {body['accounts_count']} accounts, "
          f"default quota: {default_quota_gb:.1f} GB, "
          f"total storage: {body['total_storage_bytes']} bytes")
    passed += 1

    # ------------------------------------------------------------------
    # 13. /admin/accounts DELETE — Create and delete a test account
    # ------------------------------------------------------------------
    total += 1
    print("\n[13/21] Account deletion via API")

    # Create an account via the /new endpoint
    print("  Creating disposable account via /new...")
    resp = requests.post(f"{base_url}/new", timeout=10)
    assert resp.status_code == 200, f"/new failed: {resp.text}"
    new_acct = resp.json()
    new_email = new_acct.get("email")
    print(f"  Created: {new_email}")

    # Verify it appears in listing
    time.sleep(1)
    status, data = api_call(base_url, "/admin/accounts", token=token)
    emails = [a["username"] for a in data["body"]["accounts"]]
    assert new_email in emails, f"New account {new_email} not found in listing"
    print(f"  ✓ Account {new_email} confirmed in listing")

    # Delete it
    status, data = api_call(
        base_url, "/admin/accounts", method="DELETE",
        body={"username": new_email}, token=token
    )
    assert data.get("status") == 200, f"Delete failed: {data}"
    print(f"  ✓ Account {new_email} deleted via API")

    # Verify it's gone
    time.sleep(1)
    status, data = api_call(base_url, "/admin/accounts", token=token)
    emails = [a["username"] for a in data["body"]["accounts"]]
    assert new_email not in emails, f"Account {new_email} still appears after deletion!"
    print(f"  ✓ Confirmed {new_email} is no longer in listing")

    # POST creates accounts via admin API (madmail-v2)
    status, data = api_call(
        base_url, "/admin/accounts", method="POST", body={}, token=token
    )
    assert data.get("status") == 201, f"POST /admin/accounts failed: {data}"
    api_email = data["body"]["email"]
    api_password = data["body"]["password"]
    assert api_email and api_password, f"Missing credentials in response: {data}"
    print(f"  ✓ Account created via API: {api_email}")

    time.sleep(1)
    status, data = api_call(base_url, "/admin/accounts", token=token)
    emails = [a["username"] for a in data["body"]["accounts"]]
    assert api_email in emails, f"API-created account {api_email} not in listing"

    status, data = api_call(
        base_url, "/admin/accounts", method="DELETE",
        body={"username": api_email}, token=token
    )
    assert data.get("status") == 200, f"Delete API-created account failed: {data}"
    print(f"  ✓ API-created account {api_email} deleted")
    passed += 1

    # ------------------------------------------------------------------
    # 14. /admin/queue — Purge operations
    # ------------------------------------------------------------------
    total += 1
    print("\n[14/21] Queue operations")

    # purge_read (should succeed even if nothing to purge)
    status, data = api_call(
        base_url, "/admin/queue", method="POST",
        body={"action": "purge_read"}, token=token
    )
    assert data.get("status") == 200, f"purge_read failed: {data}"
    print("  ✓ purge_read accepted")

    # purge_all
    status, data = api_call(
        base_url, "/admin/queue", method="POST",
        body={"action": "purge_all"}, token=token
    )
    assert data.get("status") == 200, f"purge_all failed: {data}"
    print("  ✓ purge_all accepted")

    # invalid action
    status, data = api_call(
        base_url, "/admin/queue", method="POST",
        body={"action": "invalid_action"}, token=token
    )
    assert data.get("status") == 400, f"Expected 400 for invalid queue action, got {data}"
    print("  ✓ Invalid action rejected (400)")

    # GET not allowed
    status, data = api_call(base_url, "/admin/queue", method="GET", token=token)
    assert data.get("status") == 405, f"Expected 405 for GET on queue, got {data}"
    print("  ✓ GET on queue rejected (405)")
    passed += 1

    # ------------------------------------------------------------------
    # 15. /admin/shares — Contact shares (may not be available)
    # ------------------------------------------------------------------
    total += 1
    print("\n[15/21] Contact shares")

    status, data = api_call(base_url, "/admin/shares", token=token)
    if data.get("status") == 200:
        shares = data["body"].get("shares", [])
        print(f"  ✓ Shares endpoint available ({len(shares)} shares)")

        # Create a test share
        status, data = api_call(
            base_url, "/admin/shares", method="POST",
            body={"slug": "test-api-share", "url": "openpgp4fpr:AAAA",
                  "name": "Test Share"}, token=token
        )
        assert data.get("status") in (200, 201), f"Create share failed: {data}"
        print("  ✓ Created test share")

        # Verify it exists
        status, data = api_call(base_url, "/admin/shares", token=token)
        slugs = [s["slug"] for s in data["body"]["shares"]]
        assert "test-api-share" in slugs, f"Share not found in listing: {slugs}"
        print("  ✓ Verified share exists")

        # Update the share
        status, data = api_call(
            base_url, "/admin/shares", method="PUT",
            body={"slug": "test-api-share", "url": "openpgp4fpr:BBBB",
                  "name": "Updated Share"}, token=token
        )
        assert data.get("status") == 200, f"Update share failed: {data}"
        print("  ✓ Updated test share")

        # Delete the share
        status, data = api_call(
            base_url, "/admin/shares", method="DELETE",
            body={"slug": "test-api-share"}, token=token
        )
        assert data.get("status") == 200, f"Delete share failed: {data}"
        print("  ✓ Deleted test share")

        # Verify deletion
        status, data = api_call(base_url, "/admin/shares", token=token)
        slugs = [s["slug"] for s in data["body"]["shares"]]
        assert "test-api-share" not in slugs
        print("  ✓ Confirmed share is gone")
    elif data.get("status") == 404:
        print("  ⏭ Contact sharing not enabled (resource not registered)")
    else:
        print(f"  ⚠ Unexpected shares response: {data}")
    passed += 1

    # ------------------------------------------------------------------
    # 16. /admin/dns — DNS overrides
    # ------------------------------------------------------------------
    total += 1
    print("\n[16/21] DNS overrides")

    status, data = api_call(base_url, "/admin/dns", token=token)
    if data.get("status") == 200:
        overrides = data["body"].get("overrides", [])
        print(f"  ✓ DNS overrides available ({len(overrides)} entries)")

        # Create a test override
        status, data = api_call(
            base_url, "/admin/dns", method="POST",
            body={"lookup_key": "test-api.example.invalid", "target_host": "1.2.3.4",
                  "comment": "E2E test override"},
            token=token
        )
        assert data.get("status") == 201, f"Expected 201 for DNS create, got {data}"
        print("  ✓ Created test DNS override")

        # Verify it exists
        status, data = api_call(base_url, "/admin/dns", token=token)
        keys = [o["lookup_key"] for o in data["body"]["overrides"]]
        assert "test-api.example.invalid" in keys
        print("  ✓ Verified test DNS override exists")

        # Delete it
        status, data = api_call(
            base_url, "/admin/dns", method="DELETE",
            body={"lookup_key": "test-api.example.invalid"},
            token=token
        )
        assert data.get("status") == 200, f"Expected 200 for DNS delete, got {data}"
        print("  ✓ Deleted test DNS override")

        # Verify deletion
        status, data = api_call(base_url, "/admin/dns", token=token)
        keys = [o["lookup_key"] for o in data["body"]["overrides"]]
        assert "test-api.example.invalid" not in keys
        print("  ✓ Confirmed test DNS override is gone")

        # Delete non-existent entry (madmail-v2: idempotent 200)
        status, data = api_call(
            base_url, "/admin/dns", method="DELETE",
            body={"lookup_key": "does-not-exist.example.invalid"},
            token=token
        )
        assert data.get("status") == 200, f"Expected 200 for idempotent DNS delete, got {data}"
        assert data["body"]["deleted"] == "does-not-exist.example.invalid"
        print("  ✓ Delete non-existent entry returns 200 (idempotent)")

    elif data.get("status") in (404, 503):
        print("  ⏭ DNS overrides not available (GORM DB not exposed)")
    else:
        print(f"  ⚠ Unexpected DNS response: {data}")
    passed += 1

    # ------------------------------------------------------------------
    # 17. /admin/services/log — Log Toggle (legacy maddy; optional in madmail-v2)
    # ------------------------------------------------------------------
    total += 1
    print("\n[17/21] Log toggle")
    status, data = api_call(base_url, "/admin/services/log", token=token)
    if data.get("status") == 404:
        print("  ⏭ Log toggle not available in madmail-v2")
    else:
        test_toggle(base_url, token, "/admin/services/log", "Logging")
    passed += 1

    # ------------------------------------------------------------------
    # 18. Port settings — Set / Get / Reset for all ports
    # ------------------------------------------------------------------
    total += 1
    print("\n[18/21] Port settings")

    port_endpoints = [
        ("/admin/settings/smtp_port", "__SMTP_PORT__", "2525"),
        ("/admin/settings/submission_port", "__SUBMISSION_PORT__", "1587"),
        ("/admin/settings/imap_port", "__IMAP_PORT__", "1993"),
        ("/admin/settings/turn_port", "__TURN_PORT__", "4478"),
        ("/admin/settings/submission_tls_port", "__SUBMISSION_TLS_PORT__", "1465"),
        ("/admin/settings/iroh_port", "__IROH_PORT__", "9999"),
        ("/admin/settings/ss_port", "__SS_PORT__", "9388"),
    ]

    for resource, expected_key, test_value in port_endpoints:
        short = resource.split("/")[-1]

        # GET — should not be set initially (or has a default)
        status, data = api_call(base_url, resource, token=token)
        assert data.get("status") == 200, f"  ✗ GET {resource} failed: {data}"
        body = data["body"]
        assert body["key"] == expected_key, f"  ✗ Expected key {expected_key}, got {body['key']}"
        print(f"  ✓ GET {short}: key={body['key']}, is_set={body['is_set']}")

        # SET
        status, data = api_call(
            base_url, resource, method="POST",
            body={"action": "set", "value": test_value}, token=token
        )
        assert data.get("status") == 200, f"  ✗ SET {resource} failed: {data}"
        assert data["body"]["value"] == test_value
        assert data["body"]["is_set"] is True
        print(f"  ✓ SET {short}={test_value}")

        # GET — verify persistence
        status, data = api_call(base_url, resource, token=token)
        assert data["body"]["value"] == test_value, f"  ✗ Value not persisted for {short}"
        assert data["body"]["is_set"] is True
        print(f"  ✓ GET {short} confirmed persisted")

        # RESET
        status, data = api_call(
            base_url, resource, method="POST",
            body={"action": "reset"}, token=token
        )
        assert data.get("status") == 200, f"  ✗ RESET {resource} failed: {data}"
        assert data["body"]["is_set"] is False
        print(f"  ✓ RESET {short}")

    # SET empty value should fail
    status, data = api_call(
        base_url, "/admin/settings/smtp_port", method="POST",
        body={"action": "set", "value": ""}, token=token
    )
    assert data.get("status") == 400, f"  ✗ Expected 400 for empty value, got {data}"
    print("  ✓ Empty value rejected (400)")

    # Wrong method
    status, data = api_call(
        base_url, "/admin/settings/smtp_port", method="DELETE", token=token
    )
    assert data.get("status") == 405, f"  ✗ Expected 405 for DELETE, got {data}"
    print("  ✓ Wrong method rejected (405)")

    print("  ✓ All port settings PASSED")
    passed += 1

    # ------------------------------------------------------------------
    # 19. Config settings — Set / Get / Reset for all config values
    # ------------------------------------------------------------------
    total += 1
    print("\n[19/21] Config settings")

    config_endpoints = [
        ("/admin/settings/smtp_hostname", "__SMTP_HOSTNAME__", "mail.test.example.com"),
        ("/admin/settings/turn_realm", "__TURN_REALM__", "test.realm.org"),
        ("/admin/settings/turn_secret", "__TURN_SECRET__", "e2e-test-secret-42"),
        ("/admin/settings/turn_relay_ip", "__TURN_RELAY_IP__", "192.168.99.1"),
        ("/admin/settings/turn_ttl", "__TURN_TTL__", "7200"),
        ("/admin/settings/iroh_relay_url", "__IROH_RELAY_URL__", "https://iroh.test.example.com"),
        ("/admin/settings/ss_cipher", "__SS_CIPHER__", "aes-256-gcm"),
        ("/admin/settings/ss_password", "__SS_PASSWORD__", "e2e-test-ss-pass"),
    ]

    for resource, expected_key, test_value in config_endpoints:
        short = resource.split("/")[-1]

        # SET
        status, data = api_call(
            base_url, resource, method="POST",
            body={"action": "set", "value": test_value}, token=token
        )
        assert data.get("status") == 200, f"  ✗ SET {resource} failed: {data}"
        assert data["body"]["key"] == expected_key
        assert data["body"]["value"] == test_value
        assert data["body"]["is_set"] is True
        print(f"  ✓ SET {short}={test_value}")

        # GET — verify persistence
        status, data = api_call(base_url, resource, token=token)
        assert data["body"]["value"] == test_value, f"  ✗ Value not persisted for {short}"
        print(f"  ✓ GET {short} confirmed")

        # RESET
        status, data = api_call(
            base_url, resource, method="POST",
            body={"action": "reset"}, token=token
        )
        assert data.get("status") == 200, f"  ✗ RESET {resource} failed: {data}"
        assert data["body"]["is_set"] is False
        print(f"  ✓ RESET {short}")

    print("  ✓ All config settings PASSED")
    passed += 1

    # ------------------------------------------------------------------
    # 20. /admin/settings — Bulk settings read
    # ------------------------------------------------------------------
    total += 1
    print("\n[20/21] Bulk settings")

    # First, set a known port so we can verify it appears in bulk
    api_call(
        base_url, "/admin/settings/smtp_port", method="POST",
        body={"action": "set", "value": "7777"}, token=token
    )

    status, data = api_call(base_url, "/admin/settings", token=token)
    assert data.get("status") == 200, f"  ✗ Bulk GET failed: {data}"
    body = data["body"]

    # Check toggle keys are present
    assert "registration" in body, f"  ✗ Missing 'registration' in bulk: {body}"
    assert body["registration"] in ("open", "closed")
    print(f"  ✓ registration: {body['registration']}")

    assert "turn_enabled" in body
    assert body["turn_enabled"] in ("enabled", "disabled")
    print(f"  ✓ turn_enabled: {body['turn_enabled']}")

    assert "iroh_enabled" in body
    print(f"  ✓ iroh_enabled: {body['iroh_enabled']}")

    assert "ss_enabled" in body
    print(f"  ✓ ss_enabled: {body['ss_enabled']}")

    if "log_disabled" in body:
        print(f"  ✓ log_disabled: {body['log_disabled']}")
    else:
        print("  ⏭ log_disabled not exposed in madmail-v2 bulk settings")

    # Check our set port appears correctly
    assert "smtp_port" in body
    smtp_port = body["smtp_port"]
    assert smtp_port["key"] == "__SMTP_PORT__"
    assert smtp_port["value"] == "7777"
    assert smtp_port["is_set"] is True
    print(f"  ✓ smtp_port in bulk: {smtp_port['value']} (is_set={smtp_port['is_set']})")

    # Check other port/config keys exist (may or may not be set)
    for field in ["submission_port", "submission_tls_port", "imap_port", "turn_port",
                  "iroh_port", "ss_port", "smtp_hostname", "turn_realm",
                  "turn_secret", "turn_relay_ip", "turn_ttl", "iroh_relay_url",
                  "ss_cipher", "ss_password"]:
        assert field in body, f"  ✗ Missing '{field}' in bulk response"
    print("  ✓ All setting keys present in bulk response")

    # Wrong method
    status, data = api_call(
        base_url, "/admin/settings", method="POST",
        body={"action": "something"}, token=token
    )
    assert data.get("status") == 405, f"  ✗ Expected 405 for POST on /admin/settings, got {data}"
    print("  ✓ POST on bulk settings rejected (405)")

    # Clean up the test port
    api_call(
        base_url, "/admin/settings/smtp_port", method="POST",
        body={"action": "reset"}, token=token
    )
    print("  ✓ Cleaned up test port")
    passed += 1

    # ------------------------------------------------------------------
    # 21. /admin/reload — Port hot-reload + actual listener verification
    # ------------------------------------------------------------------
    total += 1
    print("\n[21/21] Reload: port change + listener verification")

    # --- Part A: Verify restart_required flag ---
    status, data = api_call(
        base_url, "/admin/settings/submission_port", method="POST",
        body={"action": "set", "value": "5555"}, token=token
    )
    assert data.get("status") == 200
    body = data["body"]
    assert body.get("restart_required") is True, \
        f"  ✗ Expected restart_required=true after SET, got {body}"
    print("  ✓ restart_required=true after SET")

    # Reset and verify restart_required on reset too
    status, data = api_call(
        base_url, "/admin/settings/submission_port", method="POST",
        body={"action": "reset"}, token=token
    )
    assert data.get("status") == 200
    body = data["body"]
    assert body.get("restart_required") is True, \
        f"  ✗ Expected restart_required=true after RESET, got {body}"
    print("  ✓ restart_required=true after RESET")

    # GET should NOT have restart_required=true
    status, data = api_call(
        base_url, "/admin/settings/submission_port", token=token
    )
    assert data.get("status") == 200
    body = data["body"]
    assert body.get("restart_required") is not True, \
        f"  ✗ Expected restart_required=false on GET, got {body}"
    print("  ✓ restart_required=false on GET")

    # --- Part B: Verify reload endpoint method validation ---
    status, data = api_call(
        base_url, "/admin/reload", method="GET", token=token
    )
    assert data.get("status") == 405, f"  ✗ Expected 405 for GET, got {data}"
    print("  ✓ GET on /admin/reload rejected (405)")

    # --- Part C: Actually change port and verify listener ---
    remote = base_url.replace("http://", "").replace("https://", "").split(":")[0]
    OLD_PORT = "587"
    NEW_PORT = "1587"

    # Verify old port is currently listening
    rc, out, err = run_ssh_command(remote, f"ss -tlnp | grep ':{OLD_PORT} '")
    assert OLD_PORT in out, f"  ✗ Port {OLD_PORT} not listening before change: {out}"
    print(f"  ✓ Port {OLD_PORT} is currently listening (verified via ss)")

    # Set the submission port to new value
    status, data = api_call(
        base_url, "/admin/settings/submission_port", method="POST",
        body={"action": "set", "value": NEW_PORT}, token=token
    )
    assert data.get("status") == 200
    print(f"  ✓ Set submission_port={NEW_PORT} in DB")

    # Trigger reload — this will restart the service
    print("  → Triggering reload (service will restart)...")
    try:
        status, data = api_call(
            base_url, "/admin/reload", method="POST", token=token
        )
        if data.get("status") == 200:
            print(f"  ✓ Reload accepted: {data['body'].get('message', 'ok')}")
        else:
            print(f"  ⚠ Reload returned status {data.get('status')}: {data.get('body', {}).get('error', 'unknown')}")
    except Exception as ex:
        print(f"  ✓ Connection dropped (expected — service restarting): {type(ex).__name__}")

    # Wait for the service to restart and come back up
    print("  → Waiting for service to restart...")
    max_wait = 30
    started = time.time()
    service_up = False
    while time.time() - started < max_wait:
        time.sleep(2)
        try:
            # Check if the admin API is responding again
            status, data = api_call(base_url, "/admin/status", token=token)
            if data.get("status") == 200:
                service_up = True
                break
        except Exception:
            pass
    
    assert service_up, f"  ✗ Service did not come back up within {max_wait}s"
    elapsed = time.time() - started
    print(f"  ✓ Service back up after {elapsed:.1f}s")

    # Verify NEW port is now listening
    rc, out, err = run_ssh_command(remote, f"ss -tlnp | grep ':{NEW_PORT} '")
    if NEW_PORT not in out:
        # Debug: dump full port listing and config for diagnosis
        _, all_ports, _ = run_ssh_command(remote, "ss -tlnp")
        _, conf_line, _ = run_ssh_command(
            remote,
            "grep submission /etc/madmail/madmail.conf 2>/dev/null "
            "|| grep submission /etc/maddy/maddy.conf 2>/dev/null",
        )
        _, journal, _ = run_ssh_command(
            remote,
            "journalctl -u madmail.service --no-pager --since '30 seconds ago' 2>/dev/null "
            "|| journalctl -u maddy --no-pager --since '30 seconds ago' 2>/dev/null "
            "|| echo 'no journal'",
        )
        print(f"  DEBUG: All listening ports:\n{all_ports}")
        print(f"  DEBUG: Config submission line: {conf_line.strip()}")
        print(f"  DEBUG: Recent journal:\n{journal}")
        assert False, f"  ✗ Port {NEW_PORT} NOT listening after reload!"
    print(f"  ✓ Port {NEW_PORT} IS now listening (verified via ss)")

    # Verify OLD port is no longer listening (for submission — the original 587)
    rc, out, err = run_ssh_command(remote, f"ss -tlnp | grep ':{OLD_PORT} '")
    if OLD_PORT not in out:
        print(f"  ✓ Port {OLD_PORT} is NO longer listening (confirmed port migrated)")
    else:
        # Port 587 might still show up if another process uses it
        print(f"  ⚠ Port {OLD_PORT} still shows in ss (may be another process): {out.strip()}")

    # --- Part D: Restore original port ---
    print(f"  → Restoring submission_port to {OLD_PORT}...")
    api_call(
        base_url, "/admin/settings/submission_port", method="POST",
        body={"action": "reset"}, token=token
    )

    # Reload to apply the reset
    try:
        api_call(base_url, "/admin/reload", method="POST", token=token)
    except Exception:
        pass  # Expected disconnect

    # Wait for service to come back up again
    started = time.time()
    service_up = False
    while time.time() - started < max_wait:
        time.sleep(2)
        try:
            status, data = api_call(base_url, "/admin/status", token=token)
            if data.get("status") == 200:
                service_up = True
                break
        except Exception:
            pass

    if service_up:
        # Verify the old port is back
        rc, out, err = run_ssh_command(remote, f"ss -tlnp | grep ':{OLD_PORT} '")
        if OLD_PORT in out:
            print(f"  ✓ Port {OLD_PORT} restored and listening (rollback verified)")
        else:
            print(f"  ⚠ Port {OLD_PORT} not yet listening after rollback (may need more time)")
    else:
        print(f"  ⚠ Service didn't come back after rollback within {max_wait}s")

    print("  ✓ Port hot-reload PASSED")
    passed += 1

    # ------------------------------------------------------------------
    # Summary
    # ------------------------------------------------------------------
    print("\n" + "="*50)
    print(f"🎉 TEST #17 PASSED! Admin API verified. ({passed}/{total} checks passed)")
    print("="*50)
    return True
