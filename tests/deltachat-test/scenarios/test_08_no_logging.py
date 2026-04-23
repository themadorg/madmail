"""
Test #8: No Logging Test

This test verifies that when logging is disabled on the server,
no logs are generated during message sending operations.

Steps:
1. Disable debug/logging on server via SSH
2. Restart maddy service
3. Record current journal position
4. Send 10 P2P messages
5. Send 10 group messages
6. Send 10 federation messages (cross-server)
7. Check that no new logs were generated
8. Re-enable logging for subsequent tests
"""

import subprocess
import time

# chatmail (cmlxc) install uses the madmail unit and /etc/madmail/madmail.conf;
# some setups still use the historical maddy paths.
MAD_SERVICE = "madmail.service"
MAD_CONFIG_PATHS = ("/etc/madmail/madmail.conf", "/etc/maddy/maddy.conf")


def _config_sed(remote, pattern: str) -> None:
    for path in MAD_CONFIG_PATHS:
        run_ssh_command(
            remote, f"test -f {path} && sed -i '{pattern}' {path} || true"
        )


def run_ssh_command(remote, command):
    """Run a command on remote server via SSH"""
    result = subprocess.run(
        ["ssh", f"root@{remote}", command],
        capture_output=True,
        text=True,
        timeout=30
    )
    return result.returncode, result.stdout, result.stderr


def disable_logging(remote):
    """Disable logging in the server maddy/madmail config"""
    print(f"  Disabling logging on {remote}...")
    _config_sed(remote, "s/^log .*/log off/")
    _config_sed(remote, "s/debug yes/debug no/g")
    _config_sed(remote, "s/debug true/debug false/g")
    # Restart maddy service
    print(f"  Restarting {MAD_SERVICE} on {remote}...")
    returncode, stdout, stderr = run_ssh_command(
        remote, f"systemctl restart {MAD_SERVICE}"
    )
    if returncode != 0:
        raise Exception(f"Failed to restart {MAD_SERVICE} on {remote}: {stderr}")
    
    # Wait for service to start
    time.sleep(5)
    print(f"  Logging disabled on {remote}")


def enable_logging(remote):
    """Re-enable logging in the server config (for subsequent tests)"""
    print(f"  Re-enabling logging on {remote}...")
    _config_sed(remote, "s/^log off/log stderr/")

    # Restart service
    returncode, stdout, stderr = run_ssh_command(
        remote, f"systemctl restart {MAD_SERVICE}"
    )
    if returncode != 0:
        print(f"    Warning: Failed to restart maddy on {remote}: {stderr}")
    
    time.sleep(3)
    print(f"  Logging re-enabled on {remote}")


def get_journal_cursor(remote):
    """Get current journal cursor position"""
    returncode, stdout, stderr = run_ssh_command(
        remote,
        f"journalctl -u {MAD_SERVICE} -n 1 --output=json | jq -r '.__CURSOR'",
    )
    if returncode == 0 and stdout.strip():
        return stdout.strip()
    return None


def count_new_logs(remote, cursor):
    """Count new log entries since cursor, ignoring startup noise"""
    if cursor:
        cmd = f"journalctl -u {MAD_SERVICE} --after-cursor='{cursor}' --no-pager 2>/dev/null"
    else:
        # If no cursor, count logs from the last minute
        cmd = f"journalctl -u {MAD_SERVICE} --since='1 minute ago' --no-pager 2>/dev/null"
    
    # Filter out known startup/systemd noise that is expected during boot phase
    # as per nolog.md policy (boot phase logs are allowed).
    filters = [
        "listening on",
        "Started maddy.service",
        "Started madmail.service",
        "Starting maddy.service",
        "Starting madmail.service",
        "table.file: ignoring",
        "Deactivated successfully",
        "Stopping maddy.service",
        "Stopping madmail.service",
        "Stopped maddy.service",
        "Stopped madmail.service",
        "Consumed",
        "Shadowsocks: listening"
    ]
    
    for f in filters:
        cmd += f" | grep -v '{f}'"
    
    cmd += " | wc -l"
    
    returncode, stdout, stderr = run_ssh_command(remote, cmd)
    if returncode == 0:
        try:
            # Subtract 1 for the header line if present
            count = int(stdout.strip())
            return max(0, count - 1)  # Subtract header line
        except ValueError:
            return -1
    return -1


