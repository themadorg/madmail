# PGP-Only Email Policy

Madmail/Chatmail enforces a "PGP-Only" policy to ensure that all communications are secure and private by default. This document describes which email requests are accepted and which are rejected by the server.

## Overview

The server validates every message passing through it, whether it is submitted via **SMTP** or uploaded via **IMAP APPEND**. The default rule is that only properly encrypted **PGP/MIME** messages are allowed.

## Accepted Requests

The following types of messages are accepted by the server:

### 1. PGP/MIME Encrypted Messages
Messages that follow the [RFC 3156](https://tools.ietf.org/html/rfc3156) standard for PGP/MIME encryption:
- **Content-Type**: `multipart/encrypted; protocol="application/pgp-encrypted"`
- **Structure**:
  - The first part must be `application/pgp-encrypted` with the content `Version: 1`.
  - The second part must be `application/octet-stream` containing a valid OpenPGP "Symmetrically Encrypted and Integrity Protected Data Packet" (SEIDP).

### 2. Secure Join Handshake
To allow users to establish a verified connection (bootstrapping trust), initial Secure Join requests are accepted unencrypted:
- **Headers**: Contains `Secure-Join: vc-request` or `Secure-Join: vg-request`.
- **Body**: Equals `secure-join: vc-request` or `secure-join: vg-request` (case-insensitive).

### 3. Automated System Messages (Bounces)
Certain automated messages are allowed to ensure the mail system remains functional:
- **Sender**: Must be from `mailer-daemon@`.
- **Content-Type**: `multipart/report` (typically used for Delivery Status Notifications).
- **Auto-Submitted**: Must have the `Auto-Submitted` header set (and not to `no`).

### 4. Whitelisted Passthroughs
Administrators can configure specific exceptions:
- **Passthrough Senders**: Emails from specific addresses listed in the configuration.
- **Passthrough Recipients**: Emails to specific addresses or entire domains (e.g., `@example.com`) listed in the configuration.

---

## Rejected Requests

Any message that does not meet the criteria above will be rejected with an error. Common reasons for rejection include:

### 1. Unencrypted Plain Text
Standard unencrypted emails (whether plain text or HTML) are rejected.
- **SMTP Error**: `523 Encryption Needed: Invalid Unencrypted Mail`
- **IMAP Error**: `Encryption Needed: Invalid Unencrypted Mail`

### 2. Invalid PGP Structure
Messages that claim to be encrypted but have structural errors:
- Missing `Version: 1` in the first part.
- Invalid or corrupted OpenPGP packets in the second part.
- More than two parts in a `multipart/encrypted` message.

### 3. Header Mismatches
To prevent spoofing and ensure accountability, the server checks that:
- The **MIME From** header exactly matches the **Envelope Sender** (MAIL FROM).
- If they do not match, the message is rejected with `554 From header does not match envelope sender`.

### 4. Invalid Recipient Format
Messages addressed to malformed email addresses are rejected before encryption checks are even performed.

## Implementation Details

The logic for these checks is implemented in the following components:
- `internal/check/pgp_encryption/`: The SMTP check module.
- `internal/pgp_verify/`: Core PGP and Secure Join validation logic.
- `internal/endpoint/imap/`: The IMAP wrapper that enforces these checks on `APPEND` commands.
