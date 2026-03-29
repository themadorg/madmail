# WebIMAP API Reference

WebIMAP provides a complete HTTP + WebSocket interface for IMAP and SMTP operations. There are two access modes:

| Mode | Best for |
|------|----------|
| **REST API** | Simple, stateless request/response (cURL, scripts, bots) |
| **WebSocket** | Persistent connections — real-time push + bidirectional commands (web apps, SDKs) |

Both modes share the same authentication, data types, and semantics.

---

## Authentication

### REST endpoints

Supply credentials as HTTP headers on **every** request:

| Header | Description |
|--------|-------------|
| `X-Email` | Full email address |
| `X-Password` | Account password |

### WebSocket

Supply credentials as **query parameters** during the upgrade:

```
wss://host/webimap/ws?email=USER&password=PASS
```

> CORS is enabled (`Access-Control-Allow-Origin: *`) on all endpoints.
> All endpoints respond to `OPTIONS` preflight requests with `204 No Content` and the headers:
> `Access-Control-Allow-Methods: GET, POST, PUT, DELETE, OPTIONS`
> `Access-Control-Allow-Headers: Content-Type, X-Email, X-Password`

---

## Data Types

### MailboxInfo

```json
{
  "name": "INBOX",
  "attributes": ["\\HasNoChildren", "\\Inbox"],
  "messages": 42,
  "unseen": 3
}
```

### Address

```json
{ "name": "Alice", "mailbox": "alice", "host": "example.com" }
```

### Envelope

```json
{
  "date": "2024-03-20T19:51:00Z",
  "subject": "Hello World",
  "from": [Address],
  "to": [Address],
  "cc": [Address],
  "message_id": "<...>",
  "in_reply_to": "<...>"
}
```

> **Note:** `bcc`, `reply_to`, and `sender` fields are not included in the Envelope. The `cc` field is omitted from JSON when empty.

### MessageSummary

```json
{
  "uid": 123,
  "seq_num": 1,
  "flags": ["\\Seen"],
  "size": 2048,
  "date": "2024-03-20T19:51:00Z",
  "envelope": Envelope
}
```

### MessageDetail

`MessageSummary` + a `body` field containing the raw RFC 822 content:

```json
{
  "uid": 123,
  "seq_num": 1,
  "flags": ["\\Seen"],
  "size": 2048,
  "date": "2024-03-20T19:51:00Z",
  "envelope": Envelope,
  "body": "From: alice@example.com\r\nTo: bob@example.com\r\n..."
}
```

---

# REST API

## Registration

### `POST /new` — Create New Account

Generates a random email address and password. Only available when registration is open.

**Response:**

```json
{
  "email": "a1b2c3d4@example.com",
  "password": "pA s$w0rd..."
}
```

---

## Mailboxes

### `GET /mailboxes` — List Mailboxes

**Response:** `Array<MailboxInfo>`

---

## Messages

### `GET /messages` — List / Long-poll Messages

| Query param | Default | Description |
|-------------|---------|-------------|
| `mailbox` | `INBOX` | Mailbox to read from |
| `since_uid` | `0` | Return messages with UID > this value |
| `wait` | `0` | Seconds to wait for new messages (max 120) |

**Response:** `Array<MessageSummary>`

### `GET /message/{uid}` — Get Full Message

| Query param | Default | Description |
|-------------|---------|-------------|
| `mailbox` | `INBOX` | Mailbox to read from |

**Response:** `MessageDetail`

### `DELETE /message/{uid}` — Delete Message

Sets `\Deleted` flag and expunges.

**Response:** `{"status": "deleted"}`

### `POST /message/flags` — Update Flags

**Body:**

```json
{
  "mailbox": "INBOX",
  "uid": 123,
  "flags": ["\\Seen", "$Label1"],
  "op": "add"
}
```

`op` values: `add`, `remove`, `set`.

**Response:** `{"status": "ok"}`

---

## Sending Email (WebSMTP)

### `POST /send` — Send Email

Also available at legacy path `/websmtp/send`.

**Body:**

