import subprocess
import time
import os
import sys

class LXCManager:
    def __init__(self, logger=None, cpu_limit=None, mem_limit=None):
        self.containers = ["madmail-server1", "madmail-server2"]
        self.ips = {}
        self.logger = logger or print
        self.cpu_limit = cpu_limit
        self.mem_limit = mem_limit

    def _run(self, cmd, check=True):
        if isinstance(cmd, str):
            cmd = cmd.split()
        self.logger(f"Running: {' '.join(cmd)}")
        result = subprocess.run(cmd, capture_output=True, text=True)
        if check and result.returncode != 0:
            self.logger(f"STDOUT: {result.stdout}")
            self.logger(f"STDERR: {result.stderr}")
            raise Exception(f"Command failed with code {result.returncode}: {' '.join(cmd)}")
        return result.stdout.strip()

    def _sudo(self, cmd, check=True):
        path_env = "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"
        if isinstance(cmd, str):
            cmd = cmd.split()
        return self._run(["sudo", "env", path_env] + cmd, check=check)

    def setup(self):
        self.logger("Setting up LXC environment...")
        
        # Check if madmail binary exists
        madmail_bin = os.path.abspath("build/maddy")
        if not os.path.exists(madmail_bin):
            madmail_bin = os.path.abspath("maddy")
            
        if not os.path.exists(madmail_bin):
            self.logger(f"Error: madmail binary not found at {madmail_bin}. Please run 'make build' first.")
            sys.exit(1)

        for name in self.containers:
            if name in self._sudo("lxc-ls").split():
                self.logger(f"Container {name} already exists. Destroying...")
                self._sudo(f"lxc-stop -n {name} -k", check=False)
                self._sudo(f"lxc-destroy -n {name}")

            self.logger(f"Creating container {name} (Debian 12)...")
            # Using download template for Debian Bookworm (12)
            self._sudo(f"lxc-create -n {name} -t download -- -d debian -r bookworm -a amd64")

            # Apply resource limits to config
            config_path = f"/var/lib/lxc/{name}/config"
            if self.mem_limit:
                self.logger(f"Setting memory limit to {self.mem_limit} for {name}...")
                self._sudo(["sh", "-c", f"echo 'lxc.cgroup2.memory.max = {self.mem_limit}' | tee -a {config_path}"])
            if self.cpu_limit:
                self.logger(f"Setting CPU limit to {self.cpu_limit} cores for {name}...")
                # We'll use cpuset.cpus to pin to a specific core, e.g. core 0
                self._sudo(["sh", "-c", f"echo 'lxc.cgroup2.cpuset.cpus = 0' | tee -a {config_path}"])

            self.logger(f"Starting container {name}...")
            self._sudo(f"lxc-start -n {name}")

        self.logger("Waiting for containers to get IPs...")
        for name in self.containers:
            ip = ""
            for _ in range(30):
                info = self._sudo(f"lxc-info -n {name} -i")
                if "IP:" in info:
                    ip = info.split("IP:")[1].strip()
                    if ip:
                        break
                time.sleep(2)
            if not ip:
                raise Exception(f"Failed to get IP for container {name}")
            self.ips[name] = ip
            self.logger(f"Container {name} IP: {ip}")

        for name in self.containers:
            self.logger(f"Configuring container {name}...")
            # Install dependencies
            self._sudo(["lxc-attach", "-n", name, "--", "sh", "-c", "env PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin apt-get update"])
            self._sudo(["lxc-attach", "-n", name, "--", "sh", "-c", "env PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin apt-get install -y openssh-server ca-certificates curl iproute2 jq"])
            
            # Create helper lib directory required by systemd unit sandboxing
            self._sudo(["lxc-attach", "-n", name, "--", "mkdir", "-p", "/usr/lib/maddy"])

            # Setup root SSH access
            self._sudo(["lxc-attach", "-n", name, "--", "mkdir", "-p", "/root/.ssh"])
            pub_key_path = os.path.expanduser("~/.ssh/id_rsa.pub")
            if not os.path.exists(pub_key_path):
                self.logger("Generating SSH key for the host...")
                subprocess.run(["ssh-keygen", "-t", "rsa", "-N", "", "-f", os.path.expanduser("~/.ssh/id_rsa")], check=True)
            
            with open(pub_key_path, 'r') as f:
                pub_key = f.read()
            self._sudo(["lxc-attach", "-n", name, "--", "sh", "-c", f"echo '{pub_key.strip()}' >> /root/.ssh/authorized_keys"])
            
            self._sudo(["lxc-attach", "-n", name, "--", "sh", "-c", "env PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin sed -i 's/#PermitRootLogin prohibit-password/PermitRootLogin yes/' /etc/ssh/sshd_config"])
            self._sudo(["lxc-attach", "-n", name, "--", "sh", "-c", "echo 'root:root' | /usr/sbin/chpasswd"])
            self._sudo(["lxc-attach", "-n", name, "--", "sh", "-c", "env PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin systemctl restart ssh"])

            # Push madmail binary
            self.logger(f"Pushing madmail binary to {name}...")
            ip = self.ips[name]
            subprocess.run(f"ssh-keyscan {ip} >> ~/.ssh/known_hosts", shell=True, capture_output=True)
            
            # Copy to /tmp/maddy first to avoid "text file busy" if we run from destination
            with open(madmail_bin, 'rb') as f:
                subprocess.run(["sudo", "lxc-attach", "-n", name, "--", "sh", "-c", "cat > /tmp/maddy && chmod +x /tmp/maddy"], input=f.read())

            self.logger(f"Installing madmail on {name}...")
            # Run install from /tmp/maddy to target /usr/local/bin/maddy
            # Provide dummy secrets for SS and TURN since they are required in config but not auto-generated in non-interactive mode
            self._sudo(["lxc-attach", "-n", name, "--", "sh", "-c", f"env PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin /tmp/maddy install --simple --ip {ip} --non-interactive --ss-password testing --turn-secret testing --debug"])
            self._sudo(["lxc-attach", "-n", name, "--", "sh", "-c", "env PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin systemctl start maddy"])

        self.logger("LXC environment ready.")
        return self.ips[self.containers[0]], self.ips[self.containers[1]]

    def cleanup(self):
        self.logger("Cleaning up LXC environment...")
        for name in self.containers:
            self.logger(f"Destroying container {name}...")
            self._sudo(f"lxc-stop -n {name} -k", check=False)
            self._sudo(f"lxc-destroy -n {name}", check=False)
