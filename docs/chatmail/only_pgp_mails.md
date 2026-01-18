# PGP-Only Email Policy

Madmail/Chatmail enforces a "PGP-Only" policy to ensure that all communications are secure and private by default. This document describes which email requests are accepted and which are rejected by the server.

## Overview

The server validates every message passing through it, whether it is submitted via **SMTP** or uploaded via **IMAP APPEND**. The default rule is that only properly encrypted **PGP/MIME** messages are allowed.

## Accepted Requests

The following types of messages are accepted by the server:

### 1. PGP/MIME Encrypted Messages
Messages must strictly follow the [RFC 3156](https://tools.ietf.org/html/rfc3156) standard for PGP/MIME encryption:
- **Content-Type**: `multipart/encrypted; protocol="application/pgp-encrypted"`
- **Structure**:
  - The first part must be `application/pgp-encrypted` with the content `Version: 1`.
  - The second part must be `application/octet-stream` containing valid OpenPGP data.

#### The Verification Algorithm
The server performs a deep inspection of the OpenPGP payload without decrypting it:
- **Armor Handling**: If ASCII armored, the server extracts the Base64 content, stripping the `-----BEGIN PGP MESSAGE-----` header, optional PGP headers, and the **CRC24 checksum** (lines starting with `=`).
- **Packet Format**: Only **New Format** packets (as per RFC 4880, indicated by bits `0xC0`) are accepted.
- **Length Encoding**: The algorithm correctly handles:
  - One-octet, two-octet, and five-octet length encodings.
  - **Partial Body Lengths**: It iteratively processes partial lengths (224-254) to accurately calculate packet boundaries.
- **Packet Sequence**:
  - Validates that the payload consists of one or more **PKESK** (Public-Key Encrypted Session Key, Type 1) or **SKESK** (Symmetric-Key Encrypted Session Key, Type 3) packets.
  - The sequence **must** terminate with exactly one **SEIPD** (Symmetrically Encrypted and Integrity Protected Data, Type 18) packet.

### 2. Secure Join Handshake
To allow users to establish a verified connection (bootstrapping trust), initial Secure Join requests are accepted unencrypted:
- **Headers**: Contains `Secure-Join: vc-request` or `Secure-Join: vg-request`.
- **Body**: Equals `secure-join: vc-request` or `secure-join: vg-request` (case-insensitive).

### 3. Automated System Messages (Bounces)
Certain automated messages are allowed for system health:
- **Sender**: Envelope from must be `mailer-daemon@`.
- **Content-Type**: `multipart/report`.
- **Auto-Submitted**: Must be present and not set to `no`.

### 4. Whitelisted Passthroughs
Administrators can configure specific exceptions:
- **Passthrough Senders**: Configured via `passthrough_senders`.
- **Passthrough Recipients**: Configured via `passthrough_recipients` (supports full addresses or domain-wide `@example.com`).

---

## Rejected Requests

Any message that does not meet the criteria above will be rejected:

### 1. Unencrypted Plain Text
Standard unencrypted emails are rejected with error code **523**.
- **SMTP Error**: `523 Encryption Needed: Invalid Unencrypted Mail`
- **IMAP Error**: `Encryption Needed: Invalid Unencrypted Mail`

### 2. Invalid PGP Structure
- Missing `Version: 1` in the first part of `multipart/encrypted`.
- Unexpected packet types (e.g., plain literal data packets or signatures without encryption).
- Corrupted length encodings or invalid Base64.
- More than two parts in the MIME structure.

### 3. Header Mismatches
To prevent spoofing:
- The **MIME From** address must exactly match the **Envelope Sender** (MAIL FROM).
- Failure results in: `554 From header does not match envelope sender`.

### 4. Invalid Recipient Format
Malformed addresses are rejected with code **554** before encryption validation.

## Implementation Details

The logic for these checks is implemented in the following components:
- `internal/check/pgp_encryption/`: The SMTP check module.
- `internal/pgp_verify/`: Core PGP and Secure Join validation logic.
- `internal/endpoint/imap/`: The IMAP wrapper that enforces these checks on `APPEND` commands.
- `internal/endpoint/chatmail/`: The HTTP delivery endpoint (MX-Deliv).