```json
{
  "from": "alice@example.com",
  "to": ["bob@example.com"],
  "body": "From: alice@example.com\r\nTo: bob@example.com\r\nSubject: Test\r\n\r\nHello Bob!"
}
```

- `from` must match the authenticated `X-Email` (case-insensitive).
- `body` must be a valid RFC 5322 message (headers + CRLF + body).
- Only PGP-encrypted messages and SecureJoin handshakes are accepted.
- **Local recipients** (same domain) are delivered to their IMAP mailbox directly.
- **Remote recipients** (different domain) are routed through `target.remote` (`outbound_delivery` module), which delivers via HTTP `/mxdeliv` to the recipient's server, falling back to traditional SMTP.
- If `target.remote` is not configured, sending to external domains returns an error.

**Response:** `{"status": "sent"}`

### Delivery Architecture

```
  Client sends to: alice@local.com + bob@other.com
                          │
                    ┌─────┴─────┐
              local │           │ remote
              ▼                 ▼
   Storage.DeliveryTarget  RemoteTarget (target.remote)
         │                      │
    IMAP mailbox            HTTP POST /mxdeliv
    (direct insert)         (fallback: SMTP port 25)
```

The `mail_domain` setting determines local vs remote. If the recipient's domain matches `mail_domain`, it's local. Otherwise, it's routed through `outbound_delivery` (the `target.remote` module).

---

# WebSocket Protocol

The WebSocket endpoint at `/webimap/ws` provides a **bidirectional JSON command protocol** with automatic push notifications for new messages.

## Connection

```
wss://host/webimap/ws?email=USER&password=PASS&mailbox=INBOX&since_uid=0
```

| Query param | Default | Description |
|-------------|---------|-------------|
| `email` | *(required)* | Account email |
| `password` | *(required)* | Account password |
| `mailbox` | `INBOX` | Mailbox to watch for new messages |
| `since_uid` | `0` | Only push messages with UID > this |

## Protocol Envelope

### Client → Server (Request)

```json
{
  "req_id": "abc123",
  "action": "fetch",
  "data": { "uid": 42 }
}
```

| Field | Type | Description |
|-------|------|-------------|
| `req_id` | string | Client-generated correlation ID — echoed back in the response |
| `action` | string | Command name (see table below) |
| `data` | object | Action-specific payload |

### Server → Client (Response)

```json
{
  "req_id": "abc123",
  "action": "result",
  "data": { ... }
}
```

| Field | Value | Description |
|-------|-------|-------------|
| `req_id` | string | Echoed from the request |
| `action` | `"result"` | Success |
| `action` | `"error"` | Failure — `data` is an error message string |

### Server → Client (Push)

New message notifications have **no `req_id`**:

```json
{
  "action": "new_message",
  "data": MessageSummary
}
```

The push sends a `MessageSummary` (uid + envelope only). Use the `fetch` action to retrieve the full body.

## Keepalive

- Server sends a WebSocket **ping** frame every **30 seconds**.
- Client must respond with **pong** (or send any data) within **60 seconds** or the connection is closed.

---

## WebSocket Actions

### `send` — Send Email

Send an email through the same WebSocket connection.

```json
{
  "req_id": "s1",
  "action": "send",
  "data": {
    "from": "alice@example.com",
    "to": ["bob@example.com"],
    "body": "From: alice@example.com\r\nTo: bob@example.com\r\nSubject: Hello\r\n\r\nHi Bob!"
  }
}
```

| Field | Required | Default | Description |
|-------|----------|---------|-------------|
| `from` | no | authenticated email | Must match authenticated user |
| `to` | yes | — | Array of recipient addresses (local or external) |
| `body` | yes | — | Raw RFC 5322 message |

Recipients on the same domain are delivered locally; external recipients are routed through `target.remote` (HTTP `/mxdeliv` → SMTP fallback).

**Response:** `{"status": "sent"}`

---

### `fetch` — Get Full Message by UID

```json
{
  "req_id": "f1",
  "action": "fetch",
  "data": { "mailbox": "INBOX", "uid": 42 }
}
```

| Field | Required | Default | Description |
|-------|----------|---------|-------------|
| `mailbox` | no | `INBOX` | Mailbox name |
| `uid` | yes | — | Message UID |

