# Browser and Web Access

Not everyone wants to use a full email client like Delta Chat or Thunderbird. Chatmail servers also offer ways for people to access their mail directly from a web browser.

## The Classic Web Interface

Most chatmail servers include a simple webmail-style interface. From here users can:

- Read and send messages
- Manage folders
- Change basic settings

The exact look depends on the server operator, but it is usually reachable at:

```
https://your-server/
```

or sometimes at a path like `/webmail` or `/mail`.

This interface is convenient for occasional use or for people who don’t want to install anything.

## WebIMAP / Web Access for Delta Chat Desktop

Delta Chat Desktop can connect to a chatmail server using a normal browser-style connection (HTTPS + WebSocket) instead of raw IMAP/SMTP.

This is useful when:

- The user is behind a very restrictive network that blocks normal mail ports.
- They want to use Delta Chat from a browser or from inside another application.
- They are on a device where installing a full desktop client is inconvenient.

The server advertises these web endpoints automatically. Delta Chat Desktop will usually offer the web connection method when it detects it is available.

## Contact Sharing (`/share`)

Chatmail servers have a simple web-based way to share contacts.

A user can go to:

```
https://your-server/share
```

They can generate a link or QR code that other people can open in their browser or Delta Chat app. This makes it easy to invite someone to start a secure chat without having to exchange long keys manually.

After someone opens the link and creates an account (or logs in), the two sides can securely connect.

This feature is especially popular with people who run public or semi-public servers and want an easy onboarding method.

## Invite Links (`/inv/`)

Similar to contact sharing, operators and users can create invite links. These links can be configured to require a registration token, which gives the server admin some control over who can create accounts.

## The `/new` Registration Page

This is the main public page for creating new accounts:

```
https://your-server/new
```

It is intentionally simple. A user only needs to choose a username and password (and a registration token if required).

Many operators put this link on their website, in group chats, or on flyers when they want to let people sign up easily.

## Documentation Served by the Server

A running chatmail server also serves its own documentation at:

```
https://your-server/docs/
```

This documentation is available in multiple languages (English, Persian, Russian, Spanish, etc.) and covers the basics for users of that specific server.

This is very useful when you run a public or community server — users can read instructions directly from your server without needing external websites.

## Security Considerations for Web Access

- All modern chatmail servers redirect HTTP to HTTPS when a proper certificate is installed.
- The web interfaces use the same login system as normal IMAP/SMTP (the user’s regular password).
- Even if someone only uses the web interface, their messages are still stored encrypted on disk (following the normal PGP-only rules).

## When to Recommend Web Access

- For users who are not comfortable installing apps
- For quick checks on someone else’s computer
- In situations where installing software is restricted (schools, workplaces, etc.)
- For the initial contact-sharing step before switching to Delta Chat

Most regular users will still prefer Delta Chat on their phone and desktop for daily use.

## Next

- Common problems and how to solve them: [Troubleshooting](./10-troubleshooting.md)
- Managing the server day-to-day: [Admin & CLI](./07-admin-and-cli.md)
