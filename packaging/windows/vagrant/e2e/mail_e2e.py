#!/usr/bin/env python3
# Copyright (C) 2026 themadorg
# SPDX-License-Identifier: AGPL-3.0-or-later
"""
Full mail-path E2E against a local Madmail instance on Windows (or any host).

Creates two accounts via `madmail accounts create`, sends a message over
submission STARTTLS (587), verifies receipt over IMAP STARTTLS (143).

Self-signed TLS is accepted for this test harness only.
"""

from __future__ import annotations

import argparse
import email
import imaplib
import smtplib
import ssl
import subprocess
import sys
import time
import uuid
from email.message import EmailMessage
from pathlib import Path


def run(cmd: list[str], check: bool = True) -> subprocess.CompletedProcess[str]:
    print("+", " ".join(cmd), flush=True)
    return subprocess.run(cmd, check=check, text=True, capture_output=True)


def create_account(
    madmail: str, config: str, state: str, email_addr: str, password: str
) -> None:
    r = run(
        [
            madmail,
            "--config",
            config,
            "--state-dir",
            state,
            "accounts",
            "create",
            email_addr,
            "--password",
            password,
        ]
    )
    if r.stdout:
        print(r.stdout, end="")
    if r.stderr:
        print(r.stderr, end="", file=sys.stderr)


def ssl_ctx() -> ssl.SSLContext:
    ctx = ssl.create_default_context()
    ctx.check_hostname = False
    ctx.verify_mode = ssl.CERT_NONE
    return ctx


def smtp_send(
    host: str,
    port: int,
    user: str,
    password: str,
    to_addr: str,
    subject: str,
    body: str,
) -> str:
    msg = EmailMessage()
    msg["From"] = user
    msg["To"] = to_addr
    msg["Subject"] = subject
    mid = f"<{uuid.uuid4()}@madmail-vagrant.local>"
    msg["Message-ID"] = mid
    msg.set_content(body)

    with smtplib.SMTP(host, port, timeout=60) as smtp:
        smtp.ehlo()
        smtp.starttls(context=ssl_ctx())
        smtp.ehlo()
        smtp.login(user, password)
        smtp.send_message(msg)
    return mid


def imap_wait_for_subject(
    host: str,
    port: int,
    user: str,
    password: str,
    subject: str,
    timeout_s: float = 60.0,
) -> None:
    deadline = time.time() + timeout_s
    last_err: Exception | None = None
    while time.time() < deadline:
        try:
            imap = imaplib.IMAP4(host, port, timeout=30)
            imap.starttls(ssl_ctx())
            imap.login(user, password)
            imap.select("INBOX")
            typ, data = imap.search(None, "ALL")
            if typ == "OK" and data and data[0]:
                for num in data[0].split():
                    typ, msg_data = imap.fetch(num, "(RFC822)")
                    if typ != "OK" or not msg_data or not msg_data[0]:
                        continue
                    raw = msg_data[0][1]
                    if isinstance(raw, bytes):
                        m = email.message_from_bytes(raw)
                        if m.get("Subject") == subject:
                            imap.logout()
                            return
            imap.logout()
        except Exception as e:  # noqa: BLE001 — retry until timeout
            last_err = e
        time.sleep(1.5)
    raise TimeoutError(
        f"message with subject {subject!r} not found within {timeout_s}s; last_err={last_err}"
    )


def main() -> int:
    p = argparse.ArgumentParser(description="Madmail Windows Vagrant mail E2E")
    p.add_argument(
        "--madmail",
        default=r"C:\Program Files\Madmail\madmail.exe",
        help="Path to madmail.exe",
    )
    p.add_argument(
        "--config",
        default=r"C:\ProgramData\Madmail\config\madmail.conf",
    )
    p.add_argument(
        "--state-dir",
        default=r"C:\ProgramData\Madmail\data",
    )
    p.add_argument("--host", default="127.0.0.1")
    p.add_argument("--smtp-port", type=int, default=587)
    p.add_argument("--imap-port", type=int, default=143)
    p.add_argument("--domain", default="127.0.0.1", help="Address domain for accounts")
    args = p.parse_args()

    madmail = args.madmail
    if not Path(madmail).is_file():
        print(f"error: madmail not found: {madmail}", file=sys.stderr)
        return 2

    domain = args.domain.strip("[]")
    suffix = uuid.uuid4().hex[:8]
    alice = f"alice-{suffix}@{domain}"
    bob = f"bob-{suffix}@{domain}"
    password = "VagrantTestPass1!"

    print("==> Create accounts")
    create_account(madmail, args.config, args.state_dir, alice, password)
    create_account(madmail, args.config, args.state_dir, bob, password)

    subject = f"vagrant-e2e-{suffix}"
    body = f"hello from madmail windows vagrant e2e ({suffix})"

    print("==> SMTP submit (STARTTLS)")
    mid = smtp_send(
        args.host, args.smtp_port, alice, password, bob, subject, body
    )
    print(f"    sent Message-ID {mid}")

    print("==> IMAP receive (STARTTLS)")
    imap_wait_for_subject(
        args.host, args.imap_port, bob, password, subject, timeout_s=90.0
    )
    print("    bob received message OK")

    # Reverse direction
    subject2 = f"vagrant-e2e-reply-{suffix}"
    print("==> Reverse: bob -> alice")
    smtp_send(args.host, args.smtp_port, bob, password, alice, subject2, "reply")
    imap_wait_for_subject(
        args.host, args.imap_port, alice, password, subject2, timeout_s=90.0
    )
    print("    alice received reply OK")

    print("==> PASS: mail E2E completed")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except subprocess.CalledProcessError as e:
        print(e.stdout or "", end="")
        print(e.stderr or "", end="", file=sys.stderr)
        print(f"error: command failed: {e.returncode}", file=sys.stderr)
        raise SystemExit(e.returncode) from e
