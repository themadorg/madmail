"""
Test #20: Madexchanger End-to-End Test

Verifies that the madexchanger correctly relays emails between two
Madmail servers using the exchange_url config directive.

    ┌─────────────┐             ┌──────────────┐            ┌─────────────┐
    │ Madmail S1   │─exchange──▷│ Madexchanger │──forward──▷│ Madmail S2   │
    │ target.remote│   _url     │ (port 443)   │            │              │
    └─────────────┘             └──────────────┘            └─────────────┘
                                      ▲
    ┌─────────────┐   exchange_url    │
    │ Madmail S2   │──────────────────┘
    └─────────────┘

Flow:
  1. Spin up 3 LXC containers: server1, server2, exchanger
  2. Install + start Madmail on server1 and server2
     with exchange_url pointing to the exchanger
  3. Install + start Madexchanger on exchanger (port 443, self-signed TLS)
  4. Create Delta Chat accounts on both servers
  5. Send messages both ways (server1 → server2, server2 → server1)
  6. Verify messages arrive (proving the exchanger routed them correctly)
  7. Verify the exchanger's /admin/stats shows route counters
"""

import os
import sys
import time
import json
import subprocess
import urllib.request

# For running inside the test suite or standalone
TEST_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, TEST_DIR)


# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

SSH_OPTS = "-o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o ConnectTimeout=10"
EXCHANGER_PORT = 443
ADMIN_TOKEN = "e2e-test-token"


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def ssh(remote, cmd, timeout=30):
    """Run a command on a remote LXC container via SSH."""
    full = f"ssh {SSH_OPTS} root@{remote} '{cmd}'"
    result = subprocess.run(full, shell=True, capture_output=True, text=True, timeout=timeout)
    if result.returncode != 0:
        print(f"    [SSH {remote}] FAILED: {cmd}")
        print(f"    stdout: {result.stdout[:500]}")
        print(f"    stderr: {result.stderr[:500]}")
    return result


def sudo(cmd, check=True, input_data=None):
    """Run a command with sudo."""
    path_env = "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"
    if isinstance(cmd, str):
        cmd = cmd.split()
    full = ["sudo", "env", path_env] + cmd
    if input_data:
        result = subprocess.run(full, input=input_data, capture_output=True)
    else:
        result = subprocess.run(full, capture_output=True, text=True)
    if check and result.returncode != 0:
        stdout = result.stdout.decode() if isinstance(result.stdout, bytes) else result.stdout
        stderr = result.stderr.decode() if isinstance(result.stderr, bytes) else result.stderr
        raise Exception(f"Command failed: {' '.join(full)}\nstdout: {stdout}\nstderr: {stderr}")
    return result.stdout.strip() if not input_data else result.stdout


def wait_for_port(ip, port, timeout=30):
    """Wait for a TCP port to be reachable."""
    import socket
    start = time.time()
    while time.time() - start < timeout:
        try:
            s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            s.settimeout(2)
            s.connect((ip, port))
            s.close()
            return True
        except Exception:
            time.sleep(1)
    return False


# ---------------------------------------------------------------------------
# LXC Container Management
# ---------------------------------------------------------------------------

