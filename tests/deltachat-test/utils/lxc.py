import subprocess
import time
import os
import sys

class LXCManager:
    def __init__(self, memory_limit="1G", cpu_limit="1", logger=None):
        self.containers = ["madmail-server1", "madmail-server2"]
        self.ips = {}
        self.logger = logger or print
        self.memory_limit = memory_limit
        self.cpu_limit = cpu_limit

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

    def _sudo(self, cmd, check=True, input_data=None, quiet=False):
        path_env = "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"
        if isinstance(cmd, str):
            cmd = cmd.split()
        return self._run(["sudo", "env", path_env] + cmd, check=check, input_data=input_data, quiet=quiet)

    def setup(self, containers=None, binary_path=None, reuse_existing=False):
        if containers:
            self.containers = containers
            
        self.logger("Setting up LXC environment...")
        
        # Check if madmail binary exists
        madmail_bin = binary_path or os.path.abspath("build/maddy")
        if not os.path.exists(madmail_bin):
            madmail_bin = os.path.abspath("maddy")
            
        if not os.path.exists(madmail_bin):
            self.logger(f"Error: madmail binary not found at {madmail_bin}. Please check the path.")
            sys.exit(1)

        existing_containers = self._sudo("lxc-ls").split()
        for name in self.containers:
            if name in existing_containers:
                if reuse_existing:
                    self.logger(f"Container {name} already exists. Reusing it (reuse_existing=True).")
                    # Ensure it's started
                    info = self._sudo(f"lxc-info -n {name}")
                    if "STOPPED" in info:
                        self.logger(f"Starting stopped container {name}...")
                        self._sudo(f"lxc-start -n {name}")
                    continue
                else:
                    self.logger(f"Container {name} already exists. Destroying...")
                    self._sudo(f"lxc-stop -n {name} -k", check=False)
                    self._sudo(f"lxc-destroy -n {name}")

            self.logger(f"Creating container {name} (Debian 12)...")
            # Using download template for Debian Bookworm (12)
            self._sudo(f"lxc-create -n {name} -t download -- -d debian -r bookworm -a amd64")

            # Apply resource limits
            # Check for cgroup v2 (standard on Debian 12)
            config_path = f"/var/lib/lxc/{name}/config"
            self._sudo(["sh", "-c", f"echo 'lxc.cgroup2.memory.max = {self.memory_limit}' >> {config_path}"])
            # Mapping cpu limit to cpuset.cpus is tricky if we don't know which cores are free.
            # However, we can use cpu.max for CFS quota. 1 core = 100000 100000
            # For simplicity, if cpu_limit is an integer, we'll try to use cpu.max
            try:
                cpu_quota = int(self.cpu_limit) * 100000
                self._sudo(["sh", "-c", f"echo 'lxc.cgroup2.cpu.max = {cpu_quota} 100000' >> {config_path}"])
            except ValueError:
                pass

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
            # If we are reusing, we might want to skip installation if maddy is already there
            # But the binary might have changed, so we usually want to re-push and restart.
            # For "reuse same ip each time", just re-pushing maddy is fine.
            
            ip = self.ips[name]
            
            # Check if maddy is already running with the correct IP
            # For simplicity, we'll re-run install but it's faster if we skip apt-get
            if reuse_existing:
                deps_installed = self._sudo(["lxc-attach", "-n", name, "--", "which", "sshd"], check=False)
                if not deps_installed:
                    self.logger(f"Dependencies not found in reused container {name}. Installing...")
                else:
                    self.logger(f"Dependencies already present in {name}. Skipping apt-get.")
                    # Still push the binary and restart maddy to be sure
                    self._push_and_start(name, madmail_bin, ip)
                    continue

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

            self._push_and_start(name, madmail_bin, ip)

        self.logger("LXC environment ready.")
        return [self.ips[name] for name in self.containers]

    def _push_and_start(self, name, madmail_bin, ip):
        # Push madmail binary
        self.logger(f"Pushing madmail binary to {name}...")
        subprocess.run(f"ssh-keyscan {ip} >> ~/.ssh/known_hosts", shell=True, capture_output=True)
        
        # Copy to /tmp/maddy first to avoid "text file busy" if we run from destination
        with open(madmail_bin, 'rb') as f:
            self._sudo(["lxc-attach", "-n", name, "--", "sh", "-c", "cat > /tmp/maddy && chmod +x /tmp/maddy"], input_data=f.read())

        self.logger(f"Installing madmail on {name}...")
        # Run install from /tmp/maddy to target /usr/local/bin/maddy
        self._sudo(["lxc-attach", "-n", name, "--", "sh", "-c", f"env PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin /tmp/maddy install --simple --ip {ip} --non-interactive --ss-password testing --turn-secret testing --debug --enable-iroh"])
        self._sudo(["lxc-attach", "-n", name, "--", "sh", "-c", "env PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin systemctl restart maddy"])

        self.logger("LXC environment ready.")
        return [self.ips[name] for name in self.containers]

    def get_stats(self, name):
        """Get CPU and Memory usage for a container (quiet mode for monitoring)."""
        try:
            # Use quiet mode to avoid spamming logs
            info = self._sudo(f"lxc-info -n {name}", quiet=True)
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
        for name in self.containers:
            self.logger(f"Destroying container {name}...")
            self._sudo(f"lxc-stop -n {name} -k", check=False)
            self._sudo(f"lxc-destroy -n {name}", check=False)

