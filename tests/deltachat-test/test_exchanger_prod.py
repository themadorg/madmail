#!/usr/bin/env python3
"""
Quick production test: Send a message between two madmail servers
via the madexchanger relay.

Usage:
    .venv/bin/python test_exchanger_prod.py
"""

import sys
import os
import time
import json
import ssl
import random
import string
import urllib.request
import urllib.parse

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from deltachat_rpc_client import DeltaChat, Rpc, EventType

S1 = ""
S2 = ""
EXCHANGER = ""
EXCHANGER_PORT = 443
ADMIN_TOKEN = "madex-prod-token-2026"


def random_string(length=12):
    return ''.join(random.choices(string.ascii_lowercase + string.digits, k=length))


def register_account(server_ip):
    """Register a new account via chatmail /new endpoint and return QR-style login."""
    username = random_string(12)
    password = random_string(20)
    data = urllib.parse.urlencode({"user": username, "password": password}).encode()
    req = urllib.request.Request(f"https://{server_ip}/new", data=data, method="POST")
    ctx = ssl.create_default_context()
    ctx.check_hostname = False
    ctx.verify_mode = ssl.CERT_NONE
    with urllib.request.urlopen(req, timeout=10, context=ctx) as resp:
        result = json.loads(resp.read())
    # The API returns email as user@[IP] — get the raw parts
    raw_email = result["email"]
    passwd = result["password"]
    # Parse actual username (strip brackets from domain)
    if "@[" in raw_email:
        user_part = raw_email.split("@[")[0]
    else:
        user_part = raw_email.split("@")[0]
    print(f"  Registered: {user_part}@{server_ip}")
    return user_part, passwd


def create_dc_account(dc, server_ip, username, password):
    """Create and configure a DC account using dclogin QR format."""
    print(f"  Creating account {username}@{server_ip}...")
    account = dc.add_account()
    encoded_password = urllib.parse.quote(password, safe="")
    login_uri = (
        f"dclogin:{username}@{server_ip}/?"
        f"p={encoded_password}&v=1&ip=993&sp=465&ic=3&ss=default"
    )
    account.set_config_from_qr(login_uri)
    account.set_config("displayname", f"Test {random_string(4)}")
    account.start_io()

    # Give the account time to connect and reach IDLE
    print(f"  Waiting 10s for IMAP connection...")
    time.sleep(10)
    addr = account.get_config("addr")
    print(f"  ✓ {addr} ready.")
    return account


def get_exchanger_stats():
    """Query exchanger admin API for stats."""
    rpc_body = json.dumps({
        "method": "GET",
        "resource": "/admin/stats",
        "headers": {"Authorization": f"Bearer {ADMIN_TOKEN}"},
        "body": {}
    }).encode()
    req = urllib.request.Request(
        f"https://{EXCHANGER}:{EXCHANGER_PORT}/api/admin",
        data=rpc_body,
        headers={"Content-Type": "application/json"},
        method="POST"
    )
    ctx = ssl.create_default_context()
    ctx.check_hostname = False
    ctx.verify_mode = ssl.CERT_NONE
    try:
        with urllib.request.urlopen(req, timeout=10, context=ctx) as resp:
            return json.loads(resp.read())
    except Exception as e:
        print(f"  ⚠ Failed to get stats: {e}")
        return None


def main():
    timestamp = time.strftime("%Y%m%d_%H%M%S")
    print("=" * 60)
    print("Madexchanger Production Relay Test")
    print("=" * 60)
    print(f"  S1: {S1}")
    print(f"  S2: {S2}")
    print(f"  Exchanger: {EXCHANGER}:{EXCHANGER_PORT}")
    print()

    # Pre-test stats
    print("── Pre-test exchanger stats ──")
    stats_before = get_exchanger_stats()
    relayed_before = 0
    if stats_before:
        body = stats_before.get("body", {})
        relayed_before = body.get("total_relayed", 0)
        print(f"  Relayed: {relayed_before}, Errors: {body.get('total_errors', 0)}")
    print()

    # Register accounts via API first
    print("── Step 1: Registering accounts ──")
    user1, passwd1 = register_account(S1)
    user2, passwd2 = register_account(S2)
    print()

    rpc = Rpc(accounts_dir=f"/tmp/exchanger_test_{timestamp}")
    with rpc:
        dc = DeltaChat(rpc)

        # Create DC accounts using same dclogin format as test_01
        print("── Step 2: Creating DC accounts ──")
        acc1 = create_dc_account(dc, S1, user1, passwd1)
        acc2 = create_dc_account(dc, S2, user2, passwd2)
        acc1_email = acc1.get_config("addr")
        acc2_email = acc2.get_config("addr")
        print(f"  acc1: {acc1_email}")
        print(f"  acc2: {acc2_email}")
        print()

        # Secure Join first (chatmail rejects unencrypted messages)
        print("── Step 3: Secure Join ──")
        from scenarios import test_03_secure_join
        test_03_secure_join.run(rpc, acc1, acc2)
        print("  ✓ Secure join completed")
        print()

        # Send S1 → S2
        print("── Step 4: Send acc1 → acc2 ──")
        msg_text = f"Exchanger Test S1→S2 [{timestamp}]"
        contact1 = acc1.get_contact_by_addr(acc2_email)
        chat1 = contact1.create_chat()
        chat1.send_text(msg_text)
        print(f"  Sent: {msg_text}")

        print(f"  Waiting for acc2 to receive...")
        start = time.time()
        received = False
        while time.time() - start < 90:
            event = acc2.wait_for_event()
            if event and event.kind == EventType.INCOMING_MSG:
                elapsed = time.time() - start
                print(f"  ✓ Message received by acc2 in {elapsed:.1f}s")
                received = True
                break
        if not received:
            print("  ✗ Message NOT received within 90s")
        print()

        # Send S2 → S1
        print("── Step 5: Send acc2 → acc1 ──")
        msg_text2 = f"Exchanger Test S2→S1 [{timestamp}]"
        contact2 = acc2.get_contact_by_addr(acc1_email)
        chat2 = contact2.create_chat()
        chat2.send_text(msg_text2)
        print(f"  Sent: {msg_text2}")

        print(f"  Waiting for acc1 to receive...")
        start = time.time()
        received2 = False
        while time.time() - start < 90:
            event = acc1.wait_for_event()
            if event and event.kind == EventType.INCOMING_MSG:
                elapsed = time.time() - start
                print(f"  ✓ Message received by acc1 in {elapsed:.1f}s")
                received2 = True
                break
        if not received2:
            print("  ✗ Message NOT received within 90s")
        print()

    # Post-test stats
    print("── Step 6: Exchanger stats ──")
    time.sleep(2)
    stats_after = get_exchanger_stats()
    if stats_after:
        body = stats_after.get("body", {})
        relayed_after = body.get("total_relayed", 0)
        new_relayed = relayed_after - relayed_before
        print(f"  Relayed: {relayed_after} (new: {new_relayed})")
        print(f"  Errors:  {body.get('total_errors', 0)}")
        if new_relayed > 0:
            print(f"\n  ✓ Exchanger relayed {new_relayed} new messages!")
        else:
            print(f"\n  ⚠ No new messages recorded in exchanger stats")

    print()
    if received and received2:
        print("=" * 60)
        print("✓ PASSED: Both messages delivered!")
        print("=" * 60)
    else:
        print("=" * 60)
        print("✗ FAILED: Not all messages delivered")
        print("=" * 60)
        sys.exit(1)


if __name__ == "__main__":
    main()
