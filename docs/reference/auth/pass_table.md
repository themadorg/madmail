# Password table

auth.pass_table module implements username:password authentication by looking up the
password hash using a table module (maddy-tables(5)). It can be used
to load user credentials from text file (via table.file module) or SQL query
(via table.sql_table module).


Definition:
```
auth.pass_table [block name] {
	table <table config>

}
```
Shortened variant for inline use:
```
pass_table <table> [table arguments] {
	[additional table config]
}
```

Example, read username:password pair from the text file:
```
smtp tcp://0.0.0.0:587 {
	auth pass_table file /etc/maddy/smtp_passwd
	...
}
```

## Password hashes

pass_table expects the used table to contain certain structured values with
hash algorithm name, salt and other necessary parameters.

You should use `maddy hash` command to generate suitable values.
See `maddy hash --help` for details.

## Just-In-Time (JIT) Registration

Automatic account creation during authentication attempts is controlled by the JIT registration flag (`__JIT_REGISTRATION_ENABLED__`), which is separate from the general registration flag (`__REGISTRATION_OPEN__`).

- **JIT Registration** (`__JIT_REGISTRATION_ENABLED__`): Controls whether accounts are automatically created during IMAP/SMTP login attempts with non-existent credentials and email delivery to non-existent recipients.

If `__JIT_REGISTRATION_ENABLED__` is not explicitly set, it defaults to the value of `__REGISTRATION_OPEN__`.

Use the `maddy creds jit` command to manage JIT registration:

```
maddy creds jit enable   # Enable automatic account creation
maddy creds jit disable  # Disable automatic account creation
maddy creds jit status   # Show current JIT registration status
```

## maddy creds

If the underlying table is a "mutable" table (see maddy-tables(5)) then
the `maddy creds` command can be used to modify the underlying tables
via pass_table module. It will act on a "local credentials store" and will write
appropriate hash values to the table.
