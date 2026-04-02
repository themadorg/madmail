# Running LXC Tests Without Root

The madmail E2E test suite uses LXC containers to spin up isolated mail
servers for federation testing. Previously every `lxc-*` command ran under
`sudo`, which meant the entire test had to be executed as root.

This document describes the migration to **unprivileged LXC containers** so
tests run as a normal user.

## TL;DR — Quick Start

```bash
# One-time setup (requires sudo, takes ~30 s)
sudo bash tests/deltachat-test/setup_lxc_unprivileged.sh

# Run tests — no sudo needed
uv run python3 tests/deltachat-test/main.py --lxc --test-7
```

---

## How It Works

### Two-Tier Privilege Model

| Tier | What | Needs root? |
|------|------|-------------|
| **LXC operations** | `lxc-create`, `lxc-start`, `lxc-attach`, `lxc-stop`, `lxc-destroy`, `lxc-ls`, `lxc-info` | **No** — unprivileged containers run in a user namespace |
| **Host networking** | `iptables` (NAT), `dnsmasq` (DNS), `/etc/hosts` entries | **Yes** — but made passwordless via a sudoers.d snippet |

After the one-time setup, the test script calls `lxc-*` commands directly
(no sudo) and only uses `sudo` for the handful of host-networking operations —
which are pre-authorized to run without a password prompt.

### What Changed in the Code

**`tests/deltachat-test/utils/lxc.py`**

- The `_sudo()` method was renamed to `_exec()` and no longer prepends `sudo`.
  All LXC commands now execute under the calling user's identity.
- The container config path changed from `/var/lib/lxc/<name>/config`
  (root-owned, privileged) to `~/.local/share/lxc/<name>/config` (user-owned,
  unprivileged).
- Host-level `sudo` calls (iptables, dnsmasq, sed on `/etc/hosts`) remain
  unchanged — they use direct `subprocess.run(["sudo", ...])` and work
  without a password thanks to the sudoers snippet.

---

## One-Time Setup Details

The setup script (`tests/deltachat-test/setup_lxc_unprivileged.sh`) configures
six things:

### 1. Packages

Installs `lxc`, `lxc-templates`, `uidmap`, `dnsmasq-base`, `iptables`, and
`conntrack`.

### 2. Subordinate UID/GID Ranges

Adds entries to `/etc/subuid` and `/etc/subgid` (e.g. `user:100000:65536`).
These ranges let the kernel allocate a block of host UIDs/GIDs for the
container's user namespace. Container root (UID 0) maps to host UID 100000,
container UID 1 maps to 100001, and so on — none of them are real root.

### 3. User LXC Config

Creates `~/.config/lxc/default.conf` with:
- Network: `veth` pair attached to the `lxcbr0` bridge
- UID/GID mapping matching the allocated subuid/subgid ranges
- AppArmor set to `unconfined` (required for unprivileged container startup)

### 4. Network Permissions (`lxc-usernet`)

Adds the user to `/etc/lxc/lxc-usernet` so they can create up to 10 veth
interfaces on the `lxcbr0` bridge without root.

### 5. `lxc-net` Service

Enables and starts the `lxc-net` service which manages:
- The `lxcbr0` bridge interface
- A DHCP server (dnsmasq) that assigns IPs to containers

### 6. Sudoers Snippet

Installs `/etc/sudoers.d/madmail-lxc-test` which allows the user to run
these specific commands without a password:

| Command | Purpose |
|---------|---------|
| `iptables` | NAT masquerade for container internet access |
| `fuser -k 53/udp` | Kill any process (dnsmasq) using port 53 |
| `fuser -k 53/tcp` | Kill any process (dnsmasq) using port 53 |
| `ip addr add/del` | Manage bridge IPs for test DNS |
| `tee -a /etc/hosts` | Add test domain entries on the host |
| `sed -i ... /etc/hosts` | Remove test entries on cleanup |
| `cat /tmp/madmail-test-dnsmasq.pid` | Read dnsmasq PID for cleanup |

---

## Container Storage

Unprivileged containers live under the user's home directory:

```
~/.local/share/lxc/
├── madmail-server1/
│   ├── config
│   └── rootfs/
└── madmail-server2/
    ├── config
    └── rootfs/
```

From the host's perspective, all files inside `rootfs/` are owned by the
mapped UID range (100000+), not by your user or root.

---

## Undoing the Setup

```bash
# Remove the sudoers snippet
sudo rm /etc/sudoers.d/madmail-lxc-test

# Remove user LXC config
rm -rf ~/.config/lxc

# Remove containers
rm -rf ~/.local/share/lxc

# Remove subuid/subgid entries (optional)
sudo sed -i "/^$(whoami):/d" /etc/subuid /etc/subgid

# Remove lxc-usernet entry (optional)
sudo sed -i "/^$(whoami) /d" /etc/lxc/lxc-usernet
```

---

## Troubleshooting

### `lxc-create` fails with "No uid mapping for container root"

The subuid/subgid ranges are not configured. Re-run the setup script:
```bash
sudo bash tests/deltachat-test/setup_lxc_unprivileged.sh
```

### Container has no IP after 60 seconds

The `lxc-net` service may not be running:
```bash
sudo systemctl restart lxc-net
```

The `lxc-net` service (managed by systemd) often starts its own `dnsmasq` process
bound to `10.0.3.1:53`. This will prevent our test `dnsmasq` from starting.
The test script now calls `sudo fuser -k 53/udp` to clear this port before
starting its own instance.

### `lxc-start` fails with "Permission denied - Could not access /home/user"
The container root (mapped to UID 100000) needs execute permission on all
parent directories to reach its rootfs. The setup script now adds an ACL
(`setfacl -m u:100000:x ~`) to allow bridge traversal without compromising
home directory security.

### Arch Linux: `iptables-nft` conflict
On Arch, the standard `iptables` package conflicts with `iptables-nft`. The
setup script automatically detects Arch and installs `iptables-nft` to match.
