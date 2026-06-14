"""
Test #21: Madexchanger PHP End-to-End Test

Verifies that the PHP implementation of madexchanger correctly relays emails.
Matches the scenario of test_20 but uses the PHP version on Apache.
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
# For PHP version on standard Apache, we use port 80 (HTTP)
# Madmail's target.remote will try HTTPS first, then fallback to HTTP.
EXCHANGER_PORT = 80

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def ssh(remote, cmd, timeout=30):
    full = f"ssh {SSH_OPTS} root@{remote} '{cmd}'"
    result = subprocess.run(full, shell=True, capture_output=True, text=True, timeout=timeout)
    return result

def sudo(cmd, check=True, input_data=None):
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

class ExchangerPHPLXC:
    CONTAINERS = ["mad-php-s1", "mad-php-s2", "mad-php-ex"]

    def __init__(self, logger=None):
        self.logger = logger or print
        self.ips = {}

    def setup(self, madmail_bin, php_dir):
        self.logger("Setting up PHP Exchanger E2E LXC environment...")

        existing = sudo("lxc-ls").split()
        for name in self.CONTAINERS:
            if name in existing:
                sudo(f"lxc-stop -n {name} -k", check=False)
                sudo(f"lxc-destroy -n {name}")
            sudo(f"lxc-create -n {name} -t download -- -d debian -r bookworm -a amd64")
            sudo(f"lxc-start -n {name}")

        self.logger("  Waiting for IPs...")
        for name in self.CONTAINERS:
            ip = ""
            for _ in range(30):
                info = sudo(f"lxc-info -n {name} -i")
                if "IP:" in info:
                    ip = info.split("IP:")[1].strip().split("\n")[0].strip()
                    if ip: break
                time.sleep(2)
            if not ip: raise Exception(f"Failed to get IP for {name}")
            self.ips[name] = ip
            self.logger(f"  {name}: {ip}")

        ex_ip = self.ips[self.CONTAINERS[2]]

        # Base install
        for name in self.CONTAINERS:
            self.logger(f"  Installing base packages on {name}...")
            sudo(["lxc-attach", "-n", name, "--", "sh", "-c",
                  "apt-get update -qq && apt-get install -y -qq openssh-server ca-certificates curl iproute2 openssl jq"])
            
            # Setup SSH
            sudo(["lxc-attach", "-n", name, "--", "mkdir", "-p", "/root/.ssh"])
            pub_key = open(os.path.expanduser("~/.ssh/id_rsa.pub")).read().strip()
            sudo(["lxc-attach", "-n", name, "--", "sh", "-c", f"echo '{pub_key}' >> /root/.ssh/authorized_keys"])
            sudo(["lxc-attach", "-n", name, "--", "sh", "-c", "sed -i 's/#PermitRootLogin prohibit-password/PermitRootLogin yes/' /etc/ssh/sshd_config"])
            sudo(["lxc-attach", "-n", name, "--", "sh", "-c", "systemctl restart ssh"])

        # Install PHP+Apache on exchanger
        ex_name = self.CONTAINERS[2]
        self.logger(f"  Installing Apache+PHP on {ex_name}...")
        sudo(["lxc-attach", "-n", ex_name, "--", "apt-get", "install", "-y", "-qq", "apache2", "libapache2-mod-php", "php-curl"])
        sudo(["lxc-attach", "-n", ex_name, "--", "a2enmod", "rewrite"])

        # Upload PHP files
        self.logger(f"  Uploading PHP files to {ex_name}...")
        import tarfile
        import io
        tar_stream = io.BytesIO()
        with tarfile.open(fileobj=tar_stream, mode='w') as tar:
            for f in os.listdir(php_dir):
                if f.endswith(('.php', '.css', '.htaccess')):
                    tar.add(os.path.join(php_dir, f), arcname=f)
        
        sudo(["lxc-attach", "-n", ex_name, "--", "tar", "x", "-C", "/var/www/html"], input_data=tar_stream.getvalue())
        sudo(["lxc-attach", "-n", ex_name, "--", "chown", "-R", "www-data:www-data", "/var/www/html"])
        
        # Configure Apache to allow .htaccess
        apache_conf = """
