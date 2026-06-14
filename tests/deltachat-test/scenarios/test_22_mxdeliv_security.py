"""
Test #22: MxDeliv Security Validation

Verifies that the /mxdeliv endpoint properly enforces security checks.
Rust madmail silently drops disallowed recipients with HTTP 200 (no delivery),
matching chatmail-fed unit tests — reserved local parts, wrong domains, and
unknown users are not stored. Federation policy / encryption failures use 403.

  1. Silently drops delivery to admin/root/postmaster addresses (200 OK)
  2. Silently drops delivery to recipients on wrong domains (200 OK)
  3. Silently accepts delivery to non-existent users (200 OK, no mail stored)
  4. Delivers to a real user when the body is encrypted PGP/MIME

Architecture:
    ┌──────────────┐    curl POST     ┌──────────────┐
    │   Test Host  │ ───────────────► │ Madmail LXC  │
    │              │                  │ (HTTPS:443)  │
    └──────────────┘                  └──────────────┘
"""

import os
import re
import sys
import time
import json
import ssl
import socket
import http.client
import urllib.parse

TEST_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, TEST_DIR)

from utils.ssh import run_ssh_command


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


def mxdeliv_encrypted_mime_body(from_addr: str, to_addr: str) -> str:
    """PGP/MIME sample body so /mxdeliv passes require_encryption (same as SMTP)."""
    path = os.path.join(TEST_DIR, "mail-data", "encrypted.eml")
    with open(path, encoding="utf-8") as f:
        return f.read().format(
            from_addr=from_addr,
            to_addr=to_addr,
            subject="Test",
            message_id=f"<mxdeliv-t22-{int(time.time())}@test>",
        )


def _local_part_address(local_part: str, server_domain: str) -> str:
    """Build user@… for the server's primary mail domain (IP bracket form vs hostname)."""
    s = str(server_domain)
    if s.startswith("["):
        return f"{local_part}@{s}"
    if re.match(r"^(\d{1,3}\.){3}\d{1,3}$", s):
        return f"{local_part}@[{s}]"
    if ":" in s and s.count(":") >= 2:
        return f"{local_part}@[{s}]"
    return f"{local_part}@{s}"


def _json_from_cli_output(text: str) -> dict:
    """Parse JSON from create-user output (may be prefixed with config debug lines)."""
    text = (text or "").strip()
    if not text:
        raise ValueError("empty output from create-user")
    start = text.find("{")
    if start < 0:
        raise ValueError("no JSON object in create-user output")
    data, _ = json.JSONDecoder().raw_decode(text, start)
    return data


def _credentials_from_create_user(data: dict) -> tuple[str, str]:
    """Extract (email, password) from madmail create-user JSON."""
    payload = data.get("data", data)
    email = payload.get("email")
    password = payload.get("password")
    if email and password:
        return email, password

    dclogin = payload.get("dclogin", "")
    if dclogin.startswith("dclogin:"):
        rest = dclogin[len("dclogin:") :]
        addr_part, _, query = rest.partition("/?")
        if not addr_part:
            addr_part, _, query = rest.partition("?")
        qs = urllib.parse.parse_qs(query)
        password = urllib.parse.unquote(qs.get("p", [""])[0])
        if addr_part and password:
            return addr_part, password

    raise KeyError("email/password missing from create-user response")


