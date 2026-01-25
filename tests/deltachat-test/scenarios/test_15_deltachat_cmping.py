#!/usr/bin/env python3
"""
Functional test #15: Delta Chat cmping-style test using deltachat-rpc-client.

This test uses the actual Delta Chat core (deltachat-rpc-server) to verify
that the madmail server works correctly with real Delta Chat clients.

Run with: python3 test_15_deltachat_cmping.py --maddy-bin ./build/maddy
"""

import argparse
import os
import random
import shutil
import signal
import socket
import string
import subprocess
import sys
import tempfile
import threading
import time
import urllib.parse

# Check for required dependencies
try:
    from deltachat_rpc_client import DeltaChat, EventType, Rpc
except ImportError:
    print("ERROR: deltachat-rpc-client not installed. Run: pip install deltachat-rpc-client")
    sys.exit(1)


def generate_credentials():
    """Generate random username and password for trust-on-first-login.
    
    Returns:
        tuple: (username, password) where username is 12 chars and password is 20 chars
    """
    chars = string.ascii_lowercase + string.digits
    username = "".join(random.choices(chars, k=12))
    password = "".join(random.choices(chars, k=20))
    return username, password


def create_dclogin_url(ip_address: str, username: str, password: str, imap_port: int, smtp_port: int) -> str:
    """Create a dclogin URL for Delta Chat account configuration.
    
    Args:
        ip_address: Server IP address
        username: User name (email local part)
        password: User password
        imap_port: IMAP port number
        smtp_port: SMTP/submission port number
        
    Returns:
        str: dclogin URL like dclogin:username@ip/?p=password&v=1&ip=993&sp=465&ic=3&ss=default
    """
    encoded_password = urllib.parse.quote(password, safe="")
    
    # Format: dclogin:username@host/?query
    # Parameters:
    #   p = password
    #   v = version (1)
    #   ip = IMAP port
    #   sp = SMTP port  
    #   ic = IMAP certificate (3 = accept any)
    #   ss = SMTP security (default)
    qr_url = (
        f"dclogin:{username}@{ip_address}/?"
        f"p={encoded_password}&v=1&ip={imap_port}&sp={smtp_port}&ic=3&ss=default"
    )
    return qr_url


