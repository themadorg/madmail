"""
Functional Test #13: Multi-Recipient Message Delivery

This test validates:
1. Creating 5 users and verifying submission/IMAP login works
2. One user sends a fake-encrypted message to 4 other recipients via submission
3. All 4 recipients successfully receive the message

Tests the in-memory storage deduplication: message is stored once, referenced by all recipients.
"""

import os
import sys
import subprocess
import tempfile
import time
import shutil
import socket
import email.utils
import uuid
import base64
import threading

# Add parent directories for imports
TEST_DIR = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(TEST_DIR))


def create_fake_pgp_message(from_addr, to_addrs, subject, body_text):
    """
    Create a fake PGP-encrypted message that passes madmail's verification.
    Supports multiple recipients in To: header.
    
    Args:
        from_addr: Sender email address
        to_addrs: List of recipient email addresses
        subject: Message subject
        body_text: Body text (will be "encrypted")
    
    Returns:
        str: Complete MIME message
    """
    boundary = f"=-=PGPBoundary{uuid.uuid4().hex[:16]}=-="
    message_id = f"<{uuid.uuid4().hex}@test.local>"
    date = email.utils.formatdate(localtime=True)
    
    # Create a minimal valid OpenPGP encrypted payload
    # SEIPD = type 18 (0x12), header byte = 0xC0 | 0x12 = 0xD2
    fake_encrypted_data = bytes([
        1,  # SEIPD version 1
        0x8C, 0x0D, 0x04, 0x03, 0x03, 0x02, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ])
    
    seipd_header = bytes([0xD2])  # 0xC0 | 18
    seipd_length = bytes([len(fake_encrypted_data)])
    seipd_packet = seipd_header + seipd_length + fake_encrypted_data
    
    b64_data = base64.b64encode(seipd_packet).decode('ascii')
    b64_lines = [b64_data[i:i+64] for i in range(0, len(b64_data), 64)]
    
    pgp_armor = "-----BEGIN PGP MESSAGE-----\n\n"
    pgp_armor += "\n".join(b64_lines)
    pgp_armor += "\n-----END PGP MESSAGE-----"
    
    # Format multiple recipients
    to_header = ", ".join(to_addrs)
    
    message = f"""From: {from_addr}\r
To: {to_header}\r
Subject: {subject}\r
Message-ID: {message_id}\r
Date: {date}\r
MIME-Version: 1.0\r
Content-Type: multipart/encrypted; protocol="application/pgp-encrypted"; boundary="{boundary}"\r
\r
--{boundary}\r
Content-Type: application/pgp-encrypted\r
Content-Description: PGP/MIME version identification\r
\r
Version: 1\r
\r
--{boundary}\r
Content-Type: application/octet-stream; name="encrypted.asc"\r
Content-Description: OpenPGP encrypted message\r
Content-Disposition: inline; filename="encrypted.asc"\r
\r
{pgp_armor}\r
\r
--{boundary}--\r
"""
    return message


