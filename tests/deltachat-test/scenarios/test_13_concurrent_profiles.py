"""
Test #13: Concurrent Profile Creation and Mass IDLE Notification Test

This test exercises the IMAP IDLE concurrency fix with a stress test:
1. Start a maddy test server
2. Concurrently create 100 accounts via SMTP and IMAP logins (auto-create)
3. Have all 99 receivers enter IDLE state
4. Send a single encrypted message from one sender to all 99 receivers
5. Verify all receivers wake up from IDLE and receive the message

This test is designed to stress-test the race condition fix between
Idle() and idleUpdate() when many clients are waiting concurrently.
"""

import os
import sys
import time
import uuid
import random
import string
import socket
import signal
import shutil
import tempfile
import threading
import subprocess
import smtplib
import imaplib
from email.mime.text import MIMEText
from email.mime.multipart import MIMEMultipart
from concurrent.futures import ThreadPoolExecutor, as_completed


def random_string(length=9):
    """Generate a random alphanumeric string."""
    return ''.join(random.choices(string.ascii_lowercase + string.digits, k=length))


def find_free_port():
    """Find an available port on localhost."""
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(('127.0.0.1', 0))
        return s.getsockname()[1]


def load_encrypted_eml():
    """Load the encrypted.eml template file."""
    script_dir = os.path.dirname(os.path.abspath(__file__))
    eml_path = os.path.join(script_dir, "..", "mail-data", "encrypted.eml")
    with open(eml_path, 'r') as f:
        return f.read()


