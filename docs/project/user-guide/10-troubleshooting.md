# Troubleshooting Common Problems

This guide collects the most frequent issues that server admins and users run into, along with practical ways to diagnose and fix them.

## Users Cannot Register or Log In

### Possible causes

- Registration is currently closed on the server.
- The user is trying to register with a registration token that is invalid, expired, or already used up.
- The username contains characters that are not allowed by the server’s credential policy.
- The user is connecting to the wrong hostname or port.

### What to check

1. Open the admin web interface and look at the **Registration** section. Is it set to “open” or “closed”?
2. If you use registration tokens, create a fresh one and test with it.
3. Ask the user to try a very simple username (only lowercase letters and numbers) during testing.
4. Check the server logs (or ask the user to tell you the exact error message they see in Delta Chat).

## Users Cannot Receive Mail (“Quota Exceeded”)

This is extremely common.

### What usually happened

The user (or one of their devices) has accumulated a lot of old messages and hit their storage limit.

### Quick fixes

- Ask the user to delete old messages in Delta Chat (especially in the DeltaChat folder).
- Temporarily increase their quota in the admin interface so they can clean up.
- If the server has retention enabled, old read messages will eventually be deleted automatically.

### Prevention

Set reasonable default quotas and consider enabling automatic deletion of old read messages (see the Quota & Maintenance guide).

## Messages Are Slow or Not Arriving Between Servers (Federation Issues)

### What to check

1. Look at the **Federation** section in the admin web interface. It shows success rate and latency for other servers.
2. Check whether the peer's federation endpoint responds (from outside your network):

   ```bash
   curl -sI https://other.server/mxdeliv
   ```

   A response like **405** or **400** means the endpoint is reachable. Connection refused or timeout means DNS, firewall, or nginx routing — not missing DKIM/SPF records.

3. Check the outbound queue in the **admin web UI** (Federation / queue views). A large backlog usually means the other server is down or there is a network problem. (or `madmail queue list` / `madmail queue status` for the outbound retry store.)

4. On **your** server, confirm inbound **443** (and ideally **80**) are reachable so peers can deliver to you:

   ```bash
   curl -sI https://YOUR_DOMAIN/mxdeliv
   ```

### Common causes

- Firewall or cloud security group blocking **outbound 443** (you → them) or **inbound 443** (them → you).
- Wrong or missing **`A`/`AAAA`** record for your hostname.
- Federation policy not set to accept (`madmail federation list`; default is accept).
- Port **25** blocked when delivery falls back to SMTP (many VPS providers block it).
- The other server is temporarily unreachable or misconfigured.

**cmdeploy / `554 5.7.1 No DKIM signature found`:** Madmail must sign outbound federation (fixed in #146). Run **`madmail dkim show`**, publish the printed TXT at `default._domainkey`, then **`madmail dkim check`**. Same-stack Madmail↔Madmail does not require that TXT. See [DNS and Mail Authentication](./12-dns-mail-auth.md).

In most cases messages will eventually go through via the SMTP fallback if HTTPS is blocked — but fixing **443** is the right first step between chatmail relays.

## Calls (Voice/Video) Don’t Work

### Checklist

- Is TURN enabled in the admin settings (`__TURN_ENABLED__`)?
- Are the required UDP ports open in the firewall? (The admin interface usually tells you which ports are needed.)
- Confirm `$(public_ip)` / `turn_server` / `relay_ip` in the config is an address **clients can reach**. On dual-stack installs without `--ip`, madmail prefers **public IPv4** for this field; if you only have usable IPv6 (or the wrong address was written), set `--ip` on reinstall or edit the config and restart.
- Are you testing between two users who are both behind difficult NAT? In that case the relay is required.
- Try forcing “relay only” mode in a test (some debug tools exist for this).

If calls only work when both users are on the same local network, the relay is probably not reachable from the outside.

## Admin Interface Shows “Not Available” or Placeholder

On a properly installed production server this usually means the admin web interface was never built into the binary.

On most public servers the operator runs the install process or uses a package that already includes the web UI. If you compiled the server yourself, you need to build the admin web interface and embed it.

For normal admins using pre-built packages or the official install method, this almost never happens.

## Server Uses Too Much Disk Space

### Quick things to check

- How many dormant/inactive accounts exist?
- Is message retention enabled and aggressive enough?
- Are there users with unusually large quotas who are not cleaning up?

Use the **Maintenance** and **Accounts** sections in the admin interface. Cleaning up old accounts and enabling sensible retention usually solves most space problems.

## Logs Are Empty or Almost Empty (Normal)

If you are running the server with No-Log mode enabled (very common), you will see almost nothing in the logs by design. This is not a bug.

Only enable normal logging temporarily when you are actively debugging a problem.

## “I Changed Something in the Config but Nothing Happened”

Many settings are stored in the database (`settings` table) and can be changed through the admin interface without touching config files.

Some settings still come from the static config file (`chatmail.toml` or the maddy-style config). Those usually require a reload or restart.

The safest way is almost always:

```bash
madmail reload
# Windows: Restart-Service Madmail  (or madmail … reload then restart if needed)
```

If that doesn’t pick up the change, a full restart is the next step.

## Windows-Specific Issues

| Symptom | Likely cause | What to try |
|---------|----------------|-------------|
| SmartScreen / Defender flags setup | Unsigned CI/Release PE (false positive) | [Defender / UAC guide](../../../packaging/windows/README.md#windows-defender-smartscreen-and-uac) |
| Service missing after setup | Install stopped early (e.g. Let's Encrypt) | Read `%ProgramData%\Madmail\install.log`; finish with self-signed or fix TCP 80 |
| LE HTTP-01 Invalid | Firewall / reachability (port free ≠ LE can connect) | Open inbound **TCP 80** host + cloud; re-test from the internet |
| `/admin` blank or 404 | SPA not embedded / not enabled | Use CLI + token; see [Admin & CLI](./07-admin-and-cli.md) |
| Custom HTML not showing | `www_dir` not set or service not restarted | [Customizing HTML](./17-customizing-html-pages.md#windows) |

## Getting More Information

When something is really not working, these commands are your friends:

```bash
madmail status                 # basic health
madmail logs                   # if your system uses journalctl or similar
sqlite3 /var/lib/madmail/chatmail.db "SELECT * FROM settings;"   # Linux default path (SQLite backend)
```

On PostgreSQL, inspect `settings` with `psql` against the DSN in `auth.pass_table`. See [PostgreSQL as the application database](./18-postgres.md) (empty database, not a SQLite copy).

Windows:

```powershell
& "C:\Program Files\Madmail\madmail.exe" `
  --config "$env:ProgramData\Madmail\config\madmail.conf" `
  --state-dir "$env:ProgramData\Madmail\data" status
Get-Content "$env:ProgramData\Madmail\install.log" -ErrorAction SilentlyContinue
Get-Service Madmail
```

Also check the admin web interface **Status** page when the SPA is available — it often shows which ports are listening.

## When in Doubt

1. Look at the admin web interface first (it has the most useful overview).
2. Check the federation and queue status.
3. Look at recent accounts and quota usage.
4. Only then dive into raw logs or the database.

Most problems turn out to be quota, registration settings, or federation reachability — all of which are easy to see in the web admin.

## Next

If you run into something that isn’t covered here, feel free to improve this guide or ask in the community channels used by chatmail operators.
