# Email for AI agents

Madmail can give an agent its own mailbox on a domain you control. Existing
WebSMTP and WebIMAP endpoints let a client send and receive PGP-encrypted mail
over HTTPS, using ordinary mailbox credentials. No agent-specific server or
admin API access is needed.

This is encrypted chatmail, not an unrestricted plaintext email service. Your
client must construct and decrypt OpenPGP messages and manage recipient keys.
For a full messaging client, a chatmail-core-based tool such as
[Sigma](https://github.com/omidz4t/sigma) can handle more of that protocol work.
The REST example below demonstrates the underlying mail APIs independently.

## Install for agents on Linux

For a **fresh Linux x86_64 server with systemd and glibc 2.38 or newer**, this
one-line recipe downloads the latest stable release, installs Madmail with a
domain and ACME TLS, enables WebSMTP and WebIMAP, and enables/starts the service.
It uses existing installer options and CLI settings; no config-file editing or
new installer flag is required. Other defaults, including encrypted-submission
policy, stay the same. The two APIs are enabled server-wide, not per account.

Before running it:

- Use a fresh server and an empty working directory. Do not run full installation
  over an existing deployment: it can overwrite configuration. Existing operators
  should use the API enable commands in step 1 instead.
- Set up your domain's DNS and allow the required ports using the
  [installation guide](../../guide/install.md). ACME needs inbound TCP 80;
  the server also needs outbound access for downloads and certificate issuance.
- Have `curl`, a trusted CA store, and `sudo` installed. Replace
  `agents.example.org` and `admin@example.org` in the command with your domain
  and ACME contact email. No passwords or private keys belong in this command.
- For older glibc systems such as Debian 12, use `madmail-linux-amd64-legacy`
  instead of `madmail-linux-amd64`. For other architectures and Windows, follow
  [Quick Start](02-quick-start.md) and then enable the APIs in step 1.

```bash
curl -fsSL https://github.com/themadorg/madmail/releases/latest/download/madmail-linux-amd64 -o madmail && chmod +x madmail && sudo ./madmail install --simple --domain agents.example.org --acme-email admin@example.org --lang en && sudo /usr/local/bin/madmail --config /etc/madmail/madmail.conf --state-dir /var/lib/madmail websmtp enable && sudo /usr/local/bin/madmail --config /etc/madmail/madmail.conf --state-dir /var/lib/madmail webimap enable && sudo systemctl enable --now madmail
```

The `&&` chain stops at the first failure; it does not roll back completed steps.
If it fails, fix the reported problem before continuing with the remaining CLI
steps; do not blindly repeat full installation. Installation does not create an
agent mailbox or encryption key. Continue below to create a dedicated account,
keep its credentials outside the model, and verify encrypted mail delivery.

## 1. Prepare the server and account

Start with a [standard deployment](02-quick-start.md), a domain you control,
and a valid, trusted HTTPS certificate. See the [DNS guide](12-dns-mail-auth.md)
for mail exchanged with other servers. This walkthrough uses
`https://agents.example.org` and `planner01@agents.example.org`; replace them
with your own domain and account.

As the operator, create a dedicated account on the server:

```bash
sudo madmail accounts create planner01@agents.example.org
```

Enter a unique mailbox password in a private terminal. The current CLI can echo
the entered password and prints a login link containing it: treat the entire
terminal session and its output as secrets; do not record or share them. Store the
password in a secret manager or a restricted credential file, separate from
model prompts, conversation history, source control, and logs. Do not put it
in command-line arguments. See [account creation](../../guide/cli/accounts-create.md).
Existing accounts do not require opening public registration.

Enable both APIs on the server:

```bash
sudo madmail websmtp enable
sudo madmail webimap enable
sudo madmail websmtp status
sudo madmail webimap status
```

Both APIs are disabled by default. These are server-wide settings, effective
on the next HTTP request without a restart or reload. If file policy protects
a setting, follow the [configuration guide](../../guide/configuration.md).
See the [WebSMTP](../../guide/cli/websmtp.md) and
[WebIMAP](../../guide/cli/webimap.md) command references.
A backend agent client does not need browser CORS configuration.

Use HTTPS for mailbox credentials even if public HTTP is available for other
purposes. Do not disable certificate verification or follow redirects with
authentication headers. The mailbox password is not the admin token; never
give an agent the server's admin token, server data directory, or another
account's credentials.

## 2. Prepare the client's encryption key

On the client machine, install Python 3 and GnuPG using your operating system's
package manager. The example uses only Python's standard library and the local
`gpg` program; it installs no Python packages.

Create an encryption key for this account interactively:

```bash
gpg --quick-generate-key 'planner01@agents.example.org' rsa3072 encr 1y
gpg --list-keys --with-fingerprint planner01@agents.example.org
```

Protect the private key with a passphrase, enter it through GnuPG's local
pinentry, and copy the full fingerprint of this account's key for the next step.
Do not give the model the private key or its passphrase. Keep a protected backup
and plan for expiry, revocation, and recovery. For real conversations, verify
recipients' keys through a trusted channel; this self-test encrypts only to your
own key and does not exchange keys with other users.

Madmail enforces encrypted submission; HTTPS alone does not replace OpenPGP.
See [the privacy model](04-privacy-and-security.md) and
[WebIMAP's API design](../../TDD/10-webimap.md).

## 3. Verify encrypted send and receive

Save the following as `agent_mailbox_smoke.py` on the client, then run it from
an interactive terminal. It prompts for the mailbox password, reads an INBOX
cursor, shows a harmless test message for approval, encrypts it to your own key,
sends it to your own mailbox, retrieves that specific message, and decrypts it
locally. It does not print other inbox messages or mark them for deletion.

```python
#!/usr/bin/env python3
"""Operator-run encrypted WebSMTP/WebIMAP self-test (Python standard library + GnuPG)."""
import getpass
import json
import re
import ssl
import subprocess
import sys
import time
import uuid
from email import policy
from email.message import EmailMessage
from email.parser import Parser
from email.utils import formatdate, make_msgid
from urllib import error, parse, request

if len(sys.argv) != 4:
    raise SystemExit("Usage: python3 agent_mailbox_smoke.py HTTPS_ORIGIN EMAIL KEY_FINGERPRINT")
origin, address, fingerprint = sys.argv[1:]
url = parse.urlsplit(origin)
if (url.scheme != "https" or not url.hostname or url.username or url.password
        or url.query or url.fragment or url.path not in ("", "/")):
    raise SystemExit("Use a trusted HTTPS origin, without credentials, query, or path.")
if not re.fullmatch(r"(?:[0-9A-Fa-f]{40}|[0-9A-Fa-f]{64})", fingerprint):
    raise SystemExit("Use the full fingerprint of your own verified encryption key.")
origin = origin.rstrip("/")
password = getpass.getpass("Mailbox password (not the admin token): ")

class NoRedirect(request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None  # Never forward mailbox credentials to another URL.

client = request.build_opener(
    NoRedirect(), request.HTTPSHandler(context=ssl.create_default_context()))

def api(path, payload=None):
    headers = {"X-Email": address, "X-Password": password, "Accept": "application/json"}
    data = None
    if payload is not None:
        headers["Content-Type"] = "application/json"
        data = json.dumps(payload).encode("utf-8")
    try:
        with client.open(request.Request(origin + path, data=data, headers=headers), timeout=20) as response:
            raw = response.read(2 * 1024 * 1024 + 1)
    except error.HTTPError as exc:
        raise SystemExit(f"Mail API returned HTTP {exc.code}; see the troubleshooting table.") from None
    except error.URLError:
        raise SystemExit("HTTPS failed; check DNS, network access and certificate trust.") from None
    if len(raw) > 2 * 1024 * 1024:
        raise SystemExit("Response too large for this smoke test.")
    return json.loads(raw)

existing = api("/webimap/messages?mailbox=INBOX")
since = max((item["uid"] for item in existing), default=0)
plaintext = ("Madmail agent mailbox self-test: " + str(uuid.uuid4()) + "\n").encode()
subject = "Madmail agent mailbox self-test"
print(f"Send to: {address}\nSubject: {subject}\nContent: {plaintext.decode()}")
if input("Operator approval: type SEND to send this test message: ") != "SEND":
    raise SystemExit("Cancelled; no message was sent.")

inner = EmailMessage(policy=policy.SMTP)
inner.set_content(plaintext.decode())
cleartext_mime = inner.as_bytes()
ciphertext = subprocess.run(
    ["gpg", "--batch", "--armor", "--encrypt", "--recipient", fingerprint],
    input=cleartext_mime, capture_output=True, check=True).stdout.decode("ascii")
message = EmailMessage(policy=policy.SMTP)
message["From"] = address
message["To"] = address
message["Date"] = formatdate(localtime=False)
message["Subject"] = subject
message["Message-ID"] = make_msgid(domain=address.rsplit("@", 1)[-1])
message.set_type("multipart/encrypted")
message.set_param("protocol", "application/pgp-encrypted")
version = EmailMessage(policy=policy.SMTP)
version["Content-Type"] = "application/pgp-encrypted"
version.set_payload("Version: 1\r\n")
encrypted = EmailMessage(policy=policy.SMTP)
encrypted["Content-Type"] = "application/octet-stream"
encrypted.set_payload(ciphertext)
message.attach(version)
message.attach(encrypted)
result = api("/websmtp/send", {"from": address, "to": [address], "body": message.as_string()})
if result.get("status") != "sent":
    raise SystemExit("Unexpected send response; inspect the server without logging credentials.")

wanted_id = str(message["Message-ID"]).strip("<>")
for attempt in range(12):
    summaries = api(f"/webimap/messages?mailbox=INBOX&since_uid={since}&wait=5")
    for summary in summaries:
        if summary.get("envelope", {}).get("message_id", "").strip("<>") != wanted_id:
            continue
        uid = summary["uid"]
        if type(uid) is not int or not 0 < uid < 2**32:
            raise SystemExit("Invalid UID in the server response.")
        detail = api(f"/webimap/message/{uid}?mailbox=INBOX")
        fetched = Parser(policy=policy.default).parsestr(detail["body"])
        parts = list(fetched.iter_parts())
        if fetched.get_content_type() != "multipart/encrypted" or len(parts) != 2:
            raise SystemExit("The received message is not the expected PGP/MIME message.")
        recovered = subprocess.run(
            ["gpg", "--batch", "--decrypt"], input=parts[1].get_payload(decode=True),
            capture_output=True, check=True).stdout
        if recovered != cleartext_mime:
            raise SystemExit("Decrypted content did not match the approved test message.")
        print(f"PASS: encrypted send, receive and local decryption verified (UID {uid}).")
        raise SystemExit(0)
    time.sleep(1)
raise SystemExit("Message not found within the test window; do not resend automatically.")
```

Run it with your own origin, address, and full key fingerprint:

```bash
python3 agent_mailbox_smoke.py https://agents.example.org planner01@agents.example.org YOUR_FULL_KEY_FINGERPRINT
```

Enter the mailbox password and type `SEND` only after reviewing the displayed
recipient and content. GnuPG may need its local pinentry or an already unlocked
key to decrypt. A successful run prints `PASS: encrypted send, receive and local
decryption verified` with the received UID. This verifies delivery within the
local server; test an approved exchange with another server separately before
relying on federation.

The script is an operator-run smoke test, not a production agent loop. It leaves
one encrypted test message in INBOX. Its response-size limit is 2 MiB, and its
first request lists the whole INBOX, so run it on a new or nearly empty
mailbox; a large INBOX makes it stop before sending. A timeout
or unexpected response after sending is ambiguous: check the mailbox before
retrying, because the server may already have accepted the message.

## API contract for an integration

Authenticate REST requests with `X-Email` and `X-Password` headers over HTTPS.
The response bodies below are direct JSON values, without a `data` envelope.

| Operation | Request | Response |
| --- | --- | --- |
| Send | `POST /websmtp/send` with JSON `from`, `to` (array), and `body` (complete PGP/MIME RFC5322 message string) | `{"status":"sent"}` when accepted |
| List | `GET /webimap/messages?mailbox=INBOX&since_uid=123&wait=5` | Array of summaries including `uid` and `envelope.message_id` |
| Retrieve | `GET /webimap/message/124?mailbox=INBOX` | Message details; `body` is the complete raw message string |

The authenticated account must match the message's sender. The subject and
routing headers in this example are visible to the server; the content is
OpenPGP-encrypted. Incoming `body` is not automatically decrypted for the model.
Parse MIME and decrypt through a trusted local component first. Keep cursors per
account and mailbox. Acceptance is not proof of external delivery; use bounded
retries and deduplication rather than sending again on every timeout.

## Keep the agent's authority outside email

Treat every incoming subject, header, body, attachment, and link as untrusted
input, including after successful decryption. A sender may embed instructions
to disclose secrets, change destinations, run commands, or override policy.
Encryption and a valid signature do not make those instructions authoritative.

For a production agent:

- Use a dedicated account, restricted runtime, recipient allowlist, and sending
  limits. Retrieve credentials through a trusted component; keep them out of the
  model context. Avoid logging authentication headers or decrypted mail.
- Give the model the minimum parsed content needed for its task. Do not execute
  attachments, automatically open links, or accept policy changes from email.
- Require human approval before sensitive or external sends. Show the exact
  recipients, plaintext, and attachments through a trusted interface. Bind a
  single-use approval to that immutable draft; a changed draft needs new approval.
- Enforce that approval in the host application or mail broker. Neither the
  model nor an incoming message can approve a send, alter the allowlist, or read
  secrets. Fail closed if approval is missing or expired.

The smoke test's terminal prompt illustrates review by an operator. It is not an
authorization boundary if an autonomous process can control its stdin or edit
the script. Build that separation before connecting an unattended agent.

## Example: a shared agent community

A Sigma House-style group could operate `house.example.org`, with separate
mailboxes such as `planner01@house.example.org`, `research01@house.example.org`,
and `reviewer01@house.example.org`. Each agent has its own password, encryption
key, and sending policy; the operator keeps administration separate. Members
can exchange encrypted messages across independent Madmail or compatible
chatmail servers, subject to their operators' policies.

This is an illustrative deployment, not a claim that Sigma or a particular
community uses or endorses Madmail. Sigma is a separate chatmail-core-based
client; using it does not require reimplementing its client protocol with the
REST example above.

## Troubleshooting

| Result | Check |
| --- | --- |
| HTTP 404 | Enable the API needed by that request on the correct server. |
| HTTP 401 | Verify the mailbox address and password, not an admin token. |
| HTTP 400 | Check the API error locally for malformed mail, a `From` header that does not match the authenticated account, encryption, or federation-policy rejection; do not publish credentials or private content. |
| HTTP 403 | Check sender authorization and account blocking. |
| HTTP 413 | Check message-size and account-quota limits. |
| HTTPS failure | Check DNS, certificate trust, and network reachability; keep certificate verification enabled. |
| Accepted but not received | Inspect delivery and recipient reachability; do not assume a successful POST proves delivery or resend blindly. |
| GnuPG failure | Check the full fingerprint, encryption capability, expiry, and local private-key access. |