**Response:** `MessageDetail`

---

### `list_mailboxes` — List All Mailboxes

```json
{
  "req_id": "lm1",
  "action": "list_mailboxes",
  "data": {}
}
```

No fields required in `data`.

**Response:** `Array<MailboxInfo>`

---

### `list_messages` — List Messages in a Mailbox

```json
{
  "req_id": "lm2",
  "action": "list_messages",
  "data": { "mailbox": "INBOX", "since_uid": 100 }
}
```

| Field | Required | Default | Description |
|-------|----------|---------|-------------|
| `mailbox` | no | `INBOX` | Mailbox name |
| `since_uid` | no | `0` | Only return messages with UID > this |

**Response:** `Array<MessageSummary>`

---

### `flags` — Update Message Flags

```json
{
  "req_id": "fl1",
  "action": "flags",
  "data": {
    "mailbox": "INBOX",
    "uid": 42,
    "flags": ["\\Seen"],
    "op": "add"
  }
}
```

| Field | Required | Default | Description |
|-------|----------|---------|-------------|
| `mailbox` | no | `INBOX` | Mailbox name |
| `uid` | yes | — | Message UID |
| `flags` | yes | — | Array of IMAP flags |
| `op` | yes | — | `"add"`, `"remove"`, or `"set"` |

**Response:** `{"status": "ok"}`

---

### `delete` — Delete a Message

Sets `\Deleted` flag and expunges.

```json
{
  "req_id": "d1",
  "action": "delete",
  "data": { "mailbox": "INBOX", "uid": 42 }
}
```

| Field | Required | Default | Description |
|-------|----------|---------|-------------|
| `mailbox` | no | `INBOX` | Mailbox name |
| `uid` | yes | — | Message UID |

**Response:** `{"status": "deleted"}`

---

### `move` — Move a Message to Another Mailbox

Copies the message to the destination mailbox, then deletes the original.

```json
{
  "req_id": "mv1",
  "action": "move",
  "data": {
    "mailbox": "INBOX",
    "dest_mailbox": "Archive",
    "uid": 42
  }
}
```

| Field | Required | Default | Description |
|-------|----------|---------|-------------|
| `mailbox` | no | `INBOX` | Source mailbox |
| `dest_mailbox` | yes | — | Destination mailbox |
| `uid` | yes | — | Message UID |

**Response:** `{"status": "moved"}`

---

### `copy` — Copy a Message to Another Mailbox

```json
{
  "req_id": "cp1",
  "action": "copy",
  "data": {
    "mailbox": "INBOX",
    "dest_mailbox": "Archive",
    "uid": 42
  }
}
```

| Field | Required | Default | Description |
|-------|----------|---------|-------------|
| `mailbox` | no | `INBOX` | Source mailbox |
| `dest_mailbox` | yes | — | Destination mailbox |
| `uid` | yes | — | Message UID |

**Response:** `{"status": "copied"}`

---

### `search` — Search Messages

Searches the envelope fields (Subject, From name, From address) for the query string. Case-insensitive.

```json
{
  "req_id": "sr1",
  "action": "search",
  "data": { "mailbox": "INBOX", "query": "meeting" }
}
```

| Field | Required | Default | Description |
|-------|----------|---------|-------------|
| `mailbox` | no | `INBOX` | Mailbox to search |
| `query` | yes | — | Search string |

**Response:** `Array<MessageSummary>` (matching messages only)

---

### `create_mailbox` — Create a Mailbox

```json
{
  "req_id": "cm1",
  "action": "create_mailbox",
  "data": { "name": "Projects" }
}
```

| Field | Required | Description |
|-------|----------|-------------|
| `name` | yes | Name of the new mailbox |

**Response:** `{"status": "created"}`

---

### `delete_mailbox` — Delete a Mailbox

```json
{
  "req_id": "dm1",
  "action": "delete_mailbox",
  "data": { "name": "OldFolder" }
}
```

| Field | Required | Description |
|-------|----------|-------------|
| `name` | yes | Name of the mailbox to delete |

**Response:** `{"status": "deleted"}`

---