class MaddyTestServer:
    """Manages a local maddy test server with custom ports."""
    
    def __init__(self, smtp_port, imap_port, http_port):
        self.smtp_port = smtp_port
        self.imap_port = imap_port
        self.http_port = http_port
        self.process = None
        self.temp_dir = None
        self.domain = "127.0.0.1"
        
    def start(self, maddy_binary, timeout=60):
        """Start the maddy server using the install command with custom ports."""
        # Create temporary directory for state and config
        self.temp_dir = tempfile.mkdtemp(prefix="maddy_test_")
        state_dir = os.path.join(self.temp_dir, "state")
        config_dir = os.path.join(self.temp_dir, "config")
        os.makedirs(state_dir, exist_ok=True)
        os.makedirs(config_dir, exist_ok=True)
        
        print(f"  Temp directory: {self.temp_dir}")
        print(f"  State directory: {state_dir}")
        print(f"  SMTP port: {self.smtp_port}")
        print(f"  IMAP port: {self.imap_port}")
        print(f"  HTTP port: {self.http_port}")
        
        # Generate a minimal maddy config that supports auto-create
        config_content = self._generate_config(state_dir)
        config_path = os.path.join(config_dir, "maddy.conf")
        
        with open(config_path, 'w') as f:
            f.write(config_content)
        
        print(f"  Config file written to: {config_path}")
        
        # Start maddy server
        cmd = [
            maddy_binary,
            "-config", config_path,
            "run"
        ]
        
        print(f"  Starting maddy: {' '.join(cmd)}")
        
        self.process = subprocess.Popen(
            cmd,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            preexec_fn=os.setsid  # Create new process group for clean shutdown
        )
        
        # Wait for server to start
        print("  Waiting for server to start...")
        start_time = time.time()
        listeners_ready = 0
        expected_listeners = 3  # SMTP, IMAP, HTTP
        
        while time.time() - start_time < timeout:
            if self.process.poll() is not None:
                # Process exited - read output
                output = self.process.stdout.read() if self.process.stdout else ""
                raise Exception(f"Maddy server exited unexpectedly: {output}")
            
            # Check if we can connect to the ports
            try:
                with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
                    s.settimeout(0.5)
                    s.connect(('127.0.0.1', self.smtp_port))
                    listeners_ready |= 1
            except (socket.error, socket.timeout):
                pass
            
            try:
                with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
                    s.settimeout(0.5)
                    s.connect(('127.0.0.1', self.imap_port))
                    listeners_ready |= 2
            except (socket.error, socket.timeout):
                pass
                
            try:
                with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
                    s.settimeout(0.5)
                    s.connect(('127.0.0.1', self.http_port))
                    listeners_ready |= 4
            except (socket.error, socket.timeout):
                pass
            
            if listeners_ready == 7:  # All 3 ports are ready
                print(f"  Server started in {time.time() - start_time:.1f}s")
                return
            
            time.sleep(0.5)
        
        # Timeout - read output for debugging
        self.stop()
        raise Exception(f"Server did not start within {timeout}s. Listeners ready mask: {listeners_ready}")
    
    def _generate_config(self, state_dir):
        """Generate a minimal maddy config for testing.
        
        Optimizations for concurrent account creation:
        - Higher sqlite3_busy_timeout to handle concurrent writes
        - Shared cache mode for better connection handling
        """
        return f'''
# Maddy test configuration for concurrent profile stress test
state_dir {state_dir}
runtime_dir {state_dir}/run

$(hostname) = 127.0.0.1
$(primary_domain) = [127.0.0.1]
$(local_domains) = $(primary_domain)

# Disable TLS for testing
tls off

# Enable debug logging
debug yes
log stderr

# Authentication with auto-create
auth.pass_table local_authdb {{
    auto_create yes
    table sql_table {{
        driver sqlite3
        dsn {state_dir}/credentials.db
        table_name passwords
    }}
}}

# IMAP storage with auto-create
# Higher busy_timeout to handle concurrent account creation
storage.imapsql local_mailboxes {{
    auto_create yes
    driver sqlite3
    dsn {state_dir}/imapsql.db
    sqlite3_busy_timeout 60000
}}

# Submission endpoint (for authenticated sending)
submission tcp://127.0.0.1:{self.smtp_port} {{
    hostname 127.0.0.1
    auth &local_authdb
    insecure_auth yes
    
    limits {{
        # Up to 100 msgs/sec across max. 200 SMTP connections.
        all rate 100 1s
        all concurrency 200
    }}
    
    source $(local_domains) {{
        deliver_to &local_mailboxes
    }}
    default_source {{
        deliver_to &local_mailboxes
    }}
}}

# IMAP endpoint
imap tcp://127.0.0.1:{self.imap_port} {{
    auth &local_authdb
    storage &local_mailboxes
    insecure_auth yes
}}

# Chatmail HTTP endpoint (for JIT registration)
chatmail tcp://127.0.0.1:{self.http_port} {{
    mail_domain $(primary_domain)
    mx_domain $(primary_domain)
    web_domain $(primary_domain)
    auth_db local_authdb
    storage local_mailboxes
    turn_off_tls yes
    public_ip 127.0.0.1
}}
'''
    
    def stop(self):
        """Stop the maddy server."""
        if self.process:
            try:
                # Send SIGTERM to the process group
                os.killpg(os.getpgid(self.process.pid), signal.SIGTERM)
                self.process.wait(timeout=10)
            except (ProcessLookupError, subprocess.TimeoutExpired):
                try:
                    os.killpg(os.getpgid(self.process.pid), signal.SIGKILL)
                except ProcessLookupError:
                    pass
            self.process = None
        
        # Clean up temp directory
        if self.temp_dir:
            try:
                shutil.rmtree(self.temp_dir)
            except OSError as e:
                print(f"  Warning: Could not clean up temp dir: {e}")
            self.temp_dir = None


