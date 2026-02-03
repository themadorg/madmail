# Iroh Relay Integration in Chatmail

This document explains how the Iroh Relay (formerly DERP) integration works in Madmail, covering discovery via IMAP, the integrated relay server, and client-side P2P coordination.

## Overview

Delta Chat uses the Iroh networking stack for high-performance, real-time Peer-to-Peer (P2P) communication, particularly for WebXDC applications. Since many users are behind NAT or firewalls that prevent direct connections, an **Iroh Relay** acts as a fallback to route encrypted traffic between peers.

The system consists of three main parts:
1.  **IMAP Metadata Extension**: Provides clients with the local Iroh Relay URL.
2.  **Integrated Iroh Relay**: A sidecar process or integrated service that relays Iroh packets.
3.  **Chatmail Core**: Handles the P2P swarm logic, joining gossip topics, and sending/receiving real-time data.

## Discovery via IMAP

Delta Chat clients discover the Iroh Relay by querying the IMAP server for specific metadata.

-   **IMAP Capability**: The server advertises the `METADATA` capability (RFC 5464).
-   **Metadata Key**: `/shared/vendor/deltachat/irohrelay`
-   **Request**: `GETMETADATA "" /shared/vendor/deltachat/irohrelay`

The server responds with the full URL of the relay, for example: `http://mail.example.org:3340`.

## Integrated Iroh Relay

Madmail bundles a specific version of `iroh-relay` (currently **v0.35.0** to match the Delta Chat core version) as an integrated sidecar.

### Protocol Support
- **iroh-relay-v1**: The relay implements the standard Iroh relay protocol. 
- **WebSocket Upgrade**: Connections are established via HTTP/HTTPS and upgraded to a WebSocket-based relay protocol using the `iroh-relay-v1` subprotocol.

### Authentication
In the current Chatmail/Madmail implementation, the relay is configured with `access = "everyone"`. This allows any Delta Chat user on the server to use the relay for P2P coordination without additional setup.

## Real-time P2P (WebXDC)

When a WebXDC application requests a real-time connection:
1.  The client fetches the Relay URL via IMAP.
2.  The client connects to the Relay and receives a long-lived connection.
3.  The client advertises its **Iroh Node ID** to other participants via standard Delta Chat messages (real-time advertisement).
4.  Peers use the advertised Node IDs and the Relay to establish a gossip swarm.
5.  Data is sent P2P where possible, or via the Relay as a fallback.

## Configuration

The Iroh Relay is enabled by default in new Madmail installations. It is configured in `maddy.conf`:

```hcl
endpoint.imap imap {
    # ... other config ...
    iroh_relay_url http://$(public_ip):3340
}
```

The relay itself is managed as a systemd service (`iroh-relay.service`) with its configuration in `/etc/maddy/iroh-relay.toml`:

```toml
enable_relay = true
http_bind_addr = "[::]:3340"
enable_stun = false
enable_metrics = false
access = "everyone"
```

> **Note**: `enable_stun` is typically set to `false` in the Iroh Relay config if the server already provides a dedicated TURN/STUN service, to avoid port conflicts.

## Testing and Verification

You can verify the Iroh integration using the E2E test suite:

- **Discovery Test**: `uv run python3 tests/deltachat-test/main.py --test-15`
- **Real-time P2P Test**: `uv run python3 tests/deltachat-test/main.py --test-16`

Server-side logs can be inspected via:
```bash
journalctl -u iroh-relay.service -f
```
