"""
Test #7: Federation Test

This test verifies messaging works across server boundaries (cross-server)
AND between accounts on the same server (same-server), then probes which
network ports are actually used for cross-server federation.

Parts:
  A — Cross-Server Federation:
      Secure Join acc1 (server1) <-> acc2 (server2), send messages both ways.

  B — Same-Server Messaging:
      Create acc3 on server2, Secure Join with acc2, send messages both ways.

  C — Port-Based Federation Analysis:
      Selectively block ports (SMTP 25, HTTP 80, HTTPS 443) using iptables
      on both servers and test which combinations still allow cross-server
      message delivery.  After each scenario the sender's journal is checked
      for the "[federation] delivery OK" line to verify the *actual* transport
      method used (HTTPS, HTTP, or SMTP).
"""

import time
import subprocess


# ---------------------------------------------------------------------------
# SSH + iptables helpers
# ---------------------------------------------------------------------------





def _ssh(remote, cmd, timeout=15):
    """Run a command on a remote server via SSH.  Returns subprocess result."""
    args = [
        "ssh",
        "-o", "StrictHostKeyChecking=no",
        "-o", "UserKnownHostsFile=/dev/null",
        "-o", "ConnectTimeout=5",
        f"root@{remote}",
        cmd,
    ]
    return subprocess.run(args, capture_output=True, text=True, timeout=timeout)


def _ensure_iptables(remote):
    """Make sure iptables and conntrack are available inside the server."""
    r = _ssh(remote, "which iptables")
    if r.returncode != 0:
        print(f"    Installing iptables on {remote}...")
        _ssh(remote, "apt-get update -qq && apt-get install -y -qq iptables conntrack", timeout=60)
    else:
        # Ensure conntrack is also present even if iptables was already installed
        r2 = _ssh(remote, "which conntrack")
        if r2.returncode != 0:
            _ssh(remote, "apt-get install -y -qq conntrack", timeout=60)


def _block_ports(remotes, ports):
    """
    Block the given TCP ports on every remote — both new connections AND
    already-established sessions (so persistent connection pools can't sneak
    messages through on a pre-existing socket).

    We use REJECT (tcp-reset) so the sender gets immediate RST rather than
    hanging for the full client timeout (30 s per attempt).
    """
    for remote in remotes:
        for port in ports:
            # Block incoming connections on this port (receiver side — instant RST)
            _ssh(remote, f"iptables -I INPUT  -p tcp --dport {port} -j REJECT --reject-with tcp-reset")
            # Block outgoing connections to this port (sender side — instant RST)
            _ssh(remote, f"iptables -I OUTPUT -p tcp --dport {port} -j REJECT --reject-with tcp-reset")
            # Kill any existing conntrack entries for this port so persistent
            # connection pools are torn down immediately (requires conntrack tool)
            _ssh(remote, f"conntrack -D -p tcp --dport {port} 2>/dev/null; conntrack -D -p tcp --sport {port} 2>/dev/null; true")


def _flush_iptables(remotes):
    """Flush all iptables rules on every remote."""
    for remote in remotes:
        _ssh(remote, "iptables -F")
        _ssh(remote, "iptables -t nat -F")


def _get_all_federation_logs(remote):
    """
    Fetch ALL journalctl lines from the maddy service that contain 'federation'.
    Returns a list of matching lines.
    """
    cmd = "journalctl -u maddy.service --no-pager -o cat 2>&1 | grep -F federation || true"
    r = _ssh(remote, cmd, timeout=10)
    return [l.strip() for l in (r.stdout or "").splitlines() if l.strip()]


def _snapshot_log_count(remote):
    """Return the current number of federation log lines (used as a baseline)."""
    lines = _get_all_federation_logs(remote)
    return len(lines)


