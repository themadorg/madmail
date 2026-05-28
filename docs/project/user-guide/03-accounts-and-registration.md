# Accounts and Registration

One of the things that makes chatmail servers feel different from traditional mail servers is how accounts are created.

## Just-in-Time (JIT) Registration

By default, **you do not create accounts in advance**.

When someone connects to your server for the first time with Delta Chat (or any other email program) and enters a username + password, the server checks:

- Does this username already exist?
- Is registration currently allowed?

If the answer to both is “no account yet, but registration is open”, the server creates the account on the spot, stores a secure hash of the password, and sets up the mailbox.

From the user’s point of view it feels instant. They just enter an address and a password and they’re in.

This is called “Just-in-Time” registration.

## Two Ways People Usually Get Accounts

### 1. Open Registration (the default for many small servers)
Anyone who knows the server address can create an account by simply logging in with Delta Chat.

This is convenient and welcoming, but it also means anyone can make as many accounts as they want.

### 2. Registration Tokens (invite / controlled access)
The person running the server can create special one-time or limited-use tokens.

When creating an account via the web “/new” page (or sometimes through Delta Chat), the user must provide a valid token.

Benefits:
- You can limit who can create accounts.
- You get an audit trail (“this account was created with token X”).
- Tokens can expire or have a maximum number of uses.

Most operators who want any control use tokens for at least some accounts.

## The Web Registration Page (/new)

Every chatmail server has a simple web page at:

```
https://your-server/new
```

(or http in development)

From this page a user (or another program) can create an account by choosing a username and password and optionally entering a registration token.

This page is especially useful when you want to give people a link to sign up without them having to configure an email client first.

## Blocking and Removing Accounts

As the operator you have several tools:

- **Blocklist** — A blocked user cannot log in and cannot receive mail. Existing messages may still be on disk until purged.
- **Delete** — Completely removes the account, password hash, and (optionally) all their stored mail.
- **Quota** — You can lower or remove a user’s storage quota. When they hit the limit they stop receiving new mail.

All of these actions are available both from the admin web interface and from the command line.

## What Information Does the Server Store About Accounts?

For a normal account the server stores:

- The normalized username
- A cryptographic hash of the password (never the password itself)
- Current storage usage and quota
- Timestamps of first and last login (used for maintenance)
- Which registration token (if any) was used to create the account

It does **not** store:
- Plaintext passwords
- The content of messages (those live in the Maildir as encrypted files)
- Detailed logs of who emailed whom (especially if the server is running in No-Log mode)

## Common Questions from Operators

**“Can I pre-create accounts for people?”**

Yes. You can use the CLI (`madmail create-user ...`) or the admin web interface. The account will exist immediately and the user can log in with the password you set.

**“What if someone creates a lot of throwaway accounts?”**

Use registration tokens + limited uses, or simply turn registration off and only create accounts manually for people you trust.

**“Can users change their own password?”**

Currently password changes are usually done by the operator (via admin tools) or by the user deleting the account and creating a new one. Some clients support password change via IMAP, but support varies.

**“What happens to a user’s mail when I delete the account?”**

By default the mail is deleted along with the account. There are options to keep the files if you need them for legal or migration reasons.

## Registration Tokens in Practice

Many operators have a small internal process like this:

1. Someone asks for an account.
2. Operator creates a token with 1 use and a comment (“for Alice”).
3. Operator sends Alice the signup link that includes the token (or just the token + the /new page address).
4. Alice creates her account.
5. The token is now used up and cannot be reused.

This gives lightweight control without a lot of ceremony.

## Next

- How the strong privacy rules actually work: [Privacy & Security](./04-privacy-and-security.md)
- Day-to-day management from the web or command line: [Admin & CLI](./07-admin-and-cli.md)
- What to do when something goes wrong: [Troubleshooting](./10-troubleshooting.md)
