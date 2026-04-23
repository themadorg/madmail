# SMTP timing / limit sweep (10–70 MB). For a roundtrip hash check
# without the journalctl no-logging assertions, use test_23_bigfile_roundtrip
# and for cmlxc: relay_minitest/test_bigfile.py.
import os
import hashlib
import time
import shutil
import subprocess
from deltachat_rpc_client.const import MessageState, EventType

MAD_SERVICE = "madmail.service"
MAD_CONFIG_PATHS = ("/etc/madmail/madmail.conf", "/etc/maddy/maddy.conf")

# --- SSH Utility Functions (adapted from test_08) ---

def run_ssh_command(remote, command):
    """Run a command on remote server via SSH"""
    result = subprocess.run(
        ["ssh", f"root@{remote}", command],
        capture_output=True,
        text=True,
        timeout=30
    )
    return result.returncode, result.stdout, result.stderr


def _config_sed(remote, pattern: str) -> None:
    for path in MAD_CONFIG_PATHS:
        run_ssh_command(
            remote, f"test -f {path} && sed -i '{pattern}' {path} || true"
        )


def disable_logging(remote):
    """Disable logging in server config"""
    print(f"  Disabling logging on {remote}...")
    for pattern in (
        "s/^log .*/log off/",
        "s/debug .*/debug no/g",
    ):
        _config_sed(remote, pattern)
    run_ssh_command(remote, f"systemctl restart {MAD_SERVICE}")
    time.sleep(5)


def enable_logging(remote):
    """Re-enable logging"""
    print(f"  Re-enabling logging on {remote}...")
    _config_sed(remote, "s/^log off/log stderr/")
    run_ssh_command(remote, f"systemctl restart {MAD_SERVICE}")
    time.sleep(3)


def set_server_limits(remote, limit):
    """Set appendlimit and max_message_size in server config"""
    print(f"  Setting limits to {limit} on {remote}...")
    for pat in (
        f"s/appendlimit [0-9A-Z]*/appendlimit {limit}/g",
        f"s/max_message_size [0-9A-Z]*/max_message_size {limit}/g",
    ):
        _config_sed(remote, pat)
    run_ssh_command(remote, f"systemctl restart {MAD_SERVICE}")
    time.sleep(3)


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
        "Shadowsocks: listening",
        "signal received"
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


