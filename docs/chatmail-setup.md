# Getting Started & Setup Guide

This guide explains how to get started with the Delta Chat ecosystem and set up your own Madmail server.

## 📱 Step 1: Get Delta Chat
Delta Chat is the messaging application that connects to this server. It works like email but feels like a modern chat app.
- [**Delta Chat Official Website**](https://delta.chat)
- [**Download Apps** (Android, iOS, Desktop)](https://delta.chat/en/download)

---

## 🚀 Step 2: Fast Server Deployment (IP-Based)

Madmail is designed for rapid deployment. Many servers currently operating in Iran use direct IP addresses to bypass DNS-related delays.

### A. Automated Installation (wget)
Run the following command on a clean **Debian** or **Ubuntu** server to install Madmail rapidly using your public IP:

```bash
wget http://[SOURCE_SERVER_IP]/madmail && chmod +x madmail && sudo ./madmail install --simple --ip [YOUR_PUBLIC_IP] && sudo systemctl start maddy
```

### B. Installation via SCP (Local Binary)
If you have downloaded the `madmail` binary locally (e.g., from [Telegram](https://t.me/the_madmail)), you can upload and install it via `scp`:

1.  **Upload the binary**:
    ```bash
    scp madmail root@[YOUR_SERVER_IP]:/root/
    ```
2.  **Run the installation**:
    ```bash
    ssh root@[YOUR_SERVER_IP] "chmod +x /root/madmail && ./root/madmail install --simple --ip [YOUR_SERVER_IP] && systemctl start maddy"
    ```

*Note: Replace `[SOURCE_SERVER_IP]` with the IP of any existing Madmail server. Replace `[YOUR_PUBLIC_IP]` with the IP of your new server. Always [verify the binary hash](./binary-verification.md) before installation.*

### Interactive Setup Tips
During the `--simple` installation, if you are setting up an IP-based server, enter your **Server IP** for both requested fields:
1.  **Primary domain**: Enter your Public IP (e.g., `1.2.3.4`).
2.  **Public IP address**: Confirm your Public IP (e.g., `1.2.3.4`).

### Quick Update (Recommended)
The safest way to update is using the built-in upgrade commands which automatically verify the digital signature:

```bash
# Update from a URL (automatically downloads and verifies)
sudo maddy update http://[SOURCE_SERVER_IP]/madmail

# OR Upgrade from a local file
sudo maddy upgrade /path/to/new/madmail
```

---

## 🛠 Advanced & Manual Setup

While the scripts above are the fastest way to get online, you can also perform a customized installation or use Docker.

### 1. Manual Installation
For full control, run the installer without the `--simple` flag:
```bash
sudo ./madmail install
```

### 2. Docker Deployment
See the [**Docker Documentation**](./docker.md) for detailed environment variables and volume mappings.

### 3. Managing JIT Registration
JIT registration controls automatic account creation during login attempts and email delivery:

```bash
# Enable automatic account creation
sudo maddy --config /etc/maddy/maddy.conf creds jit enable

# Disable automatic account creation
sudo maddy --config /etc/maddy/maddy.conf creds jit disable

# Check JIT registration status
sudo maddy --config /etc/maddy/maddy.conf creds jit status
```

### 4. Automatic Cleanup

Messages are automatically cleaned up after the configured retention period to minimize data retention.

Unused accounts (accounts that have never logged in) can also be automatically cleaned up. Configure this in `maddy.conf`:

```maddy
storage.imapsql local_mailboxes {
    # ... other config ...
    retention 480h  # 20 days
    unused_account_retention 720h  # 30 days
    auth_db local_authdb
}
```

This will:
- Delete messages older than 20 days (480 hours)
- Delete accounts that never logged in (first_login_at = 1) and were created more than 30 days ago (720 hours)
- Existing legacy accounts (first_login_at = 0 or NULL) are protected from deletion during migration by setting their first_login_at to the current time

### 5. Prerequisites & Troubleshooting
- **Digital Signature**: All official binaries are signed. The server will reject unsigned or tampered binaries during the `update` process. See [Signature Verification](./signature.md).
- **Open Ports**: Ensure the following ports are open:
    - `80` (HTTP) / `443` (HTTPS) - Web registration and deployment
    - `25` (SMTP) - Federation
    - `465` / `587` (Submission)
    - `143` / `993` (IMAP)
    - `3340` (HTTP) - **Iroh Relay** (required for WebXDC real-time P2P)
- **Configuration**: Settings are stored at `/etc/maddy/maddy.conf`.
- **Iroh Relay**: Managed as a separate service `iroh-relay.service`. If real-time P2P isn't working, check `journalctl -u iroh-relay`.
- **OS Support**: Best supported on Debian and Ubuntu.

### 🤝 Community & Support
For the latest binaries and installation tips, join the Telegram channel:
👉 [**Madmail Telegram Channel**](https://t.me/the_madmail)

---
*Technical Note: The web-based installation instructions are served from [`internal/endpoint/chatmail/www/deploy.html`](../internal/endpoint/chatmail/www/deploy.html), while the actual installation logic is implemented in [`internal/cli/ctl/install.go`](../internal/cli/ctl/install.go).*