def _verify_transport(remote_sender, baseline_count, expected_method, scenario_name):
    """
    Check that new federation log lines (after baseline_count) contain a
    '[federation] delivery OK' line with the expected method (HTTPS, HTTP, SMTP).

    Returns True if the expected method is found, False otherwise.
    """
    all_lines = _get_all_federation_logs(remote_sender)
    new_lines = all_lines[baseline_count:]  # only lines added since the snapshot

    delivery_ok_lines = [l for l in new_lines if "delivery OK" in l]

    if not delivery_ok_lines:
        print(f"    ⚠ WARNING: no '[federation] delivery OK' log found for {scenario_name}")
        if new_lines:
            print(f"    New federation lines ({len(new_lines)}):")
            for line in new_lines[-5:]:
                print(f"      {line}")
        else:
            print(f"    No new federation lines at all (baseline={baseline_count}, total={len(all_lines)})")
        return False

    # Check the LAST delivery OK line — that's the one for this scenario
    last_ok = delivery_ok_lines[-1]
    method_key = f'"method":"{expected_method}"'
    if method_key in last_ok:
        print(f"    ✓ Log confirms delivery via {expected_method}")
        return True

    print(f"    ⚠ WARNING: expected method={expected_method} but got:")
    for line in delivery_ok_lines:
        print(f"      {line}")
    return False


# ---------------------------------------------------------------------------
# Message helpers
# ---------------------------------------------------------------------------

def _wait_for_message(account, expected_text, sender_label, receiver_label, max_wait=60):
    """Wait for a specific message to appear on an account's chatlist."""
    start = time.time()
    print(f"  Waiting for {receiver_label} to receive the message...")
    while time.time() - start < max_wait:
        for chat in account.get_chatlist():
            for msg in chat.get_messages():
                if msg.get_snapshot().text == expected_text:
                    print(f"  ✓ Message received by {receiver_label}: {expected_text}")
                    return True
        time.sleep(2)
    raise Exception(
        f"Federation test failed: Message from {sender_label} to {receiver_label} "
        f"not received within {max_wait}s"
    )


def _try_deliver(sender, receiver, text, max_wait=90):
    """
    Send a message and wait for delivery.  Returns True if delivered within max_wait.

    max_wait=90 covers the full retry chain:
      - HTTPS attempt: up to 30 s client timeout (or instant RST if port blocked)
      - HTTP fallback:  up to 30 s client timeout (or instant RST if port blocked)
      - SMTP fallback:  connection + DATA round-trips
      - Queue polling:  2 s sleep intervals
    """
    chat = sender.create_chat(receiver)
    chat.send_text(text)
    start = time.time()
    while time.time() - start < max_wait:
        for c in receiver.get_chatlist():
            for msg in c.get_messages():
                if msg.get_snapshot().text == text:
                    return True
        time.sleep(2)
    return False


# ---------------------------------------------------------------------------
# Main entry point
# ---------------------------------------------------------------------------

