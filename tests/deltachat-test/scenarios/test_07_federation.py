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
      message delivery.
"""

import time
import subprocess


# ---------------------------------------------------------------------------
# SSH + iptables helpers
# ---------------------------------------------------------------------------

SSH_OPTS = "-o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o ConnectTimeout=5"


def _ssh(remote, cmd, timeout=15):
    """Run a command on a remote server via SSH.  Returns subprocess result."""
    full = f"ssh {SSH_OPTS} root@{remote} '{cmd}'"
    return subprocess.run(full, shell=True, capture_output=True, text=True, timeout=timeout)


def _ensure_iptables(remote):
    """Make sure iptables is available inside the server."""
    r = _ssh(remote, "which iptables")
    if r.returncode != 0:
        print(f"    Installing iptables on {remote}...")
        _ssh(remote, "apt-get update -qq && apt-get install -y -qq iptables", timeout=30)


def _block_ports(remotes, ports):
    """Block the given TCP ports (INPUT + OUTPUT) on every remote."""
    for remote in remotes:
        for port in ports:
            _ssh(remote, f"iptables -A INPUT  -p tcp --dport {port} -j REJECT --reject-with tcp-reset")
            _ssh(remote, f"iptables -A OUTPUT -p tcp --dport {port} -j REJECT --reject-with tcp-reset")


def _flush_iptables(remotes):
    """Flush all iptables rules on every remote."""
    for remote in remotes:
        _ssh(remote, "iptables -F")


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


def _try_deliver(sender, receiver, text, max_wait=30):
    """Send a message and wait for delivery.  Returns True if delivered within max_wait."""
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
        },
        {
            "name": "HTTP Only (80)",
            "desc": "Block SMTP (25) + HTTPS (443) → only HTTP (80) available",
            "block": [25, 443],
        },
        {
            "name": "SMTP Only (25)",
            "desc": "Block HTTP (80) + HTTPS (443) → only SMTP (25) available",
            "block": [80, 443],
        },
    ]

    results = []

    for i, scenario in enumerate(scenarios, 1):
        name = scenario["name"]
        desc = scenario["desc"]
        ports_to_block = scenario["block"]

        print(f"  Scenario {i}/3: {name}")
        print(f"    {desc}")

        # 1. Flush — start from a clean slate
        _flush_iptables(remotes)
        time.sleep(3)  # let any queued messages drain

        # 2. Block the specified ports on both servers
        _block_ports(remotes, ports_to_block)
        print(f"    Blocked ports {ports_to_block} on both servers")
        time.sleep(1)  # let rules take effect

        # 3. Try to deliver a unique message acc1 → acc2
        msg = f"PortTest [{name}]: acc1 -> acc2 [{timestamp}]"
        print(f"    Sending: {msg}")
        delivered = _try_deliver(acc1, acc2, msg, max_wait=30)

        if delivered:
            print(f"    ✓ DELIVERED — federation works via {name}")
        else:
            print(f"    ✗ NOT DELIVERED — federation does NOT work via {name}")

        results.append({"name": name, "delivered": delivered})

    # 4. Restore — flush all rules so subsequent tests are not affected
    _flush_iptables(remotes)
    print("\n  iptables flushed on both servers (rules restored).")

    # Summary
    print("\n  ┌─────────────────────────┬────────────┐")
    print("  │ Scenario                │ Result     │")
    print("  ├─────────────────────────┼────────────┤")
    for r in results:
        status = "✓ WORKS " if r["delivered"] else "✗ FAILED"
        print(f"  │ {r['name']:<23} │ {status}  │")
    print("  └─────────────────────────┴────────────┘")

    print("\n✓ Part C complete: Port-based federation analysis finished!")
    print("✓ Federation test complete: All parts (A + B + C) verified!")
    return acc3