class ExchangerLXC:
    """Manages 3 LXC containers: 2 Madmail + 1 Madexchanger."""

    CONTAINERS = ["madmail-xch-s1", "madmail-xch-s2", "madmail-xch-ex"]

    def __init__(self, logger=None):
        self.logger = logger or print
        self.ips = {}

    def setup(self, madmail_bin, exchanger_bin):
        """Create and configure all 3 containers."""
        self.logger("Setting up Exchanger E2E LXC environment...")

        existing = sudo("lxc-ls").split()

        for name in self.CONTAINERS:
            if name in existing:
                self.logger(f"  Destroying existing container {name}...")
                sudo(f"lxc-stop -n {name} -k", check=False)
                sudo(f"lxc-destroy -n {name}")

            self.logger(f"  Creating container {name}...")
            sudo(f"lxc-create -n {name} -t download -- -d debian -r bookworm -a amd64")

            # Resource limits
            config_path = f"/var/lib/lxc/{name}/config"
            sudo(["sh", "-c", f"echo 'lxc.cgroup2.memory.max = 1G' >> {config_path}"])
            sudo(["sh", "-c", f"echo 'lxc.cgroup2.cpu.max = 100000 100000' >> {config_path}"])

            self.logger(f"  Starting container {name}...")
            sudo(f"lxc-start -n {name}")

        # Wait for IPs
        self.logger("  Waiting for container IPs...")
        for name in self.CONTAINERS:
            ip = ""
            for _ in range(30):
                info = sudo(f"lxc-info -n {name} -i")
                if "IP:" in info:
                    ip = info.split("IP:")[1].strip().split("\n")[0].strip()
                    if ip:
                        break
                time.sleep(2)
            if not ip:
                raise Exception(f"Failed to get IP for container {name}")
            self.ips[name] = ip
            self.logger(f"  {name}: {ip}")

        s1_ip = self.ips[self.CONTAINERS[0]]
        s2_ip = self.ips[self.CONTAINERS[1]]
        ex_ip = self.ips[self.CONTAINERS[2]]

        # Install base packages on all containers
        for name in self.CONTAINERS:
            self.logger(f"  Installing packages on {name}...")
            sudo(["lxc-attach", "-n", name, "--", "sh", "-c",
                  "env PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin "
                  "apt-get update -qq && "
                  "env PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin "
                  "apt-get install -y -qq openssh-server ca-certificates curl iproute2 openssl jq"])

            # Setup SSH
            sudo(["lxc-attach", "-n", name, "--", "mkdir", "-p", "/root/.ssh"])
            pub_key_path = os.path.expanduser("~/.ssh/id_rsa.pub")
            with open(pub_key_path, 'r') as f:
                pub_key = f.read().strip()
            sudo(["lxc-attach", "-n", name, "--", "sh", "-c",
                  f"echo '{pub_key}' >> /root/.ssh/authorized_keys"])
            sudo(["lxc-attach", "-n", name, "--", "sh", "-c",
                  "env PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin "
                  "sed -i 's/#PermitRootLogin prohibit-password/PermitRootLogin yes/' /etc/ssh/sshd_config"])
            sudo(["lxc-attach", "-n", name, "--", "sh", "-c",
                  "echo 'root:root' | /usr/sbin/chpasswd"])
            sudo(["lxc-attach", "-n", name, "--", "sh", "-c",
                  "env PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin systemctl restart ssh"])

        # ====================
        # Install Madexchanger FIRST (so we have the IP for madmail's exchange_url)
        # ====================
        ex_name = self.CONTAINERS[2]
        self.logger(f"  Installing Madexchanger on {ex_name} ({ex_ip})...")
        with open(exchanger_bin, 'rb') as f:
            sudo(["lxc-attach", "-n", ex_name, "--", "sh", "-c",
                  "cat > /usr/local/bin/madexchanger && chmod +x /usr/local/bin/madexchanger"],
                 input_data=f.read())

        # Generate self-signed TLS cert for the exchanger
        self.logger("  Generating self-signed TLS certificate for exchanger...")
        sudo(["lxc-attach", "-n", ex_name, "--", "sh", "-c",
              f"openssl req -x509 -newkey rsa:2048 -keyout /etc/ssl/exchanger.key "
              f"-out /etc/ssl/exchanger.crt -days 365 -nodes "
              f"-subj '/CN={ex_ip}' -addext 'subjectAltName=IP:{ex_ip}'"])

        # Create exchanger config (port 443, TLS)
        exchanger_config = f"""\
listen: "0.0.0.0:{EXCHANGER_PORT}"
receive_path: "/mxdeliv"
forward_timeout: 30
skip_tls_verify: true
max_body_size: 33554432
log_level: "debug"
relay_mode: "all"
database_path: "/var/lib/madexchanger/madexchanger.db"
tls:
  cert_file: "/etc/ssl/exchanger.crt"
  key_file: "/etc/ssl/exchanger.key"
admin_web:
  enabled: true
  path: "/admin"
  token: "{ADMIN_TOKEN}"
"""
        sudo(["lxc-attach", "-n", ex_name, "--", "mkdir", "-p", "/etc/madexchanger"])
        sudo(["lxc-attach", "-n", ex_name, "--", "mkdir", "-p", "/var/lib/madexchanger"])
        sudo(["lxc-attach", "-n", ex_name, "--", "sh", "-c",
              f"cat > /etc/madexchanger/config.yml << 'EXCHCONF'\n{exchanger_config}EXCHCONF"])

        # Create systemd unit for madexchanger
        unit = """\
[Unit]
Description=Madexchanger - Email Relay Proxy
After=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/madexchanger -config /etc/madexchanger/config.yml
Restart=always
RestartSec=3
WorkingDirectory=/var/lib/madexchanger

[Install]
WantedBy=multi-user.target
"""
        sudo(["lxc-attach", "-n", ex_name, "--", "sh", "-c",
              f"cat > /etc/systemd/system/madexchanger.service << 'UNIT'\n{unit}UNIT"])
        sudo(["lxc-attach", "-n", ex_name, "--", "sh", "-c",
              "env PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin "
              "systemctl daemon-reload && systemctl enable madexchanger && systemctl start madexchanger"])

        # Wait for exchanger to be ready
        self.logger(f"  Waiting for exchanger port {EXCHANGER_PORT}...")
        if not wait_for_port(ex_ip, EXCHANGER_PORT, timeout=30):
            status = ssh(ex_ip, "journalctl -u madexchanger --no-pager -n 20")
            self.logger(f"  Exchanger logs:\n{status.stdout}")
            raise Exception(f"Madexchanger not reachable at {ex_ip}:{EXCHANGER_PORT}")
        self.logger(f"  ✓ Exchanger is running at {ex_ip}:{EXCHANGER_PORT} (HTTPS)")

        # ====================
        # Install Madmail on server containers with endpoint_rewrite
        # ====================
        for name in [self.CONTAINERS[0], self.CONTAINERS[1]]:
            ip = self.ips[name]
            self.logger(f"  Installing Madmail on {name} ({ip})...")
            sudo(["lxc-attach", "-n", name, "--", "mkdir", "-p", "/usr/lib/maddy"])
            with open(madmail_bin, 'rb') as f:
                sudo(["lxc-attach", "-n", name, "--", "sh", "-c",
                      "cat > /tmp/maddy && chmod +x /tmp/maddy"], input_data=f.read())
            sudo(["lxc-attach", "-n", name, "--", "sh", "-c",
                  f"env PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin "
                  f"/tmp/maddy install --simple --ip {ip} --non-interactive "
                  f"--ss-password testing --turn-secret testing --debug"])

            # Add endpoint_rewrite to maddy.conf
            # The directive goes inside the target.remote block
            self.logger(f"  Adding endpoint_rewrite to maddy.conf on {name}...")
            sudo(["lxc-attach", "-n", name, "--", "sh", "-c",
                  f"sed -i '/^target\\.remote /a\\    endpoint_rewrite {ex_ip}' /etc/maddy/maddy.conf"])

            # Verify it was inserted
            verify = sudo(["lxc-attach", "-n", name, "--", "grep", "endpoint_rewrite", "/etc/maddy/maddy.conf"], check=False)
            if isinstance(verify, bytes):
                verify = verify.decode()
            if "endpoint_rewrite" not in str(verify):
                raise Exception(f"Failed to inject endpoint_rewrite into maddy.conf on {name}")

            sudo(["lxc-attach", "-n", name, "--", "sh", "-c",
                  "env PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin systemctl restart maddy"])

        # Wait for madmail services
        for name in [self.CONTAINERS[0], self.CONTAINERS[1]]:
            ip = self.ips[name]
            self.logger(f"  Waiting for Madmail on {name} ({ip}:443)...")
            if not wait_for_port(ip, 443, timeout=60):
                status = ssh(ip, "journalctl -u maddy --no-pager -n 30")
                self.logger(f"  Madmail logs:\n{status.stdout}")
                raise Exception(f"Madmail not ready on {ip}:443")

        self.logger(f"\n  ✓ All services ready!")
        self.logger(f"    Madmail S1: {s1_ip} (endpoint_rewrite → {ex_ip})")
        self.logger(f"    Madmail S2: {s2_ip} (endpoint_rewrite → {ex_ip})")
        self.logger(f"    Exchanger: {ex_ip}:{EXCHANGER_PORT} (HTTPS, self-signed)")
        return s1_ip, s2_ip, ex_ip

    def get_exchanger_stats(self, ex_ip):
        """Query the exchanger's admin API for stats."""
        rpc_body = json.dumps({
            "method": "GET",
            "resource": "/admin/stats",
            "headers": {"Authorization": f"Bearer {ADMIN_TOKEN}"},
            "body": {}
        }).encode()

        req = urllib.request.Request(
            f"https://{ex_ip}:{EXCHANGER_PORT}/api/admin",
            data=rpc_body,
            headers={"Content-Type": "application/json"},
            method="POST"
        )
        import ssl
        ctx = ssl.create_default_context()
        ctx.check_hostname = False
        ctx.verify_mode = ssl.CERT_NONE
        try:
            with urllib.request.urlopen(req, timeout=10, context=ctx) as resp:
                return json.loads(resp.read())
        except Exception as e:
            print(f"    WARNING: Failed to get exchanger stats: {e}")
            return None

    def get_exchanger_routes(self, ex_ip):
        """Query the exchanger's admin API for route counters."""
        rpc_body = json.dumps({
            "method": "GET",
            "resource": "/admin/routes",
            "headers": {"Authorization": f"Bearer {ADMIN_TOKEN}"},
            "body": {}
        }).encode()

        req = urllib.request.Request(
            f"https://{ex_ip}:{EXCHANGER_PORT}/api/admin",
            data=rpc_body,
            headers={"Content-Type": "application/json"},
            method="POST"
        )
        import ssl
        ctx = ssl.create_default_context()
        ctx.check_hostname = False
        ctx.verify_mode = ssl.CERT_NONE
        try:
            with urllib.request.urlopen(req, timeout=10, context=ctx) as resp:
                return json.loads(resp.read())
        except Exception as e:
            print(f"    WARNING: Failed to get exchanger routes: {e}")
            return None

    def get_exchanger_logs(self, ex_ip, lines=50):
        """Get recent exchanger logs."""
        result = ssh(ex_ip, f"journalctl -u madexchanger --no-pager -n {lines}")
        return result.stdout

    def cleanup(self):
        """Destroy all containers."""
        self.logger("Cleaning up Exchanger E2E containers...")
        for name in self.CONTAINERS:
            sudo(f"lxc-stop -n {name} -k", check=False)
            sudo(f"lxc-destroy -n {name}", check=False)