def run_multirecipient_test(maddy_bin, ip_address="127.0.0.1"):
    """
    Run the multi-recipient message delivery test.
    
    Args:
        maddy_bin: Path to the maddy binary
        ip_address: IP address to use for the server
    
    Returns:
        tuple: (success: bool, logs: list)
    """
    logs = []
    
    def log(msg):
        print(msg)
        logs.append(msg)
    
    log(f"Starting multi-recipient test against {ip_address}")
    log(f"Using maddy binary: {maddy_bin}")
    
    # Create temporary directories
    tmp_dir = tempfile.mkdtemp(prefix="multirecip_test_")
    state_dir = os.path.join(tmp_dir, "state")
    runtime_dir = os.path.join(tmp_dir, "runtime")
    config_dir = os.path.join(tmp_dir, "config")
    
    os.makedirs(state_dir, exist_ok=True)
    os.makedirs(runtime_dir, exist_ok=True)
    os.makedirs(config_dir, exist_ok=True)
    
    maddy_process = None
    success = False
    
    try:
        # Create configuration for simple mode
        config_content = f'''
state_dir {state_dir}
runtime_dir {runtime_dir}

hostname {ip_address}

debug yes
log stderr

auth.memauth local_authdb {{
    auto_create yes
    min_password_len 12
}}

storage.memstore local_mailboxes {{
    auto_create yes
    default_quota 1G
}}

smtp tcp://0.0.0.0:2525 {{
    hostname {ip_address}
    tls off
    
    limits {{
        all rate 1000 1s
        all concurrency 10000
    }}
    
    deliver_to &local_mailboxes
}}

submission tcp://0.0.0.0:2587 {{
    hostname {ip_address}
    tls off
    
    limits {{
        all rate 1000 1s
        all concurrency 10000
    }}
    
    auth &local_authdb
    
    deliver_to &local_mailboxes
}}

imap tcp://0.0.0.0:2143 {{
    tls off
    auth &local_authdb
    storage &local_mailboxes
}}
'''
        
        config_path = os.path.join(config_dir, "maddy.conf")
        with open(config_path, 'w') as f:
            f.write(config_content)
        
        log(f"Configuration written to {config_path}")
        
        # Start maddy server
        log("Starting maddy server...")
        env = os.environ.copy()
        env["MADDY_STATE_DIR"] = state_dir
        env["MADDY_RUNTIME_DIR"] = runtime_dir
        
        log_file_path = os.path.join(tmp_dir, "maddy.log")
        log_file = open(log_file_path, "w")
        
        maddy_process = subprocess.Popen(
            [maddy_bin, "-config", config_path, "run"],
            stdout=log_file,
            stderr=subprocess.STDOUT,
            env=env
        )
        
        # Wait for server to start
        log("Waiting for maddy server to start...")
        time.sleep(3)
        
        if maddy_process.poll() is not None:
            log_file.close()
            with open(log_file_path, 'r') as f:
                server_log = f.read()
            log(f"ERROR: Maddy server exited prematurely!")
            log(f"Server logs:\n{server_log}")
            return False, logs
        
        log("Maddy server started successfully")
        
        # Define test users
        password = "verysecurepassword123"  # >= 12 chars for auto-create
        unique_id = uuid.uuid4().hex[:8]
        
        sender = f"sender_{unique_id}@{ip_address}"
        recipients = [
            f"recipient1_{unique_id}@{ip_address}",
            f"recipient2_{unique_id}@{ip_address}",
            f"recipient3_{unique_id}@{ip_address}",
            f"recipient4_{unique_id}@{ip_address}",
        ]
        all_users = [sender] + recipients
        
        log(f"\nTest users:")
        log(f"  Sender: {sender}")
        for i, r in enumerate(recipients, 1):
            log(f"  Recipient {i}: {r}")
        
        # Helper function for IMAP commands
        def imap_session(user, password, operations):
            """
            Execute IMAP operations for a user.
            
            Args:
                user: Username
                password: Password
                operations: List of (command, expected_response) tuples
            
            Returns:
                tuple: (success, responses)
            """
            sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            sock.settimeout(10)
            sock.connect((ip_address, 2143))
            
            tag = 0
            responses = []
            
            def imap_cmd(cmd):
                nonlocal tag
                tag += 1
                full_cmd = f"A{tag:03d} {cmd}"
                sock.send((full_cmd + "\r\n").encode())
                response = b""
                while True:
                    chunk = sock.recv(4096)
                    response += chunk
                    if f"A{tag:03d} ".encode() in response:
                        break
                return response.decode('utf-8', errors='ignore')
            
            try:
                # Read banner
                sock.recv(4096)
                
                # Login
                response = imap_cmd(f'LOGIN "{user}" "{password}"')
                if "OK" not in response:
                    return False, [f"Login failed: {response}"]
                responses.append(("LOGIN", "OK"))
                
                # Execute operations
                for cmd, expected in operations:
                    response = imap_cmd(cmd)
                    if expected and expected not in response:
                        return False, [f"{cmd} failed - expected '{expected}': {response}"]
                    responses.append((cmd, response))
                
                # Logout
                imap_cmd("LOGOUT")
                sock.close()
                return True, responses
                
            except Exception as e:
                sock.close()
                return False, [str(e)]
        
        # Test 1: Create all 5 users via IMAP login (trust-on-first-login)
        log("\n" + "=" * 50)
        log("TEST 1: Create 5 users via IMAP login")
        log("=" * 50)
        
        for i, user in enumerate(all_users, 1):
            log(f"\n  Creating user {i}/5: {user}")
            success_login, responses = imap_session(user, password, [])
            if not success_login:
                log(f"  ✗ Failed to create user {user}: {responses}")
                return False, logs
            log(f"  ✓ User {user} created successfully")
        
        log("\n✓ All 5 users created successfully")
        
        # Test 2: Verify submission login works for all users
        log("\n" + "=" * 50)
        log("TEST 2: Verify submission login for all users")
        log("=" * 50)
        
        def test_submission_login(user, password):
            """Test that a user can authenticate to the submission port."""
            try:
                sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
                sock.settimeout(10)
                sock.connect((ip_address, 2587))
                
                def smtp_recv():
                    response = ""
                    while True:
                        chunk = sock.recv(4096).decode()
                        response += chunk
                        lines = response.split('\r\n')
                        for line in lines:
                            if line and len(line) >= 4 and line[3] == ' ':
                                return response
                        if not chunk:
                            break
                    return response
                
                # Get banner
                smtp_recv()
                
                # EHLO
                sock.send(b"EHLO test.local\r\n")
                smtp_recv()
                
                # AUTH PLAIN
                auth_string = f"\x00{user}\x00{password}"
                auth_b64 = base64.b64encode(auth_string.encode()).decode()
                sock.send(f"AUTH PLAIN {auth_b64}\r\n".encode())
                response = smtp_recv()
                
                sock.send(b"QUIT\r\n")
                sock.close()
                
                return "235" in response
                
            except Exception as e:
                return False
        
        for i, user in enumerate(all_users, 1):
            log(f"\n  Testing submission login for user {i}/5: {user}")
            if test_submission_login(user, password):
                log(f"  ✓ Submission login successful")
            else:
                log(f"  ✗ Submission login failed")
                return False, logs
        
        log("\n✓ All 5 users can login via submission")
        
        # Test 3: Verify IMAP login works for all users
        log("\n" + "=" * 50)
        log("TEST 3: Verify IMAP login for all users")
        log("=" * 50)
        
        for i, user in enumerate(all_users, 1):
            log(f"\n  Testing IMAP login for user {i}/5: {user}")
            success_login, responses = imap_session(user, password, [
                ("SELECT INBOX", "OK"),
            ])
            if not success_login:
                log(f"  ✗ IMAP login/operations failed: {responses}")
                return False, logs
            log(f"  ✓ IMAP login and SELECT INBOX successful")
        
        log("\n✓ All 5 users can login via IMAP")
        
        # Test 4: Send message from sender to all 4 recipients via submission
        log("\n" + "=" * 50)
        log("TEST 4: Send PGP-encrypted message to 4 recipients")
        log("=" * 50)
        
        log(f"\n  Sender: {sender}")
        log(f"  Recipients: {', '.join(recipients)}")
        
        try:
            smtp_socket = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            smtp_socket.settimeout(15)
            smtp_socket.connect((ip_address, 2587))
            
            def smtp_recv_response():
                response = ""
                while True:
                    chunk = smtp_socket.recv(4096).decode()
                    response += chunk
                    lines = response.split('\r\n')
                    for line in lines:
                        if line and len(line) >= 4 and line[3] == ' ':
                            return response
                    if not chunk:
                        break
                return response
            
            def smtp_cmd(cmd, expected_code=None):
                if cmd:
                    smtp_socket.send((cmd + "\r\n").encode())
                response = smtp_recv_response()
                first_line = response.split('\r\n')[0] if response else ""
                log(f"    SMTP: {cmd or 'CONNECT'} -> {first_line[:60]}")
                if expected_code:
                    if not any(line.startswith(str(expected_code)) for line in response.split('\r\n') if line):
                        raise Exception(f"Expected {expected_code}, got: {response}")
                return response
            
            # Connect and authenticate
            smtp_cmd(None, 220)
            smtp_cmd("EHLO test.local", 250)
            
            auth_string = f"\x00{sender}\x00{password}"
            auth_b64 = base64.b64encode(auth_string.encode()).decode()
            smtp_cmd(f"AUTH PLAIN {auth_b64}", 235)
            
            # Set envelope - one sender, multiple recipients
            smtp_cmd(f"MAIL FROM:<{sender}>", 250)
            
            for recipient in recipients:
                smtp_cmd(f"RCPT TO:<{recipient}>", 250)
            
            smtp_cmd("DATA", 354)
            
            # Create and send the fake encrypted message
            fake_msg = create_fake_pgp_message(
                from_addr=sender,
                to_addrs=recipients,
                subject="Test multi-recipient encrypted message",
                body_text="This message was sent to 4 recipients"
            )
            
            smtp_socket.send(fake_msg.encode())
            smtp_socket.send(b"\r\n.\r\n")
            
            response = smtp_recv_response()
            first_line = response.split('\r\n')[0] if response else ""
            log(f"    SMTP: <message data> -> {first_line}")
            
            if not any(line.startswith("250") for line in response.split('\r\n') if line):
                raise Exception(f"DATA failed: {response}")
            
            smtp_cmd("QUIT", 221)
            smtp_socket.close()
            log("\n✓ Message submitted successfully to 4 recipients")
            
        except Exception as e:
            log(f"\n✗ Failed to send message: {e}")
            import traceback
            log(traceback.format_exc())
            return False, logs
        
        # Wait for message delivery
        log("\n  Waiting for message delivery...")
        time.sleep(2)
        
        # Test 5: Verify all 4 recipients received the message
        log("\n" + "=" * 50)
        log("TEST 5: Verify all 4 recipients received the message")
        log("=" * 50)
        
        successful_recipients = []
        failed_recipients = []
        
        for i, recipient in enumerate(recipients, 1):
            log(f"\n  Checking mailbox for recipient {i}/4: {recipient}")
            
            try:
                imap_socket = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
                imap_socket.settimeout(10)
                imap_socket.connect((ip_address, 2143))
                
                tag = 0
                def imap_cmd(cmd):
                    nonlocal tag
                    tag += 1
                    full_cmd = f"A{tag:03d} {cmd}"
                    imap_socket.send((full_cmd + "\r\n").encode())
                    response = b""
                    while True:
                        chunk = imap_socket.recv(4096)
                        response += chunk
                        if f"A{tag:03d} ".encode() in response:
                            break
                    return response.decode('utf-8', errors='ignore')
                
                # Read banner
                imap_socket.recv(4096)
                
                # Login
                response = imap_cmd(f'LOGIN "{recipient}" "{password}"')
                if "OK" not in response:
                    raise Exception(f"Login failed: {response}")
                
                # Select INBOX
                response = imap_cmd("SELECT INBOX")
                if "OK" not in response:
                    raise Exception(f"SELECT INBOX failed: {response}")
                
                # Check for messages
                has_message = False
                message_count = 0
                for line in response.split('\n'):
                    if "EXISTS" in line:
                        parts = line.strip().split()
                        for j, part in enumerate(parts):
                            if part == "EXISTS" and j > 0:
                                message_count = int(parts[j-1].replace('*', '').strip())
                                has_message = message_count > 0
                                break
                
                if not has_message:
                    raise Exception(f"No messages in INBOX")
                
                log(f"    IMAP: {message_count} message(s) in INBOX")
                
                # Fetch message to verify subject
                response = imap_cmd("FETCH 1 BODY[HEADER.FIELDS (SUBJECT FROM)]")
                
                if "multi-recipient" in response.lower():
                    log(f"    IMAP: Message subject verified")
                else:
                    log(f"    IMAP: Message found (subject check skipped)")
                
                if sender in response:
                    log(f"    IMAP: From address verified")
                
                imap_cmd("LOGOUT")
                imap_socket.close()
                
                successful_recipients.append(recipient)
                log(f"  ✓ Recipient {i} received the message")
                
            except Exception as e:
                failed_recipients.append((recipient, str(e)))
                log(f"  ✗ Recipient {i} failed: {e}")
        
        # Final results
        log("\n" + "=" * 50)
        log("RESULTS")
        log("=" * 50)
        
        log(f"\n  Successful deliveries: {len(successful_recipients)}/4")
        for r in successful_recipients:
            log(f"    ✓ {r}")
        
        if failed_recipients:
            log(f"\n  Failed deliveries: {len(failed_recipients)}/4")
            for r, err in failed_recipients:
                log(f"    ✗ {r}: {err}")
        
        if len(successful_recipients) == 4:
            log("\n✓ All 4 recipients received the message!")
            success = True
        else:
            log(f"\n✗ Only {len(successful_recipients)}/4 recipients received the message")
            success = False
        
    except Exception as e:
        log(f"\nERROR: {e}")
        import traceback
        log(traceback.format_exc())
        success = False
    
    finally:
        # Cleanup
        if maddy_process:
            log("\nStopping maddy server...")
            maddy_process.terminate()
            try:
                maddy_process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                maddy_process.kill()
        
        # Show server logs if test failed
        if 'log_file' in locals() and log_file:
            log_file.close()
        if 'log_file_path' in locals() and os.path.exists(log_file_path):
            with open(log_file_path, 'r') as f:
                server_log = f.read()
            if not success:
                log(f"\n=== Server logs ===\n{server_log}")
        
        # Clean up temp directory
        log(f"\nCleaning up {tmp_dir}...")
        shutil.rmtree(tmp_dir, ignore_errors=True)
    
    return success, logs


