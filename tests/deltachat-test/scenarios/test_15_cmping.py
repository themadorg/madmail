#!/usr/bin/env python3
"""
Functional test #15: cmping-style ping test using Delta Chat core.

This test uses the real Delta Chat RPC server (deltachat-rpc-server) to:
1. Create multiple accounts on a maddy server using dclogin URLs
2. Set up a group chat with sender and receivers
3. Send ping messages and measure round-trip time via IMAP IDLE
4. Analyze verbose logging output for issues in the maddy message pipeline

Run with: python3 test_15_cmping.py --maddy-bin ./build/maddy -vvv -c 5

Requires: pip install deltachat-rpc-client deltachat-rpc-server
"""

import argparse
import os
import queue
import random
import signal
import socket
import string
import subprocess
import sys
import tempfile
import threading
import time
import urllib.parse
from statistics import stdev

# Import Delta Chat RPC client
try:
    from deltachat_rpc_client import DeltaChat, EventType, Rpc
except ImportError:
    print("Error: deltachat-rpc-client not installed. Run: pip install deltachat-rpc-client")
    sys.exit(1)


def get_rpc_server_path():
    """Get path to deltachat-rpc-server binary."""
    # Try to find it in the deltachat_rpc_server package
    try:
        import deltachat_rpc_server
        pkg_dir = os.path.dirname(deltachat_rpc_server.__file__)
        rpc_path = os.path.join(pkg_dir, "deltachat-rpc-server")
        if os.path.exists(rpc_path):
            return rpc_path
    except ImportError:
        pass
    
    # Try system path
    import shutil
    path = shutil.which("deltachat-rpc-server")
    if path:
        return path
    
    raise RuntimeError("deltachat-rpc-server not found. Run: pip install deltachat-rpc-server")


def generate_credentials():
    """Generate random username and password for IP-based login.
    
    Returns:
        tuple: (username, password) where username is 12 chars and password is 20 chars
    """
    chars = string.ascii_lowercase + string.digits
    username = "".join(random.choices(chars, k=12))
    password = "".join(random.choices(chars, k=20))
    return username, password


def create_dclogin_url(ip_address: str, imap_port: int = 993, smtp_port: int = 465):
    """Create a dclogin URL for IP address-based login.
    
    Args:
        ip_address: IP address of the mail server
        imap_port: IMAP port (default 993)
        smtp_port: SMTP/submission port (default 465)
    
    Returns:
        tuple: (username, password) - credentials generated for the account
    """
    username, password = generate_credentials()
    return username, password


