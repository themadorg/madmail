# Admin Interface and Command Line Tools

Once your chatmail server is running, you will spend most of your time in the admin tools.

There are two main ways to manage a server:

1. The web admin dashboard (recommended for most day-to-day work)
2. The `madmail` command-line tool (the binary is also sometimes called `chatmail` in the source, but on most systems you run `madmail`)

Both talk to the same underlying system.

One particularly useful CLI command for production servers is `madmail upgrade` (see the Quick Start guide for how signed binary upgrades work).

## The Web Admin Dashboard

This is the most user-friendly way to manage the server.

### Accessing It

In the official releases and normal installations, the Admin Web Interface is **included directly in the `madmail` binary**.

It is served at:

```
https://your-server/admin/
```

(or a custom path you configure).

You log in with the **admin token** — a long random string stored in the `admin_token` file on the server (or the one shown during `madmail install`).

### What You Can Do in the Web Interface

Typical sections include:

- **Accounts** — list users, see last login, storage usage, create or delete accounts
- **Registration** — open or close public registration, manage registration tokens
- **Blocklist** — ban users (they can no longer log in or receive mail)
- **Federation** — see which other servers you talk to, success/failure rates, latency
- **Quota & Limits** — change default storage quota, message size limit
- **Settings / Toggles** — turn registration on/off, enable/disable TURN, change logging behavior, etc.
- **Queue** — inspect the outbound delivery queue
- **Status** — what listeners are actually running, basic health

Most changes take effect quickly (often without a full restart).

### Alternative: Hosted Admin Panel

If you don’t want to embed the admin web UI in the binary (or want the latest version without rebuilding), you can use the publicly hosted admin panel at:

**https://admin.madmail.chat**