### `rename_mailbox` — Rename a Mailbox

```json
{
  "req_id": "rm1",
  "action": "rename_mailbox",
  "data": { "old_name": "Projects", "new_name": "Work" }
}
```

| Field | Required | Description |
|-------|----------|-------------|
| `old_name` | yes | Current mailbox name |
| `new_name` | yes | New mailbox name |

**Response:** `{"status": "renamed"}`

---

## WebSocket Quick Reference

| Action | Direction | Description |
|--------|-----------|-------------|
| `send` | Client → Server | Send an email |
| `fetch` | Client → Server | Get full message by UID |
| `list_mailboxes` | Client → Server | List all mailboxes with counts |
| `list_messages` | Client → Server | List message summaries |
| `flags` | Client → Server | Add/remove/set IMAP flags |
| `delete` | Client → Server | Delete a message |
| `move` | Client → Server | Move message between mailboxes |
| `copy` | Client → Server | Copy message between mailboxes |
| `search` | Client → Server | Search messages by envelope fields |
| `create_mailbox` | Client → Server | Create a new mailbox |
| `delete_mailbox` | Client → Server | Delete a mailbox |
| `rename_mailbox` | Client → Server | Rename a mailbox |
| `new_message` | Server → Client | Push notification for new messages |

---

## Wire Protocol: What Messages Look Like

Every message on the WebSocket is a single JSON object. There are exactly **3 shapes**:

### 1. Success Response

Server responds to a client request with `action: "result"`:

```json
{
  "req_id": "abc123",
  "action": "result",
  "data": [
    {
      "name": "INBOX",
      "attributes": ["\\HasNoChildren", "\\Inbox"],
      "messages": 42,
      "unseen": 3
    }
  ]
}
```

### 2. Error Response

Server responds to a client request with `action: "error"`:

```json
{
  "req_id": "abc123",
  "action": "error",
  "data": "message not found"
}
```

### 3. Push Notification (no req_id)

Server pushes a new message notification — **no `req_id`**, the client did not ask for it:

```json
{
  "action": "new_message",
  "data": {
    "uid": 456,
    "seq_num": 12,
    "flags": [],
    "size": 3072,
    "date": "2026-03-24T21:30:00Z",
    "envelope": {
      "date": "2026-03-24T21:30:00Z",
      "subject": "Hello from Bob",
      "from": [{ "name": "Bob", "mailbox": "bob", "host": "example.com" }],
      "to": [{ "mailbox": "alice", "host": "example.com" }],
      "message_id": "<msg-456@example.com>"
    }
  }
}
```

> The push sends `MessageSummary` only (no body). Use `fetch` to get the full content.

---

## Full Session Example (Wire Trace)

Below is a complete session showing every message as it appears on the wire. `→` = client sends, `←` = server responds.

**1. List mailboxes**

```
→  {"req_id":"1", "action":"list_mailboxes", "data":{}}
←  {"req_id":"1", "action":"result", "data":[
      {"name":"INBOX", "attributes":["\\Inbox"], "messages":42, "unseen":3},
      {"name":"Sent",  "attributes":["\\Sent"],  "messages":15, "unseen":0},
      {"name":"Trash", "attributes":["\\Trash"], "messages":2,  "unseen":0}
    ]}
```

**2. List messages**

```
→  {"req_id":"2", "action":"list_messages", "data":{"mailbox":"INBOX", "since_uid":0}}
←  {"req_id":"2", "action":"result", "data":[
      {"uid":120, "seq_num":1, "flags":["\\Seen"], "size":1024, "date":"2026-03-24T10:00:00Z",
       "envelope":{"date":"2026-03-24T10:00:00Z", "subject":"Weekly Report",
         "from":[{"name":"Carol", "mailbox":"carol", "host":"example.com"}],
         "to":[{"mailbox":"alice", "host":"example.com"}]}},
      {"uid":123, "seq_num":2, "flags":[], "size":2048, "date":"2026-03-24T14:00:00Z",
       "envelope":{"date":"2026-03-24T14:00:00Z", "subject":"Meeting Tomorrow",
         "from":[{"name":"Bob", "mailbox":"bob", "host":"example.com"}],
         "to":[{"mailbox":"alice", "host":"example.com"}]}}
    ]}
```