def configure_account_for_ip(account, username: str, password: str, ip_address: str, imap_port: int, smtp_port: int):
    """Manually configure a Delta Chat account for IP-based mail server.
    
    This bypasses auto-discovery which doesn't work well with IP addresses.
    """
    email = f"{username}@{ip_address}"
    
    # Set basic credentials
    account.set_config("addr", email)
    account.set_config("mail_pw", password)
    
    # Configure IMAP server directly (bypass auto-discovery)
    account.set_config("mail_server", ip_address)
    account.set_config("mail_port", str(imap_port))
    account.set_config("mail_security", "3")  # 3 = PLAIN (no TLS)
    
    # Configure SMTP server directly
    account.set_config("send_server", ip_address)
    account.set_config("send_port", str(smtp_port))
    account.set_config("send_security", "3")  # 3 = PLAIN (no TLS)
    
    # Disable OAuth2 and other auto-discovery
    account.set_config("server_flags", "0")
    
    return email


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
        # Delta Chat uses these ports: IMAP=993/143, SMTP=465/587
        self.imap_port = 3993  # Non-standard port for testing
        self.smtp_port = 3465  # Non-standard port for testing (submission)
        self.verbose = 0
        self.log_output = []
        
    def start(self, verbose: int = 0):
        """Start the maddy server."""
        self.verbose = verbose
        self.config_dir = tempfile.mkdtemp(prefix="maddy_config_")
        self.runtime_dir = tempfile.mkdtemp(prefix="maddy_runtime_")
        self.state_dir = tempfile.mkdtemp(prefix="maddy_state_")
        
        # Create config file using memstore and memauth
        # Note: Delta Chat uses specific ports and expects certain behavior
        config = f"""
hostname {self.domain}
state_dir {self.state_dir}
runtime_dir {self.runtime_dir}

log stderr
debug yes

# In-memory authentication with trust-on-first-login
auth.memauth local_auth {{
    auto_create yes
    min_password_len 12
}}

# In-memory storage
storage.memstore local_store {{
    auto_create yes
}}

# IMAP server (Delta Chat expects port 993 or configured port)
imap tcp://0.0.0.0:{self.imap_port} {{
    tls off
    auth &local_auth
    storage &local_store
}}

# Submission server (Delta Chat expects port 465 or 587)
submission tcp://0.0.0.0:{self.smtp_port} {{
    tls off
    auth &local_auth
    
    source {self.domain} {{
        deliver_to &local_store
    }}
    
    default_source {{
        deliver_to &local_store
    }}
}}
"""
        config_path = os.path.join(self.config_dir, "maddy.conf")
        with open(config_path, "w") as f:
            f.write(config)
        
        # Start maddy
        env = os.environ.copy()
        self.process = subprocess.Popen(
            [self.maddy_bin, "-config", config_path],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env=env,
        )
        
        # Start log reader thread
        def read_logs():
            while self.process and self.process.poll() is None:
                line = self.process.stderr.readline()
                if line:
                    decoded = line.decode('utf-8', errors='replace').strip()
                    self.log_output.append(decoded)
                    if self.verbose >= 3:
                        print(f"[MADDY] {decoded}")
        
        self.log_thread = threading.Thread(target=read_logs, daemon=True)
        self.log_thread.start()
        
        # Wait for server to be ready
        self._wait_for_port(self.imap_port)
        self._wait_for_port(self.smtp_port)
        
        if verbose >= 1:
            print(f"# Maddy server started on ports: IMAP={self.imap_port}, SMTP={self.smtp_port}")
        
    def _wait_for_port(self, port: int, timeout: float = 10.0):
        """Wait for a port to become available."""
        start = time.time()
        while time.time() - start < timeout:
            try:
                sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
                sock.settimeout(1)
                sock.connect(("127.0.0.1", port))
                sock.close()
                return
            except (socket.error, ConnectionRefusedError):
                time.sleep(0.1)
        raise RuntimeError(f"Port {port} not available after {timeout}s")
    
    def stop(self):
        """Stop the maddy server."""
        if self.process:
            self.process.terminate()
            try:
                self.process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.process.kill()
            self.process = None
        
        # Clean up directories
        import shutil
        for d in [self.config_dir, self.runtime_dir, self.state_dir]:
            if d and os.path.exists(d):
                shutil.rmtree(d, ignore_errors=True)
    
    def get_logs(self):
        """Get all collected log output."""
        return self.log_output
    
    def get_ip(self):
        """Get the IP address of the server."""
        return "127.0.0.1"


class AccountMaker:
    """Creates and manages Delta Chat accounts."""
    
    def __init__(self, dc: DeltaChat):
        self.dc = dc
        self.online = []
    
    def wait_all_online(self, timeout: float = 60.0):
        """Wait for all accounts to be online (IMAP INBOX IDLE)."""
        remaining = list(self.online)
        start = time.time()
        while remaining and time.time() - start < timeout:
            ac = remaining.pop(0)
            try:
                # Wait for IMAP_INBOX_IDLE event which indicates account is ready
                event = ac.wait_for_event(timeout=10)
                if event and event.kind == EventType.IMAP_INBOX_IDLE:
                    continue
                # Put back if not ready yet
                remaining.append(ac)
            except Exception:
                remaining.append(ac)
        
        if remaining:
            raise TimeoutError(f"Timeout waiting for {len(remaining)} accounts to be online")
    
    def _add_online(self, account):
        """Start IO for an account and track it."""
        account.start_io()
        self.online.append(account)
    
    def get_account(self, ip_address: str, imap_port: int, smtp_port: int):
        """Create a new account on the server."""
        account = self.dc.add_account()
        
        # Generate credentials
        username, password = create_dclogin_url(ip_address, imap_port, smtp_port)
        
        try:
            # Manually configure account for IP address (bypasses auto-discovery)
            email = configure_account_for_ip(account, username, password, ip_address, imap_port, smtp_port)
        except Exception as e:
            print(f"✗ Failed to configure account: {e}")
            raise
        
        try:
            self._add_online(account)
        except Exception as e:
            print(f"✗ Failed to bring account online: {e}")
            raise
        
        return account


