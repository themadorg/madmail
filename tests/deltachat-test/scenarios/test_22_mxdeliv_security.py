"""
Test #22: MxDeliv Security Validation

Verifies that the /mxdeliv endpoint properly enforces security checks:
  1. Rejects delivery to admin/root/postmaster addresses
  2. Rejects delivery to recipients on wrong domains
  3. Silently accepts delivery to non-existent users (returns 200 OK)
  4. Mixed recipients: valid domain accepted, wrong domain ignored

Architecture:
    ┌──────────────┐    curl POST     ┌──────────────┐
    │   Test Host  │ ───────────────► │ Madmail LXC  │
    │              │                  │ (HTTPS:443)  │
    └──────────────┘                  └──────────────┘
"""

import os
import sys
import time
import json
import subprocess
import ssl
import socket
import http.client

TEST_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, TEST_DIR)

SSH_OPTS = "-o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o ConnectTimeout=10"


def ssh(remote, cmd, timeout=30):
    full = f"ssh {SSH_OPTS} root@{remote} '{cmd}'"
    result = subprocess.run(full, shell=True, capture_output=True, text=True, timeout=timeout)
    return result


def mxdeliv_post(host, mail_from, mail_to_list, body, use_https=True):
    """
    Send a POST request to /mxdeliv with the given parameters.
    Uses http.client to properly support multiple headers with the same name.
    Returns (status_code, response_body).
    """
    try:
        if use_https:
            import ssl as _ssl
            ctx = _ssl.create_default_context()
            ctx.check_hostname = False
            ctx.verify_mode = _ssl.CERT_NONE
            conn = http.client.HTTPSConnection(host, 443, timeout=15, context=ctx)
        else:
            conn = http.client.HTTPConnection(host, 80, timeout=15)

        # Build headers — http.client supports multiple headers with same name
        headers = {
            "X-Mail-From": mail_from,
            "Content-Type": "application/octet-stream",
        }

        conn.putrequest("POST", "/mxdeliv")
        for k, v in headers.items():
            conn.putheader(k, v)
        # Add each X-Mail-To as a separate header
        for rcpt in mail_to_list:
            conn.putheader("X-Mail-To", rcpt)
        body_bytes = body.encode()
        conn.putheader("Content-Length", str(len(body_bytes)))
        conn.endheaders()
        conn.send(body_bytes)

        resp = conn.getresponse()
        return resp.status, resp.read().decode()
    except Exception as e:
        return -1, str(e)


def make_rfc822_body(from_addr, to_addr, subject="Test", body_text="Hello"):
    """Build a minimal RFC 822 message."""
    return (
        f"From: {from_addr}\r\n"
        f"To: {to_addr}\r\n"
        f"Subject: {subject}\r\n"
        f"Date: Thu, 10 Apr 2026 12:00:00 +0000\r\n"
        f"Message-ID: <test-security-{int(time.time())}@test>\r\n"
        f"\r\n"
        f"{body_text}\r\n"
    )


def create_user_on_server(server_ip):
    """Create a user on the remote server using 'maddy create-user' CLI.
    Returns (email, password) tuple.
    """
    result = ssh(server_ip, "/tmp/maddy create-user", timeout=15)
    if result.returncode != 0:
        raise Exception(f"Failed to create user on {server_ip}: {result.stderr}")
    try:
        data = json.loads(result.stdout.strip())
        return data["email"], data["password"]
    except (json.JSONDecodeError, KeyError) as e:
        raise Exception(f"Failed to parse create-user output: {result.stdout}") from e


