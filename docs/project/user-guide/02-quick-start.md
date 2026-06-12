# Quick Start: Running a Chatmail Server

This guide walks through practical paths to a working chatmail server, from completely local testing to a public deployment.

## Quick Setup

Download the release binary for your architecture, then install and start the service:

```bash
ARCH=$(uname -m | sed 's/x86_64/amd64/;s/aarch64/arm64/')
curl -fsSL "https://github.com/themadorg/madmailv2/releases/latest/download/madmail-linux-${ARCH}" \
  -o madmail
chmod +x madmail
```

### With a public IP (no domain)

Trusted TLS via Let's Encrypt IP certificate (~6-day renewal, port 80 required):

```bash
sudo ./madmail install --simple --ip YOUR_IP \
  --auto-ip-cert \
  --acme-email you@example.com \
  --lang en
sudo systemctl enable madmail
sudo systemctl start madmail
```

> Replace `YOUR_IP` with your server's public IPv4 or IPv6 address.

Self-signed TLS (testing / internal — omit `--auto-ip-cert`):

```bash
sudo ./madmail install --simple --ip YOUR_IP --lang en
sudo systemctl enable madmail
sudo systemctl start madmail
```

### With a domain

Standard Let's Encrypt certificate (90-day renewal, DNS must point to your server):

```bash
sudo ./madmail install --simple --domain mail.example.org \
  --acme-email you@example.com \
  --lang en
sudo systemctl enable madmail
sudo systemctl start madmail
```

> Replace `mail.example.org` with your hostname and `you@example.com` with a valid contact email.

More detail:

- [Simple IP + ACME install](../../install-simple-ip-acme.md)
- [IP vs domain deployment](./11-deployment-ip-domain-certs.md)
- [Local development](../../local-dev.md)

## Local Testing (for Operators)

If you just want to test the server locally as an operator (without building from source), a typical approach is to use a pre-built release binary or run it via the official install process in a container/VM.

Detailed instructions for **developers** who are building, modifying, and testing the source code locally (Rust, `make restart`, ports for Delta Chat desktop, etc.) live in the developer documentation:

→ **[Local Development Workflow](../15-development-workflow.md)**

That guide covers the full edit → build → test loop used by people working on the madmail-v2 code.

## Getting a Pre-built Binary (Releases)

To obtain a ready-to-use binary without compiling:

1. Go to the releases page:  
   **https://github.com/themadorg/madmail/releases**

2. Download the latest release for your architecture (usually `madmail-linux-amd64` or similar).

   Example with `wget`:

   ```bash
   wget https://github.com/themadorg/madmail/releases/download/vX.Y.Z/madmail-linux-amd64
   chmod +x madmail-linux-amd64
   sudo mv madmail-linux-amd64 /usr/local/bin/madmail
   ```

   The binaries in the official releases are the same ones produced by this project.

## Secure Upgrades with `madmail upgrade`

Once you have `madmail` installed, future updates can be done securely with the built-in upgrade command:

```bash
sudo madmail upgrade <path-or-url>
```

### How it works

- If you give it a local file or a download URL, `madmail upgrade`:
  1. Downloads the new binary (if a URL was provided).
  2. **Verifies the digital signature** (Ed25519) that is appended to the release binary.
  3. If the signature is invalid or missing, the upgrade is **aborted** — the binary is never installed. This protects against compromised or tampered releases.
  4. Stops the `madmail.service` (and iroh-relay if present).
  5. Atomically replaces the running binary.
  6. Restarts the service(s).

This is the supported way to update a production server with signature verification.

The same signed binaries from https://github.com/themadorg/madmail/releases can be used with `madmail upgrade`.

## Option 1: Public Server on a Real IP

This path installs a full production setup (systemd service, user, directories, and certificate).

See the dedicated short guide:

**[Install on a Public IP with Let’s Encrypt](../install-simple-ip-acme.md)**

It boils down to (run as root):

```bash
madmail install --simple --ip YOUR.PUBLIC.IP.ADDRESS \
  --auto-ip-cert \
  --acme-email you@yourdomain.com
```

This command:
- Creates the system user and directories
- Generates a strong admin token
- Obtains a browser-trusted certificate (even though you only have an IP, no DNS name)
- Writes a basic config
- Sets up the systemd service

After it finishes, your server is reachable on the normal mail ports and you can connect with Delta Chat using your IP address or a domain that points to it.

## Option 2: Using the Admin Web Interface

In normal installations and official releases, the Admin Web Interface is already included in the `madmail` binary.

After you have a running server:

1. Open `https://your-server/admin/` (or the path you configured).

2. Log in with the token from the `admin_token` file (or the one shown during `madmail install`).

You can enable, disable, or change the path of the admin web interface using CLI commands (see the [Admin & CLI guide](./07-admin-and-cli.md)).

From the web interface you can:
- Open or close registration
- Create registration tokens (for invite-only)
- Ban users
- View federation health
- Change quotas and limits
- Manage the server without typing commands

## First Things Most Operators Do

After the server is running:

1. Decide whether new users can register freely (`registration open` or closed).
2. If you want some control, create a few registration tokens.
3. Set a reasonable default storage quota.
4. Make sure you have a backup strategy for the `state_dir` (the database + the `mail/` folder).
5. (Optional but nice) Point a domain name at the server and update the config.

## Where the Server Stores Its Data (Default Locations)

By default, almost everything lives under a single **state directory**. This makes backups and management straightforward.