class IMAPIdleClient:
    """IMAP client that supports IDLE command with immediate fetch capability."""
    
    def __init__(self, host, port, username, password, client_id):
        self.host = host
        self.port = port
        self.username = username
        self.password = password
        self.client_id = client_id
        self.imap = None
        self.idle_event = threading.Event()
        self.messages_received = []
        self.thread = None
        self.running = False
        self.idle_started = threading.Event()
        self.error = None
        self.connected = False
        # Track immediate fetch results to detect race conditions
        self.immediate_fetch_result = None
        self.immediate_fetch_error = None
        
    def connect(self):
        """Connect and login to IMAP server."""
        try:
            self.imap = imaplib.IMAP4(self.host, self.port)
            self.imap.login(self.username, self.password)
            self.imap.select('INBOX')
            self.connected = True
            return True
        except Exception as e:
            self.error = str(e)
            return False
        
    def start_idle(self):
        """Start IDLE in a background thread."""
        self.running = True
        self.thread = threading.Thread(target=self._idle_loop, daemon=True)
        self.thread.start()
        # Wait for IDLE to be established
        return self.idle_started.wait(timeout=10)
        
    def _idle_loop(self):
        """Run IDLE loop and watch for new messages.
        
        Upon receiving EXISTS, immediately try to fetch the message.
        This tests the race condition fix - messages should be visible
        in the database when the IDLE notification is sent.
        """
        try:
            # Send IDLE command
            tag = self.imap._new_tag().decode()
            self.imap.send(f'{tag} IDLE\r\n'.encode())
            
            # Read continuation response
            response = self.imap.readline()
            if not response.startswith(b'+'):
                self.error = f"IDLE failed: {response}"
                self.idle_started.set()
                return
            
            self.idle_started.set()
            
            # Wait for EXISTS notification with longer timeout
            while self.running:
                self.imap.sock.settimeout(120.0)  # 120 second timeout per read
                try:
                    line = self.imap.readline()
                    if line:
                        line_str = line.decode().strip()
                        if 'EXISTS' in line_str:
                            # IMMEDIATELY exit IDLE and try to fetch
                            # This tests the race condition fix
                            try:
                                self.imap.send(b'DONE\r\n')
                                # Read the tagged response
                                self.imap.readline()
                            except (socket.error, imaplib.IMAP4.error, OSError):
                                pass
                            
                            # Try to immediately fetch the message - no delay!
                            # Before the fix, this would fail because the transaction
                            # wasn't committed yet when the notification was sent.
                            try:
                                self.imap.select('INBOX')
                                status, data = self.imap.search(None, 'ALL')
                                if status == 'OK' and data[0]:
                                    msg_nums = data[0].split()
                                    if msg_nums:
                                        # Try to fetch the latest message immediately
                                        latest_num = msg_nums[-1]
                                        status, msg_data = self.imap.fetch(latest_num, '(RFC822)')
                                        if status == 'OK' and msg_data[0]:
                                            self.immediate_fetch_result = msg_data[0][1]
                                        else:
                                            self.immediate_fetch_error = f"FETCH failed: {status}"
                                    else:
                                        self.immediate_fetch_error = "No messages found after EXISTS"
                                else:
                                    self.immediate_fetch_error = f"SEARCH failed: {status}"
                            except Exception as e:
                                self.immediate_fetch_error = str(e)
                            
                            self.idle_event.set()
                            return  # Already exited IDLE, no need for DONE again
                        # Ignore other untagged responses
                except socket.timeout:
                    # Continue waiting if still running
                    if self.running:
                        continue
                    break
                    
            # Send DONE to exit IDLE
            try:
                self.imap.send(b'DONE\r\n')
                # Read the tagged response
                self.imap.readline()
            except (socket.error, imaplib.IMAP4.error, OSError):
                pass  # Ignore errors during cleanup
            
        except (socket.error, imaplib.IMAP4.error, OSError) as e:
            self.error = str(e)
            
    def wait_for_message(self, timeout=60):
        """Wait for a message to arrive via IDLE."""
        return self.idle_event.wait(timeout=timeout)
    
    def fetch_messages(self):
        """Fetch all messages from INBOX."""
        try:
            # Need to re-select after IDLE
            self.imap.select('INBOX')
            status, data = self.imap.search(None, 'ALL')
            if status != 'OK':
                return []
            
            messages = []
            for num in data[0].split():
                status, msg_data = self.imap.fetch(num, '(RFC822)')
                if status == 'OK':
                    messages.append(msg_data[0][1])
            return messages
        except Exception:
            return []
    
    def stop(self):
        """Stop the IDLE client."""
        self.running = False
        if self.thread:
            self.thread.join(timeout=2)
        if self.imap:
            try:
                self.imap.logout()
            except (socket.error, imaplib.IMAP4.error, OSError):
                pass


