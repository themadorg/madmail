# PostgreSQL as the application database

Madmail v2 can store accounts, settings, quotas, tokens, and federation state in **PostgreSQL** instead of SQLite. Mail bodies stay on disk (Maildir). Contact-sharing pages stay in a separate SQLite file (`sharing.db`).

**This is a backend choice for an empty database.** It is not a conversion of an existing SQLite install. Changing `driver` does not copy `credentials.db` / `chatmail.db` into Postgres. There is no `madmail` command that does that copy.

`madmail install` always writes SQLite. Point the config at Postgres after install (or on a custom config), then **restart** the process so it opens the new driver. `madmail reload` does not re-read `driver` / `dsn`.

## What lives where

| Data | Where |
|------|--------|
| Password hashes, settings, quotas, blocklist, registration tokens, federation stats, port overrides, push tokens | The **application SQL database** (SQLite file *or* Postgres) |
| Message files, IMAP flags, UID lists | `{state_dir}/mail/` (Maildir) |
| Outbound retry jobs | `{state_dir}/remote_queue/` |
| Admin API token | `{state_dir}/admin_token` (file, not SQL) |
| `/share` contact pages | `{state_dir}/sharing.db` (always SQLite) |

Back up Maildir and `state_dir` the same way on either backend. For Postgres, also back up that database with normal Postgres tools (`pg_dump`, snapshots). SQLite WAL copy tricks do not apply.

## One database in v2

Go Madmail often used two SQLite files (`credentials.db` and `imapsql.db`). v2 opens **one** application database: the `table sql_table` under `auth.pass_table`.

Set **both** `auth.pass_table` and `storage.imapsql` to the same Postgres `driver` and `dsn` so the config file is not misleading. Only the auth `sql_table` DSN is what the process actually opens.

## Fresh Postgres (recommended path)

1. Create an empty database and a role that can create tables (sqlx migrations run on first start).

   ```sql
   CREATE USER madmail WITH PASSWORD 'choose-a-secret';
   CREATE DATABASE madmail OWNER madmail;
   ```

2. Install Madmail as usual (`madmail install …`). That still writes SQLite in the generated config.

3. Edit `/etc/madmail/madmail.conf` (or your `--config` file). Use the same DSN in both blocks:

   ```
   auth.pass_table local_authdb {
       auto_create yes
       jit_domain $(primary_domain)
       table sql_table {
           driver postgres
           dsn host=127.0.0.1 port=5432 user=madmail password=choose-a-secret dbname=madmail sslmode=disable
           table_name passwords
       }
   }

   storage.imapsql local_mailboxes {
       auto_create yes
       driver postgres
       dsn host=127.0.0.1 port=5432 user=madmail password=choose-a-secret dbname=madmail sslmode=disable
       retention 24h
       default_quota 1G
       appendlimit 100M
   }
   ```

   URL form is also accepted: `postgres://madmail:choose-a-secret@127.0.0.1:5432/madmail?sslmode=disable`.

4. Stop SQLite-using processes if you already started the server once. Restart Madmail (`systemctl restart madmail`, or `docker restart madmail`). First boot against an empty Postgres database applies `crates/chatmail-db/migrations/postgres/`.

5. Confirm with `madmail status` and a login or `madmail accounts list`. Leftover `credentials.db` / `chatmail.db` files in `state_dir` are unused after the switch; they are not deleted automatically.

Use `sslmode=require` (or stricter) when Postgres is not on localhost. Do not commit DSNs with real passwords.

## Docker

The image default and `install` still use SQLite. After bootstrap, put Postgres `driver` / `dsn` in the bind-mounted `madmail.conf` (same two blocks as above) and restart the container. Run Postgres as a sibling container or an external host the Madmail container can reach.

There is no Compose environment variable that selects Postgres. `MADDY_HOSTNAME` / `MADDY_DOMAIN` only fill hostname and mail domain in the bundled config.

See [Docker deployment](../../guide/docker.md#custom-configuration).

## What this does not do

- Copy rows from an existing SQLite file into Postgres.
- Move Maildir, the retry queue, TLS material, or `admin_token`.
- Move `sharing.db`.
- Upgrade a live server in place by flipping `driver` while it is running.

If you already have users on SQLite and want them on Postgres, that is a **data copy**, not this backend switch. Keep using SQLite for now, or stand up a **new** Postgres server and recreate accounts. A SQLite → Postgres migration guide (and copy tool) is **in the works**.

A server that was **already** on Postgres under Go Madmail is a different case: v2 can open that schema and fill missing tables. That is not an SQLite conversion.

## Related

- [Quick start — where data lives](./02-quick-start.md#where-the-server-stores-its-data-default-locations)
- [Configuration (TDD)](../../TDD/13-configuration.md)
- [Data models](../../TDD/17-data-models.md)
- [Native install](../../guide/install.md)