**3. Fetch full message (get body)**

```
→  {"req_id":"3", "action":"fetch", "data":{"uid":123}}
←  {"req_id":"3", "action":"result", "data":{
      "uid":123, "seq_num":2, "flags":[], "size":2048,
      "date":"2026-03-24T14:00:00Z",
      "envelope":{"date":"2026-03-24T14:00:00Z", "subject":"Meeting Tomorrow",
        "from":[{"name":"Bob", "mailbox":"bob", "host":"example.com"}],
        "to":[{"mailbox":"alice", "host":"example.com"}]},
      "body":"From: Bob <bob@example.com>\r\nTo: Alice <alice@example.com>\r\nSubject: Meeting Tomorrow\r\nDate: Tue, 24 Mar 2026 14:00:00 +0000\r\n\r\nHi Alice,\r\n\r\nCan we meet tomorrow at 3pm?\r\n\r\nBob"
    }}
```

**4. New message arrives (server push — no req_id)**

```
←  {"action":"new_message", "data":{
      "uid":456, "seq_num":3, "flags":[], "size":512,
      "date":"2026-03-24T21:30:00Z",
      "envelope":{"date":"2026-03-24T21:30:00Z", "subject":"Quick question",
        "from":[{"name":"Dave", "mailbox":"dave", "host":"example.com"}],
        "to":[{"mailbox":"alice", "host":"example.com"}]}
    }}
```

**5. Client fetches the pushed message**

```
→  {"req_id":"4", "action":"fetch", "data":{"uid":456}}
←  {"req_id":"4", "action":"result", "data":{
      "uid":456, "seq_num":3, "flags":[], "size":512,
      "date":"2026-03-24T21:30:00Z",
      "envelope":{"date":"2026-03-24T21:30:00Z", "subject":"Quick question",
        "from":[{"name":"Dave", "mailbox":"dave", "host":"example.com"}],
        "to":[{"mailbox":"alice", "host":"example.com"}]},
      "body":"From: Dave <dave@example.com>\r\nTo: alice@example.com\r\nSubject: Quick question\r\n\r\nHey, are you free today?"
    }}
```

**6. Mark as read**

```
→  {"req_id":"5", "action":"flags", "data":{"uid":456, "flags":["\\Seen"], "op":"add"}}
←  {"req_id":"5", "action":"result", "data":{"status":"ok"}}
```

**7. Send a reply**

```
→  {"req_id":"6", "action":"send", "data":{
      "to":["dave@example.com"],
      "body":"From: alice@example.com\r\nTo: dave@example.com\r\nSubject: Re: Quick question\r\n\r\nYes, let's talk at 5pm!"
    }}
←  {"req_id":"6", "action":"result", "data":{"status":"sent"}}
```

**8. Move old message to archive**

```
→  {"req_id":"7", "action":"move", "data":{"uid":120, "dest_mailbox":"Archive"}}
←  {"req_id":"7", "action":"result", "data":{"status":"moved"}}
```

**9. Search for messages**

```
→  {"req_id":"8", "action":"search", "data":{"query":"meeting"}}
←  {"req_id":"8", "action":"result", "data":[
      {"uid":123, "seq_num":2, "flags":["\\Seen"], "size":2048,
       "date":"2026-03-24T14:00:00Z",
       "envelope":{"subject":"Meeting Tomorrow",
         "from":[{"name":"Bob", "mailbox":"bob", "host":"example.com"}],
         "to":[{"mailbox":"alice", "host":"example.com"}]}}
    ]}
```

**10. Error example**

```
→  {"req_id":"9", "action":"fetch", "data":{"uid":99999}}
←  {"req_id":"9", "action":"error", "data":"message not found"}
```

---

## Example: JavaScript WebSocket Client

```javascript
const ws = new WebSocket(
  `wss://${host}/webimap/ws?email=${email}&password=${password}`
);

let reqCounter = 0;
const pending = new Map();

