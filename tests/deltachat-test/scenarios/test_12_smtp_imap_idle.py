"""
Test #12: SMTP/IMAP IDLE Test

This test verifies the full email flow with IMAP IDLE functionality:
1. Start a maddy test server using the "simple" config with custom ports
2. Let one "sender" login to SMTP and IMAP with a random address (auto-create)
3. Let 3 receivers login to IMAP with random addresses and wait in IDLE state
4. Sender sends one encrypted message to all receivers
5. Verify all receivers wake up from IDLE and receive the message
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
        print(f"  Config directory: {config_dir}")
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
        """Generate a minimal maddy config for testing."""
        return f'''
# Maddy test configuration for SMTP/IMAP IDLE test
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
storage.imapsql local_mailboxes {{
    auto_create yes
    driver sqlite3
    dsn {state_dir}/imapsql.db
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
    
    def __init__(self, host, port, username, password):
        self.host = host
        self.port = port
        self.username = username
        self.password = password
        self.imap = None
        self.idle_event = threading.Event()
        self.messages_received = []
        self.thread = None
        self.running = False
        self.idle_started = threading.Event()
        self.error = None
        # Track immediate fetch results to detect race conditions
        self.immediate_fetch_result = None
        self.immediate_fetch_error = None
        
    def connect(self):
        """Connect and login to IMAP server."""
        self.imap = imaplib.IMAP4(self.host, self.port)
        self.imap.login(self.username, self.password)
        self.imap.select('INBOX')
        print(f"    IMAP connected: {self.username}")
        
    def start_idle(self):
        """Start IDLE in a background thread."""
        self.running = True
        self.thread = threading.Thread(target=self._idle_loop, daemon=True)
        self.thread.start()
        # Wait for IDLE to be established
        self.idle_started.wait(timeout=5)
        
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
                print(f"    IDLE failed for {self.username}: {response}")
                self.error = f"IDLE failed: {response}"
                self.idle_started.set()
                return
            
            print(f"    {self.username} entered IDLE state")
            self.idle_started.set()
            
            # Wait for EXISTS notification with longer timeout
            while self.running:
                self.imap.sock.settimeout(60.0)  # 60 second timeout per read
                try:
                    line = self.imap.readline()
                    if line:
                        line_str = line.decode().strip()
                        if 'EXISTS' in line_str:
                            print(f"    {self.username} received EXISTS: {line_str}")
                            
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
                                            print(f"    {self.username} immediately fetched message successfully")
                                        else:
                                            self.immediate_fetch_error = f"FETCH failed: {status}"
                                            print(f"    {self.username} immediate FETCH failed: {status}")
                                    else:
                                        self.immediate_fetch_error = "No messages found after EXISTS"
                                        print(f"    {self.username} no messages found immediately after EXISTS!")
                                else:
                                    self.immediate_fetch_error = f"SEARCH failed: {status}"
                                    print(f"    {self.username} immediate SEARCH failed: {status}")
                            except Exception as e:
                                self.immediate_fetch_error = str(e)
                                print(f"    {self.username} immediate fetch error: {e}")
                            
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
            print(f"    IDLE error for {self.username}: {e}")
            
    def wait_for_message(self, timeout=30):
        """Wait for a message to arrive via IDLE."""
        return self.idle_event.wait(timeout=timeout)
    
    def fetch_messages(self):
        """Fetch all messages from INBOX."""
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
    
    def stop(self):
        """Stop the IDLE client."""
        self.running = False
        if self.thread:
            self.thread.join(timeout=5)
        if self.imap:
            try:
                self.imap.logout()
            except (socket.error, imaplib.IMAP4.error, OSError):
                pass


def run(test_dir=None, maddy_binary=None):
    """Run the SMTP/IMAP IDLE test."""
    print("\n" + "="*50)
    print("TEST #12: SMTP/IMAP IDLE Test")
    print("="*50)
    
    # Find maddy binary
    if not maddy_binary:
        possible_paths = [
            os.path.abspath("build/maddy"),
            os.path.abspath("maddy"),
            "/usr/local/bin/maddy",
        ]
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
        
        # Generate random credentials
        domain = "[127.0.0.1]"  # Bracketed IP for email addresses
        
        sender_user = random_string(8)
        sender_pass = random_string(16)
        sender_email = f"{sender_user}@{domain}"
        
        receivers = []
        for i in range(3):
            user = random_string(8)
            passwd = random_string(16)
            email = f"{user}@{domain}"
            receivers.append({
                'username': f"{user}@{domain}",
                'password': passwd,
                'email': email,
            })
        
        print(f"\nStep 2: Creating sender account: {sender_email}")
        
        # Test SMTP login for sender (this auto-creates the account)
        smtp = smtplib.SMTP('127.0.0.1', smtp_port)
        smtp.login(f"{sender_user}@{domain}", sender_pass)
        print(f"  Sender SMTP login successful")
        
        # Test IMAP login for sender
        sender_imap = imaplib.IMAP4('127.0.0.1', imap_port)
        sender_imap.login(f"{sender_user}@{domain}", sender_pass)
        sender_imap.select('INBOX')
        print(f"  Sender IMAP login successful")
        
        print(f"\nStep 3: Creating {len(receivers)} receiver accounts and starting IDLE...")
        
        # Create receiver IMAP clients and start IDLE
        receiver_clients = []
        for i, recv in enumerate(receivers):
            print(f"  Receiver {i+1}: {recv['email']}")
            client = IMAPIdleClient('127.0.0.1', imap_port, recv['username'], recv['password'])
            client.connect()
            client.start_idle()
            receiver_clients.append(client)
        
        # Verify all IDLE sessions are established
        print(f"  Verifying all receivers are in IDLE state...")
        for i, client in enumerate(receiver_clients):
            if client.error:
                raise Exception(f"Receiver {i+1} failed to enter IDLE: {client.error}")
        print(f"  All {len(receivers)} receivers are in IDLE state")
        
        print(f"\nStep 4: Sending encrypted message to all receivers...")
        
        # Load the encrypted email template
        eml_template = load_encrypted_eml()
        
        # Generate a unique message ID for verification
        test_message_id = f"<test-{uuid.uuid4()}@test.local>"
        
        print(f"  Message ID: {test_message_id}")
        
        # Send message to each receiver individually
        to_addrs = [recv['email'] for recv in receivers]
        print(f"  Recipients: {to_addrs}")
        
        for recv in receivers:
            # Create the email using the template with all placeholders
            eml_content = eml_template.format(
                from_addr=sender_email,
                to_addr=recv['email'],
                subject="Test encrypted message",
                message_id=test_message_id
            )
            
            # Send via SMTP
            smtp.sendmail(sender_email, [recv['email']], eml_content)
            print(f"    Sent to {recv['email']}")
        
        print(f"  All messages sent via SMTP")
        
        print(f"\nStep 5: Verifying all receivers wake up from IDLE and receive the message...")
        print("  (Testing for IDLE race condition - messages must be immediately fetchable)")
        
        # Wait for all receivers to get the message
        all_received = True
        immediate_fetch_worked = True
        for i, client in enumerate(receiver_clients):
            print(f"  Waiting for receiver {i+1}...")
            if client.wait_for_message(timeout=30):
                print(f"    ✓ Receiver {i+1} woke up from IDLE")
                
                # Check if immediate fetch worked (race condition test)
                if client.immediate_fetch_error:
                    print(f"    ✗ RACE CONDITION DETECTED! Receiver {i+1} immediate fetch failed: {client.immediate_fetch_error}")
                    immediate_fetch_worked = False
                elif client.immediate_fetch_result:
                    msg_id_stripped = test_message_id.strip('<>')
                    msg_content = client.immediate_fetch_result.decode('utf-8', errors='replace')
                    if msg_id_stripped in msg_content:
                        print(f"    ✓ Receiver {i+1} immediately fetched correct message (no race condition)")
                    else:
                        print(f"    ✓ Receiver {i+1} immediately fetched a message (content verification separate)")
                else:
                    print(f"    ? Receiver {i+1} no immediate fetch result recorded")
                
                # Also verify via regular fetch
                messages = client.fetch_messages()
                found_message = False
                msg_id_stripped = test_message_id.strip('<>')
                for msg in messages:
                    msg_content = msg.decode('utf-8', errors='replace')
                    if msg_id_stripped in msg_content:
                        found_message = True
                        break
                
                if found_message:
                    print(f"    ✓ Receiver {i+1} received correct message")
                else:
                    print(f"    ✗ Receiver {i+1} did not receive the expected message (ID: {test_message_id})")
                    all_received = False
            else:
                print(f"    ✗ Receiver {i+1} did not receive message in time")
                all_received = False
        
        # Cleanup
        smtp.quit()
        sender_imap.logout()
        for client in receiver_clients:
            client.stop()
        
        if not immediate_fetch_worked:
            raise Exception("RACE CONDITION DETECTED: Messages were not visible when IDLE notification was received")
        
        if not all_received:
            raise Exception("Not all receivers received the message correctly")
        
        print("\n" + "="*50)
        print("🎉 TEST #12 PASSED! SMTP/IMAP IDLE test successful.")
        print("  ✓ No race condition detected - messages immediately fetchable after IDLE")
        print("="*50)
        return True
        
    finally:
        print("\nCleaning up...")
        server.stop()


if __name__ == "__main__":
    # Allow running standalone
    import argparse
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", help="Path to maddy binary")
    args = parser.parse_args()
    
    try:
        run(maddy_binary=args.binary)
    except Exception as e:
        print(f"\n❌ TEST FAILED: {e}")
        import traceback
        traceback.print_exc()
        sys.exit(1)