### Default State Directory

- **On production servers**: `/var/lib/madmail`
- **Legacy installs** (older madmail/maddy setups): `/var/lib/maddy`
- **Local development/testing**: `./data` (relative to where you run the binary)

You can override this with `--state-dir` on the command line or the `state_dir` setting in the config file.

### Folder Structure Inside the State Directory

```
state_dir/
├── admin_token                  # 64-character admin API token (file permissions 0600)
├── chatmail.db                  # Main SQLite database (settings, accounts, quotas, federation stats, blocklist, etc.)
├── credentials.db               # Legacy authentication database (used by some older paths)
├── mail/                        # All user mailboxes in Maildir format
│   ├── alice@example.com/
│   │   └── Maildir/
│   │       ├── cur/             # Messages with flags
│   │       ├── new/             # Newly delivered messages
│   │       └── tmp/
│   └── bob@example.com/
│       └── folders/
│           └── DeltaChat/
│               └── Maildir/     # Most Delta Chat history lives here
├── remote_queue/                # Persistent storage for the outbound delivery retry queue
├── certs/                       # TLS certificates and keys (when using file-based TLS)
│   ├── fullchain.pem
│   └── privkey.pem
└── autocert/                    # Let's Encrypt / ACME account keys and state
```

### Other Important Locations (Outside the State Dir)

- **Main configuration**: Usually `/etc/madmail/madmail.conf` (or `chatmail.toml`)
- **Logs**: Typically viewed with `journalctl -u madmail` (systemd). Some setups also log to a file.
- **Systemd unit**: `madmail.service`

### Why This Layout?

- One directory (`state_dir`) to back up gives you the database + every user's mail.
- Mail storage uses standard Maildir, so it is easy to inspect, migrate, or debug with normal Unix tools.
- Delta Chat stores the majority of chat history inside each user's `folders/DeltaChat/Maildir/` folder.
- The admin token lives as a separate file (never inside the database) for security reasons.

### Backups

The simplest reliable backup is a snapshot or copy of the entire `state_dir` (ideally while the service is stopped, or using proper tools that handle SQLite WAL files correctly).

## Self-Serving Binary and Documentation

Every chatmail server can serve its own current binary at the `/madmail` path:

```
https://your-server.example.com/madmail
```

This is very useful for bootstrapping other servers.

**Recommended practice:**

- For your **first** server, download the binary from the official GitHub releases (https://github.com/themadorg/madmail/releases). This gives you a known-good, properly signed starting point.

- After you have at least one trusted server running, you can download binaries from other chatmail servers using the `/madmail` path. This is safe because official releases are digitally signed. When you later run `madmail upgrade` with a binary from another server (or a URL), it will verify the signature before installing.

Every server also serves a full set of documentation directly at:

```
https://your-server.example.com/docs/
```

This includes multi-language guides for users and operators. The documentation is always available from the server itself, even in restricted network environments.



## Downloading the Binary Directly From Any Server

Every chatmail server can serve its own binary at the path `/madmail`.

Example:

```
https://your-server.example.com/madmail
```

This can simplify bootstrapping or updating other servers.

**Best practice for initial installation:**

For the very first server you set up, it is recommended to download the binary from the official GitHub releases:

→ https://github.com/themadorg/madmail/releases

This ensures you start with a known-good, properly signed release.

**After the first server is running:**

You can download the binary from any other trusted chatmail server (including your own) using the `/madmail` path.

This works safely because **official binaries are digitally signed**. When you later run:

```bash
madmail upgrade /path/to/downloaded/binary
# or
madmail upgrade https://another-server.com/madmail
```

the `madmail upgrade` command will verify the signature before installing the new binary. If the signature is invalid or missing, the upgrade is aborted.

This allows you to bootstrap an entire network of servers even in restricted environments, as long as you have at least one trusted starting point.

## Built-in Documentation on Every Server

Every running chatmail server also serves its own documentation at:

```
https://your-server.example.com/docs/
```

This documentation is available in multiple languages and includes practical guides for users and operators, including a guide on customizing the web interface and HTML pages.

This helps when setting up a new server or helping users — they can read documentation from the server itself without external internet access.

## Customizing the Web Interface and HTML Pages

If you want to change the look of the public site, registration page, documentation, or other HTML pages served by the server, see:

**[Customizing the HTML Pages](./17-customizing-html-pages.md)**

This covers how to set a custom `www_dir`, what you can override, and security considerations.

## Choosing the Right Deployment Type (IP vs Domain + Certificates)

Most people setting up a real server ask: "Should I use my IP address or a domain name? Do I need a proper certificate?"

See the dedicated guide that explains all four common combinations:

**[IP-only vs Domain, With or Without Certificates](./11-deployment-ip-domain-certs.md)**

It also links to the exact `madmail install` commands for each case.

## Getting Help When Something Goes Wrong

- `make logs` is your first stop.
- The [Troubleshooting guide](./10-troubleshooting.md) covers the most common issues.
- The admin interface has a status section that shows what listeners are actually running.

## Next Steps

- Learn how accounts and registration actually work: [Accounts & Registration](./03-accounts-and-registration.md)
- Understand the privacy model: [Privacy & Security](./04-privacy-and-security.md)
- See how to manage the server day-to-day: [Admin & CLI](./07-admin-and-cli.md)

You now have a working chatmail server. Most people are surprised by how little ongoing work it requires.
