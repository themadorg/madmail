# Quota, Retention, and Maintenance

As a server admin you will regularly deal with storage limits, old messages, and accounts that are no longer used. This guide explains how these things work on a chatmail server and what you can do about them.

## Storage Quota

Every account has a maximum amount of storage it is allowed to use. This is called the **quota**.

### Default Quota

When you install the server (or when an account is created), it usually gets a default quota. Common values are 1 GB or 5 GB per user, but you can change this.

You can view and change the default quota in the admin web interface under **Quota** or **Settings**, or with the CLI:

```bash
madmail message-size          # shows current limits
```

### Per-User Quota

You can give individual users more or less storage than the default. This is useful for power users or for problematic accounts.

In the admin web interface you can usually adjust a user’s quota directly from the accounts list.

### What Happens When a User Hits Their Quota?

- They can no longer receive new mail (incoming messages are rejected with a “quota exceeded” error).
- They can still send messages (as long as the recipient accepts them).
- Their IMAP client will usually show a clear warning.

Most Delta Chat clients handle this reasonably well and tell the user they need to delete old messages or ask the admin for more space.

### Monitoring Quota Usage

The admin interface shows current usage for all accounts. You can quickly spot users who are using a lot of storage. This is often the first place to look when someone complains they can’t receive mail.

## Automatic Cleanup (Retention)

Chatmail servers can automatically delete old messages after a certain time. This is called **retention** or **message retention**.

### Why Use Retention?

- Prevents the server from filling up with years of old chat history.
- Reduces disk usage and backup size.
- Many operators consider old messages less important after a few months.

### How It Works

The server has two main retention policies (configurable in the admin interface or via settings):

- **Delete read messages older than X days**
- **Delete all messages older than Y days** (even unread ones)

Typical values used by many public chatmail servers are:
- Delete read messages after 30 or 60 days
- Delete everything after 180 or 365 days

Messages in the “DeltaChat” folder (where most Delta Chat traffic lives) are treated the same as other folders unless you configure otherwise.

### Important Warning

Retention is **permanent**. Once a message is deleted by the cleanup job, it is gone forever. There is no recycle bin.

Always communicate this clearly to your users if you enable aggressive retention.

## Dormant / Inactive Accounts

Over time some accounts stop being used. These are called **dormant accounts**.

The server can detect accounts that have not logged in for a long time. You can then:

- Send a warning email (if the account still has an address that can receive mail)
- Reduce or remove their quota
- Delete the account entirely after a warning period

This is useful for keeping the server clean and freeing up storage.

You can see lists of dormant accounts in the admin web interface under **Maintenance** or **Accounts**.

## Manual Maintenance Tasks

As an admin you will sometimes need to do things by hand:

- Delete a specific user’s old messages to free space
- Lower a user’s quota when they are abusing storage
- Remove accounts that were created but never really used
- Clean up after a user leaves the server

All of these actions are available in the admin web interface. The CLI also has commands for the most common tasks (`madmail delete`, `madmail accounts`, etc.).

## Best Practices for Most Admins

1. Set a reasonable default quota (1 GB is often enough for normal Delta Chat use).
2. Enable automatic deletion of old **read** messages (e.g. after 60–90 days). This removes the majority of junk without angering users.
3. Consider deleting everything older than 1 year if you want to keep the server very small.
4. Regularly check the list of dormant accounts and clean them up.
5. Watch the overall disk usage in the admin status page.

## Backup Considerations

Anything related to quota and retention directly affects your backups:

- The more aggressive your retention, the smaller your backups will be.
- When you delete an account, its mail is usually deleted too (unless you choose to keep the files).

Always have a backup strategy for the entire `state_dir` (database + mail folders) before doing large cleanup operations.

## Next

- How users can access their mail from a browser: [Browser and Web Access](./09-browser-and-web-access.md)
- What to do when something goes wrong: [Troubleshooting](./10-troubleshooting.md)
