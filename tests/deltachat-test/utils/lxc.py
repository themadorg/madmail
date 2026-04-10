import subprocess
import time
import os
import sys

# Test domain names for LXC federation testing
DOMAIN1 = "s1.test"
DOMAIN2 = "s2.test"

class LXCManager:
    def __init__(self, memory_limit="1G", cpu_limit="1", logger=None):
        self.containers = ["madmail-server1", "madmail-server2"]
        self.ips = {}
        self.domains = {}  # container name -> domain (or None for IP-only)
        self.logger = logger or print
        self.memory_limit = memory_limit
        self.cpu_limit = cpu_limit
        self._dnsmasq_pid = None  # PID of the test dnsmasq process

    def _run(self, cmd, check=True, input_data=None, quiet=False):
        if isinstance(cmd, str):
            cmd = cmd.split()
        
        if not quiet:
            self.logger(f"Running: {' '.join(cmd)}")
        if input_data:
            result = subprocess.run(cmd, input=input_data, capture_output=True)
        else:
            result = subprocess.run(cmd, capture_output=True, text=True)
            
        if check and result.returncode != 0:
            stdout = result.stdout.decode() if isinstance(result.stdout, bytes) else result.stdout
            stderr = result.stderr.decode() if isinstance(result.stderr, bytes) else result.stderr
            self.logger(f"STDOUT: {stdout}")
            self.logger(f"STDERR: {stderr}")
            raise Exception(f"Command failed with code {result.returncode}: {' '.join(cmd)}")
            
        return result.stdout.strip() if not input_data else result.stdout

    def _exec(self, cmd, check=True, input_data=None, quiet=False):
        """Run a command with proper PATH set (unprivileged LXC — no sudo)."""
        path_env = "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"
        if isinstance(cmd, str):
            cmd = cmd.split()
        return self._run(["env", path_env] + cmd, check=check, input_data=input_data, quiet=quiet)

    def _ensure_host_nat(self):
        """Ensure the host has NAT masquerade rules for the LXC bridge subnet."""
        # Check if rule already exists
        check = subprocess.run(
            ["sudo", "iptables", "-t", "nat", "-C", "POSTROUTING",
             "-s", "10.0.3.0/24", "!", "-d", "10.0.3.0/24", "-j", "MASQUERADE"],
            capture_output=True
        )
        if check.returncode != 0:
            self.logger("Adding missing NAT masquerade rule for LXC subnet 10.0.3.0/24...")
            subprocess.run(
                ["sudo", "iptables", "-t", "nat", "-A", "POSTROUTING",
                 "-s", "10.0.3.0/24", "!", "-d", "10.0.3.0/24", "-j", "MASQUERADE"],
                check=True
            )
            # Also ensure FORWARD is open for lxcbr0
            subprocess.run(["sudo", "iptables", "-I", "FORWARD", "-i", "lxcbr0", "-j", "ACCEPT"],
                           capture_output=True)
            subprocess.run(["sudo", "iptables", "-I", "FORWARD", "-o", "lxcbr0", "-j", "ACCEPT"],
                           capture_output=True)
        else:
            self.logger("NAT masquerade rule for LXC subnet already present.")

    def _setup_dns(self):
        """
        Start a lightweight dnsmasq on the LXC bridge (10.0.3.1) that resolves
        test domains to container IPs and forwards everything else upstream.

        Containers will use 10.0.3.1 as their DNS server.
        """
        # Build address entries for each container that has a domain
        addr_args = []
        for name, domain in self.domains.items():
            if domain:
                ip = self.ips[name]
                addr_args.extend(["--address", f"/{domain}/{ip}"])
                self.logger(f"DNS: {domain} → {ip}")

        if not addr_args:
            self.logger("No domain-based servers; skipping dnsmasq setup.")
            return

        # Kill any existing test dnsmasq (not the lxc-net one)
        self._stop_dns()

        # Try to start our own dnsmasq on 10.0.3.1
        cmd = [
            "sudo", "dnsmasq",
            "--no-daemon",          # stay in foreground (we'll background it)
            "--listen-address=10.0.3.1",
            "--bind-interfaces",
            "--no-resolv",
            "--server=8.8.8.8",     # upstream DNS
            "--server=1.1.1.1",
            "--log-queries",
            "--no-hosts",
            "--pid-file=/tmp/madmail-test-dnsmasq.pid",
        ] + addr_args

        self.logger(f"Starting test dnsmasq on 10.0.3.1...")
        proc = subprocess.Popen(
            cmd,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        self._dnsmasq_pid = proc.pid
        time.sleep(1)  # let it bind

        # Verify it's running
        if proc.poll() is not None:
            # dnsmasq failed — likely because lxc-net's dnsmasq is already on port 53.
            # Fall back: inject domain records into lxc-net's hosts file and SIGHUP it.
            self.logger(f"  dnsmasq couldn't bind (lxc-net owns port 53). Using /etc/hosts fallback.")
            self._dnsmasq_pid = None
            hosts_lines = []
            for name, domain in self.domains.items():
                if domain:
                    ip = self.ips[name]
                    hosts_lines.append(f"{ip} {domain}  # madmail-test\n")
            if hosts_lines:
                subprocess.run(
                    ["sudo", "tee", "-a", "/etc/hosts"],
                    input="".join(hosts_lines).encode(),
                    capture_output=True, check=True,
                )
                # Also write to a separate hosts file and wire it into lxc-net's dnsmasq
                subprocess.run(
                    ["sudo", "sh", "-c",
                     f"echo '{''.join(hosts_lines).strip()}' > /tmp/madmail-test-hosts"],
                    capture_output=True,
                )
                # SIGHUP lxc-net's dnsmasq to re-read /etc/hosts
                subprocess.run(
                    ["sudo", "killall", "-HUP", "dnsmasq"],
                    capture_output=True,
                )
                for line in hosts_lines:
                    self.logger(f"  Fallback hosts entry: {line.strip()}")
        else:
            self.logger(f"dnsmasq running (PID {self._dnsmasq_pid})")

        # Also add /etc/hosts entries on the HOST so the Delta Chat RPC client
        # (running on the host) can resolve test domains for IMAP/SMTP connections.
        self._host_entries = []
        for name, domain in self.domains.items():
            if domain:
                ip = self.ips[name]
                entry = f"{ip} {domain}"
                # Check if already added by fallback above
                check = subprocess.run(
                    ["grep", "-q", f"{domain}.*# madmail-test", "/etc/hosts"],
                    capture_output=True,
                )
                if check.returncode != 0:
                    subprocess.run(
                        ["sudo", "tee", "-a", "/etc/hosts"],
                        input=f"{entry}  # madmail-test\n".encode(),
                        capture_output=True, check=True,
                    )
                self._host_entries.append(domain)
                self.logger(f"Host /etc/hosts: {entry}")

    def _stop_dns(self):
        """Stop the test dnsmasq if running and clean up /etc/hosts."""
        # Only kill our test dnsmasq — NOT the lxc-net dnsmasq that provides
        # DHCP to containers. Killing that breaks the next test run.

        # Try PID file first
        try:
            pid_data = subprocess.run(
                ["sudo", "cat", "/tmp/madmail-test-dnsmasq.pid"],
                capture_output=True, text=True
            )
            if pid_data.returncode == 0 and pid_data.stdout.strip():
                subprocess.run(["sudo", "kill", pid_data.stdout.strip()], capture_output=True)
                subprocess.run(["sudo", "rm", "-f", "/tmp/madmail-test-dnsmasq.pid"], capture_output=True)
        except Exception:
            pass

        # Also kill by our stored PID
        if self._dnsmasq_pid:
            subprocess.run(["sudo", "kill", str(self._dnsmasq_pid)], capture_output=True)
            self._dnsmasq_pid = None

        # Remove test entries from /etc/hosts
        subprocess.run(
            ["sudo", "sed", "-i", "/# madmail-test$/d", "/etc/hosts"],
            capture_output=True,
        )

    def setup(self, containers=None, binary_path=None, reuse_existing=False):
        if containers:
            self.containers = containers

        self._ensure_host_nat()

        # Restart lxc-net to ensure the bridge DHCP server (dnsmasq) is running.
        # A previous test run's _stop_dns() may have killed it via fuser -k 53/udp.
        subprocess.run(["sudo", "systemctl", "restart", "lxc-net"], capture_output=True)
        time.sleep(1)

        self.logger("Setting up LXC environment...")
        
        # Check if madmail binary exists
        madmail_bin = binary_path or os.path.abspath("build/maddy")
        if not os.path.exists(madmail_bin):
            madmail_bin = os.path.abspath("maddy")
            
        if not os.path.exists(madmail_bin):
            self.logger(f"Error: madmail binary not found at {madmail_bin}. Please check the path.")
            sys.exit(1)

        existing_containers = self._exec("lxc-ls").split()
        for name in self.containers:
            if name in existing_containers:
                if reuse_existing:
                    self.logger(f"Container {name} already exists. Reusing it (reuse_existing=True).")
                    # Ensure it's started
                    info = self._exec(f"lxc-info -n {name}")
                    if "STOPPED" in info:
                        self.logger(f"Starting stopped container {name}...")
                        self._exec(f"lxc-start -n {name}")
                    continue
                else:
                    self.logger(f"Container {name} already exists. Destroying...")
                    self._exec(f"lxc-stop -n {name} -k", check=False)
                    self._exec(f"lxc-destroy -n {name}")

            self.logger(f"Creating container {name} (Debian 12)...")
            # Using download template for Debian Bookworm (12)
            self._exec(f"lxc-create -n {name} -t download -- -d debian -r bookworm -a amd64")

            # Apply resource limits
            # Check for cgroup v2 (standard on Debian 12)
            config_path = os.path.expanduser(f"~/.local/share/lxc/{name}/config")
            self._exec(["sh", "-c", f"echo 'lxc.cgroup2.memory.max = {self.memory_limit}' >> {config_path}"])
            # Mapping cpu limit to cpuset.cpus is tricky if we don't know which cores are free.
            # However, we can use cpu.max for CFS quota. 1 core = 100000 100000
            # For simplicity, if cpu_limit is an integer, we'll try to use cpu.max
            try:
                cpu_quota = int(self.cpu_limit) * 100000
                self._exec(["sh", "-c", f"echo 'lxc.cgroup2.cpu.max = {cpu_quota} 100000' >> {config_path}"])
            except ValueError:
                pass

            self.logger(f"Starting container {name}...")
            self._exec(f"lxc-start -n {name}")

        self.logger("Waiting for containers to get IPs...")
        for name in self.containers:
            ip = ""
            for _ in range(30):
                info = self._exec(f"lxc-info -n {name} -i")
                if "IP:" in info:
                    ip = info.split("IP:")[1].strip()
                    if ip:
                        break
                time.sleep(2)
            if not ip:
                raise Exception(f"Failed to get IP for container {name}")
            self.ips[name] = ip
            self.logger(f"Container {name} IP: {ip}")

        # Assign domains: server1 gets a domain, server2 stays IP-only
        self.domains[self.containers[0]] = DOMAIN1
        if len(self.containers) > 1:
            self.domains[self.containers[1]] = None  # IP-only

        # Set up DNS before configuring containers (so DNS is ready when maddy starts)
        self._setup_dns()

        for name in self.containers:
            # If we are reusing, we might want to skip installation if maddy is already there
            # But the binary might have changed, so we usually want to re-push and restart.
            # For "reuse same ip each time", just re-pushing maddy is fine.
            
            ip = self.ips[name]
            domain = self.domains.get(name)
            
            # Check if maddy is already running with the correct IP
            # For simplicity, we'll re-run install but it's faster if we skip apt-get
            if reuse_existing:
                deps_installed = self._exec(["lxc-attach", "-n", name, "--", "which", "sshd"], check=False)
                if not deps_installed:
                    self.logger(f"Dependencies not found in reused container {name}. Installing...")
                else:
                    self.logger(f"Dependencies already present in {name}. Skipping apt-get.")
                    # Still push the binary and restart maddy to be sure
                    self._push_and_start(name, madmail_bin, ip, domain=domain)
                    continue

            self.logger(f"Configuring container {name}...")
            # Fix DNS: point to our local dnsmasq for domain resolution
            dns_server = "10.0.3.1" if any(d for d in self.domains.values()) else "8.8.8.8"
            self._exec(["lxc-attach", "-n", name, "--", "sh", "-c",
                        f"echo 'nameserver {dns_server}' > /etc/resolv.conf; echo 'nameserver 8.8.8.8' >> /etc/resolv.conf"])
            # Install dependencies
            self._exec(["lxc-attach", "-n", name, "--", "sh", "-c", "env PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin apt-get update"])
            self._exec(["lxc-attach", "-n", name, "--", "sh", "-c", "env PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin apt-get install -y openssh-server ca-certificates curl iproute2 jq"])
            
            # Create helper lib directory required by systemd unit sandboxing
            self._exec(["lxc-attach", "-n", name, "--", "mkdir", "-p", "/usr/lib/maddy"])

            # Setup root SSH access
            self._exec(["lxc-attach", "-n", name, "--", "mkdir", "-p", "/root/.ssh"])
            pub_key_path = os.path.expanduser("~/.ssh/id_rsa.pub")
            if not os.path.exists(pub_key_path):
                self.logger("Generating SSH key for the host...")
                subprocess.run(["ssh-keygen", "-t", "rsa", "-N", "", "-f", os.path.expanduser("~/.ssh/id_rsa")], check=True)
            
            with open(pub_key_path, 'r') as f:
                pub_key = f.read()
            self._exec(["lxc-attach", "-n", name, "--", "sh", "-c", f"echo '{pub_key.strip()}' >> /root/.ssh/authorized_keys"])
            
            self._exec(["lxc-attach", "-n", name, "--", "sh", "-c", "env PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin sed -i 's/#PermitRootLogin prohibit-password/PermitRootLogin yes/' /etc/ssh/sshd_config"])
            self._exec(["lxc-attach", "-n", name, "--", "sh", "-c", "echo 'root:root' | /usr/sbin/chpasswd"])
            self._exec(["lxc-attach", "-n", name, "--", "sh", "-c", "env PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin systemctl restart ssh"])

            self._push_and_start(name, madmail_bin, ip, domain=domain)

        self.logger("LXC environment ready.")
        # Return both IPs and domain info
        return [self.ips[name] for name in self.containers]

    def get_server_info(self):
        """Return structured info about each server for use by federation tests.
        
        Returns a list of dicts:
            [
                {"ip": "10.0.3.X", "domain": "s1.test"},
                {"ip": "10.0.3.Y", "domain": None},
            ]
        """
        result = []
        for name in self.containers:
            result.append({
                "ip": self.ips.get(name),
                "domain": self.domains.get(name),
            })
        return result

    def _push_and_start(self, name, madmail_bin, ip, domain=None):
        # Push madmail binary
        self.logger(f"Pushing madmail binary to {name}...")
        subprocess.run(f"ssh-keyscan {ip} >> ~/.ssh/known_hosts", shell=True, capture_output=True)
        
        # Copy to /tmp/maddy first to avoid "text file busy" if we run from destination
        with open(madmail_bin, 'rb') as f:
            self._exec(["lxc-attach", "-n", name, "--", "sh", "-c", "cat > /tmp/maddy && chmod +x /tmp/maddy"], input_data=f.read())

        self.logger(f"Installing madmail on {name}...")
        # Build install command
        install_cmd = (
            f"env PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin "
            f"/tmp/maddy install --simple --ip {ip} --non-interactive "
            f"--ss-password testing --turn-secret testing --debug --enable-iroh"
        )
        if domain:
            install_cmd += f" --domain {domain}"
            self.logger(f"  Domain: {domain}, IP: {ip}")
        else:
            self.logger(f"  IP-only: {ip}")

        self._exec(["lxc-attach", "-n", name, "--", "sh", "-c", install_cmd])
        self._exec(["lxc-attach", "-n", name, "--", "sh", "-c", "env PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin systemctl restart maddy"])

        self.logger("LXC environment ready.")
        return [self.ips[name] for name in self.containers]

    def get_stats(self, name):
        """Get CPU and Memory usage for a container (quiet mode for monitoring)."""
        try:
            # Use quiet mode to avoid spamming logs
            info = self._exec(f"lxc-info -n {name}", quiet=True)
            mem = 0
            for line in info.split('\n'):
                if "Memory use:" in line:
                    mem_str = line.split("Memory use:")[1].strip()
                    if "MiB" in mem_str:
                        mem = float(mem_str.replace("MiB", "").strip())
                    elif "GiB" in mem_str:
                        mem = float(mem_str.replace("GiB", "").strip()) * 1024
                    elif "KiB" in mem_str:
                        mem = float(mem_str.replace("KiB", "").strip()) / 1024
                    else:
                        mem = float(mem_str.split()[0]) / (1024 * 1024)
            
            cpu_time = 0
            for line in info.split('\n'):
                if "CPU use:" in line:
                    cpu_time = float(line.split("CPU use:")[1].strip().replace("s", ""))
            
            return {"mem_mb": mem, "cpu_seconds": cpu_time}
        except:
            return {"mem_mb": 0, "cpu_seconds": 0}

    def cleanup(self):
        self.logger("Cleaning up LXC environment...")
        self._stop_dns()
        for name in self.containers:
            self.logger(f"Destroying container {name}...")
            self._exec(f"lxc-stop -n {name} -k", check=False)
            self._exec(f"lxc-destroy -n {name}", check=False)
