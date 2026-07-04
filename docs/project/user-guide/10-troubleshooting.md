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

3. Check the outbound queue in the **admin web UI** (Federation / queue views). A large backlog usually means the other server is down or there is a network problem. (`madmail queue` CLI is not implemented yet — use the admin UI.)

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

**Unlikely:** missing SPF, DKIM TXT, or DMARC records — chatmail-to-chatmail delivery uses HTTP `/mxdeliv` and PGP, not those DNS records. See [DNS and Mail Authentication](./12-dns-mail-auth.md).

In most cases messages will eventually go through via the SMTP fallback if HTTPS is blocked — but fixing **443** is the right first step between chatmail relays.

## Calls (Voice/Video) Don’t Work

### Checklist

- Is TURN enabled in the admin settings (`__TURN_ENABLED__`)?
- Are the required UDP ports open in the firewall? (The admin interface usually tells you which ports are needed.)
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
```

If that doesn’t pick up the change, a full restart is the next step.

## Getting More Information

When something is really not working, these commands are your friends:

```bash
madmail status                 # basic health
madmail logs                   # if your system uses journalctl or similar
sqlite3 /var/lib/maddy/chatmail.db "SELECT * FROM settings;"
```

Also check the admin web interface **Status** page — it often shows exactly which ports are actually listening.

## When in Doubt

1. Look at the admin web interface first (it has the most useful overview).
2. Check the federation and queue status.
3. Look at recent accounts and quota usage.
4. Only then dive into raw logs or the database.

Most problems turn out to be quota, registration settings, or federation reachability — all of which are easy to see in the web admin.

## Next

If you run into something that isn’t covered here, feel free to improve this guide or ask in the community channels used by chatmail operators.