def run(sender, receiver, test_dir, remotes):
    REMOTE1, REMOTE2 = remotes
    
    print("\n" + "="*50)
    print("TEST #9: Big File Transfers with No-Log & Limits")
    print("="*50)
    
    try:
        # Step 0: Enable logging to capture potential initialization and connectivity issues
        print("\nStep 0: Enabling logging to capture setup issues...")
        enable_logging(REMOTE1)
        enable_logging(REMOTE2)

        # Step 1: Initialize Servers with limits and no logging
        print("\nStep 1: Setting initial 50M limit and disabling logs...")
        set_server_limits(REMOTE1, "50M")
        set_server_limits(REMOTE2, "50M")
        disable_logging(REMOTE1)
        disable_logging(REMOTE2)
        
        # Wait for service restart logs to settle before recording cursor
        print("  Waiting for logs to settle...")
        time.sleep(2)
        cursor1 = get_journal_cursor(REMOTE1)
        cursor2 = get_journal_cursor(REMOTE2)
        
        sizes_mb = [10, 20, 30, 40, 50, 60, 70]
        results = []
        
        receiver_email = receiver.get_config("addr")
        receiver_contact = sender.get_contact_by_addr(receiver_email)
        if receiver_contact:
            chat = receiver_contact.create_chat()
        else:
            chat = sender.create_chat(receiver)
        
        limit_increased = False
        
        for size in sizes_mb:
            print(f"\n--- Testing {size}MB file ---")
            
            # Generate local file
            file_path = os.path.join(test_dir, f"bigfile_{size}MB_sent.bin")
            with open(file_path, "wb") as f:
                f.write(os.urandom(size * 1024 * 1024))
            
            sender.clear_all_events()
            
            # Send
            msg = chat.send_file(os.path.abspath(file_path))
            
            # Measure Encryption
            start_crypt = time.time()
            crypt_duration = 0
            while True:
                snap = msg.get_snapshot()
                if snap.state >= MessageState.OUT_PENDING:
                    crypt_duration = time.time() - start_crypt
                    break
                if snap.state == MessageState.OUT_FAILED:
                    print(f"  FAILED during encryption for {size}MB")
                    break
                time.sleep(0.1)
            
            if crypt_duration == 0:
                results.append({"size": size, "status": "FAIL_CRYPT"})
                continue

            print(f"  Encryption: {crypt_duration:.2f}s")
            
            # Measure Sending
            start_send = time.time()
            send_duration = 0
            failed = False
            
            while True:
                snap = msg.get_snapshot()
                if snap.state >= MessageState.OUT_DELIVERED:
                    send_duration = time.time() - start_send
                    break
                if snap.state == MessageState.OUT_FAILED:
                    # Note: Base64 encoding adds ~33% overhead. 
                    # 40MB binary ~ 53MB SMTP message.
                    print(f"  Failure detected for {size}MB. (Note: 50M limit includes ~33% Base64 overhead)")
                    failed = True
                    break
                time.sleep(1.0)
            
            if failed and size > 50 and not limit_increased:
                print(f"\n>>> Increasing limits to 100M to allow {size}MB+ transfers...")
                set_server_limits(REMOTE1, "100M")
                set_server_limits(REMOTE2, "100M")
                limit_increased = True
                
                # Update cursors after restart to avoid counting startup logs
                print("  Updating journal cursors after limit change...")
                time.sleep(5)
                cursor1 = get_journal_cursor(REMOTE1)
                cursor2 = get_journal_cursor(REMOTE2)
                
                print(f">>> Retrying {size}MB transfer...")
                # Re-send the same file
                msg = chat.send_file(os.path.abspath(file_path))
                start_send = time.time()
                while True:
                    snap = msg.get_snapshot()
                    if snap.state >= MessageState.OUT_DELIVERED:
                        send_duration = time.time() - start_send
                        break
                    if snap.state == MessageState.OUT_FAILED:
                        raise Exception(f"Retry failed for {size}MB even after increasing limit")
                    time.sleep(1.0)
                failed = False

            if failed:
                results.append({"size": size, "status": "FAIL_SMTP"})
                continue
                
            print(f"  SMTP Transfer: {send_duration:.2f}s")
            results.append({
                "size": size,
                "crypt": crypt_duration,
                "send": send_duration,
                "total": crypt_duration + send_duration,
                "status": "SUCCESS"
            })
            
        # Summary Table
        print("\n" + "="*70)
        print(f"{'Size (MB)':<10} | {'Status':<10} | {'Crypt (s)':<12} | {'Send (s)':<12} | {'Total (s)':<12}")
        print("-" * 70)
        for res in results:
            if res["status"] == "SUCCESS":
                print(f"{res['size']:<10} | {res['status']:<10} | {res['crypt']:<12.2f} | {res['send']:<12.2f} | {res['total']:<12.2f}")
            else:
                print(f"{res['size']:<10} | {res['status']:<10} | {'-':<12} | {'-':<12} | {'-':<12}")
        print("="*70)
        
        # Step 2: Check for logs
        print("\nStep 2: Checking for new logs during big file transfers...")
        new_logs1 = count_new_logs(REMOTE1, cursor1)
        new_logs2 = count_new_logs(REMOTE2, cursor2)
        
        print(f"  Server 1 new log entries: {new_logs1}")
        print(f"  Server 2 new log entries: {new_logs2}")
        
        MAX_ALLOWED_LOGS = 10  # Reasonable threshold for minor system noise
        if cursor1 is None or cursor2 is None:
            MAX_ALLOWED_LOGS = 80
        if new_logs1 > MAX_ALLOWED_LOGS or new_logs2 > MAX_ALLOWED_LOGS:
            print(f"\n✗ FAILED: Unexpected logs were generated during big file transfer!")
            
            # Show what logs were generated
            print("\n  Recent filtered logs from Server 1:")
            _, logs1, _ = run_ssh_command(
                REMOTE1,
                f"journalctl -u {MAD_SERVICE} --after-cursor='{cursor1}' --no-pager 2>/dev/null",
            )
            print(logs1)

            print("\n  Recent filtered logs from Server 2:")
            _, logs2, _ = run_ssh_command(
                REMOTE2,
                f"journalctl -u {MAD_SERVICE} --after-cursor='{cursor2}' --no-pager 2>/dev/null",
            )
            print(logs2)

            raise Exception(f"Logs were generated during big file transfer. Server1: {new_logs1}, Server2: {new_logs2}")
        else:
            print(f"\n✓ SUCCESS: No significant logs generated during transfers!")

        return True
        
    finally:
        print("\nRestoring server settings...")
        enable_logging(REMOTE1)
        enable_logging(REMOTE2)
        # Restore a reasonable default limit
        set_server_limits(REMOTE1, "100M")
        set_server_limits(REMOTE2, "100M")