class MaddyServer:
    """Manages a maddy server instance for testing."""
    
    def __init__(self, maddy_bin: str, domain: str = "127.0.0.1"):
        self.maddy_bin = maddy_bin
        self.domain = domain
        self.process = None
        self.config_dir = None
        self.runtime_dir = None
        self.state_dir = None
        # Use high ports to avoid permission issues
        self.imap_port = 2143
        self.smtp_port = 2525
        self.submission_port = 2587
        
    def start(self):
        """Start the maddy server."""
        self.config_dir = tempfile.mkdtemp(prefix="maddy_config_")
        self.runtime_dir = tempfile.mkdtemp(prefix="maddy_runtime_")
        self.state_dir = tempfile.mkdtemp(prefix="maddy_state_")
        
        # Create configuration file
        config = f"""
# Maddy test configuration for Delta Chat cmping test
hostname {self.domain}
state_dir {self.state_dir}
runtime_dir {self.runtime_dir}

debug yes
log stderr

# In-memory authentication with trust-on-first-login
auth.memauth local_auth {{
    auto_create yes
    min_password_len 12
}}

# In-memory message storage
storage.memstore local_store {{
    auto_create yes
    default_quota 100M
}}

# IMAP server (no TLS for testing)
imap tcp://0.0.0.0:{self.imap_port} {{
    tls off
    auth &local_auth
    storage &local_store
}}

# SMTP server for receiving (no TLS for testing)
smtp tcp://0.0.0.0:{self.smtp_port} {{
    hostname {self.domain}
    tls off
    deliver_to &local_store
}}

# Submission server (no TLS for testing)  
submission tcp://0.0.0.0:{self.submission_port} {{
    hostname {self.domain}
    tls off
    auth &local_auth
    deliver_to &local_store
}}
"""
        config_path = os.path.join(self.config_dir, "maddy.conf")
        with open(config_path, "w") as f:
            f.write(config)
            
        # Start maddy
        env = os.environ.copy()
        env["MADDY_STATE_DIR"] = self.state_dir
        env["MADDY_RUNTIME_DIR"] = self.runtime_dir
        
        self.process = subprocess.Popen(
            [self.maddy_bin, "-config", config_path, "run"],
            env=env,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        
        # Wait for server to start
        time.sleep(2)
        
        # Verify server is running
        if self.process.poll() is not None:
            stdout, stderr = self.process.communicate()
            print(f"Maddy failed to start!")
            print(f"stdout: {stdout.decode()}")
            print(f"stderr: {stderr.decode()}")
            raise RuntimeError("Maddy server failed to start")
            
        # Check if IMAP port is listening
        max_retries = 10
        for i in range(max_retries):
            try:
                sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
                sock.settimeout(1)
                sock.connect(("127.0.0.1", self.imap_port))
                sock.close()
                print(f"✓ Maddy server started (IMAP port {self.imap_port})")
                return
            except (socket.error, ConnectionRefusedError):
                time.sleep(0.5)
                
        raise RuntimeError(f"Maddy IMAP port {self.imap_port} not responding")
        
    def stop(self):
        """Stop the maddy server."""
        if self.process:
            self.process.terminate()
            try:
                self.process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait()
            self.process = None
            
        # Cleanup temp directories
        for d in [self.config_dir, self.runtime_dir, self.state_dir]:
            if d and os.path.exists(d):
                shutil.rmtree(d, ignore_errors=True)
                
    def get_logs(self) -> str:
        """Get server logs."""
        if self.process and self.process.stderr:
            return self.process.stderr.read().decode() if self.process.stderr.readable() else ""
        return ""


class AccountMaker:
    """Creates and manages Delta Chat accounts for testing."""
    
    def __init__(self, dc: DeltaChat, verbose: int = 0):
        self.dc = dc
        self.online = []
        self.verbose = verbose
        
    def wait_all_online(self, timeout: int = 60):
        """Wait for all accounts to become online."""
        remaining = list(self.online)
        start_time = time.time()
        
        while remaining and (time.time() - start_time) < timeout:
            ac = remaining[-1]
            try:
                event = ac.wait_for_event()
                if event.kind == EventType.IMAP_INBOX_IDLE:
                    remaining.pop()
                    if self.verbose >= 2:
                        print(f"  Account {ac.get_config('addr')} is online")
                elif event.kind == EventType.ERROR:
                    if self.verbose >= 1:
                        print(f"  ✗ ERROR during account setup: {event.msg}")
            except Exception as e:
                if self.verbose >= 1:
                    print(f"  ✗ Exception waiting for account: {e}")
                break
                
        if remaining:
            raise TimeoutError(f"Accounts did not come online within {timeout}s")
            
    def get_account(self, ip_address: str, imap_port: int, smtp_port: int) -> object:
        """Create and configure a new account.
        
        Args:
            ip_address: Server IP address
            imap_port: IMAP port
            smtp_port: SMTP/submission port
            
        Returns:
            Configured account object
        """
        account = self.dc.add_account()
        
        # Generate credentials
        username, password = generate_credentials()
        email_addr = f"{username}@{ip_address}"
        
        if self.verbose >= 2:
            print(f"  Configuring account: {email_addr}")
            
        try:
            # Set configurations directly (more reliable for IP addresses)
            account.set_config('addr', email_addr)
            account.set_config('mail_pw', password)
            account.set_config('mail_server', ip_address)
            account.set_config('mail_port', str(imap_port))
            account.set_config('send_server', ip_address)
            account.set_config('send_port', str(smtp_port))
            account.set_config('imap_certificate_checks', '3')  # Accept any certificate
            account.set_config('smtp_certificate_checks', '3')  # Accept any certificate
            account.set_config('mail_security', '3')  # Plain/no TLS
            account.set_config('send_security', '3')  # Plain/no TLS
            
            # Configure the account (this triggers connection to server)
            account.configure()
        except Exception as e:
            print(f"  ✗ Failed to configure account {email_addr}: {e}")
            raise
            
        # Start the account's I/O
        account.start_io()
        self.online.append(account)
        
        return account


def run_cmping_test(args):
    """Run the cmping-style test with Delta Chat accounts."""
    
    print("=" * 60)
    print("Test 15: Delta Chat cmping-style functional test")
    print("=" * 60)
    
    # Start maddy server
    server = MaddyServer(args.maddy_bin)
    try:
        print("\n1. Starting maddy server...")
        server.start()
        
        # Create accounts directory with accounts.toml
        # (deltachat-rpc-server 2.39.0 requires accounts.toml to exist)
        accounts_dir = tempfile.mkdtemp(prefix="dc_accounts_")
        accounts_toml_path = os.path.join(accounts_dir, "accounts.toml")
        with open(accounts_toml_path, "w") as f:
            f.write("selected_account = 0\nnext_id = 1\naccounts = []\n")
        
        print(f"\n2. Using accounts directory: {accounts_dir}")
        
        try:
            with Rpc(accounts_dir=accounts_dir) as rpc:
                dc = DeltaChat(rpc)
                maker = AccountMaker(dc, verbose=args.verbose)
                
                # Create sender and receiver accounts
                print(f"\n3. Creating {1 + args.numrecipients} accounts...")
                
                sender = maker.get_account("127.0.0.1", server.imap_port, server.submission_port)
                print(f"  ✓ Sender account created: {sender.get_config('addr')}")
                
                receivers = []
                for i in range(args.numrecipients):
                    receiver = maker.get_account("127.0.0.1", server.imap_port, server.submission_port)
                    receivers.append(receiver)
                    print(f"  ✓ Receiver {i+1} account created: {receiver.get_config('addr')}")
                    
                # Wait for all accounts to be online
                print("\n4. Waiting for all accounts to be online...")
                try:
                    maker.wait_all_online(timeout=30)
                    print("  ✓ All accounts online")
                except TimeoutError as e:
                    print(f"  ✗ {e}")
                    return False
                    
                # Create group chat
                print("\n5. Creating group chat...")
                group = sender.create_group("cmping-test")
                for receiver in receivers:
                    contact = sender.create_contact(receiver)
                    group.add_contact(contact)
                print(f"  ✓ Group created with {len(receivers)} members")
                
                # Send initial message to promote group
                print("\n6. Sending initial group message...")
                group.send_text("cmping test initialized")
                print("  ✓ Initial message sent")
                
                # Wait for receivers to join
                print("\n7. Waiting for receivers to join group...")
                joined = 0
                deadline = time.time() + 30
                
                while joined < len(receivers) and time.time() < deadline:
                    for i, receiver in enumerate(receivers):
                        try:
                            event = receiver.wait_for_event()
                            if event.kind == EventType.INCOMING_MSG:
                                msg = receiver.get_message_by_id(event.msg_id)
                                snapshot = msg.get_snapshot()
                                if "cmping test initialized" in snapshot.text:
                                    chat_id = snapshot.chat_id
                                    receiver_group = receiver.get_chat_by_id(chat_id)
                                    receiver_group.accept()
                                    joined += 1
                                    print(f"  ✓ Receiver {i+1} joined group ({joined}/{len(receivers)})")
                        except Exception as e:
                            if args.verbose >= 2:
                                print(f"  ? Exception: {e}")
                            pass
                            
                if joined < len(receivers):
                    print(f"  ⚠ Only {joined}/{len(receivers)} receivers joined")
                    
                # Send ping messages
                print(f"\n8. Sending {args.count} ping messages...")
                sent = 0
                received = 0
                
                ping_id = "".join(random.choices(string.ascii_lowercase + string.digits, k=10))
                
                for seq in range(args.count):
                    msg_text = f"ping-{ping_id}-{seq}-{time.time():.4f}"
                    group.send_text(msg_text)
                    sent += 1
                    print(f"  → Sent ping {seq+1}/{args.count}")
                    
                    # Wait a bit between messages
                    time.sleep(args.interval)
                    
                # Wait for messages to be received
                print(f"\n9. Waiting for messages to be received...")
                expected_total = sent * len(receivers)
                deadline = time.time() + 30
                
                while received < expected_total and time.time() < deadline:
                    for receiver in receivers:
                        try:
                            event = receiver.wait_for_event()
                            if event.kind == EventType.INCOMING_MSG:
                                msg = receiver.get_message_by_id(event.msg_id)
                                snapshot = msg.get_snapshot()
                                if f"ping-{ping_id}" in snapshot.text:
                                    received += 1
                                    if args.verbose >= 2:
                                        print(f"  ← Received ({received}/{expected_total})")
                        except Exception:
                            pass
                            
                print(f"  ✓ Received {received}/{expected_total} messages")
                
                # Calculate statistics
                loss = (1 - received / expected_total) * 100 if expected_total > 0 else 100
                
                print(f"\n10. Results:")
                print(f"  Sent: {sent} messages")
                print(f"  Expected: {expected_total} deliveries ({sent} x {len(receivers)} receivers)")
                print(f"  Received: {received} deliveries")
                print(f"  Loss: {loss:.2f}%")
                
                success = received == expected_total
                if success:
                    print("\n✓ TEST PASSED: All messages delivered successfully!")
                else:
                    print(f"\n✗ TEST FAILED: {expected_total - received} messages lost")
                    
                return success
                
        finally:
            # Cleanup accounts directory
            shutil.rmtree(accounts_dir, ignore_errors=True)
            
    finally:
        print("\nStopping maddy server...")
        server.stop()


def main():
    parser = argparse.ArgumentParser(
        description="Delta Chat cmping-style functional test for madmail"
    )
    parser.add_argument(
        "--maddy-bin",
        required=True,
        help="Path to maddy binary",
    )
    parser.add_argument(
        "-c", "--count",
        type=int,
        default=5,
        help="Number of ping messages to send (default: 5)",
    )
    parser.add_argument(
        "-i", "--interval",
        type=float,
        default=1.0,
        help="Seconds between messages (default: 1.0)",
    )
    parser.add_argument(
        "-g", "--numrecipients",
        type=int,
        default=2,
        help="Number of recipients (default: 2)",
    )
    parser.add_argument(
        "-v", "--verbose",
        action="count",
        default=0,
        help="Increase verbosity (-v, -vv, -vvv)",
    )
    
    args = parser.parse_args()
    
    if not os.path.exists(args.maddy_bin):
        print(f"ERROR: Maddy binary not found: {args.maddy_bin}")
        sys.exit(1)
        
    success = run_cmping_test(args)
    sys.exit(0 if success else 1)


if __name__ == "__main__":
    main()