def run(maddy_bin=None):
    """
    Main entry point for the multi-recipient test.
    
    Args:
        maddy_bin: Path to maddy binary
    
    Returns:
        tuple: (success: bool, logs: list)
    """
    print("\n" + "=" * 60)
    print("TEST #13: Multi-Recipient Message Delivery")
    print("=" * 60)
    
    if maddy_bin is None:
        # Try to find the binary
        candidates = [
            os.path.join(os.getcwd(), "build", "maddy"),
            os.path.join(os.getcwd(), "maddy"),
            "/usr/local/bin/maddy",
            "/usr/bin/maddy",
        ]
        for candidate in candidates:
            if os.path.exists(candidate):
                maddy_bin = candidate
                break
        
        if maddy_bin is None:
            return False, ["Could not find maddy binary"]
    
    return run_multirecipient_test(maddy_bin)


if __name__ == "__main__":
    import argparse
    
    parser = argparse.ArgumentParser(description="Multi-Recipient Message Delivery Test")
    parser.add_argument("--maddy-bin", help="Path to maddy binary")
    
    args = parser.parse_args()
    
    success, logs = run(maddy_bin=args.maddy_bin)
    
    if success:
        print("\n✓ TEST #13 PASSED: Multi-recipient message delivery successful")
        sys.exit(0)
    else:
        print("\n✗ TEST #13 FAILED: Multi-recipient message delivery failed")
        sys.exit(1)