class Pinger:
    """cmping-style message pinger using Delta Chat core."""
    
    def __init__(self, args, sender, group, receivers):
        self.args = args
        self.sender = sender
        self.group = group
        self.receivers = receivers
        self.addr1 = sender.get_config("addr")
        self.receivers_addrs = [receiver.get_config("addr") for receiver in receivers]
        self.receivers_addrs_str = ", ".join(self.receivers_addrs)
        self.relay1 = self.addr1.split("@")[1] if "@" in self.addr1 else self.addr1
        self.relay2 = self.receivers_addrs[0].split("@")[1] if self.receivers_addrs and "@" in self.receivers_addrs[0] else self.relay1
        
        print(f"CMPING {self.relay1}({self.addr1}) -> {self.relay2}(group with {len(receivers)} members: {self.receivers_addrs_str}) count={args.count} interval={args.interval}s")
        
        # Generate unique ping identifier
        chars = string.ascii_lowercase + string.digits
        self.tx = "".join(random.choices(chars, k=30))
        
        self.sent = 0
        self.received = 0
        self._send_thread = threading.Thread(target=self.send_pings, daemon=True)
        self._send_thread.start()
    
    @property
    def loss(self):
        expected_total = self.sent * len(self.receivers)
        return 100.0 if expected_total == 0 else (1 - self.received / expected_total) * 100
    
    def send_pings(self):
        """Send ping messages in background thread."""
        for seq in range(self.args.count):
            text = f"{self.tx} {time.time():.4f} {seq:17}"
            self.group.send_text(text)
            self.sent += 1
            time.sleep(self.args.interval)
        
        # Wait a bit then force quit if main didn't finish
        time.sleep(60)
        os.kill(os.getpid(), signal.SIGINT)
    
    def receive(self):
        """Generator that yields received ping messages."""
        num_pending = self.args.count * len(self.receivers)
        start_clock = time.time()
        received_by_receiver = {}
        
        # Create event queue for all receivers
        event_queue = queue.Queue()
        
        def receiver_thread(receiver_idx, receiver):
            """Thread to listen to events from a single receiver."""
            while True:
                try:
                    event = receiver.wait_for_event()
                    event_queue.put((receiver_idx, receiver, event))
                except Exception:
                    event_queue.put((receiver_idx, receiver, None))
                    break
        
        # Start threads for each receiver
        threads = []
        for idx, receiver in enumerate(self.receivers):
            t = threading.Thread(target=receiver_thread, args=(idx, receiver), daemon=True)
            t.start()
            threads.append(t)
        
        while num_pending > 0:
            try:
                receiver_idx, receiver, event = event_queue.get(timeout=1.0)
                if event is None:
                    continue
                
                if event.kind == EventType.INCOMING_MSG:
                    msg = receiver.get_message_by_id(event.msg_id)
                    text = msg.get_snapshot().text
                    parts = text.strip().split()
                    
                    if len(parts) == 3 and parts[0] == self.tx:
                        seq = int(parts[2])
                        if seq not in received_by_receiver:
                            received_by_receiver[seq] = set()
                        if receiver_idx not in received_by_receiver[seq]:
                            ms_duration = (time.time() - float(parts[1])) * 1000
                            self.received += 1
                            num_pending -= 1
                            received_by_receiver[seq].add(receiver_idx)
                            yield seq, ms_duration, len(text), receiver_idx
                            start_clock = time.time()
                            
                elif event.kind == EventType.ERROR:
                    if self.args.verbose >= 1:
                        print(f"ERROR: {event.msg}")
                elif event.kind == EventType.MSG_FAILED:
                    msg = receiver.get_message_by_id(event.msg_id)
                    text = msg.get_snapshot().text
                    print(f"Message failed: {text}")
                elif event.kind in (EventType.INFO, EventType.WARNING) and self.args.verbose >= 1:
                    ms_now = (time.time() - start_clock) * 1000
                    print(f"INFO {ms_now:07.1f}ms: {event.msg}")
                    
            except queue.Empty:
                continue


