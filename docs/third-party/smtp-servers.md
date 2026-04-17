# External SMTP server

It is possible to use maddy as an IMAP server only and have it interface with
external SMTP server using standard protocols.

Here is the minimal configuration that creates a local IMAP index, credentials
database and IMAP endpoint:
```
# Credentials DB.
table.pass_table local_authdb {
    table sql_table {
        driver sqlite3
        dsn credentials.db
        table_name passwords
    }
}

# IMAP storage/index.
storage.imapsql local_mailboxes {
    driver sqlite3
    dsn imapsql.db
}

# IMAP endpoint using these above.
imap tls://0.0.0.0:993 tcp://0.0.0.0:143 {
    auth &local_authdb
    storage &local_mailboxes
}
```

To accept local messages from an external SMTP server
it is possible to create an LMTP endpoint:
```
# LMTP endpoint on Unix socket delivering to IMAP storage
# in previous config snippet.
lmtp unix:/run/maddy/lmtp.sock {
    hostname mx.maddy.test

    deliver_to &local_mailboxes
}
```

Look up documentation for your SMTP server on how to make it
send messages using LMTP to /run/maddy/lmtp.sock.