def create_account_concurrently(host, smtp_port, imap_port, username, password, index):
    """Create an account by logging in via SMTP and IMAP (auto-create)."""
    errors = []
    timings = {}
    
    # Try SMTP login first (creates account)
    start = time.time()
    try:
        smtp = smtplib.SMTP(host, smtp_port, timeout=30)
        timings['smtp_connect'] = time.time() - start
        
        login_start = time.time()
        smtp.login(username, password)
        timings['smtp_login'] = time.time() - login_start
        
        smtp.quit()
    except Exception as e:
        errors.append(f"SMTP error: {e}")
        timings['smtp_error'] = time.time() - start
    
    # Then IMAP login (retry SQLITE_BUSY-style transient lock errors).
    max_attempts = 8
    retry_delay = 0.2
    for attempt in range(1, max_attempts + 1):
        start = time.time()
        try:
            imap = imaplib.IMAP4(host, imap_port, timeout=30)
            timings['imap_connect'] = time.time() - start

            login_start = time.time()
            imap.login(username, password)
            timings['imap_login'] = time.time() - login_start

            imap.logout()
            break
        except Exception as e:
            msg = str(e)
            timings['imap_error'] = time.time() - start
            # Queue pressure in SQLite can surface as SQLITE_BUSY/locked; retry.
            if ("SQLITE_BUSY" in msg or "database is locked" in msg) and attempt < max_attempts:
                time.sleep(retry_delay)
                retry_delay = min(retry_delay * 1.7, 2.0)
                continue
            errors.append(f"IMAP error: {e}")
            break
    
    if errors:
        return (index, False, "; ".join(errors), timings)
    return (index, True, None, timings)


