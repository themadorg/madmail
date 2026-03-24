# WebIMAP & WebSMTP REST API

This API provides an HTTP-based interface for managing mailboxes, fetching messages, and sending emails. It is designed for thin clients like the Madmail Web UI.

## Authentication

All REST endpoints (except WebSocket) require authentication via HTTP headers on every request:

-   `X-Email`: The full email address of the account.
-   `X-Password`: The account password.

CORS is enabled (`Access-Control-Allow-Origin: *`) for all endpoints to support web-based clients.

---

## Registration

### Create New Account
Generates a random email address and password on the server. This is only available if registration is open in the server configuration.

*   **URL**: `/new`
*   **Method**: `POST`
*   **Response**: `AccountResponse`

```json
{
  "email": "a1b2c3d4@example.com",
  "password": "pA s$w0rd..."
}
```

---

## Mailboxes

### List Mailboxes
Returns a list of all available mailboxes and their message counts.

*   **URL**: `/mailboxes`
*   **Method**: `GET`
*   **Response**: `Array<MailboxInfo>`

```json
[
  {
    "name": "INBOX",
    "attributes": ["\\HasNoChildren", "\\Inbox"],
    "messages": 42,
    "unseen": 3
  }
]
```

---

## Messages

### List/Long-poll Messages
Retrieves a list of message summaries. Supports long-polling via the `wait` parameter.

*   **URL**: `/messages`
*   **Method**: `GET`
*   **Query Parameters**:
    *   `mailbox` (optional): Default is `INBOX`.
    *   `since_uid` (optional): Retrieve messages with UID strictly greater than this value.
    *   `wait` (optional): Number of seconds (max 120) to wait for new messages if none are currently available.
*   **Response**: `Array<MessageSummary>`

```json
[
  {
    "uid": 123,
    "seq_num": 1,
    "flags": ["\\Seen"],
    "size": 2048,
    "date": "2024-03-20T19:51:00Z",
    "envelope": {
      "date": "2024-03-20T19:51:00Z",
      "subject": "Hello World",
      "from": [{ "name": "Alice", "mailbox": "alice", "host": "example.com" }],
      "to": [{ "mailbox": "bob", "host": "example.com" }],
      "message_id": "<...>"
    }
  }
]
```

### Get Message Detail
Retrieves the full message including the raw RFC822 body.

*   **URL**: `/message/{uid}`
*   **Method**: `GET`
*   **Query Parameters**:
    *   `mailbox` (optional): Default is `INBOX`.
*   **Response**: `MessageDetail` (MessageSummary + `body` string)

### Delete Message
Marks a message as `\Deleted` and performs an `EXPUNGE`.

*   **URL**: `/message/{uid}`
*   **Method**: `DELETE`
*   **Query Parameters**:
    *   `mailbox` (optional): Default is `INBOX`.
*   **Response**: `{"status": "deleted"}`

### Update Message Flags
Adds, removes, or sets IMAP flags for a specific message.

*   **URL**: `/message/flags`
*   **Method**: `POST`
*   *   **Body**:
```json
{
  "mailbox": "INBOX",
  "uid": 123,
  "flags": ["\\Seen", "$Label1"],
  "op": "add" 
}
```
*   `op` values: `add`, `remove`, `set`.

---

## Real-time Notifications (WebSocket)

The WebSocket endpoint pushes full `MessageDetail` objects instantly as new messages arrive in the specified mailbox.

*   **URL**: `/ws`
*   **Protocol**: `ws://` or `wss://`
*   **Query Parameters** (Auth via URL):
    *   `email`: Required.
    *   `password`: Required.
    *   `mailbox` (optional): Default `INBOX`.
    *   `since_uid` (optional): Start tracking from this UID.
*   **Behavior**:
    *   Server pings the client every 30s.
    *   Client should response with pongs (or send any data) to maintain the connection.
    *   New messages are pushed as JSON `MessageDetail` objects.

---

## Sending Email (WebSMTP)

Allows sending raw RFC5322 email messages (including headers and body).

*   **URL**: `/send` (also supports legacy `/websmtp/send`)
*   **Method**: `POST`
*   **Authentication**: `X-Email` and `X-Password` headers.
*   **Request Body**:
```json
{
  "from": "alice@example.com",
  "to": ["bob@example.com", "carol@example.com"],
  "body": "From: Alice <alice@example.com>\r\nTo: Bob <bob@example.com>\r\nSubject: Test\r\n\r\nHello Bob!"
}
```
*   **Constraints**: The `from` field in the JSON MUST match the authenticated `X-Email` header (case-insensitive).

---

## Federation & Security

### Federated Account Creation

Madmail is a federated service. To join a federated server (e.g., `example.com`), a client must first discover the **Relay IP address** for that domain. This is especially important in environments where DNS or standard domains might be restricted.

#### Discovery & Registration Workflow

1.  **Resolve Domain**: The client resolves the target domain (e.g., `example.com`) to its public IPv4 or IPv6 address.
2.  **Registration**: The client performs a `POST` request to the `/new` endpoint on the relay server.
3.  **Account Provisioning**: The server returns JSON credentials (email and password) for a new, transient account.

> [!IMPORTANT]
> Because Madmail servers often use direct IP addresses or self-signed certificates for rapid deployment, clients should typically skip TLS certificate verification when communicating with the discovery/registration endpoints.

### Domain Exchanger & Relaying

In complex network topologies, traffic can be routed through a **Madexchanger** relay. This allows a server to act as a transparent proxy for another Madmail instance, often used to bypass firewalls or optimize delivery.

-   **MX-to-MX Delivery**: Relays handle the `/mxdeliv` protocol.
-   **Client Submission**: Clients always submit to their home relay, which then handles delivery (either directly or via another relay).

### TLS and Security

The WebIMAP API requires TLS. However, many Madmail servers use self-signed certificates for IP-based communication. Client implementations (like the Madmail Web UI) often default to skipping certificate verification to ensure connectivity in IP-only deployments.