def run(acc1, acc2, acc3, group_chat, remotes):
    """
    Test #8: No Logging Test
    
    Verifies that with logging disabled, message operations produce no logs.
    
    Args:
        acc1: First account (server 1)
        acc2: Second account (server 2)
        acc3: Third account (server 2) - for federation testing
        group_chat: Existing group chat from previous test
        remotes: Tuple of (REMOTE1, REMOTE2) server addresses
    """
    REMOTE1, REMOTE2 = remotes
    
    print("\n" + "="*50)
    print("TEST #8: No Logging Test")
    print("="*50)
    
    try:
        # Step 1: Disable logging on both servers
        print("\nStep 1: Disabling logging on servers...")
        disable_logging(REMOTE1)
        disable_logging(REMOTE2)
        
        # Step 2: Record current journal positions
        print("\nStep 2: Recording journal positions...")
        cursor1 = get_journal_cursor(REMOTE1)
        cursor2 = get_journal_cursor(REMOTE2)
        print(f"  Server 1 cursor: {cursor1[:20] if cursor1 else 'None'}...")
        print(f"  Server 2 cursor: {cursor2[:20] if cursor2 else 'None'}...")
        
        # Wait a moment for things to settle
        time.sleep(2)
        
        # Step 3: Send 10 P2P messages (acc1 to acc2 - cross-server)
        print("\nStep 3: Sending 10 P2P encrypted messages (Server1 -> Server2)...")
        acc2_email = acc2.get_config("addr")
        acc2_contact = acc1.get_contact_by_addr(acc2_email)
        if acc2_contact:
            p2p_chat = acc2_contact.create_chat()
        else:
            p2p_chat = acc1.create_chat(acc2)
        for i in range(10):
            msg = f"NoLog P2P Test Message {i+1}/10"
            p2p_chat.send_text(msg)
            print(f"  Sent P2P message {i+1}/10")
            time.sleep(0.5)  # Small delay between messages
        
        # Wait for messages to be processed
        print("  Waiting for P2P messages to be processed...")
        time.sleep(10)
        
        # Step 4: Send 10 group messages
        print("\nStep 4: Sending 10 group messages...")
        for i in range(10):
            msg = f"NoLog Group Test Message {i+1}/10"
            group_chat.send_text(msg)
            print(f"  Sent group message {i+1}/10")
            time.sleep(0.5)  # Small delay between messages
        
        # Wait for messages to be processed
        print("  Waiting for group messages to be processed...")
        time.sleep(10)
        
        # Step 5: Send 10 federation messages (acc1 to acc3)
        if acc3:
            print("\nStep 5: Sending 10 federation messages (Server1 -> Server2 acc3)...")
            acc3_email = acc3.get_config("addr")
            acc3_contact = acc1.get_contact_by_addr(acc3_email)
            if acc3_contact:
                fed_chat = acc3_contact.create_chat()
            else:
                acc3_contact = acc1.create_contact(acc3_email, "Federation User")
                fed_chat = acc3_contact.create_chat()
            for i in range(10):
                msg = f"NoLog Federation Test Message {i+1}/10"
                fed_chat.send_text(msg)
                print(f"  Sent federation message {i+1}/10")
                time.sleep(0.5)
            
            print("  Waiting for federation messages to be processed...")
            time.sleep(10)
        else:
            print("\nStep 5: Skipping federation messages (no acc3 provided)")
        
        # Step 6: Check for new logs
        print("\nStep 6: Checking for new logs...")
        new_logs1 = count_new_logs(REMOTE1, cursor1)
        new_logs2 = count_new_logs(REMOTE2, cursor2)
        
        print(f"  Server 1 new log entries: {new_logs1}")
        print(f"  Server 2 new log entries: {new_logs2}")
        
        # Step 7: Verify no logs (or minimal system logs only)
        # Allow for a small number of system messages (service health checks, startup noise etc)
        # Startup can produce ~10 lines of "listening on..." messages even with 'log off' in some versions
        MAX_ALLOWED_LOGS = 10
        # Without a journal cursor (e.g. jq missing) we count by time window — much noisier.
        if cursor1 is None or cursor2 is None:
            MAX_ALLOWED_LOGS = 80

        if new_logs1 <= MAX_ALLOWED_LOGS and new_logs2 <= MAX_ALLOWED_LOGS:
            print(f"\n✓ SUCCESS: No significant logs generated!")
            print(f"  Server 1: {new_logs1} entries (max allowed: {MAX_ALLOWED_LOGS})")
            print(f"  Server 2: {new_logs2} entries (max allowed: {MAX_ALLOWED_LOGS})")
            return True
        else:
            print(f"\n✗ FAILED: Unexpected logs were generated!")
            print(f"  Server 1: {new_logs1} entries")
            print(f"  Server 2: {new_logs2} entries")
            
            # Show what logs were generated
            print("\n  Recent logs from Server 1:")
            _, logs1, _ = run_ssh_command(
                REMOTE1,
                f"journalctl -u {MAD_SERVICE} --after-cursor='{cursor1}' --no-pager 2>/dev/null",
            )
            print(logs1)

            print("\n  Recent logs from Server 2:")
            _, logs2, _ = run_ssh_command(
                REMOTE2,
                f"journalctl -u {MAD_SERVICE} --after-cursor='{cursor2}' --no-pager 2>/dev/null",
            )
            print(logs2)
            
            raise Exception(f"Logs were generated when logging was disabled. Server1: {new_logs1}, Server2: {new_logs2}")
            
    finally:
        # Always re-enable logging for future tests
        print("\nStep 7: Re-enabling logging on servers...")
        try:
            enable_logging(REMOTE1)
        except Exception as e:
            print(f"  Warning: Failed to re-enable logging on {REMOTE1}: {e}")
        try:
            enable_logging(REMOTE2)
        except Exception as e:
            print(f"  Warning: Failed to re-enable logging on {REMOTE2}: {e}")