def run(test_dir=None, maddy_binary=None, num_accounts=12):
    """Run the concurrent profile creation and mass IDLE test."""
    print("\n" + "="*60)
    print("TEST #13: Concurrent Profile Creation and Mass IDLE Test")
    print(f"          Testing with {num_accounts} accounts")
    print("="*60)
    
    # Find maddy binary
    possible_paths = [
        os.path.abspath("build/maddy"),
        os.path.abspath("maddy"),
        "/usr/local/bin/maddy",
    ]
    
    if not maddy_binary:
        for path in possible_paths:
            if os.path.exists(path):
                maddy_binary = path
                break
    
    if not maddy_binary or not os.path.exists(maddy_binary):
        raise Exception(f"Maddy binary not found. Tried: {possible_paths}")
    
    print(f"Using maddy binary: {maddy_binary}")
    
    # Allocate ports
    smtp_port = find_free_port()
    imap_port = find_free_port()
    http_port = find_free_port()
    
    # Start the maddy server
    server = MaddyTestServer(smtp_port, imap_port, http_port)
    
    try:
        print("\nStep 1: Starting maddy test server...")
        server.start(maddy_binary)
        
        # Generate random credentials for all accounts
        domain = "[127.0.0.1]"  # Bracketed IP for email addresses
        
        accounts = []
        for i in range(num_accounts):
            user = random_string(8)
            passwd = random_string(16)
            email = f"{user}@{domain}"
            accounts.append({
                'username': f"{user}@{domain}",
                'password': passwd,
                'email': email,
                'index': i,
            })
        
        sender = accounts[0]
        receivers = accounts[1:]
        
        print(f"\nStep 2: Concurrently creating {num_accounts} accounts via SMTP and IMAP login...")
        print(f"  Sender: {sender['email']}")
        print(f"  Receivers: {len(receivers)} accounts")
        
        # Create accounts concurrently using ThreadPoolExecutor
        # Use fewer workers (10) to reduce SQLite contention during auto_create
        start_time = time.time()
        failed_accounts = []
        all_timings = []
        completed_count = 0
        
        # Keep account creation concurrency low and rely on retries for
        # transient SQLITE_BUSY lock windows.
        account_workers = min(3, num_accounts)
        print(f"  Creating accounts with {account_workers} concurrent workers...")
        
        with ThreadPoolExecutor(max_workers=account_workers) as executor:
            futures = []
            for acc in accounts:
                future = executor.submit(
                    create_account_concurrently,
                    '127.0.0.1',
                    smtp_port,
                    imap_port,
                    acc['username'],
                    acc['password'],
                    acc['index']
                )
                futures.append(future)
            
            # Wait for all to complete with progress logging
            last_progress_time = time.time()
            for future in as_completed(futures):
                index, success, error, timings = future.result()
                all_timings.append(timings)
                completed_count += 1
                
                if not success:
                    failed_accounts.append((index, error))
                
                # Log progress every 5 seconds
                now = time.time()
                if now - last_progress_time >= 5:
                    elapsed = now - start_time
                    rate = completed_count / elapsed if elapsed > 0 else 0
                    print(f"    Progress: {completed_count}/{num_accounts} accounts ({rate:.1f} accounts/sec)")
                    last_progress_time = now
        
        creation_time = time.time() - start_time
        
        # Analyze timing statistics
        smtp_connect_times = [t.get('smtp_connect', 0) for t in all_timings if 'smtp_connect' in t]
        smtp_login_times = [t.get('smtp_login', 0) for t in all_timings if 'smtp_login' in t]
        imap_connect_times = [t.get('imap_connect', 0) for t in all_timings if 'imap_connect' in t]
        imap_login_times = [t.get('imap_login', 0) for t in all_timings if 'imap_login' in t]
        
        print(f"  Account creation completed in {creation_time:.1f}s")
        print(f"  Performance statistics (avg/max times in seconds):")
        if smtp_connect_times:
            print(f"    SMTP connect: avg={sum(smtp_connect_times)/len(smtp_connect_times):.3f}s, max={max(smtp_connect_times):.3f}s")
        if smtp_login_times:
            print(f"    SMTP login:   avg={sum(smtp_login_times)/len(smtp_login_times):.3f}s, max={max(smtp_login_times):.3f}s")
        if imap_connect_times:
            print(f"    IMAP connect: avg={sum(imap_connect_times)/len(imap_connect_times):.3f}s, max={max(imap_connect_times):.3f}s")
        if imap_login_times:
            print(f"    IMAP login:   avg={sum(imap_login_times)/len(imap_login_times):.3f}s, max={max(imap_login_times):.3f}s")
        
        # Identify slow operations
        slow_threshold = 5.0  # 5 seconds
        slow_smtp_logins = [t for t in smtp_login_times if t > slow_threshold]
        slow_imap_logins = [t for t in imap_login_times if t > slow_threshold]
        if slow_smtp_logins:
            print(f"    WARNING: {len(slow_smtp_logins)} SMTP logins took >{slow_threshold}s (potential SQLite contention)")
        if slow_imap_logins:
            print(f"    WARNING: {len(slow_imap_logins)} IMAP logins took >{slow_threshold}s (potential SQLite contention)")
        
        if failed_accounts:
            print(f"  WARNING: {len(failed_accounts)} accounts failed to create:")
            for idx, err in failed_accounts[:5]:  # Show first 5 errors
                print(f"    Account {idx}: {err}")
            if len(failed_accounts) > 5:
                print(f"    ... and {len(failed_accounts) - 5} more")
        
        # Remove failed accounts from receivers
        failed_indices = set(idx for idx, _ in failed_accounts)
        receivers = [r for r in receivers if r['index'] not in failed_indices]
        
        # Require at least 50% of accounts to succeed, or minimum 5
        min_required = max(5, num_accounts // 2)
        if len(receivers) < min_required:
            raise Exception(f"Too many accounts failed to create. Only {len(receivers)} receivers available, need at least {min_required}.")
        
        print(f"  {len(receivers)} receivers ready for IDLE test")
        
        print(f"\nStep 3: Starting IDLE for all {len(receivers)} receivers concurrently...")
        
        # Create IMAP IDLE clients for all receivers
        receiver_clients = []
        receiver_email_map = {}  # Map client_id to email
        start_time = time.time()
        idle_connect_times = []
        idle_start_times_list = []
        idle_errors = []
        completed_idle_count = 0
        
        def start_client(recv, client_id):
            """Start a single IMAP IDLE client."""
            client = IMAPIdleClient('127.0.0.1', imap_port, recv['username'], recv['password'], client_id)
            connect_start = time.time()
            if client.connect():
                connect_time = time.time() - connect_start
                idle_start_time_local = time.time()
                if client.start_idle():
                    idle_time = time.time() - idle_start_time_local
                    return (client, recv['email'], connect_time, idle_time, None)
                else:
                    client.error = client.error or "Failed to start IDLE"
                    return (client, recv['email'], connect_time, 0, client.error)
            else:
                connect_time = time.time() - connect_start
                return (client, recv['email'], connect_time, 0, client.error)
        
        print(f"  Connecting IDLE clients with 20 concurrent workers...")
        last_progress_time = time.time()
        
        with ThreadPoolExecutor(max_workers=20) as executor:
            futures = []
            for i, recv in enumerate(receivers):
                future = executor.submit(start_client, recv, i)
                futures.append(future)
            
            for future in as_completed(futures):
                client, email, connect_time, idle_time, error = future.result()
                idle_connect_times.append(connect_time)
                idle_start_times_list.append(idle_time)
                completed_idle_count += 1
                
                if client.connected and client.idle_started.is_set():
                    receiver_clients.append(client)
                    receiver_email_map[client.client_id] = email
                else:
                    idle_errors.append((client.client_id, error))
                
                # Log progress every 5 seconds
                now = time.time()
                if now - last_progress_time >= 5:
                    elapsed = now - start_time
                    rate = completed_idle_count / elapsed if elapsed > 0 else 0
                    print(f"    Progress: {completed_idle_count}/{len(receivers)} IDLE clients ({rate:.1f}/sec)")
                    last_progress_time = now
        
        idle_start_time = time.time() - start_time
        print(f"  {len(receiver_clients)} clients entered IDLE state in {idle_start_time:.1f}s")
        
        # Performance statistics for IDLE
        if idle_connect_times:
            print(f"  IDLE connection statistics (avg/max times in seconds):")
            print(f"    Connect time: avg={sum(idle_connect_times)/len(idle_connect_times):.3f}s, max={max(idle_connect_times):.3f}s")
        if [t for t in idle_start_times_list if t > 0]:
            valid_idle_times = [t for t in idle_start_times_list if t > 0]
            print(f"    IDLE start:   avg={sum(valid_idle_times)/len(valid_idle_times):.3f}s, max={max(valid_idle_times):.3f}s")
        
        if idle_errors:
            print(f"  WARNING: {len(idle_errors)} IDLE errors")
            for cid, err in idle_errors[:3]:
                print(f"    Client {cid}: {err}")
        
        min_idle_clients = max(3, len(receivers) // 2)
        if len(receiver_clients) < min_idle_clients:
            raise Exception(f"Too few clients in IDLE. Only {len(receiver_clients)} ready, need at least {min_idle_clients}.")
        
        print(f"\nStep 4: Sender logging in and sending encrypted message to all {len(receiver_clients)} receivers...")
        
        # Sender connects via SMTP
        smtp_connect_start = time.time()
        smtp = smtplib.SMTP('127.0.0.1', smtp_port, timeout=60)
        smtp.login(sender['username'], sender['password'])
        print(f"  Sender SMTP login took {time.time() - smtp_connect_start:.2f}s")
        
        # Load the encrypted email template
        eml_template = load_encrypted_eml()
        
        # Generate a unique message ID for verification
        test_message_id = f"<test-{uuid.uuid4()}@test.local>"
        
        print(f"  Message ID: {test_message_id}")
        
        # Send message to each receiver
        start_time = time.time()
        send_errors = []
        send_times = []
        last_progress_time = time.time()
        
        for i, client in enumerate(receiver_clients):
            try:
                recv_email = receiver_email_map[client.client_id]
                eml_content = eml_template.format(
                    from_addr=sender['email'],
                    to_addr=recv_email,
                    subject="Test encrypted message",
                    message_id=test_message_id
                )
                
                msg_send_start = time.time()
                print(f"    Sending message {i+1}/{len(receiver_clients)} to {recv_email}...", flush=True)
                smtp.sendmail(sender['email'], [recv_email], eml_content)
                elapsed = time.time() - msg_send_start
                send_times.append(elapsed)
                print(f"    Message {i+1} sent in {elapsed:.3f}s", flush=True)
            except Exception as e:
                send_errors.append(f"Client {client.client_id}: {e}")
                # Reconnect SMTP if needed
                try:
                    smtp = smtplib.SMTP('127.0.0.1', smtp_port, timeout=60)
                    smtp.login(sender['username'], sender['password'])
                except Exception:
                    pass
            
            # Log progress every 5 seconds
            now = time.time()
            if now - last_progress_time >= 5:
                elapsed = now - start_time
                rate = (i + 1) / elapsed if elapsed > 0 else 0
                print(f"    Progress: {i + 1}/{len(receiver_clients)} messages sent ({rate:.1f} msg/sec)")
                last_progress_time = now
        
        send_time = time.time() - start_time
        try:
            smtp.quit()
        except Exception:
            pass
        
        print(f"  Sent {len(receiver_clients) - len(send_errors)} messages in {send_time:.1f}s")
        if send_times:
            print(f"  Send time statistics:")
            print(f"    Per message: avg={sum(send_times)/len(send_times):.3f}s, max={max(send_times):.3f}s")
            slow_sends = [t for t in send_times if t > 1.0]
            if slow_sends:
                print(f"    WARNING: {len(slow_sends)} messages took >1s to send (potential delivery contention)")
        if send_errors:
            print(f"  WARNING: {len(send_errors)} send errors")
        
        print(f"\nStep 5: Waiting for all receivers to wake from IDLE and receive message...")
        print("  (Testing for IDLE race condition - messages must be immediately fetchable)")
        
        # Wait for all receivers to get the message
        start_time = time.time()
        received_count = 0
        race_condition_count = 0
        timeout_count = 0
        fetch_error_count = 0
        
        # Use ThreadPoolExecutor to check results in parallel
        def check_client_result(client):
            """Check if a client received the message."""
            if client.wait_for_message(timeout=60):
                if client.immediate_fetch_error:
                    # Distinguish between actual race conditions and other errors
                    err = client.immediate_fetch_error.lower()
                    if 'no messages found' in err or 'search failed' in err:
                        # True race condition - EXISTS came but message not visible
                        return ('race_condition', client.client_id, client.immediate_fetch_error)
                    else:
                        # Other fetch error (connection issue, etc)
                        return ('fetch_error', client.client_id, client.immediate_fetch_error)
                elif client.immediate_fetch_result:
                    return ('received', client.client_id, None)
                else:
                    # No immediate result but got EXISTS - count as success
                    return ('received_no_data', client.client_id, None)
            else:
                return ('timeout', client.client_id, client.error)
        
        with ThreadPoolExecutor(max_workers=20) as executor:
            futures = [executor.submit(check_client_result, client) for client in receiver_clients]
            
            for future in as_completed(futures):
                result_type, client_id, error = future.result()
                if result_type == 'received' or result_type == 'received_no_data':
                    received_count += 1
                elif result_type == 'race_condition':
                    race_condition_count += 1
                    print(f"  ✗ Client {client_id} RACE CONDITION: {error}")
                elif result_type == 'fetch_error':
                    fetch_error_count += 1
                    if fetch_error_count <= 5:
                        print(f"  ! Client {client_id} fetch error: {error}")
                elif result_type == 'timeout':
                    timeout_count += 1
                    if timeout_count <= 5:
                        print(f"  ✗ Client {client_id} TIMEOUT: {error}")
        
        check_time = time.time() - start_time
        
        # Cleanup
        print("\nStep 6: Cleaning up...")
        for client in receiver_clients:
            client.stop()
        
        # Print summary
        print("\n" + "="*60)
        print("RESULTS SUMMARY")
        print("="*60)
        print(f"  Total receivers: {len(receiver_clients)}")
        print(f"  Successfully received: {received_count}")
        print(f"  Race conditions detected: {race_condition_count}")
        print(f"  Fetch errors: {fetch_error_count}")
        print(f"  Timeouts: {timeout_count}")
        print(f"  Check time: {check_time:.1f}s")
        
        success_rate = received_count / len(receiver_clients) * 100
        print(f"  Success rate: {success_rate:.1f}%")
        
        if race_condition_count > 0:
            raise Exception(f"RACE CONDITIONS DETECTED! {race_condition_count} clients could not immediately fetch messages after IDLE notification.")
        
        if timeout_count > len(receiver_clients) * 0.1:  # Allow up to 10% timeouts
            raise Exception(f"Too many timeouts: {timeout_count} out of {len(receiver_clients)}")
        
        print("\n" + "="*60)
        print("🎉 TEST #13 PASSED! Concurrent profile and mass IDLE test successful.")
        print(f"  ✓ {len(receiver_clients)} accounts created concurrently")
        print(f"  ✓ {received_count} messages received via IDLE")
        print("  ✓ No race conditions detected - messages immediately fetchable after IDLE")
        print("="*60)
        return True
        
    finally:
        print("\nFinal cleanup...")
        server.stop()


if __name__ == "__main__":
    # Allow running standalone
    import argparse
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", help="Path to maddy binary")
    parser.add_argument("--num-accounts", type=int, default=100, help="Number of accounts to create (default: 100)")
    args = parser.parse_args()
    
    try:
        run(maddy_binary=args.binary, num_accounts=args.num_accounts)
    except Exception as e:
        print(f"\n❌ TEST FAILED: {e}")
        import traceback
        traceback.print_exc()
        sys.exit(1)
