# Multiple server addresses and selective JIT registration

[فارسی](server-domains_fa.md)

Use this recipe when one Madmail server has several DNS names or IP addresses,
and you want to choose which domains still allow new accounts through
Just-In-Time (JIT) registration.

## Serve several addresses from one server

After installation, edit `/etc/madmail/madmail.conf`:

```conf
$(primary_domain) = b.com c.com 1.1.1.1
$(local_domains) = $(primary_domain)
```

The primary-domain list accepts spaces or commas. The first entry is the
canonical server identity and default account domain. Every entry in the list
is accepted for local delivery; additional `local_domains` entries are also
supported.

Point the DNS names to the same server and configure TLS certificates covering
the addresses clients use. Listing domains does not configure DNS or obtain
certificates for all of them automatically.

| Address opened in the browser | Account domain |
|---|---|
| `https://b.com` | `user@b.com` |
| `https://c.com` | `user@c.com` |
| `https://1.1.1.1` | `user@[1.1.1.1]` |

The registration page and public `POST /new` endpoint select a browser hostname
only when it matches an accepted local domain. Matching ignores DNS-name case
and removes the port. An unknown or malformed hostname falls back to the first
primary domain.

Each full email address is a separate account with its own mailbox and
credentials. `a@b.com`, `a@c.com`, and `a@[1.1.1.1]` are not automatically aliases.
IP email domains use brackets, although login and CLI input may use a bare IPv4
address and have it normalized.

## Allow JIT registration on all listed domains

JIT creates an account when a client first logs in through IMAP or SMTP with an
unknown username and registration policy permits creation.

In the existing `auth.pass_table` block, use:

```conf
jit_domain $(primary_domain)
```

When `jit_domain` is omitted, it defaults to the full primary-domain list.
An explicit setting replaces that default allowlist. It can contain several
domains separated by spaces or commas:

```conf
jit_domain b.com c.com
```

## Retire a domain from JIT without locking out existing accounts

Keep every address in the server domain list, and narrow `jit_domain`.
For example, this authentication block allows new JIT accounts only at `c.com`:

```conf
$(primary_domain) = b.com c.com 1.1.1.1
$(local_domains) = $(primary_domain)

auth.pass_table local_authdb {
    auto_create yes
    jit_domain c.com
    table sql_table {
        driver sqlite3
        dsn credentials.db
        table_name passwords
    }
}
```

Adapt the existing configuration rather than replacing the whole file. Keep
your current database driver, DSN, storage settings, listeners, and TLS settings.
Restart the running server after changing the static configuration. On a
systemd installation:

```bash
sudo systemctl restart madmail
```

| Login attempt | Result when JIT is enabled and `jit_domain c.com` is set |
|---|---|
| Existing account at `b.com` with the correct password | Login succeeds |
| Existing account at `c.com` with the correct password | Login succeeds |
| Existing account at `[1.1.1.1]` with the correct password | Login succeeds |
| Unknown account at `c.com` | May be created if the remaining registration policy permits it |
| Unknown account at `b.com` or `[1.1.1.1]` | JIT creation is rejected |
| Existing account with an incorrect password | Login is rejected |

Existing blocked or removed accounts remain subject to their normal account
restrictions. Narrowing JIT does not change passwords or merge mailboxes.
Do not remove a retired address from the accepted local domains while its
accounts still need to receive mail.

## Keep JIT, public registration, and admin creation distinct

`jit_domain` restricts **first-login account creation through IMAP/SMTP**.
It does not disable public `/new` registration on those domains. That endpoint
has its own registration-open and registration-token policy. The browser page
may also generate credentials locally for a later JIT login; credentials for
a retired JIT domain will fail to create a new account through that path.

If your goal is to stop every public creation path on a domain, narrowing JIT
alone is insufficient. See the [registration controls](../cli/registration.md)
and [registration-token controls](../cli/registration-tokens.md).

Explicit CLI account creation is an admin operation and bypasses the JIT
domain allowlist. This lets an operator provision accounts deliberately even
on a domain retired from JIT.

## Create accounts with the CLI

Use a full address to select a domain. Omitting `--password` prompts for it:

```bash
madmail accounts create a@b.com
madmail accounts create a@c.com
madmail accounts create 'a@[1.1.1.1]'
```

These commands create three separate accounts. A full address is preserved
apart from username normalization; a bare IPv4 domain is bracketed:

```bash
madmail accounts create a@1.1.1.1
```

A username without `@` uses the first primary domain:

```bash
madmail accounts create a
```

With the configuration above, this creates `a@b.com`.
`madmail accounts create-random` and its `madmail create-user` alias also use
the first primary domain. To select a different domain, use the explicit
`accounts create user@domain` command. Keep that domain accepted locally if
the account should receive mail on this server.

Further CLI details: [accounts create](../cli/accounts-create.md) and
[accounts create-random](../cli/accounts-create-random.md).

## Troubleshooting

- New login fails on a retired domain: check whether the full email address
  already exists in the configured credentials database. An unknown account
  is subject to JIT restrictions.
- Existing login fails: check the full address, password, blocklist, running
  server version, and the credentials database selected by the configuration.
- Account is created under the wrong domain: check the browser hostname,
  accepted local domains, and the order of the primary-domain list.
- A TLS or connection error occurs before login: check DNS, certificate
  coverage, listener ports, and network reachability separately from JIT policy.

This behavior is covered by configuration/authentication tests, the Docker
multi-domain registration and login checks, and the CLI/JIT domain-retirement
end-to-end regression test.