def run(dc, remotes):
    """
    Run mxdeliv security tests against a running Madmail server.

    Args:
        dc: DeltaChat instance (unused, but kept for API consistency)
        remotes: Tuple of (REMOTE1, REMOTE2) IPs
    """
    REMOTE1, REMOTE2 = remotes

    print("\n" + "=" * 60)
    print("TEST #22: MxDeliv Security Validation")
    print("=" * 60)

    # Use REMOTE2 which is the IP-only server (mailDomain = [IP]).
    # REMOTE1 may have a domain name (s1.test) which would break IP-based tests.
    server_ip = REMOTE2
    sender = f"attacker@[{REMOTE1}]"
    print(f"  Target server: {server_ip} (IP-only)")
    print(f"  Sender: {sender}")

    # Wait for server HTTPS port to be ready
    print(f"\n  Waiting for {server_ip}:443 to be ready...")
    for attempt in range(30):
        try:
            s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            s.settimeout(2)
            s.connect((server_ip, 443))
            s.close()
            print(f"  ✓ Server ready after {attempt + 1}s")
            break
        except Exception:
            time.sleep(1)
    else:
        raise Exception(f"Server {server_ip}:443 not ready after 30s")

    # ─── Pre-requisite: Create a real user on the target server ───
    print("\n  Creating test user on target server...")
    user_email, user_password = create_user_on_server(server_ip)
    print(f"  ✓ Created user: {user_email}")

    # ─── Test 1: Reject delivery to admin@[server_ip] ───
    print("\n  Test 1: Reject delivery to admin@[server_ip]")
    body = make_rfc822_body(sender, f"admin@[{server_ip}]")
    status, resp = mxdeliv_post(server_ip, sender, [f"admin@[{server_ip}]"], body)
    if status == 404:
        print(f"  ✓ admin delivery rejected (404): {resp.strip()[:80]}")
    else:
        raise Exception(f"Test 1 FAILED: expected 404, got {status}: {resp[:200]}")

    # ─── Test 2: Reject delivery to admin@server_ip (bare IP) ───
    print("\n  Test 2: Reject delivery to admin@<bare_ip>")
    body = make_rfc822_body(sender, f"admin@{server_ip}")
    status, resp = mxdeliv_post(server_ip, sender, [f"admin@{server_ip}"], body)
    if status == 404:
        print(f"  ✓ admin@bare_ip rejected (404): {resp.strip()[:80]}")
    else:
        raise Exception(f"Test 2 FAILED: expected 404, got {status}: {resp[:200]}")

    # ─── Test 3: Reject delivery to root@ ───
    print("\n  Test 3: Reject delivery to root@[server_ip]")
    body = make_rfc822_body(sender, f"root@[{server_ip}]")
    status, resp = mxdeliv_post(server_ip, sender, [f"root@[{server_ip}]"], body)
    if status == 404:
        print(f"  ✓ root delivery rejected (404): {resp.strip()[:80]}")
    else:
        raise Exception(f"Test 3 FAILED: expected 404, got {status}: {resp[:200]}")

    # ─── Test 4: Reject delivery to postmaster@ ───
    print("\n  Test 4: Reject delivery to postmaster@[server_ip]")
    body = make_rfc822_body(sender, f"postmaster@[{server_ip}]")
    status, resp = mxdeliv_post(server_ip, sender, [f"postmaster@[{server_ip}]"], body)
    if status == 404:
        print(f"  ✓ postmaster delivery rejected (404): {resp.strip()[:80]}")
    else:
        raise Exception(f"Test 4 FAILED: expected 404, got {status}: {resp[:200]}")

    # ─── Test 5: Reject delivery to wrong domain ───
    print("\n  Test 5: Reject delivery to wrong domain (user@2.2.2.2)")
    body = make_rfc822_body(sender, "user@2.2.2.2")
    status, resp = mxdeliv_post(server_ip, sender, ["user@2.2.2.2"], body)
    if status == 404:
        print(f"  ✓ Wrong domain rejected (404): {resp.strip()[:80]}")
    else:
        raise Exception(f"Test 5 FAILED: expected 404, got {status}: {resp[:200]}")

    # ─── Test 6: Reject delivery to wrong bracketed domain ───
    print("\n  Test 6: Reject delivery to wrong bracketed domain (user@[3.3.3.3])")
    body = make_rfc822_body(sender, "user@[3.3.3.3]")
    status, resp = mxdeliv_post(server_ip, sender, ["user@[3.3.3.3]"], body)
    if status == 404:
        print(f"  ✓ Wrong bracketed domain rejected (404): {resp.strip()[:80]}")
    else:
        raise Exception(f"Test 6 FAILED: expected 404, got {status}: {resp[:200]}")

    # ─── Test 7: Reject delivery to hostname on IP-based server ───
    print("\n  Test 7: Reject delivery to hostname on IP-based server")
    body = make_rfc822_body(sender, "user@example.com")
    status, resp = mxdeliv_post(server_ip, sender, ["user@example.com"], body)
    if status == 404:
        print(f"  ✓ Wrong hostname rejected (404): {resp.strip()[:80]}")
    else:
        raise Exception(f"Test 7 FAILED: expected 404, got {status}: {resp[:200]}")

    # ─── Test 8: Accept delivery to non-existent user (200 OK but silently drop) ───
    print("\n  Test 8: Accept delivery to non-existent user (silent drop)")
    fake_user = f"nonexistent_user_test22@[{server_ip}]"
    body = make_rfc822_body(sender, fake_user)
    status, resp = mxdeliv_post(server_ip, sender, [fake_user], body)
    if status == 200:
        print(f"  ✓ Non-existent user accepted silently (200 OK)")
    else:
        raise Exception(f"Test 8 FAILED: expected 200 OK, got {status}: {resp[:200]}")

    # ─── Test 9: Mixed recipients (valid domain + invalid domain) ───
    print("\n  Test 9: Mixed recipients (real user + wrong domain)")
    invalid_rcpt = "other@2.2.2.2"
    body = make_rfc822_body(sender, user_email)
    status, resp = mxdeliv_post(server_ip, sender, [user_email, invalid_rcpt], body)
    if status == 200:
        print(f"  ✓ Mixed recipients: valid domain accepted, invalid ignored (200)")
    else:
        raise Exception(f"Test 9 FAILED: expected 200, got {status}: {resp[:200]}")

    # ─── Test 10: All recipients wrong domain → 404 ───
    print("\n  Test 10: All recipients wrong domain")
    body = make_rfc822_body(sender, "x@9.9.9.9")
    status, resp = mxdeliv_post(server_ip, sender, ["x@9.9.9.9", "y@8.8.8.8"], body)
    if status == 404:
        print(f"  ✓ All wrong-domain recipients rejected (404)")
    else:
        raise Exception(f"Test 10 FAILED: expected 404, got {status}: {resp[:200]}")

    # ─── Test 11: Delivery to real existing user succeeds ───
    print(f"\n  Test 11: Delivery to real existing user ({user_email})")
    body = make_rfc822_body(sender, user_email)
    status, resp = mxdeliv_post(server_ip, sender, [user_email], body)
    if status == 200:
        print(f"  ✓ Delivery to existing user accepted (200 OK)")
    else:
        raise Exception(f"Test 11 FAILED: expected 200, got {status}: {resp[:200]}")

    print("\n" + "=" * 60)
    print("🎉 TEST #22 PASSED! MxDeliv security validation verified.")
    print("=" * 60)
    return True
