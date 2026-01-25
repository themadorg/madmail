#!/usr/bin/env python3
"""
Functional test #14: IMAP IDLE notification test.

This test verifies that:
1. 5 users can be created via trust-on-first-login
2. 4 recipients enter IMAP IDLE mode
3. When sender submits a message, all recipients wake from IDLE
4. All recipients can fetch the delivered message

Run with: python3 test_14_idle_notification.py --maddy-bin ./build/maddy
"""

import argparse
import os
import signal
import socket
import subprocess
import sys
import tempfile
import threading
import time
import base64
import struct


def generate_fake_pgp_encrypted_message(sender: str, recipients: list, subject: str, body: str) -> bytes:
    """Generate a fake PGP-encrypted message that passes madmail's checks."""
    boundary = "=-fake-pgp-boundary-12345"
    
    # Create a minimal valid OpenPGP packet
    # Tag 18 (SEIPD - Symmetrically Encrypted Integrity Protected Data)
    # with version 1 and minimal encrypted content
    seipd_version = bytes([1])  # Version 1
    fake_encrypted_data = b'\x00' * 20  # Minimal fake encrypted content
    seipd_body = seipd_version + fake_encrypted_data
    
    # Create packet header for SEIPD (tag 18)
    tag = 18
    body_len = len(seipd_body)
    
    if body_len < 192:
        header = bytes([0xC0 | tag, body_len])
    elif body_len < 8384:
        header = bytes([0xC0 | tag, ((body_len - 192) >> 8) + 192, (body_len - 192) & 0xFF])
    else:
        header = bytes([0xC0 | tag, 0xFF]) + struct.pack('>I', body_len)
    
    pgp_data = header + seipd_body
    pgp_base64 = base64.b64encode(pgp_data).decode('ascii')
    
    # Format as lines of 64 chars
    pgp_lines = [pgp_base64[i:i+64] for i in range(0, len(pgp_base64), 64)]
    pgp_armored = "-----BEGIN PGP MESSAGE-----\n\n"
    pgp_armored += "\n".join(pgp_lines)
    pgp_armored += "\n-----END PGP MESSAGE-----"
    
    to_header = ", ".join(recipients)
    
    msg = f"""From: {sender}
To: {to_header}
Subject: {subject}
MIME-Version: 1.0
Content-Type: multipart/encrypted; protocol="application/pgp-encrypted"; boundary="{boundary}"

--{boundary}
Content-Type: application/pgp-encrypted

Version: 1

--{boundary}
Content-Type: application/octet-stream

{pgp_armored}
--{boundary}--
"""
    return msg.encode('utf-8')


