# Sending, Receiving, and Federation

This guide explains what actually happens when someone sends a message on a chatmail server — both when the recipient is on the same server and when they are on a different one.

## Local Messages (Same Server)

When Alice and Bob are both on your server:

1. Alice’s Delta Chat app (or other email client) connects to the server using IMAP and SMTP (or the WebIMAP path).
2. She composes a message. Because Delta Chat uses Autocrypt + PGP, the message is encrypted to Bob’s public key before it ever leaves her device.
3. The message is submitted to the server (usually over the submission port 465 or 587 with authentication).
4. The server performs its checks:
   - Is the sender allowed to send?
   - Is the message properly encrypted? (The PGP-only rule)
   - Is anyone over quota?
5. If everything passes, the server writes the encrypted message into Bob’s Maildir on disk.
6. If Bob is currently connected via IMAP and using IDLE, he gets a push notification almost instantly that new mail has arrived.

From the users’ point of view it feels like a normal modern chat app. The server is mostly just moving encrypted blobs around and notifying people.

## Messages to Other Chatmail Servers (Federation)

When Alice (on your server) sends to Carol who is on `carol@another.chatmail.example.net`, the process is a bit more interesting.

### Preferred Path: HTTP Federation (`/mxdeliv`)

Modern chatmail servers prefer a fast, modern method:

- Alice’s server makes an HTTPS `POST /mxdeliv` request directly to Carol’s server.
- The body of the request contains the already-encrypted message.
- A few headers tell the receiving server who the message is from (`X-Mail-From`) and who it is for.
- Carol’s server runs the same checks (PGP-only, policy, quota, blocklist).
- If accepted, the message is stored in Carol’s mailbox on her server.

This path is usually much faster and more reliable than traditional email routing.

### Fallback: Regular Email (SMTP)

If the direct HTTPS method doesn’t work (the other server is old, temporarily down, misconfigured, etc.), the sending server will fall back to normal email delivery:

- It looks up the MX records for the destination domain.
- It tries to deliver the message over SMTP (port 25), just like any other email server would.

This fallback means that chatmail users can still reach people on almost any email address in the world, while getting the better experience when both sides are proper chatmail servers.

### Why the Two Paths?

The design is pragmatic:

- Direct HTTPS federation between chatmail servers = fast, reliable, good metadata protection.
- Classic SMTP fallback = maximum compatibility with the rest of the email world.

Users and operators usually don’t have to think about which path is being used. The servers negotiate it automatically.

## What the Receiving Server Does

When any message arrives (local or federated):

1. The PGP-only gate runs again on the receiving side (defense in depth).
2. Policy and blocklist checks are performed.
3. Quota is checked.
4. The encrypted message is written to the recipient’s Maildir.
5. Any connected IMAP clients are notified (IDLE push).

Even when a message comes from another chatmail server, the receiving server still enforces its own rules. This is important for security and spam control.

## Federation Health and the Admin View

Operators can see useful information in the admin interface (or via CLI):

- Which other servers their server has recently talked to
- Success rate and average latency for deliveries to each peer
- Current size of the outbound retry queue

This helps spot when another server is having problems or when there is a routing issue.

If deliveries to a particular domain keep failing, the admin can sometimes add a DNS override or investigate the other server’s configuration.

## What Users Experience

For normal users on Delta Chat the experience is seamless:

- They just type an address and send.
- If the recipient is on another chatmail server, the message usually arrives very quickly.
- If the recipient is on a regular email provider, it still works (via the SMTP fallback).
- Delivery failures are reported back in a way Delta Chat can show nicely to the user.

The complexity of federation is deliberately hidden from the end user.

## Important Privacy Points

- Messages are encrypted on the sender’s device before they ever touch the network.
- The sending server cannot read the content (it only sees encrypted data).
- The receiving server also cannot read the content (same reason).
- Federation metadata (which servers talked to which) is much lower than traditional email in the preferred HTTPS path.

This is why many people consider chatmail federation more private than routing everything through big email providers.

## Common Operator Questions

**“Can I force all mail to another specific server to use the direct HTTPS path?”**

In many cases the server will prefer it automatically once it has successfully used the direct path once. There are also admin tools (DNS overrides, endpoint cache) for special cases.

**“What if the other server is not a chatmail server?”**

It will usually fall back to normal SMTP. Delivery may be slower and you lose some of the nice properties, but it still works for basic email.

**“Do I need special firewall rules for federation?”**

For the best experience you want outbound HTTPS (443) to be reliable. The SMTP fallback uses port 25 outbound. Most servers already allow this.

## Summary

Sending and receiving on a chatmail server works like this:

- Local messages = fast and simple (encrypted blob written to disk + push).
- Messages to other chatmail servers = preferred fast HTTPS federation with classic email as a reliable fallback.
- The strong encryption rules are enforced on both ends.
- Most of the complexity is hidden from users.

This hybrid approach gives you both excellent performance inside the chatmail network and broad compatibility with the rest of the world.

## Next

- How the server helps with voice and video calls: [Calls and Real-Time Features](./06-calls-and-real-time.md)
- Managing the server as an operator: [Admin & CLI](./07-admin-and-cli.md)
- What to do when messages aren’t arriving: [Troubleshooting](./10-troubleshooting.md)
