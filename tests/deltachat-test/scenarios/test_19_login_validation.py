"""
Test #19: Login Domain Validation

This test verifies that the IMAP LOGIN command validates the username's
domain part before creating a JIT account. It prevents abuse where any
arbitrary username can be used to create accounts.

Test cases:
1. Valid login with correct @[IP] domain → should succeed
2. URL-encoded brackets @%5bIP%5d → should be REJECTED
3. Wrong/arbitrary domain @abcd → should be REJECTED
4. Multiple @ signs x@y@z → should be REJECTED
5. Different IP address @[10.0.0.1] → should be REJECTED
"""

import socket
import ssl
import random
import string
import time
import urllib.parse
from deltachat_rpc_client import EventType


def random_string(length=9):
    chars = string.ascii_lowercase + string.digits
    return ''.join(random.choices(chars, k=length))


def imap_connect(host, port=993, use_ssl=True):
    """Create a raw IMAP connection to test LOGIN directly."""
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.settimeout(10)

    if use_ssl:
        context = ssl.create_default_context()
        context.check_hostname = False
        context.verify_mode = ssl.CERT_NONE
        sock = context.wrap_socket(sock, server_hostname=host)

    sock.connect((host, port))
    # Read greeting
    greeting = sock.recv(4096).decode('utf-8', errors='replace')
    return sock, greeting


def imap_command(sock, tag, command):
    """Send an IMAP command and read the response."""
    full_cmd = f"{tag} {command}\r\n"
    sock.sendall(full_cmd.encode('utf-8'))

    response = b""
    while True:
        chunk = sock.recv(4096)
        if not chunk:
            break
        response += chunk
        decoded = response.decode('utf-8', errors='replace')
        # Check if we got the tagged response
        if f"{tag} OK" in decoded or f"{tag} NO" in decoded or f"{tag} BAD" in decoded:
            break
    return response.decode('utf-8', errors='replace')


def test_imap_login(host, username, password, expect_success, port=993, use_ssl=True):
    """Test a single IMAP LOGIN attempt and return True if result matches expectation."""
    sock = None
    try:
        sock, greeting = imap_connect(host, port, use_ssl)
        response = imap_command(sock, "A1", f"LOGIN {username} {password}")

        is_ok = "A1 OK" in response
        is_no = "A1 NO" in response

        if expect_success and is_ok:
            return True, "OK (as expected)"
        elif not expect_success and is_no:
            return True, "REJECTED (as expected)"
        elif expect_success and is_no:
            return False, f"Expected OK but got NO: {response.strip()}"
        elif not expect_success and is_ok:
            return False, f"Expected NO but got OK: {response.strip()}"
        else:
            return False, f"Unexpected response: {response.strip()}"
    except Exception as e:
        return False, f"Connection error: {e}"
    finally:
        if sock:
            try:
                imap_command(sock, "A2", "LOGOUT")
            except Exception:
                pass
            try:
                sock.close()
            except Exception:
                pass


def run(dc, remotes):
    REMOTE1, REMOTE2 = remotes

    print("\n" + "=" * 60)
    print("TEST #19: Login Domain Validation")
    print("=" * 60)

    # Determine IMAP port and SSL settings
    imap_port = 993
    use_ssl = True

    password = random_string(16)

    # Test 1: Valid login with correct @[IP] domain
    print("\n  Test 1: Valid login with correct domain")
    username1 = f"{random_string(10)}@[{REMOTE1}]"
    success, msg = test_imap_login(REMOTE1, username1, password, True, imap_port, use_ssl)
    if success:
        print(f"  ✓ {msg}: {username1}")
    else:
        raise Exception(f"Test 1 FAILED: {msg}")

    # Test 2: Valid login with bare IP (should be normalized)
    print("\n  Test 2: Valid login with bare IP (normalized)")
    username2 = f"{random_string(10)}@{REMOTE1}"
    success, msg = test_imap_login(REMOTE1, username2, password, True, imap_port, use_ssl)
    if success:
        print(f"  ✓ {msg}: {username2}")
    else:
        raise Exception(f"Test 2 FAILED: {msg}")

    # Test 3: URL-encoded brackets - should be REJECTED
    print("\n  Test 3: URL-encoded brackets (should be rejected)")
    username3 = f"{random_string(10)}@%5b{REMOTE1}%5d"
    success, msg = test_imap_login(REMOTE1, username3, password, False, imap_port, use_ssl)
    if success:
        print(f"  ✓ {msg}: {username3}")
    else:
        raise Exception(f"Test 3 FAILED: {msg}")

    # Test 4: Wrong/arbitrary domain - should be REJECTED
    print("\n  Test 4: Wrong domain (should be rejected)")
    username4 = f"{random_string(10)}@abcd"
    success, msg = test_imap_login(REMOTE1, username4, password, False, imap_port, use_ssl)
    if success:
        print(f"  ✓ {msg}: {username4}")
    else:
        raise Exception(f"Test 4 FAILED: {msg}")

    # Test 5: Multiple @ signs - should be REJECTED
    print("\n  Test 5: Multiple @ signs (should be rejected)")
    username5 = f"x@y@z"
    success, msg = test_imap_login(REMOTE1, username5, password, False, imap_port, use_ssl)
    if success:
        print(f"  ✓ {msg}: {username5}")
    else:
        raise Exception(f"Test 5 FAILED: {msg}")

    # Test 6: Different IP address - should be REJECTED
    print("\n  Test 6: Different IP address (should be rejected)")
    username6 = f"{random_string(10)}@[10.0.0.1]"
    success, msg = test_imap_login(REMOTE1, username6, password, False, imap_port, use_ssl)
    if success:
        print(f"  ✓ {msg}: {username6}")
    else:
        raise Exception(f"Test 6 FAILED: {msg}")

    print("\n" + "=" * 60)
    print("🎉 TEST #19 PASSED! Login domain validation verified.")
    print("=" * 60)
    return True