def perform_cmping(server: MaddyServer, args):
    """Perform cmping-style test using Delta Chat core."""
    
    ip_address = server.get_ip()
    imap_port = server.imap_port
    smtp_port = server.smtp_port
    
    # Get RPC server path
    rpc_server_path = get_rpc_server_path()
    
    # Create temp directory for accounts
    accounts_dir = tempfile.mkdtemp(prefix="cmping_accounts_")
    print(f"# using accounts_dir at: {accounts_dir}")
    
    # Create initial accounts.toml - required by deltachat-rpc-server
    accounts_toml = os.path.join(accounts_dir, "accounts.toml")
    with open(accounts_toml, "w") as f:
        f.write("accounts = []\nselected_account = 1\nnext_id = 1\n")
    
    try:
        with Rpc(accounts_dir=accounts_dir, rpc_server_path=rpc_server_path) as rpc:
            dc = DeltaChat(rpc)
            maker = AccountMaker(dc)
            
            total_accounts = 1 + args.num_recipients
            accounts_created = 0
            
            # Create sender account
            print(f"# Setting up accounts: {accounts_created}/{total_accounts}", end="", flush=True)
            try:
                sender = maker.get_account(ip_address, imap_port, smtp_port)
                accounts_created += 1
                print(f"\r# Setting up accounts: {accounts_created}/{total_accounts}", end="", flush=True)
            except Exception as e:
                print(f"\r✗ Failed to setup sender account: {e}")
                return None
            
            # Create receiver accounts
            receivers = []
            for i in range(args.num_recipients):
                try:
                    receiver = maker.get_account(ip_address, imap_port, smtp_port)
                    receivers.append(receiver)
                    accounts_created += 1
                    print(f"\r# Setting up accounts: {accounts_created}/{total_accounts}", end="", flush=True)
                except Exception as e:
                    print(f"\r✗ Failed to setup receiver account {i+1}: {e}")
                    return None
            
            print(f"\r# Setting up accounts: {accounts_created}/{total_accounts} - Complete!")
            
            # Wait for all accounts to be online
            print("# Waiting for all accounts to be online...", end="", flush=True)
            try:
                maker.wait_all_online()
                print(" Done!")
            except Exception as e:
                print(f"\n✗ Timeout waiting for accounts to be online: {e}")
                return None
            
            # Create group chat
            group = sender.create_group("cmping")
            for receiver in receivers:
                contact = sender.create_contact(receiver)
                group.add_contact(contact)
            
            # Send initial message to promote group
            print("# promoting group chat by sending initial message")
            group.send_text("cmping group chat initialized")
            
            # Wait for receivers to join group
            print("# waiting for receivers to join group")
            sender_addr = sender.get_config("addr")
            for idx, receiver in enumerate(receivers):
                timeout_seconds = 30
                start_time = time.time()
                while time.time() - start_time < timeout_seconds:
                    event = receiver.wait_for_event()
                    if event.kind == EventType.INCOMING_MSG:
                        msg = receiver.get_message_by_id(event.msg_id)
                        snapshot = msg.get_snapshot()
                        sender_contact = msg.get_sender_contact()
                        sender_contact_snapshot = sender_contact.get_snapshot()
                        if (sender_contact_snapshot.address == sender_addr and
                            "cmping group chat initialized" in snapshot.text):
                            chat_id = snapshot.chat_id
                            receiver_group = receiver.get_chat_by_id(chat_id)
                            receiver_group.accept()
                            print(f"# receiver {idx} ({receiver.get_config('addr')}) joined group")
                            break
                else:
                    print(f"# WARNING: receiver {idx} did not join group within {timeout_seconds}s")
            
            # Run pinger
            pinger = Pinger(args, sender, group, receivers)
            received = {}
            current_seq = None
            seq_tracking = {}
            
            try:
                for seq, ms_duration, size, receiver_idx in pinger.receive():
                    if seq not in received:
                        received[seq] = []
                    received[seq].append(ms_duration)
                    
                    if seq not in seq_tracking:
                        seq_tracking[seq] = {
                            "count": 0,
                            "first_time": ms_duration,
                            "last_time": ms_duration,
                            "size": size,
                        }
                    seq_tracking[seq]["count"] += 1
                    seq_tracking[seq]["last_time"] = ms_duration
                    
                    if current_seq != seq:
                        if current_seq is not None:
                            print()
                        print(f"{size} bytes ME -> {pinger.relay1} -> {pinger.relay2} -> ME seq={seq} time={ms_duration:0.2f}ms", end="", flush=True)
                        current_seq = seq
                    
                    count = seq_tracking[seq]["count"]
                    total = args.num_recipients
                    if count > 1:
                        prev_ratio_len = len(f" {count-1}/{total}")
                        print("\b" * prev_ratio_len, end="", flush=True)
                    print(f" {count}/{total}", end="", flush=True)
                    
                    if count == total:
                        first_time = seq_tracking[seq]["first_time"]
                        last_time = seq_tracking[seq]["last_time"]
                        elapsed = last_time - first_time
                        print(f" (elapsed: {elapsed:0.2f}ms)", end="", flush=True)
                        
            except KeyboardInterrupt:
                pass
            
            if current_seq is not None:
                print()
            
            print(f"--- {pinger.addr1} -> {pinger.receivers_addrs_str} statistics ---")
            print(f"{pinger.sent} transmitted, {pinger.received} received, {pinger.loss:.2f}% loss")
            
            if received:
                all_durations = [d for durations in received.values() for d in durations]
                if all_durations:
                    rmin = min(all_durations)
                    ravg = sum(all_durations) / len(all_durations)
                    rmax = max(all_durations)
                    rmdev = stdev(all_durations) if len(all_durations) >= 2 else rmax
                    print(f"rtt min/avg/max/mdev = {rmin:.3f}/{ravg:.3f}/{rmax:.3f}/{rmdev:.3f} ms")
            
            return pinger
            
    finally:
        # Clean up accounts directory
        import shutil
        shutil.rmtree(accounts_dir, ignore_errors=True)


