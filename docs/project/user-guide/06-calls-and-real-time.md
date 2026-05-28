# Calls, Voice, Video, and Real-Time Features

One of the most surprising things for new chatmail operators is that the server can also handle voice and video calls — without needing any extra services.

## How Calls Work in Delta Chat

Delta Chat uses the same underlying technology as many modern video calling apps (WebRTC).

When two people want to have a call:

1. Their Delta Chat apps figure out the best way to connect directly (peer-to-peer).
2. If that doesn’t work (because of firewalls, NAT, mobile networks, etc.), they need a **relay** that both sides can reach.
3. That relay is usually a TURN server.

A chatmail server can act as that relay for the people who have accounts on it.

## Why Is the TURN Server Built In?

Traditional setups require you to:
- Run a separate TURN server (coturn, etc.)
- Configure secrets
- Somehow tell every user’s client where the TURN server is and what the temporary credentials are

Chatmail does this automatically:

- The TURN server runs as part of the same `madmail` process (or is tightly integrated).
- When a user logs into IMAP, the client automatically asks for the TURN information using a standard IMAP extension (GETMETADATA).
- The server generates short-lived credentials for that user.
- The Delta Chat client receives everything it needs with no extra configuration.

From the user’s point of view, calls “just work” as long as they can reach the chatmail server.

## What the Operator Needs to Do

Usually very little.

In the config or admin interface you will see options like:

- `turn_enable` or `__TURN_ENABLED__`
- A shared secret used to generate credentials
- Port settings

Most of the time you leave these at the defaults. The server will advertise the service to clients that support it.

You do need to make sure the TURN ports are reachable from the internet (usually UDP ports in addition to the normal TCP mail ports). The admin interface and documentation will tell you which ones.

## Privacy Considerations for Calls

When a call goes through the server’s TURN relay:

- The media (audio/video) is still end-to-end encrypted between the two devices.
- The server sees encrypted traffic, but cannot understand the content.
- The server knows that two specific accounts were using the relay at a certain time (unless No-Log is enabled).

This is a deliberate trade-off: convenience and reliability for calls, in exchange for the server learning connection metadata for those calls.

Many operators consider this acceptable because:
- The content itself stays private.
- The alternative is usually sending that metadata to a big third-party TURN provider.
- You can turn the TURN server off if you prefer (calls will then only work when the two devices can reach each other directly).

## Other Real-Time Features (Iroh, etc.)

Chatmail servers can also run or advertise an **Iroh relay**.

Iroh is a modern peer-to-peer library used by Delta Chat for things like:
- WebXDC (interactive mini-apps inside chats)
- Faster and more reliable transfer of large files/blobs
- Future experimental features

The same automatic discovery mechanism that tells clients about the TURN server also tells them about the Iroh relay.

Again, the goal is “it just works” for users while keeping everything under the control of the same operator who runs the mail server.

## What Users See

In Delta Chat, when a user on your server starts a call:

- The app already knows the relay information from the IMAP login.
- If a relay is needed, it is used transparently.
- The user does not have to enter any extra server addresses or secrets.

This is one of the big quality-of-life improvements of running on a proper chatmail server versus a generic email server.

## Troubleshooting Calls

If voice/video calls are not working well:

- Check that the TURN-related ports are open in the firewall.
- Make sure TURN is enabled in the admin settings.
- Look at the dedicated TURN E2E tests and debug scripts that come with the project (they are very useful for operators).
- Sometimes forcing “relay only” mode in testing helps diagnose whether the problem is peer-to-peer connectivity or the relay itself.

## Summary

Chatmail tries to make real-time features (calls, WebXDC, fast transfers) work as seamlessly as the messaging itself, while keeping the same operator and the same privacy model.

You get a TURN server and Iroh relay “for free” as part of running the mail server, with automatic configuration for clients.

## Next

- Day-to-day server management: [Admin & CLI](./07-admin-and-cli.md)
- Common problems (including calls): [Troubleshooting](./10-troubleshooting.md)