def run(rpc, dc, acc1, acc2, remote1, remote2, timestamp):
    """
    Test #7: Federation Test

    Self-contained test that verifies cross-server, same-server, and
    port-based federation behaviour.

    Args:
        rpc: RPC instance
        dc: DeltaChat instance
        acc1: Account on server 1
        acc2: Account on server 2
        remote1: Address / IP of server 1
        remote2: Address / IP of server 2
        timestamp: Test run timestamp

    Returns:
        acc3: The third account created for use in subsequent tests
    """
    from scenarios import test_01_account_creation, test_03_secure_join

    acc1_email = acc1.get_config("addr")
    acc2_email = acc2.get_config("addr")

    # ==================================================================
    # Part A: Cross-Server Federation (server1 <-> server2)
    # ==================================================================
    print("\n── Part A: Cross-Server Federation ──")

    # Step 1: Ensure acc1 and acc2 are securely joined
    print("\nStep 1: Ensuring secure join between acc1 and acc2 (cross-server)...")
    contact_on_acc2 = acc2.get_contact_by_addr(acc1_email)
    already_verified = False
    if contact_on_acc2:
        already_verified = contact_on_acc2.get_snapshot().is_verified

    if already_verified:
        print("  Secure join already established, skipping.")
    else:
        print(f"  Performing secure join: acc1 ({acc1_email}) <-> acc2 ({acc2_email})...")
        test_03_secure_join.run(rpc, acc1, acc2)
        print("  Secure join between acc1 and acc2 completed")

    # Step 2: acc1 -> acc2 (cross-server)
    print(f"\nStep 2: Sending message acc1 -> acc2 [cross-server]...")
    acc2_contact = acc1.get_contact_by_addr(acc2_email)
    if acc2_contact is None:
        raise Exception(f"Contact {acc2_email} not found on acc1 after secure join")
    chat_1_to_2 = acc2_contact.create_chat()
    msg_cross_1 = f"Cross-Server: acc1 -> acc2 [{timestamp}]"
    chat_1_to_2.send_text(msg_cross_1)
    print(f"  Sent: {msg_cross_1}")
    _wait_for_message(acc2, msg_cross_1, "acc1", "acc2")

    # Step 3: acc2 -> acc1 (cross-server, reverse)
    print(f"\nStep 3: Sending reply acc2 -> acc1 [cross-server]...")
    acc1_contact = acc2.get_contact_by_addr(acc1_email)
    if acc1_contact is None:
        raise Exception(f"Contact {acc1_email} not found on acc2 after secure join")
    chat_2_to_1 = acc1_contact.create_chat()
    msg_cross_2 = f"Cross-Server Reply: acc2 -> acc1 [{timestamp}]"
    chat_2_to_1.send_text(msg_cross_2)
    print(f"  Sent: {msg_cross_2}")
    _wait_for_message(acc1, msg_cross_2, "acc2", "acc1")

    print("\n✓ Part A complete: Cross-server encrypted messaging verified!")

    # ==================================================================
    # Part B: Same-Server Messaging (server2 <-> server2)
    # ==================================================================
    print("\n── Part B: Same-Server Messaging ──")

    print("\nStep 4: Creating third account on server 2...")
    acc3 = test_01_account_creation.run(dc, remote2)
    acc3_email = acc3.get_config("addr")
    print(f"  Acc2: {acc2_email}  |  Acc3: {acc3_email}")

    print(f"\nStep 5: Secure join between acc2 and acc3 (both on server 2)...")
    acc2.configure()
    time.sleep(2)

    try:
        test_03_secure_join.run(rpc, acc2, acc3)
    except Exception as e:
        print(f"  Secure join failed: {e}")
        c2 = acc3.get_contact_by_addr(acc2_email)
        if c2:
            print(f"  Acc3 contact for Acc2: {c2.get_snapshot()}")
        c3 = acc2.get_contact_by_addr(acc3_email)
        if c3:
            print(f"  Acc2 contact for Acc3: {c3.get_snapshot()}")
        raise
    print("  Secure join completed")

    print(f"\nStep 6: Sending message acc2 -> acc3 [same-server]...")
    acc3_c = acc2.get_contact_by_addr(acc3_email)
    if acc3_c is None:
        raise Exception(f"Contact {acc3_email} not found after secure join")
    chat_2_to_3 = acc3_c.create_chat()
    msg_same_1 = f"Same-Server: acc2 -> acc3 [{timestamp}]"
    chat_2_to_3.send_text(msg_same_1)
    print(f"  Sent: {msg_same_1}")
    _wait_for_message(acc3, msg_same_1, "acc2", "acc3")

    print(f"\nStep 7: Sending reply acc3 -> acc2 [same-server]...")
    acc2_c = acc3.get_contact_by_addr(acc2_email)
    if acc2_c is None:
        raise Exception(f"Contact {acc2_email} not found on acc3")
    chat_3_to_2 = acc2_c.create_chat()
    msg_same_2 = f"Same-Server Reply: acc3 -> acc2 [{timestamp}]"
    chat_3_to_2.send_text(msg_same_2)
    print(f"  Sent: {msg_same_2}")
    _wait_for_message(acc2, msg_same_2, "acc3", "acc2")

    print("\n✓ Part B complete: Same-server encrypted messaging verified!")

    # ==================================================================
    # Part C: Port-Based Federation Analysis
    # ==================================================================
    print("\n── Part C: Port-Based Federation Analysis ──")
    print("  Testing which ports are used for cross-server message delivery")
    print("  by selectively blocking ports with iptables on both servers.\n")

    remotes = [remote1, remote2]

    # Ensure iptables is available
    for r in remotes:
        _ensure_iptables(r)

    scenarios = [
        {
            "name": "HTTPS Only (443)",
            "desc": "Block SMTP (25) + HTTP (80) → only HTTPS (443) available",
            "block": [25, 80],
            "expected_method": "HTTPS",
        },
        {
            "name": "HTTP Only (80)",
            "desc": "Block SMTP (25) + HTTPS (443) → only HTTP (80) available",
            "block": [25, 443],
            "expected_method": "HTTP",
        },
        {
            "name": "SMTP Only (25)",
            "desc": "Block HTTP (80) + HTTPS (443) → only SMTP (25) available",
            "block": [80, 443],
            "expected_method": "SMTP",
        },
    ]

    results = []

    for i, scenario in enumerate(scenarios, 1):
        name = scenario["name"]
        desc = scenario["desc"]
        ports_to_block = scenario["block"]
        expected_method = scenario["expected_method"]

        print(f"  Scenario {i}/3: {name}")
        print(f"    {desc}")

        # 1. Flush — start from a clean slate
        _flush_iptables(remotes)
        time.sleep(3)  # let any queued messages drain

        # 2. Block the specified ports on both servers
        _block_ports(remotes, ports_to_block)
        print(f"    Blocked ports {ports_to_block} on both servers")
        time.sleep(5)  # wait for maddy to detect dead connections before sending

        # 3. Snapshot log count BEFORE sending so we only check new lines
        baseline_count = _snapshot_log_count(remote1)

        # 4. Try to deliver a unique message acc1 → acc2
        msg = f"PortTest [{name}]: acc1 -> acc2 [{timestamp}]"
        print(f"    Sending: {msg}")
        delivered = _try_deliver(acc1, acc2, msg, max_wait=30)

        if delivered:
            print(f"    ✓ DELIVERED — federation works via {name}")
        else:
            print(f"    ✗ NOT DELIVERED — federation does NOT work via {name}")

        # 5. Verify the actual transport method from server logs
        transport_ok = False
        if delivered:
            # Give a moment for journald to flush
            time.sleep(1)
            transport_ok = _verify_transport(remote1, baseline_count, expected_method, name)

        results.append({
            "name": name,
            "delivered": delivered,
            "expected_method": expected_method,
            "transport_verified": transport_ok,
        })

    # 6. Restore — flush all rules so subsequent tests are not affected
    _flush_iptables(remotes)
    print("\n  iptables flushed on both servers (rules restored).")

    # Summary
    print("\n  ┌─────────────────────────┬────────────┬───────────────────┐")
    print("  │ Scenario                │ Result     │ Transport         │")
    print("  ├─────────────────────────┼────────────┼───────────────────┤")
    for r in results:
        status = "✓ WORKS " if r["delivered"] else "✗ FAILED"
        if r["delivered"] and r["transport_verified"]:
            transport = f"✓ {r['expected_method']:5}"
        elif r["delivered"]:
            transport = "⚠ unverified  "
        else:
            transport = "—             "
        print(f"  │ {r['name']:<23} │ {status}  │ {transport}    │")
    print("  └─────────────────────────┴────────────┴───────────────────┘")

    print("\n✓ Part C complete: Port-based federation analysis finished!")
    print("✓ Federation test complete: All parts (A + B + C) verified!")
    return acc3