# ---------------------------------------------------------------------------
# Message helpers
# ---------------------------------------------------------------------------

def wait_for_message(account, expected_text, label, max_wait=90):
    """Wait for a specific message to appear."""
    start = time.time()
    print(f"    Waiting for {label} to receive message...")
    while time.time() - start < max_wait:
        for chat in account.get_chatlist():
            for msg in chat.get_messages():
                if msg.get_snapshot().text == expected_text:
                    elapsed = time.time() - start
                    print(f"    ✓ Message received by {label} in {elapsed:.1f}s")
                    return True
        time.sleep(2)
    raise Exception(f"Message not received by {label} within {max_wait}s: {expected_text}")


# ---------------------------------------------------------------------------
# Main Test
# ---------------------------------------------------------------------------

def run(rpc, dc, remote1, remote2, test_dir, timestamp,
        madmail_bin=None, exchanger_bin=None, keep_lxc=False):
    """
    Run the Exchanger E2E test.

    Args:
        rpc: RPC instance (from main.py)
        dc: DeltaChat instance
        remote1: Not used (we create our own containers)
        remote2: Not used
        test_dir: Directory for test output
        timestamp: Test run timestamp
        madmail_bin: Path to madmail binary (auto-detected if None)
        exchanger_bin: Path to madexchanger binary (auto-detected if None)
        keep_lxc: If True, don't destroy containers after test
    """
    from scenarios import test_01_account_creation, test_03_secure_join

    project_root = os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(
        os.path.abspath(__file__)))))

    # Auto-detect binaries
    if not madmail_bin:
        candidates = [
            os.path.join(project_root, "build/maddy"),
            os.path.join(project_root, "maddy"),
        ]
        for c in candidates:
            if os.path.exists(c):
                madmail_bin = c
                break
        if not madmail_bin:
            raise Exception(f"Madmail binary not found. Tried: {candidates}")

    if not exchanger_bin:
        candidates = [
            os.path.join(project_root, "madexchanger/madexchanger"),
            os.path.join(project_root, "madexchanger/build/madexchanger"),
        ]
        for c in candidates:
            if os.path.exists(c):
                exchanger_bin = c
                break
        if not exchanger_bin:
            raise Exception(f"Madexchanger binary not found. Tried: {candidates}")

    print(f"  Madmail binary:      {madmail_bin}")
    print(f"  Madexchanger binary: {exchanger_bin}")

    lxc = ExchangerLXC()
    s1_ip = s2_ip = ex_ip = None

    try:
        # ============================
        # Step 1: Setup infrastructure
        # ============================
        print("\n── Step 1: Setting up infrastructure ──")
        s1_ip, s2_ip, ex_ip = lxc.setup(madmail_bin, exchanger_bin)

        # ============================
        # Step 2: Create accounts
        # ============================
        print("\n── Step 2: Creating accounts ──")
        acc1 = test_01_account_creation.run(dc, s1_ip)
        acc2 = test_01_account_creation.run(dc, s2_ip)
        acc1_email = acc1.get_config("addr")
        acc2_email = acc2.get_config("addr")
        print(f"    acc1: {acc1_email}")
        print(f"    acc2: {acc2_email}")

        # ============================
        # Step 3: Secure Join
        # ============================
        print("\n── Step 3: Secure Join (acc1 <-> acc2) ──")
        test_03_secure_join.run(rpc, acc1, acc2)
        print("    ✓ Secure join completed")

        # ============================
        # Step 4: Send message S1 → S2 (routed via exchanger)
        # ============================
        print("\n── Step 4: Send acc1 → acc2 (via exchanger) ──")
        msg_1_to_2 = f"Exchanger Test S1→S2 [{timestamp}]"
        contact_acc2 = acc1.get_contact_by_addr(acc2_email)
        if not contact_acc2:
            raise Exception(f"Contact {acc2_email} not found on acc1")
        chat_1_to_2 = contact_acc2.create_chat()
        chat_1_to_2.send_text(msg_1_to_2)
        print(f"    Sent: {msg_1_to_2}")
        wait_for_message(acc2, msg_1_to_2, "acc2")

        # ============================
        # Step 5: Send message S2 → S1 (routed via exchanger)
        # ============================
        print("\n── Step 5: Send acc2 → acc1 (via exchanger) ──")
        msg_2_to_1 = f"Exchanger Test S2→S1 [{timestamp}]"
        contact_acc1 = acc2.get_contact_by_addr(acc1_email)
        if not contact_acc1:
            raise Exception(f"Contact {acc1_email} not found on acc2")
        chat_2_to_1 = contact_acc1.create_chat()
        chat_2_to_1.send_text(msg_2_to_1)
        print(f"    Sent: {msg_2_to_1}")
        wait_for_message(acc1, msg_2_to_1, "acc1")

        # ============================
        # Step 6: Verify exchanger stats
        # ============================
        print("\n── Step 6: Verifying exchanger stats ──")
        time.sleep(2)

        stats_resp = lxc.get_exchanger_stats(ex_ip)
        if stats_resp:
            stats = stats_resp.get("body", {})
            print(f"    Total relayed:  {stats.get('total_relayed', 'N/A')}")
            print(f"    Total rejected: {stats.get('total_rejected', 'N/A')}")
            print(f"    Total errors:   {stats.get('total_errors', 'N/A')}")
            print(f"    Total bytes:    {stats.get('total_bytes', 'N/A')}")
            if stats.get('total_relayed', 0) > 0:
                print("    ✓ Exchanger has relayed messages!")
            else:
                print("    ⚠ No messages recorded in stats")

        routes_resp = lxc.get_exchanger_routes(ex_ip)
        if routes_resp:
            routes = routes_resp.get("body", [])
            if routes:
                print(f"    Routes ({len(routes)} total):")
                for route in routes:
                    print(f"      {route.get('from_server', '?')} → {route.get('to_server', '?')}: "
                          f"{route.get('count', 0)} messages")
            else:
                print("    No routes recorded yet")

        # ============================
        # Step 7: Show exchanger logs
        # ============================
        print("\n── Step 7: Exchanger logs (last 30 lines) ──")
        logs = lxc.get_exchanger_logs(ex_ip, lines=30)
        for line in logs.split("\n")[-30:]:
            if line.strip():
                print(f"    {line}")

        # ============================
        # Summary
        # ============================
        print("\n" + "=" * 60)
        print("✓ Exchanger E2E Test PASSED!")
        print(f"  Messages routed through exchanger ({ex_ip}:{EXCHANGER_PORT}):")
        print(f"    acc1 ({s1_ip}) → acc2 ({s2_ip}): ✓ delivered")
        print(f"    acc2 ({s2_ip}) → acc1 ({s1_ip}): ✓ delivered")
        print("=" * 60)

    finally:
        # Collect logs before cleanup
        if ex_ip:
            try:
                with open(os.path.join(test_dir, "exchanger_debug.log"), "w") as f:
                    f.write(lxc.get_exchanger_logs(ex_ip, lines=200))
            except Exception:
                pass

        for i, name in enumerate(ExchangerLXC.CONTAINERS[:2]):
            ip = lxc.ips.get(name)
            if ip:
                try:
                    result = ssh(ip, "journalctl -u maddy --no-pager -n 200")
                    with open(os.path.join(test_dir, f"madmail_s{i+1}_debug.log"), "w") as f:
                        f.write(result.stdout)
                except Exception:
                    pass

        if keep_lxc:
            print(f"\n  Keeping containers alive:")
            print(f"    S1: {s1_ip}")
            print(f"    S2: {s2_ip}")
            print(f"    Exchanger: {ex_ip}:{EXCHANGER_PORT}")
        else:
            lxc.cleanup()
