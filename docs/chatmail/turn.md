# TURN Server Integration in Chatmail

This document explains how the TURN (Traversal Using Relays around NAT) server integration works in Madmail, covering discovery via IMAP, token-based authentication, and the internal TURN server implementation.

## Overview

Delta Chat uses WebRTC for audio and video calls. Since many users are behind NAT or firewalls, a TURN server is often required to relay media traffic. Madmail provides an integrated TURN server and a discovery mechanism for clients.

The system consists of three main parts:
1.  **IMAP Metadata Extension**: Provides clients with TURN server details and temporary credentials.
2.  **TURN Endpoint**: An integrated TURN server that validates credentials using a shared secret.
3.  **Chatmail Core**: Handles the client-side logic for parsing ICE server information and resolving hostnames.

## Discovery via IMAP

Delta Chat clients discover the TURN server by querying the IMAP server for specific metadata.

-   **IMAP Capability**: The server advertises the `METADATA` capability (RFC 5464).
-   **Metadata Key**: `/shared/vendor/deltachat/turn`
-   **Request**: `GETMETADATA "" /shared/vendor/deltachat/turn`

### Temporary Credential Generation

When a client requests the TURN metadata, the IMAP server generates temporary credentials using a **Shared Secret** mechanism:

1.  **Username**: A Unix timestamp indicating the expiration time (e.g., `current_time + 24h`).
2.  **Password**: An HMAC-SHA1 signature of the username, calculated using the `turn_secret`.
3.  **Output Format**: `hostname:port:username:password`

This allows the server to issue credentials that are valid for a limited time without storing them in a database.

## Integrated TURN Server

The TURN server is implemented in `internal/endpoint/turn/turn.go` using the `pion/turn` library.

### Authentication Flow

When a client connects to the TURN server:
1.  The client provides the `username` (expiration timestamp) and the `password` (HMAC signature).
2.  The TURN server checks if the `realm` matches.
3.  The TURN server re-calculates the HMAC-SHA1 of the provided `username` using its own copy of the `turn_secret`.
4.  If the calculated signature matches the provided `password`, the authentication is successful.

### Relay Allocation
The server uses a `MinimalRelayGenerator` to allocate relay addresses. It supports both **UDP** and **TCP** listeners. The `relay_ip` configuration determines the IP address that the TURN server tells clients to use for relaying.

## Client Core Logic (Rust)

The Rust core (`chatmail-core/src/calls.rs`) handles the ICE server list management:

-   **Parsing**: It parses the metadata string received from IMAP.
-   **Resolution**: It resolves TURN/STUN hostnames to IP addresses. This is critical for Delta Chat Desktop, which may operate in environments where DNS resolution is unreliable or unavailable to the application.
-   **Fallback**: If no TURN server is provided by the IMAP metadata, it uses hardcoded fallback servers (e.g., `turn.delta.chat`).

## Configuration

In `maddy.conf` (or equivalent configuration), the TURN and TURNS integration is configured as follows:

```hcl
endpoint.imap imap {
    # ... other config ...
    turn_enable yes
    turn_server turn.example.com
    turn_port 443
    turn_secret "your-shared-secret"
    turn_ttl 86400
    turn_prefer_tls yes  # Advertises TURNS as default
}

endpoint.turn turn {
    realm example.com
    secret "your-shared-secret"
    relay_ip 1.2.3.4
    
    # Optional TLS for TURNS
    tls {
        cert /path/to/cert.pem
        key /path/to/key.pem
    }
}
```

### Protocol Support

-   **TURN**: Standard TURN over UDP/TCP (usually port 3478).
-   **TURNS**: TURN over TLS (usually port 443 or 5349). To enable TURNS, use the `tls://` prefix in the `turn` endpoint addresses and provide a `tls` block.

The IMAP metadata now supports both `/shared/vendor/deltachat/turn` and `/shared/vendor/deltachat/turns` keys for client discovery.
