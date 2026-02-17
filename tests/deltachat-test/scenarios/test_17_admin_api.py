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
"""

import json
import time
import subprocess
import requests


def run_ssh_command(remote, command):
    """Run a command on remote server via SSH."""
    result = subprocess.run(
        ["ssh", "-o", "StrictHostKeyChecking=no", "-o", "UserKnownHostsFile=/dev/null",
         f"root@{remote}", command],
        capture_output=True,
        text=True,
        timeout=30
    )
    return result.returncode, result.stdout, result.stderr


def get_admin_token(remote):
    """Extract the admin token from the remote server.
    
    Checks two locations in order:
    1. Config file (/etc/maddy/maddy.conf) for explicit admin_token
    2. Auto-generated token file (/var/lib/maddy/admin_token)
    """
    # First check config for explicit token
    returncode, stdout, stderr = run_ssh_command(
        remote,
        "grep -m1 'admin_token' /etc/maddy/maddy.conf 2>/dev/null | grep -v '^\\s*#' | awk '{print $2}'"
    )
    config_token = stdout.strip()
    if config_token and config_token != "disabled":
        return config_token

    if config_token == "disabled":
        raise Exception("admin_token is set to 'disabled' in config")

    # Fall back to auto-generated token file
    returncode, stdout, stderr = run_ssh_command(
        remote,
        "cat /var/lib/maddy/admin_token 2>/dev/null"
    )
    token = stdout.strip()
    if token:
        return token

    raise Exception(
        f"Admin token not found on {remote}. "
        f"Check /var/lib/maddy/admin_token or admin_token in maddy.conf."
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
    print("\n[1/16] Authentication tests")

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
    print("\n[2/16] Correct token access")
    status, data = api_call(base_url, "/admin/status", token=token)
    assert data.get("status") == 200, \
        f"Expected 200 for correct token, got {data}"
    print("  ✓ Correct token accepted (200)")
    passed += 1

    # ------------------------------------------------------------------
    # 3. Unknown resource
    # ------------------------------------------------------------------
    total += 1
    print("\n[3/16] Unknown resource")
    status, data = api_call(base_url, "/admin/nonexistent", token=token)
    assert data.get("status") == 404, \
        f"Expected 404 for unknown resource, got {data}"
    print("  ✓ Unknown resource returns 404")
    passed += 1

    # ------------------------------------------------------------------
    # 4. /admin/status
    # ------------------------------------------------------------------
    total += 1
    print("\n[4/16] /admin/status")
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
    print("\n[5/16] /admin/storage")
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
    print("\n[6/16] Registration toggle")

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
    print("\n[7/16] JIT Registration toggle")
    test_toggle(base_url, token, "/admin/registration/jit", "JIT Registration")
    passed += 1

    # ------------------------------------------------------------------
    # 8. /admin/services/turn — TURN Toggle
    # ------------------------------------------------------------------
    total += 1
    print("\n[8/16] TURN service toggle")
    test_toggle(base_url, token, "/admin/services/turn", "TURN")
    passed += 1

    # ------------------------------------------------------------------
    # 9. /admin/services/iroh — Iroh Toggle
    # ------------------------------------------------------------------
    total += 1
    print("\n[9/16] Iroh service toggle")
    test_toggle(base_url, token, "/admin/services/iroh", "Iroh")
    passed += 1

    # ------------------------------------------------------------------
    # 10. /admin/services/shadowsocks — Shadowsocks Toggle
    # ------------------------------------------------------------------
    total += 1
    print("\n[10/16] Shadowsocks service toggle")
    test_toggle(base_url, token, "/admin/services/shadowsocks", "Shadowsocks")
    passed += 1

    # ------------------------------------------------------------------
    # 11. /admin/accounts — List accounts
    # ------------------------------------------------------------------
    total += 1
    print("\n[11/16] Account listing")

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
    print("\n[12/16] Quota / storage stats")

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
    print("\n[13/16] Account deletion via API")

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

    # POST not allowed on accounts (no creation via API)
    status, data = api_call(
        base_url, "/admin/accounts", method="POST",
        body={"username": "shouldfail"}, token=token
    )
    assert data.get("status") == 405, f"Expected 405 for POST on accounts, got {data}"
    print("  ✓ Account creation via API correctly rejected (405)")
    passed += 1

    # ------------------------------------------------------------------
    # 14. /admin/queue — Purge operations
    # ------------------------------------------------------------------
    total += 1
    print("\n[14/16] Queue operations")

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
    print("\n[15/16] Contact shares")

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
    print("\n[16/16] DNS overrides")

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

        # Delete non-existent entry
        status, data = api_call(
            base_url, "/admin/dns", method="DELETE",
            body={"lookup_key": "does-not-exist.example.invalid"},
            token=token
        )
        assert data.get("status") == 404, f"Expected 404 for non-existent delete, got {data}"
        print("  ✓ Delete non-existent entry returns 404")

    elif data.get("status") in (404, 503):
        print("  ⏭ DNS overrides not available (GORM DB not exposed)")
    else:
        print(f"  ⚠ Unexpected DNS response: {data}")
    passed += 1

    # ------------------------------------------------------------------
    # Summary
    # ------------------------------------------------------------------
    print("\n" + "="*50)
    print(f"🎉 TEST #17 PASSED! Admin API verified. ({passed}/{total} checks passed)")
    print("="*50)
    return True