function request(action, data) {
  return new Promise((resolve, reject) => {
    const req_id = String(++reqCounter);
    pending.set(req_id, { resolve, reject });
    ws.send(JSON.stringify({ req_id, action, data }));
  });
}

ws.onmessage = (evt) => {
  const msg = JSON.parse(evt.data);

  // Push notification (no req_id)
  if (msg.action === "new_message") {
    console.log("New message:", msg.data.uid, msg.data.envelope.subject);
    // Optionally fetch the full body:
    // request("fetch", { uid: msg.data.uid }).then(detail => ...);
    return;
  }

  // Response to a client request
  const p = pending.get(msg.req_id);
  if (p) {
    pending.delete(msg.req_id);
    msg.action === "error" ? p.reject(msg.data) : p.resolve(msg.data);
  }
};

// Usage:
ws.onopen = async () => {
  // List mailboxes
  const mailboxes = await request("list_mailboxes", {});

  // List messages
  const messages = await request("list_messages", { since_uid: 0 });

  // Fetch full message
  const detail = await request("fetch", { uid: messages[0].uid });

  // Send an email
  await request("send", {
    to: ["bob@example.com"],
    body: "From: ...\r\nTo: ...\r\nSubject: ...\r\n\r\nBody text"
  });

  // Mark as read
  await request("flags", { uid: 42, flags: ["\\Seen"], op: "add" });

  // Move to archive
  await request("move", { uid: 42, dest_mailbox: "Archive" });

  // Search
  const results = await request("search", { query: "meeting" });
};
```

---

## Authentication Details

### How Auth Works

Both REST and WebSocket use the same auth backend (`PlainUserDB.AuthPlain`):

1. Client provides `email` + `password`
2. Server calls `AuthPlain(email, password)` against the password table
3. On success, server calls `Storage.GetOrCreateIMAPAcct(email)` to get the IMAP user
4. The IMAP user session is held for the duration of the request (REST) or the WebSocket connection

### REST Auth

Credentials are sent as HTTP headers on **every request**:

```
X-Email: alice@example.com
X-Password: s3cret
```

### WebSocket Auth

Credentials are sent as **query parameters** during the WebSocket upgrade handshake:

```
wss://host/webimap/ws?email=alice@example.com&password=s3cret
```

> **Security note:** The password appears in the URL. Over `wss://` (TLS) the URL is encrypted on the wire, but it may appear in:
> - Server access logs
> - Reverse proxy logs (nginx, Caddy)
> - Browser history
>
> This is acceptable for Madmail's use case (machine-generated, disposable passwords), but be aware if deploying custom auth.

### Auth Errors

| Situation | REST response | WebSocket response |
|-----------|--------------|-------------------|
| Missing credentials | `401` with `{"error": "missing credentials"}` | `401 Unauthorized` (before upgrade) |
| Wrong password | `401` with `{"error": "authentication failed"}` | `401 Unauthorized` (before upgrade) |
| Storage error | `500` with `{"error": "storage error: ..."}` | `500 Internal Server Error` (before upgrade) |

---

## Server Configuration

WebIMAP is registered automatically by the `chatmail` endpoint module. The relevant configuration in `maddy.conf`:

```
chatmail tcp://0.0.0.0:443 {
    mail_domain   example.com
    mx_domain     mx.example.com
    web_domain    chat.example.com
    public_ip     203.0.113.10       # Important for IP-based deployments

    auth_db       local_authdb
    storage       &local_mailboxes

    tls file /path/to/cert.pem /path/to/key.pem
}
```

### Outbound Delivery (Required for External Send)

For the `send` action to deliver to **external domains**, the `outbound_delivery` module must be configured:

```
target.remote outbound_delivery {
    hostname     mx.example.com
    tls_client {
        protocols tls1.2 tls1.3
    }
}
```

At startup, WebIMAP automatically discovers the `outbound_delivery` module from the global registry. If found, external send is enabled. If not, only local delivery works.

The outbound pipeline:
1. Resolves the recipient's domain
2. Checks the endpoint cache for per-destination overrides
3. Tries HTTPS `POST /mxdeliv` to the recipient's server
4. Falls back to traditional SMTP (port 25) if HTTP fails