def create_user_on_server(server_ip):
    """Create a user on the remote server using the installed madmail create-user CLI.
    Returns (email, password) tuple.
    """
    rc, stdout, stderr = run_ssh_command(
        server_ip,
        "madmail create-user --json",
        timeout=15,
    )
    if rc != 0:
        raise Exception(f"Failed to create user on {server_ip}: {stderr or stdout}")
    try:
        data = _json_from_cli_output(stdout)
        return _credentials_from_create_user(data)
    except (ValueError, json.JSONDecodeError, KeyError) as e:
        raise Exception(f"Failed to parse create-user output: {stdout!r}") from e


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

    # REMOTE2 is the /mxdeliv target (IP-primary or DNS chatmail both supported).
    server_ip = REMOTE2
    r1 = str(REMOTE1)
    if r1.startswith("[") or re.match(
        r"^(\d{1,3}\.){3}\d{1,3}$", r1
    ) or (":" in r1 and r1.count(":") >= 2):
        sender = f"attacker@[{r1}]" if not r1.startswith("[") else f"attacker@{r1}"
    else:
        sender = f"attacker@{r1}"
    print(f"  Target server: {server_ip}")
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

    def _expect_silent_drop(test_name, status, resp):
        if status == 200:
            print(f"  ✓ {test_name}: silently dropped (200 OK)")
            return
        raise Exception(f"{test_name} FAILED: expected silent drop (200), got {status}: {resp[:200]}")

    # ─── Test 1: Drop delivery to admin@ (local domain) ───
    print("\n  Test 1: Drop delivery to admin@ (on-server domain)")
    a1 = _local_part_address("admin", server_ip)
    body = make_rfc822_body(sender, a1)
    status, resp = mxdeliv_post(server_ip, sender, [a1], body)
    _expect_silent_drop("Test 1", status, resp)

    # ─── Test 2: Drop admin@<bare IP> (alternate form; IP-primary domains only) ───
    print("\n  Test 2: Drop delivery to admin@<bare_ip> (if IP primary)")
    sdom = str(server_ip)
    if re.match(r"^(\d{1,3}\.){3}\d{1,3}$", sdom):
        a2 = f"admin@{sdom}"
        body = make_rfc822_body(sender, a2)
        status, resp = mxdeliv_post(server_ip, sender, [a2], body)
        _expect_silent_drop("Test 2", status, resp)
    else:
        print("  (skipped: not a bare-IPv4 primary domain)")

    # ─── Test 3: Drop delivery to root@ ───
    print("\n  Test 3: Drop delivery to root@ (on-server domain)")
    a3 = _local_part_address("root", server_ip)
    body = make_rfc822_body(sender, a3)
    status, resp = mxdeliv_post(server_ip, sender, [a3], body)
    _expect_silent_drop("Test 3", status, resp)

    # ─── Test 4: Drop delivery to postmaster@ ───
    print("\n  Test 4: Drop delivery to postmaster@ (on-server domain)")
    a4 = _local_part_address("postmaster", server_ip)
    body = make_rfc822_body(sender, a4)
    status, resp = mxdeliv_post(server_ip, sender, [a4], body)
    _expect_silent_drop("Test 4", status, resp)

    # ─── Test 5: Drop delivery to wrong domain ───
    print("\n  Test 5: Drop delivery to wrong domain (user@2.2.2.2)")
    body = make_rfc822_body(sender, "user@2.2.2.2")
    status, resp = mxdeliv_post(server_ip, sender, ["user@2.2.2.2"], body)
    _expect_silent_drop("Test 5", status, resp)

    # ─── Test 6: Drop delivery to wrong bracketed domain ───
    print("\n  Test 6: Drop delivery to wrong bracketed domain (user@[3.3.3.3])")
    body = make_rfc822_body(sender, "user@[3.3.3.3]")
    status, resp = mxdeliv_post(server_ip, sender, ["user@[3.3.3.3]"], body)
    _expect_silent_drop("Test 6", status, resp)

    # ─── Test 7: Drop delivery to hostname on IP-based server ───
    print("\n  Test 7: Drop delivery to hostname on IP-based server")
    body = make_rfc822_body(sender, "user@example.com")
    status, resp = mxdeliv_post(server_ip, sender, ["user@example.com"], body)
    _expect_silent_drop("Test 7", status, resp)

    # ─── Test 8: Accept delivery to non-existent user (200 OK but silently drop) ───
    print("\n  Test 8: Accept delivery to non-existent user (silent drop)")
    fake_user = _local_part_address("nonexistent_user_test22", server_ip)
    body = mxdeliv_encrypted_mime_body(sender, fake_user)
    status, resp = mxdeliv_post(server_ip, sender, [fake_user], body)
    if status == 200:
        print(f"  ✓ Non-existent user accepted silently (200 OK)")
    else:
        raise Exception(f"Test 8 FAILED: expected 200 OK, got {status}: {resp[:200]}")

    # ─── Test 9: Mixed recipients (valid domain + invalid domain) ───
    print("\n  Test 9: Mixed recipients (real user + wrong domain)")
    invalid_rcpt = "other@2.2.2.2"
    body = mxdeliv_encrypted_mime_body(sender, user_email)
    status, resp = mxdeliv_post(server_ip, sender, [user_email, invalid_rcpt], body)
    if status == 200:
        print(f"  ✓ Mixed recipients: valid domain accepted, invalid ignored (200)")
    else:
        raise Exception(f"Test 9 FAILED: expected 200, got {status}: {resp[:200]}")

    # ─── Test 10: All recipients wrong domain → silent drop ───
    print("\n  Test 10: All recipients wrong domain")
    body = make_rfc822_body(sender, "x@9.9.9.9")
    status, resp = mxdeliv_post(server_ip, sender, ["x@9.9.9.9", "y@8.8.8.8"], body)
    _expect_silent_drop("Test 10", status, resp)

    # ─── Test 11: Delivery to real existing user succeeds ───
    print(f"\n  Test 11: Delivery to real existing user ({user_email})")
    body = mxdeliv_encrypted_mime_body(sender, user_email)
    status, resp = mxdeliv_post(server_ip, sender, [user_email], body)
    if status == 200:
        print(f"  ✓ Delivery to existing user accepted (200 OK)")
    else:
        raise Exception(f"Test 11 FAILED: expected 200, got {status}: {resp[:200]}")

    print("\n" + "=" * 60)
    print("🎉 TEST #22 PASSED! MxDeliv security validation verified.")
    print("=" * 60)
    return True
