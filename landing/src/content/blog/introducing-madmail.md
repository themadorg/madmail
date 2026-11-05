---
title: "Meet Madmail: a Chatmail relay in one Rust binary"
date: 2026-10-12
author: KamyarAT
language: en
translation: introducing-madmail
description: "A Chatmail relay you run yourself, with SMTP, IMAP, federation, and administration in one Rust binary."
---

If you use [Delta Chat](https://delta.chat/), the app runs on your phone or computer, but something still has to deliver the messages. That's the part [Madmail](/) handles.

Madmail is a Chatmail relay you can run on your own server. It accepts encrypted messages, stores them for delivery, and passes them between clients and other relays. SMTP, IMAP, web endpoints and administration are all part of the same Rust binary.

The current v2 codebase is a rewrite of our earlier implementation. It's open source under AGPL-3.0-or-later, with [the code and documentation on GitHub](https://github.com/themadorg/madmail).

## How it fits into Delta Chat

Delta Chat uses mail protocols underneath a familiar chat interface. Clients send messages using SMTP submission and retrieve them using IMAP. The app handles encryption on the device; the relay handles transport and temporary storage.

A Chatmail relay is built around that use case. It enforces an encrypted-message policy, rather than serving as a regular mailbox for newsletters, password resets and work email.

The mail transport also lets independently operated relays exchange messages. You can run a relay for your community, and people using it can still talk to people on other relays. They don't all have to move to the same server.

Madmail supports Chatmail's HTTP federation endpoint, `/mxdeliv`, with SMTP fallback where applicable. Operators can also restrict federation if their deployment calls for it.

## What running Madmail looks like

For operators, the useful part of the single binary is having fewer services to install and configure. Madmail includes the SMTP and IMAP servers, registration and federation endpoints, certificate management, quotas, and administration through a CLI and web interface. It also includes TURN and Iroh services for supported calling and connectivity features.

You can run it from a Linux release binary or in Docker, and use SQLite or PostgreSQL for the database. Deployments can use a domain name or, in supported configurations, a public IP address with an automatically managed TLS certificate.

Registration policies, storage quotas and federation controls let you decide who can use the relay and how much of its resources they can use. Connection and throughput statistics help you see what's happening once it's running.

There's still a server to look after. DNS, firewall rules, delivery paths, backups and updates need attention. Putting the services in one process makes installation simpler, but those jobs remain part of running a relay.

## What the server can see

Delta Chat encrypts messages before sending them. Madmail transports and stores those encrypted messages; the relay doesn't have the keys to read their contents. Your chat history and contacts live on your devices.

Some information is still visible to the server, including sender and recipient addresses and message sizes. Chatmail commonly uses randomly generated addresses, but encryption doesn't hide everything needed to route a message. Logging and retention settings matter too, so they're part of the [operator documentation](/docs).

## When the usual mail ports are blocked

Some networks won't let clients reach IMAP on port 993 or SMTP submission on port 465. A working relay is little help if the app can't connect to it.

Madmail can also accept both protocols on port 443, sharing a TLS listener with the website. That gives compatible clients another endpoint to try when the usual mail ports are unavailable.

Port 443 won't get through every kind of filtering. It can help when the restriction is based on the port number. We'll cover how the listener tells web and mail connections apart in a [separate technical post](/blog/imap-smtp-port-443).

## Try it

For a Linux server with a domain pointing to its public IP, start by downloading the release binary:

```bash
ARCH=$(uname -m | sed 's/x86_64/amd64/;s/aarch64/arm64/')
curl -fsSL "https://github.com/themadorg/madmail/releases/latest/download/madmail-linux-${ARCH}" -o madmail
chmod +x madmail
```

Then install and start the service, replacing the hostname and email address with your own:

```bash
sudo ./madmail install --simple --domain mail.example.org \
  --acme-email you@example.com --lang en
sudo systemctl enable --now madmail
```

The hostname needs to resolve to the machine, and the required ports need to be reachable. The [quick-start guide](/docs/quick-setup) covers the setup in more detail, including other deployment options. For delivery between servers, follow the [DNS and federation guide](/docs/project/user-guide/12-dns-mail-auth) as well.

Madmail is under active development. If you try it, tell us where you get stuck. Installation problems, confusing documentation and bugs are all useful reports, and you can open them in [GitHub issues](https://github.com/themadorg/madmail/issues).