<Directory /var/www/html>
    Options Indexes FollowSymLinks
    AllowOverride All
    Require all granted
</Directory>
"""
        sudo(["lxc-attach", "-n", ex_name, "--", "sh", "-c", f"echo '{apache_conf}' > /etc/apache2/conf-available/madmail.conf"])
        sudo(["lxc-attach", "-n", ex_name, "--", "a2enconf", "madmail"])
        sudo(["lxc-attach", "-n", ex_name, "--", "systemctl", "restart", "apache2"])

        # Install Madmail on s1 and s2
        for name in [self.CONTAINERS[0], self.CONTAINERS[1]]:
            ip = self.ips[name]
            self.logger(f"  Installing Madmail on {name}...")
            with open(madmail_bin, 'rb') as f:
                sudo(["lxc-attach", "-n", name, "--", "sh", "-c", "cat > /tmp/maddy && chmod +x /tmp/maddy"], input_data=f.read())
            sudo(["lxc-attach", "-n", name, "--", "sh", "-c", f"/tmp/maddy install --simple --ip {ip} --non-interactive --ss-password testing --turn-secret testing"])
            
            # Use PHP exchanger as endpoint override
            # We use HTTP (port 80) for the PHP version for simplicity in test
            sudo(["lxc-attach", "-n", name, "--", "sh", "-c", f"sed -i '/^target\\.remote /a\\    endpoint_rewrite http://{ex_ip}' /etc/maddy/maddy.conf"])
            sudo(["lxc-attach", "-n", name, "--", "systemctl", "restart", "maddy"])

        self.logger("✓ Infrastructure ready!")
        return self.ips[self.CONTAINERS[0]], self.ips[self.CONTAINERS[1]], ex_ip

    def cleanup(self):
        for name in self.CONTAINERS:
            sudo(f"lxc-stop -n {name} -k", check=False)
            sudo(f"lxc-destroy -n {name}", check=False)

def run(rpc, dc, remote1, remote2, test_dir, timestamp, madmail_bin=None, php_dir=None, keep_lxc=False):
    from scenarios import test_01_account_creation, test_03_secure_join
    from scenarios.test_20_exchanger import wait_for_message

    project_root = os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
    if not madmail_bin:
        madmail_bin = os.path.join(project_root, "build/maddy")
    if not php_dir:
        php_dir = os.path.join(project_root, "madexchanger-php")

    lxc = ExchangerPHPLXC()
    try:
        s1_ip, s2_ip, ex_ip = lxc.setup(madmail_bin, php_dir)

        print("\n── Step 2: Creating accounts ──")
        acc1 = test_01_account_creation.run(dc, s1_ip)
        acc2 = test_01_account_creation.run(dc, s2_ip)
        
        print("\n── Step 3: Secure Join ──")
        test_03_secure_join.run(rpc, acc1, acc2)

        print("\n── Step 4: Send S1 → S2 via PHP Exchanger ──")
        msg = f"PHP Exchanger Relay Test [{timestamp}]"
        acc1.get_contact_by_addr(acc2.get_config("addr")).create_chat().send_text(msg)
        wait_for_message(acc2, msg, "acc2")

        print("\n── Step 5: Verify PHP Exchanger log ──")
        log_res = ssh(ex_ip, "cat /var/www/html/madexchanger.log")
        print(f"    Logs:\n{log_res.stdout}")
        if "relayed to" in log_res.stdout:
            print("    ✓ Found relay entry in PHP logs")
        else:
            print("    ⚠ Relay entry not found in PHP logs (check Apache logs)")

        print("=" * 60)
        print("✓ PHP Exchanger E2E Test PASSED!")
        print("=" * 60)

    finally:
        if not keep_lxc:
            lxc.cleanup()