def analyze_logs(server: MaddyServer, verbose: int):
    """Analyze server logs for issues."""
    logs = server.get_logs()
    
    issues = []
    
    # Look for common issues
    for line in logs:
        lower = line.lower()
        if "error" in lower or "failed" in lower:
            issues.append(("ERROR", line))
        elif "panic" in lower:
            issues.append(("PANIC", line))
        elif "timeout" in lower:
            issues.append(("TIMEOUT", line))
        elif "deadlock" in lower:
            issues.append(("DEADLOCK", line))
    
    if issues:
        print("\n# Log Analysis - Issues Found:")
        for issue_type, line in issues:
            print(f"  [{issue_type}] {line}")
    else:
        print("\n# Log Analysis - No critical issues found in logs")
    
    if verbose >= 3:
        print("\n# Full server logs:")
        for line in logs:
            print(f"  {line}")
    
    return issues


def main():
    parser = argparse.ArgumentParser(description=__doc__)
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
        "-g", "--num-recipients",
        type=int,
        default=3,
        help="Number of recipients (default: 3)",
    )
    parser.add_argument(
        "-i", "--interval",
        type=float,
        default=1.1,
        help="Seconds between message sending (default: 1.1)",
    )
    parser.add_argument(
        "-v", "--verbose",
        action="count",
        default=0,
        help="Increase verbosity (-v, -vv, -vvv)",
    )
    args = parser.parse_args()
    
    # Verify maddy binary exists
    if not os.path.exists(args.maddy_bin):
        print(f"✗ Maddy binary not found: {args.maddy_bin}")
        sys.exit(1)
    
    # Start server
    server = MaddyServer(args.maddy_bin)
    
    try:
        print("# Starting maddy server...")
        server.start(verbose=args.verbose)
        
        # Run cmping test
        pinger = perform_cmping(server, args)
        
        # Analyze logs
        issues = analyze_logs(server, args.verbose)
        
        # Determine success
        if pinger and pinger.loss == 0:
            print("\n✓ All tests passed!")
            sys.exit(0)
        else:
            loss = pinger.loss if pinger else 100.0
            print(f"\n✗ Test failed: {loss:.2f}% message loss")
            sys.exit(1)
            
    except Exception as e:
        print(f"✗ Test failed with exception: {e}")
        import traceback
        traceback.print_exc()
        sys.exit(1)
    finally:
        print("\n# Stopping maddy server...")
        server.stop()


if __name__ == "__main__":
    main()