### Key Settings

| Setting | Description |
|---------|-------------|
| `public_ip` | The server's public IP address. Used for Shadowsocks URLs, QR codes, and client connection strings. If clients connect via IP rather than domain, set this. |
| `mail_domain` | Domain for email addresses (e.g., `example.com` → `user@example.com`). **Also determines local vs remote recipients for send.** |
| `mx_domain` | MX domain for mail delivery |
| `web_domain` | Web domain for the chat interface |
| `turn_off_tls` | Set to `true` for plain HTTP (development only) |

---

## IP-Based Deployment

Many Madmail servers run on raw IPs without a domain name. This affects WebSocket connectivity:

### Connecting via IP

```
# REST (with self-signed cert)
curl -k https://203.0.113.10/webimap/mailboxes \
  -H "X-Email: user@[203.0.113.10]" \
  -H "X-Password: secret"

# WebSocket (with self-signed cert)
wscat -n -c "wss://203.0.113.10/webimap/ws?email=user@[203.0.113.10]&password=secret"
```

### Client Configuration for IP Deployments

When the `mail_domain` is an IP address, Madmail wraps it in brackets (`[203.0.113.10]`). Email addresses look like `user@[203.0.113.10]`. Clients **must** use this exact format.

### JavaScript — Handling Self-Signed Certificates

Browsers will refuse `wss://` connections to servers with self-signed certificates. Solutions:

1. **Visit the server URL first** (`https://203.0.113.10/`) and accept the certificate exception
2. **Use a reverse proxy** (Caddy, nginx) with a valid Let's Encrypt certificate
3. **For Node.js clients**, set `NODE_TLS_REJECT_UNAUTHORIZED=0` (development only)

### Reverse Proxy (Caddy)

If you put Caddy in front with a valid domain:

```caddyfile
chat.example.com {
    reverse_proxy localhost:8443 {
        transport http {
            tls_insecure_skip_verify
        }
    }
}
```

Then clients connect to `wss://chat.example.com/webimap/ws?...` with no certificate issues.

---

## URL / Endpoint Map

WebIMAP endpoints are registered under the `/webimap` prefix. The `/new` registration endpoint is served by the parent `chatmail` module.

| Endpoint | Method | Auth | Description |
|----------|--------|------|-------------|
| `/new` | POST | None | Create new account *(chatmail, not webimap)* |
| `/webimap/mailboxes` | GET | Headers | List mailboxes |
| `/webimap/messages` | GET | Headers | List/long-poll messages |
| `/webimap/message/{uid}` | GET | Headers | Get full message |
| `/webimap/message/{uid}` | DELETE | Headers | Delete message |
| `/webimap/message/flags` | POST | Headers | Update flags |
| `/webimap/send` | POST | Headers | Send email (local + remote) |
| `/webimap/ws` | GET (Upgrade) | Query params | WebSocket bidirectional |
| `/websmtp/send` | POST | Headers | Send email (legacy path) |

---

## Error Codes

### REST API Errors

| HTTP Status | Meaning |
|-------------|---------|
| `400` | Bad request — invalid JSON, missing required fields |
| `401` | Authentication failed — wrong or missing credentials |
| `403` | Forbidden — sender doesn't match authenticated user, or PGP required |
| `404` | Not found — message UID doesn't exist |
| `405` | Method not allowed — wrong HTTP method |
| `500` | Internal server error — storage or delivery failure |

All error responses are JSON:

```json
{ "error": "descriptive error message" }
```

### WebSocket Errors

Over WebSocket, errors are returned as response messages with `action: "error"`:

```json
{
  "req_id": "abc123",
  "action": "error",
  "data": "message not found"
}
```

Common error strings:

| Error | Cause |
|-------|-------|
| `"invalid JSON"` | Client sent malformed JSON |
| `"unknown action: xxx"` | Unrecognized action name |
| `"missing recipients"` | `send` without `to` array |
| `"sender must match authenticated user"` | `from` doesn't match login email |
| `"message not found"` | `fetch` with non-existent UID |
| `"invalid op: must be add, remove, or set"` | `flags` with wrong `op` value |
| `"dest_mailbox is required"` | `move`/`copy` without destination |
| `"query is required"` | `search` without query string |
| `"Encryption Needed: ..."` | `send` with non-PGP message |
| `"local delivery not supported"` | Storage doesn't implement DeliveryTarget |
| `"local delivery failed: ..."` | Error delivering to local IMAP mailbox |
| `"remote delivery not configured"` | No `outbound_delivery` module available |
| `"remote delivery failed: ..."` | Error delivering to external domain |

---

## PGP Policy

Madmail enforces a strict PGP-only policy. The `send` action (both REST and WebSocket) rejects messages that are:

- Not PGP-encrypted
- Not a SecureJoin handshake message

This is enforced by `pgp_verify.IsAcceptedMessage()` which checks the MIME structure of the raw RFC 5322 body.

---

## Rate Limiting & Connection Limits

- **WebSocket push interval**: Server polls IMAP every **2 seconds** for new messages
- **Keepalive**: Ping every **30s**, timeout after **60s** without pong
- **Write timeout**: Each WebSocket write has a **10-second** deadline
- **Long-poll max wait**: REST `/messages?wait=N` capped at **120 seconds**
- **No concurrent write safety issue**: The server uses a mutex to serialize WebSocket writes (push + command responses are always serialized)

---

## Debugging with cURL and wscat

### REST — List mailboxes

```bash
curl -k https://SERVER/webimap/mailboxes \
  -H "X-Email: user@example.com" \
  -H "X-Password: secret"
```

### REST — Fetch a message

```bash
curl -k https://SERVER/webimap/message/42 \
  -H "X-Email: user@example.com" \
  -H "X-Password: secret"
```

### REST — Long-poll for new messages

```bash
curl -k "https://SERVER/webimap/messages?since_uid=100&wait=30" \
  -H "X-Email: user@example.com" \
  -H "X-Password: secret"
```

### WebSocket — Interactive session with wscat

```bash
# Install: npm install -g wscat
wscat -n -c "wss://SERVER/webimap/ws?email=user@example.com&password=secret"

# Once connected, type JSON commands:
> {"req_id":"1","action":"list_mailboxes","data":{}}
> {"req_id":"2","action":"list_messages","data":{"since_uid":0}}
> {"req_id":"3","action":"fetch","data":{"uid":1}}
```

### WebSocket — Python client

```python
import asyncio, json, websockets, ssl

async def main():
    ssl_ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_CLIENT)
    ssl_ctx.check_hostname = False
    ssl_ctx.verify_mode = ssl.CERT_NONE

    url = "wss://SERVER/webimap/ws?email=user@example.com&password=secret"
    async with websockets.connect(url, ssl=ssl_ctx) as ws:
        # List mailboxes
        await ws.send(json.dumps({"req_id": "1", "action": "list_mailboxes", "data": {}}))
        print(await ws.recv())

        # Listen for push notifications
        while True:
            msg = json.loads(await ws.recv())
            if msg["action"] == "new_message":
                print(f"New: UID={msg['data']['uid']} Subject={msg['data']['envelope']['subject']}")

asyncio.run(main())
```

---

## Federation & Security

### Federated Account Creation

Madmail is a federated service. To join a federated server (e.g., `example.com`), a client must first discover the **Relay IP address** for that domain.

#### Discovery & Registration Workflow

1.  **Resolve Domain**: The client resolves the target domain to its public IP address.
2.  **Registration**: `POST /new` on the relay server.
3.  **Account Provisioning**: The server returns JSON credentials.

> **Note:** Madmail servers often use direct IP addresses or self-signed certificates. Clients should skip TLS certificate verification for discovery/registration endpoints.

### Domain Exchanger & Relaying

In complex network topologies, traffic can be routed through a **Madexchanger** relay:

-   **MX-to-MX Delivery**: Relays handle the `/mxdeliv` protocol.
-   **Client Submission**: Clients always submit to their home relay; it handles onward delivery.

### TLS and Security

The WebIMAP API requires TLS. Many Madmail servers use self-signed certificates for IP-based communication. Client implementations often default to skipping certificate verification in IP-only deployments.

