# Troubleshooting Common Problems

This guide collects the most frequent issues that server admins and users run into, along with practical ways to diagnose and fix them.

## Installation skipped the account or systemd unit

Custom paths can select local mode even when run as root. For a **new** host setup,
use `install --system` with the intended config/state/binary paths and inspect
`--dry-run` first. For an **existing** config and state directory, use `--user-only`
to repair the account/permissions or `--systemd-only` to replace the unit and reload
systemd. Do not rerun full bootstrap just to repair provisioning: it rewrites config.

Stop the service before account repair, use the original executable name and paths,
and start it explicitly afterward. Unit-only repair needs the installed executable
and service account already present. It replaces the generated unit; keep custom
unit settings in drop-ins. See the [complete repair procedure](../../guide/cli/install.md#repair-only-the-service-account-or-systemd-unit)
and [error reference](../../guide/cli/install.md#troubleshooting).

A full install stops before changing anything if the unit file is masked (run
`systemctl unmask madmail`), or if the service account's home is elsewhere and still
exists while `--state-dir` already exists, so data in the old home is not left behind. Move the
data if needed, run `--user-only` with the same paths, then retry. A state directory
that is a symlink to a data volume is fine.

Install and repair print `Skipping ownership of <path>: <reason>` for a TLS file
they will not change by path, and `Ownership of <path> left unchanged: <reason>`
for an entry in the state directory they left alone, for example
`belongs to uid 0, not to the service account; it may be another account's file`.
Install gives a file in the state directory to the service account only when it
already belongs to that account (or to the previous owner of the state directory);
files install writes there belong to the service account from the start. So a
file a root command created there, or a link to another account's file, keeps its
owner. These are protections, not errors: the service account can write to its
state directory, so install does not follow links there. Check the named file
and, if madmail needs it, change its owner yourself with the steps the output
prints: stop the service (`systemctl stop madmail`) and every process of the
service account. Lock and expire the account, so that cron and at, which check
the account with PAM, start no job for it (`usermod -L -e 1970-01-02 madmail`;
`usermod` cannot change an account that exists only in a directory service such
as LDAP or sssd: lock and expire it there); where cron or at is installed,
remove its jobs (`crontab -r -u madmail`; `atq` lists the jobs, `atrm` removes
them); stop its systemd user manager where there is one
(`loginctl terminate-user madmail`); kill its processes
(`pkill -KILL -u madmail`); `ps -u madmail` must then list none. When the
file's uid now belongs to another account, the steps do the same for that
account, after noting its expiry (`chage -l <name>`). When you are done, end the
expiry again (`usermod -e '' madmail`); give the other account its old expiry
back (`usermod -e <date> <name>`, or `usermod -e '' <name>` when it had none)
and, if it was not locked before, unlock it (`usermod -U <name>`). The service
unit does not use PAM, so the service starts while its account is locked and
expired. Do not change the file if it has more than one hard link
(`stat -c %h <path>` prints more than 1). One link does not prove that it is
madmail's file, so compare its content with what madmail writes there, or remove
it and let madmail create it again. The printed steps end with the command
`chown -h madmail:madmail <path>`. The processes must be stopped first because
`chown` looks up the directories on the way to the file again by name, and a
running process of those accounts could replace one of them with a link. A
look at the running processes alone is not enough: a process can ignore
`SIGTERM`, and cron or at can start a new one for an account whose password is
locked. When a
directory on the way to the file belongs to another account or can be written by
one, the output prints no command and says that the file needs manual
inspection. For a directory
of another account the output says that it was not entered and that its contents
were left unchanged; inspect it yourself. Keep certificates and the config file
in root-owned directories outside the state directory.

When install refuses the state directory because
`it belongs to account <name> (uid <n>), not to root or to the service account`,
the old service uid was reused by another account. That account owns the files
and directories of uid `<n>` and can move them while you work, so first stop the
service (`systemctl stop madmail`) and stop that account the same way as the
service account above (note its expiry with `chage -l <name>`, then
`usermod -L -e 1970-01-02 <name>`, or lock it in the directory service when it
exists only there; `crontab -r -u <name>` and `atrm` for its `atq` jobs where
cron or at is installed; `loginctl terminate-user <name>`;
`pkill -KILL -u <name>`; `ps -u <name>` must then list none; afterwards give it
its old expiry back and, if it was not locked before, `usermod -U <name>`). To
keep the data, give the directory itself to root (`chown -h root:root <state>`)
and run install again: files and directories of uid `<n>` in it are then listed
and left as they are. A root-owned state directory that is not named for
madmail is taken only when it is empty, holds only `lost+found` or has a
root-owned `.madmail-state` file (which install creates in a state directory it
creates), so first move data in any other such directory to one that is named
for madmail, such as `/var/lib/madmail`. List the files with
`find -H <state> -xdev -uid <n> -ls` and check them. Then, with the service and
both accounts still stopped, give directories and single-link files to the
service account with
`find -H <state> -xdev \( -type d \( ! -uid 0 ! -uid <n> ! -user madmail -o -perm -g+w -o -perm -o+w \) -prune \) -o -uid <n> \( -type d -o -type f -links 1 \) -exec chown -h madmail:madmail {} +`
and check files with more than one hard link one by one. Give the state
directory as its physical path, or keep `-H`, which follows a link given as the
starting point and no other. The `-prune` part keeps `find` out of a directory
of any other account (install's own walk does not enter one either) and out of
a directory that a group or others can write; check such a directory yourself.
`find -exec` and `chown` look up each path again by name, so the service must be
stopped and neither account may have a running process. Install also refuses a well-known system
directory, a directory directly under `/`, a user's home directory, a directory
below `/etc`, `/usr`, `/lib`, `/bin`, `/sbin`, `/boot`, `/dev`, `/proc`, `/sys`
or `/run` that is not named for madmail (such as `/etc/systemd`), a path that
resolves to one of these (also through a link to a directory that does not
exist yet), and a directory of root or of a uid without an account name that
is not empty and not named for madmail (such as `/var/lib/dpkg`) as
`--state-dir`, unless it holds only `lost+found` or has install's root-owned
`.madmail-state` file; the name that counts is the physical one, after links.
Use a directory of its own such as `/var/lib/madmail`. If a previous install was
interrupted in a directory madmail did not create, check what is in it, then
empty it or move the data to a directory named for madmail, and run install
again. It also stops when a lookup of the owner of an existing state directory
or of the service account's groups reports an error: they must be known before
install trusts anything there. Install asks the C library and then
`getent passwd`, which uses the host's NSS configuration (LDAP, sssd and so
on); the release binaries are linked statically with musl, whose C library
reads only `/etc/passwd`. Without any `getent`, an account counts as missing
only when `/etc/nsswitch.conf` is missing or lists only `files` for passwd and
group. `getent` cannot tell a directory service
that is down from an account that does not exist, so make sure such a service
is reachable when you install.

An error that starts with `refusing to` stops install before it writes the file.
It names the directory or link on the path that the service account can change,
or says that the config file has several hard links, that a config path outside
`--state-dir` as written resolves to a path inside the state directory (give a
`--config-dir` under `--state-dir` as written if it belongs there), or that
`--binary-path` is in (or resolves into) the state directory. Move the file to a
root-owned directory outside the state directory and run the command again.
With `--skip-user`, install refuses to run when the service account does not exist
but the state directory exists and is not owned by root; create the account (run
install without `--skip-user`) or give the directory to root first.

Uninstall leaves a directory in place when its name does not show that it belongs
to madmail, and lists it as `Left in place: <path> (<reason>)`. Check the listed
directory and remove what madmail created yourself.

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

In most cases messages will eventually go through via the SMTP fallback if HTTPS is blocked — but fixing **443** is the right first step between chatmail relays. That fallback is the default. `madmail federation transport set smtp block` (then `madmail reload`) turns the port 25 fallback off, and `set http block` skips plain HTTP. Add `--domain example.org` to apply one of those blocks only toward that destination; other domains keep the global mode. A refused, expired, or given-up delivery writes a delivery report into the sender's inbox, and Delta Chat shows that text on the message. Those switches are outbound federation only. They leave the port 443 IMAP/SMTP listener (`madmail shared-port`) alone, and outbound delivery still does not open SMTP to port 443.

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
