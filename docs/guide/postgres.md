# PostgreSQL

Madmail v2 can use **PostgreSQL** instead of SQLite for the application database (accounts, settings, quotas, tokens, federation). Mail stays in Maildir.

`madmail install` still writes SQLite. For an empty Postgres database, edit `driver` / `dsn` and restart. To copy an existing SQLite application database, stop the server and run `madmail db sqlite-to-postgres` (see the operator guide).

Operator guide: [PostgreSQL as the application database](../project/user-guide/18-postgres.md).