This is the same SvelteKit SPA from the [madmail-admin-web](https://github.com/themadorg/madmail-admin-web) repository, kept up to date automatically via CI/CD.

You simply enter your server’s admin API URL (for example `https://your-server/api/admin` or your custom path) and your admin token. The panel connects directly to *your* server — your data never leaves your instance.

This is especially convenient for managing your server from any device or when running a minimal installation without the embedded UI.

### Managing the Admin Web via CLI

The admin web interface is controlled through simple CLI commands. This is the recommended way to enable or customize it.

```bash
# See current status
madmail admin-web status

# Make the admin interface available (usually at /admin)
madmail admin-web enable

# Hide the admin interface (returns 404)
madmail admin-web disable

# Use a custom (and more secure) path instead of the default /admin
madmail admin-web path /admin-xyz123

# Reset the path back to the default
madmail admin-web path --reset
```

**Recommendation**: On any public server, it is strongly advised to change the admin path away from the default `/admin` using the command above.

These commands update settings in the database. After running them, usually do:

```bash
madmail reload
```

to apply the changes without a full restart.

## The Command Line (`madmail`)

The same binary that runs the server also contains a rich set of management commands. The CLI is especially useful for scripting, automation, or when you only have SSH access.

```bash
# See everything the CLI can do
madmail --help
```

### Basic Information

```bash
madmail version
madmail status                  # overall server status
madmail status --details        # per-port breakdown
```

### Admin Token & Admin Web

```bash
madmail admin-token             # display the admin bearer token
madmail admin-web status
madmail admin-web enable
madmail admin-web disable
madmail admin-web path /my-secret-admin
madmail admin-web path --reset
```

### Accounts & Users

```bash
madmail accounts list
madmail accounts info alice@example.org
madmail accounts create alice@example.org
madmail accounts delete alice@example.org --yes
madmail accounts ban bob@example.org --reason "spam"
madmail accounts unban bob@example.org --yes
madmail accounts ban-list
```

### Registration Control

```bash
madmail registration open
madmail registration closed
madmail registration status

# Registration tokens (for invite-only)
madmail registration-tokens create --max-uses 5 --comment "Team Berlin" --expires 72h
madmail registration-tokens list
madmail registration-tokens delete <token>
```

### Blocklist

```bash
madmail blocklist add baduser@example.org --reason "spam"
madmail blocklist list
madmail blocklist remove baduser@example.org --yes
```

### Federation & Delivery

```bash
madmail federation list
madmail federation status
madmail federation policy accept
madmail federation block example.com
madmail federation allow example.com
madmail federation dismiss example.com      # silent accept but drop
madmail federation dismiss-list

# Outbound queue
madmail queue list
```

### Endpoint Rewrite / Exchangers (advanced routing)

```bash
madmail endpoint-cache list
madmail endpoint-cache set a.com b.com --comment "Route via partner"
madmail endpoint-cache remove a.com
```

### Certificates

```bash
madmail certificate status
madmail certificate self-signed
madmail certificate acme --email you@example.com
```

### Ports & Limits

```bash
madmail port status
madmail port smtp set 2525
madmail port https local

madmail message-size status
madmail message-size set 50M
```

### Maintenance & Tasks

```bash
madmail tasks list
madmail tasks run prune-old-messages
madmail tasks run-all
```

### Other Useful Commands

```bash
madmail reload                     # apply most config/DB changes without full restart
madmail upgrade <path-or-url>      # securely upgrade using signed binaries
madmail html-serve /path/to/custom/www   # serve custom HTML instead of built-in UI
madmail html-export /backup/www    # export the default web files
```

### Tips

- Almost every command supports `--help` (e.g. `madmail accounts --help` or `madmail registration-tokens create --help`).
- Many operations are easier in the web admin dashboard. Use the CLI when you need scripting or only have terminal access.
- After changing ports, paths, or certain settings, a `madmail reload` (or full restart) is often required.

For the complete list of subcommands and flags, run `madmail --help`.

### CLI vs Web Interface

- Use the **web interface** for exploration and occasional tasks.
- Use the **CLI** for scripting, automation, or when you only have SSH access.
- Both are equally powerful. The web UI is basically a nice frontend over the same admin API that the CLI uses.

## The Admin API (How Everything Works)

Both the web admin dashboard and all the `madmail` CLI commands (accounts, blocklist, federation, quota, settings, etc.) talk to the **same backend**:

```
POST /api/admin
Authorization: Bearer <admin-token>
Content-Type: application/json
```

Example request body:

```json
{ "method": "accounts.list" }
```

### Getting the Admin Token

The token is stored in the file `admin_token` on the server. You can view it with:

```bash
madmail admin-token
```

You must be root (or have access to the file) to read it.

### Security Note

- This single API endpoint is the only way to manage the server remotely.
- The web UI and CLI are just convenient frontends for this API.
- For better security on public servers, always change the admin web path away from the default using `madmail admin-web path /something`.

You normally don’t need to call the raw API yourself — the web interface and CLI commands are sufficient for almost all administration tasks.

## Common Administrative Tasks

**“I want to let a specific person register but keep registration closed for everyone else”**

→ Create a registration token with 1 use and send them the link or the token.

**“Someone is abusing the server”**

→ Add them to the blocklist (web or CLI). They immediately lose the ability to log in or receive mail.

**“I need to see why messages to a certain domain are failing”**

→ Look at the federation stats in the admin interface (or `madmail federation stats`).

**“I changed something in the config file”**

→ In many cases you can just run `madmail reload`. For some changes (new listeners, TLS certificates) a full restart is safer.

## Security Notes for Admins

### Rotate the Admin Token Regularly

The admin token gives full control over the server. It is strongly recommended to **rotate it at least once per week**.

**How to rotate the token:**

1. Stop the server:
   ```bash
   systemctl stop madmail
   ```

2. Delete the current token file:
   ```bash
   rm /var/lib/maddy/admin_token     # adjust path if you use a different state_dir
   ```

3. Start the server again:
   ```bash
   systemctl start madmail
   ```

4. The server will automatically generate a new strong random token on the next request that needs it.

5. Retrieve the new token:
   ```bash
   madmail admin-token
   ```

Store the new token securely and update any scripts or automation that use it.

### Use Non-Default Paths (Strongly Recommended)

By default the Admin API is available at `/api/admin` and the Admin Web UI is usually at `/admin`.

On any public-facing server, **change both paths**:

- **Admin API path**: Set in your `chatmail.toml` (or via the `admin_path` setting):
  ```toml
  admin_path = "/api/very-secret-xyz123"
  ```
  Then reload:
  ```bash
  madmail reload
  ```

- **Admin Web path**: Use the CLI (this is the easiest way):
  ```bash
  madmail admin-web path /admin-panel-abc987
  madmail reload
  ```

After changing the web path, you will access the dashboard at:
```
https://your-server/admin-panel-abc987/
```

Using non-default paths makes automated attacks and casual discovery much harder.

## Next

- Common problems and how to diagnose them: [Troubleshooting](./10-troubleshooting.md)
- Understanding the privacy implications of being an admin: [Privacy & Security](./04-privacy-and-security.md)
