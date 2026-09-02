# PostgreSQL

Madmail v2 can use **PostgreSQL** instead of SQLite for the application database (accounts, settings, quotas, tokens, federation). Mail stays in Maildir.

This is a **backend for an empty database**, not a copy of an existing SQLite file. `madmail install` still writes SQLite; you edit `driver` / `dsn` and restart. A SQLite → Postgres migration guide (and copy tool) is **in the works**.

Operator guide: [PostgreSQL as the application database](../project/user-guide/18-postgres.md).