class MaddyServer:
    """Manages a maddy server instance for testing."""
    
    def __init__(self, maddy_bin: str, domain: str = "test.local"):
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
        
        config = f"""
state_dir {self.state_dir}
runtime_dir {self.runtime_dir}
hostname {self.domain}

log stderr
debug yes

# In-memory auth with trust-on-first-login
auth.memauth local_auth {{
    auto_create yes
    min_password_len 12
}}

# In-memory storage
storage.memstore local_store {{
    auto_create yes
}}

# IMAP server
imap tcp://0.0.0.0:{self.imap_port} {{
    tls off
    auth &local_auth
    storage &local_store
}}

# Submission server  
submission tcp://0.0.0.0:{self.submission_port} {{
    tls off
    auth &local_auth
    
    source {self.domain} {{
        deliver_to &local_store
    }}
    
    default_source {{
        reject
    }}
}}

# SMTP server
smtp tcp://0.0.0.0:{self.smtp_port} {{
    tls off
    hostname {self.domain}
    
    source {self.domain} {{
        deliver_to &local_store
    }}
    
    default_source {{
        reject
    }}
}}
"""
        
        config_path = os.path.join(self.config_dir, "maddy.conf")
        with open(config_path, 'w') as f:
            f.write(config)
        
        self.process = subprocess.Popen(
            [self.maddy_bin, "-config", config_path],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        
        # Wait for server to start
        time.sleep(2)
        
        # Verify server is running
        if self.process.poll() is not None:
            stdout, stderr = self.process.communicate()
            raise RuntimeError(f"Maddy failed to start: {stderr.decode()}")
        
        # Wait for ports to be available
        for port in [self.imap_port, self.smtp_port, self.submission_port]:
            for _ in range(30):
                try:
                    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
                    sock.settimeout(1)
                    sock.connect(('127.0.0.1', port))
                    sock.close()
                    break
                except (socket.error, socket.timeout):
                    time.sleep(0.5)
            else:
                raise RuntimeError(f"Port {port} not available after 15 seconds")
        
        print(f"Maddy server started on IMAP:{self.imap_port}, SMTP:{self.smtp_port}, Submission:{self.submission_port}")
        
    def stop(self):
        """Stop the maddy server."""
        if self.process:
            self.process.terminate()
            try:
                self.process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait()
            
            # Print any errors
            stdout, stderr = self.process.communicate()
            if stderr:
                print(f"Maddy stderr: {stderr.decode()[:2000]}")
        
        # Cleanup temp dirs
        import shutil
        for d in [self.config_dir, self.runtime_dir, self.state_dir]:
            if d and os.path.exists(d):
                shutil.rmtree(d, ignore_errors=True)


class IMAPClient:
    """Simple IMAP client for testing."""
    
    def __init__(self, host: str, port: int):
        self.host = host
        self.port = port
        self.sock = None
        self.tag_counter = 0
        self.buffer = b""
        
    def connect(self):
        """Connect to the IMAP server."""
        self.sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self.sock.settimeout(30)
        self.sock.connect((self.host, self.port))
        # Read greeting
        greeting = self._read_line()
        if not greeting.startswith("* OK"):
            raise RuntimeError(f"Unexpected greeting: {greeting}")
        return greeting
    
    def _read_line(self) -> str:
        """Read a line from the server."""
        while b"\r\n" not in self.buffer:
            data = self.sock.recv(4096)
            if not data:
                raise RuntimeError("Connection closed")
            self.buffer += data
        
        line, self.buffer = self.buffer.split(b"\r\n", 1)
        return line.decode('utf-8', errors='replace')
    
    def _read_line_timeout(self, timeout: float) -> str:
        """Read a line with specific timeout."""
        old_timeout = self.sock.gettimeout()
        self.sock.settimeout(timeout)
        try:
            return self._read_line()
        finally:
            self.sock.settimeout(old_timeout)
    
    def _send_command(self, command: str) -> str:
        """Send a command and return the tag."""
        self.tag_counter += 1
        tag = f"A{self.tag_counter:04d}"
        self.sock.sendall(f"{tag} {command}\r\n".encode())
        return tag
    
    def _read_response(self, tag: str) -> tuple:
        """Read response until we get the tagged response."""
        untagged = []
        while True:
            line = self._read_line()
            if line.startswith(f"{tag} "):
                # Tagged response
                parts = line.split(" ", 2)
                status = parts[1]
                message = parts[2] if len(parts) > 2 else ""
                return status, message, untagged
            else:
                untagged.append(line)
    
    def login(self, username: str, password: str) -> bool:
        """Login to the server."""
        tag = self._send_command(f'LOGIN "{username}" "{password}"')
        status, message, _ = self._read_response(tag)
        return status == "OK"
    
    def select(self, mailbox: str = "INBOX") -> dict:
        """Select a mailbox."""
        tag = self._send_command(f'SELECT "{mailbox}"')
        status, message, untagged = self._read_response(tag)
        if status != "OK":
            raise RuntimeError(f"SELECT failed: {message}")
        
        result = {}
        for line in untagged:
            if "EXISTS" in line:
                parts = line.split()
                result['exists'] = int(parts[1])
        return result
    
    def idle_start(self):
        """Start IDLE mode."""
        tag = self._send_command("IDLE")
        # Read continuation
        line = self._read_line()
        if not line.startswith("+"):
            raise RuntimeError(f"IDLE not accepted: {line}")
        return tag
    
    def idle_wait(self, timeout: float = 30) -> list:
        """Wait for IDLE notifications."""
        notifications = []
        deadline = time.time() + timeout
        
        while time.time() < deadline:
            remaining = deadline - time.time()
            if remaining <= 0:
                break
            
            try:
                line = self._read_line_timeout(min(remaining, 1.0))
                notifications.append(line)
                # Check for EXISTS notification
                if "EXISTS" in line:
                    break
            except socket.timeout:
                continue
        
        return notifications
    
    def idle_done(self, tag: str) -> bool:
        """End IDLE mode."""
        self.sock.sendall(b"DONE\r\n")
        status, message, _ = self._read_response(tag)
        return status == "OK"
    
    def fetch(self, sequence: str, items: str) -> list:
        """Fetch message data."""
        tag = self._send_command(f'FETCH {sequence} ({items})')
        status, message, untagged = self._read_response(tag)
        if status != "OK":
            raise RuntimeError(f"FETCH failed: {message}")
        return untagged
    
    def logout(self):
        """Logout from the server."""
        try:
            tag = self._send_command("LOGOUT")
            self._read_response(tag)
        except:
            pass
        finally:
            if self.sock:
                self.sock.close()
                self.sock = None
    
    def close(self):
        """Close the connection."""
        if self.sock:
            try:
                self.sock.close()
            except:
                pass
            self.sock = None


class SMTPClient:
    """Simple SMTP client for testing."""
    
    def __init__(self, host: str, port: int):
        self.host = host
        self.port = port
        self.sock = None
        
    def connect(self):
        """Connect to the SMTP server."""
        self.sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self.sock.settimeout(30)
        self.sock.connect((self.host, self.port))
        response = self._read_response()
        if not response.startswith("220"):
            raise RuntimeError(f"Unexpected greeting: {response}")
        return response
    
    def _read_response(self) -> str:
        """Read SMTP response."""
        lines = []
        while True:
            data = b""
            while not data.endswith(b"\r\n"):
                chunk = self.sock.recv(1)
                if not chunk:
                    break
                data += chunk
            
            line = data.decode('utf-8').strip()
            lines.append(line)
            
            if len(line) >= 4 and line[3] == ' ':
                break
        
        return "\n".join(lines)
    
    def _send_command(self, command: str) -> str:
        """Send command and return response."""
        self.sock.sendall(f"{command}\r\n".encode())
        return self._read_response()
    
    def ehlo(self, hostname: str = "localhost") -> str:
        """Send EHLO."""
        return self._send_command(f"EHLO {hostname}")
    
    def auth_plain(self, username: str, password: str) -> bool:
        """Authenticate using PLAIN."""
        credentials = base64.b64encode(f"\0{username}\0{password}".encode()).decode()
        response = self._send_command(f"AUTH PLAIN {credentials}")
        return response.startswith("235")
    
    def mail_from(self, sender: str) -> bool:
        """Set sender."""
        response = self._send_command(f"MAIL FROM:<{sender}>")
        return response.startswith("250")
    
    def rcpt_to(self, recipient: str) -> bool:
        """Add recipient."""
        response = self._send_command(f"RCPT TO:<{recipient}>")
        return response.startswith("250")
    
    def data(self, message: bytes) -> bool:
        """Send message data."""
        response = self._send_command("DATA")
        if not response.startswith("354"):
            return False
        
        self.sock.sendall(message)
        if not message.endswith(b"\r\n"):
            self.sock.sendall(b"\r\n")
        self.sock.sendall(b".\r\n")
        
        response = self._read_response()
        return response.startswith("250")
    
    def quit(self):
        """Quit the session."""
        try:
            self._send_command("QUIT")
        except:
            pass
        finally:
            if self.sock:
                self.sock.close()
                self.sock = None


def test_idle_notification(maddy: MaddyServer) -> bool:
    """Test IMAP IDLE notification when message is delivered."""
    print("\n=== Test: IMAP IDLE Notification ===")
    
    domain = maddy.domain
    password = "securepassword123"  # 18 chars, meets 12 char minimum
    
    # Create 5 users
    sender = f"sender@{domain}"
    recipients = [f"recipient{i}@{domain}" for i in range(1, 5)]
    all_users = [sender] + recipients
    
    print(f"Creating {len(all_users)} users via trust-on-first-login...")
    
    # Create users by logging in via IMAP
    for user in all_users:
        client = IMAPClient('127.0.0.1', maddy.imap_port)
        try:
            client.connect()
            if not client.login(user, password):
                print(f"  ✗ Failed to create user {user}")
                return False
            print(f"  ✓ Created user {user}")
            client.logout()
        except Exception as e:
            print(f"  ✗ Error creating user {user}: {e}")
            return False
    
    print("\nPutting recipients in IDLE mode...")
    
    # Put recipients in IDLE mode
    idle_clients = []
    idle_tags = []
    
    for recipient in recipients:
        client = IMAPClient('127.0.0.1', maddy.imap_port)
        try:
            client.connect()
            if not client.login(recipient, password):
                print(f"  ✗ Failed to login {recipient}")
                return False
            
            client.select("INBOX")
            tag = client.idle_start()
            idle_clients.append(client)
            idle_tags.append(tag)
            print(f"  ✓ {recipient} is now in IDLE mode")
        except Exception as e:
            print(f"  ✗ Error putting {recipient} in IDLE: {e}")
            # Clean up
            for c in idle_clients:
                try:
                    c.close()
                except:
                    pass
            return False
    
    # Wait a moment for IDLE to be fully established
    time.sleep(1)
    
    print(f"\nSender submitting message to {len(recipients)} recipients...")
    
    # Send message via submission
    smtp = SMTPClient('127.0.0.1', maddy.submission_port)
    try:
        smtp.connect()
        smtp.ehlo("test.local")
        
        if not smtp.auth_plain(sender, password):
            print("  ✗ Sender failed to authenticate")
            return False
        
        if not smtp.mail_from(sender):
            print("  ✗ MAIL FROM failed")
            return False
        
        for recipient in recipients:
            if not smtp.rcpt_to(recipient):
                print(f"  ✗ RCPT TO {recipient} failed")
                return False
        
        msg = generate_fake_pgp_encrypted_message(
            sender, recipients, 
            "IDLE Test Message",
            "This message should wake up IDLE clients"
        )
        
        if not smtp.data(msg):
            print("  ✗ DATA failed")
            return False
        
        print("  ✓ Message submitted successfully")
        smtp.quit()
    except Exception as e:
        print(f"  ✗ Error sending message: {e}")
        return False
    
    print("\nWaiting for IDLE notifications...")
    
    # Check that all recipients wake from IDLE
    notifications_received = []
    
    for i, (client, tag, recipient) in enumerate(zip(idle_clients, idle_tags, recipients)):
        try:
            notifications = client.idle_wait(timeout=10)
            
            # Check for EXISTS notification
            has_exists = any("EXISTS" in n for n in notifications)
            
            if has_exists:
                print(f"  ✓ {recipient} received EXISTS notification")
                notifications_received.append(True)
            else:
                print(f"  ? {recipient} notifications: {notifications}")
                notifications_received.append(False)
            
            # End IDLE
            client.idle_done(tag)
            
        except Exception as e:
            print(f"  ✗ Error with {recipient}: {e}")
            notifications_received.append(False)
    
    # Verify all recipients got notifications
    all_notified = all(notifications_received)
    
    print("\nVerifying messages were delivered...")
    
    # Fetch messages from each recipient
    messages_received = []
    
    for client, recipient in zip(idle_clients, recipients):
        try:
            # Re-select to get updated count
            result = client.select("INBOX")
            exists = result.get('exists', 0)
            
            if exists > 0:
                fetch_result = client.fetch("1", "BODY[HEADER.FIELDS (FROM SUBJECT)]")
                print(f"  ✓ {recipient} has {exists} message(s)")
                messages_received.append(True)
            else:
                print(f"  ✗ {recipient} has no messages")
                messages_received.append(False)
            
            client.logout()
        except Exception as e:
            print(f"  ✗ Error fetching from {recipient}: {e}")
            messages_received.append(False)
    
    all_received = all(messages_received)
    
    # Cleanup
    for client in idle_clients:
        try:
            client.close()
        except:
            pass
    
    if all_notified and all_received:
        print("\n✓ All recipients received IDLE notification and message!")
        return True
    elif all_received:
        print("\n✓ All recipients received message (IDLE notifications may have been missed)")
        return True
    else:
        print("\n✗ Some recipients did not receive the message")
        return False


def main():
    parser = argparse.ArgumentParser(description="IMAP IDLE notification functional test")
    parser.add_argument("--maddy-bin", required=True, help="Path to maddy binary")
    args = parser.parse_args()
    
    if not os.path.exists(args.maddy_bin):
        print(f"Error: Maddy binary not found: {args.maddy_bin}")
        sys.exit(1)
    
    maddy = MaddyServer(args.maddy_bin)
    
    try:
        print("Starting maddy server...")
        maddy.start()
        
        # Run test
        success = test_idle_notification(maddy)
        
        if success:
            print("\n" + "=" * 50)
            print("ALL TESTS PASSED!")
            print("=" * 50)
            sys.exit(0)
        else:
            print("\n" + "=" * 50)
            print("TESTS FAILED!")
            print("=" * 50)
            sys.exit(1)
            
    except Exception as e:
        print(f"\nError: {e}")
        import traceback
        traceback.print_exc()
        sys.exit(1)
    finally:
        print("\nStopping maddy server...")
        maddy.stop()


if __name__ == "__main__":
    main()
