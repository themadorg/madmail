"""Delta Chat account helpers for deltachat-test."""

from __future__ import annotations

import time

from deltachat_rpc_client import EventType


def _transport_field(transport, name: str):
    if isinstance(transport, dict):
        return transport.get(name)
    return getattr(transport, name, None)


def _configured_transport(account):
    """First transport, or None when this client has no list_transports result."""
    list_transports = getattr(account, "list_transports", None)
    if list_transports is None:
        return None
    transports = list_transports() or []
    if not transports:
        return None
    return transports[0]


def account_addr(account) -> str:
    """Email address of the account's configured transport.

    Delta Chat 2.63 stopped writing the legacy ``addr`` config key.
    """
    transport = _configured_transport(account)
    if transport is not None:
        addr = _transport_field(transport, "addr")
        if addr:
            return addr
    addr = account.get_config("addr")
    if addr:
        return addr
    raise RuntimeError(
        "account has no transport address; "
        "Delta Chat 2.63 stores it on list_transports()"
    )


def account_password(account) -> str:
    """IMAP/SMTP password of the account's configured transport.

    Delta Chat 2.63 stopped writing the legacy ``mail_pw`` config key.
    """
    transport = _configured_transport(account)
    if transport is not None:
        password = _transport_field(transport, "password")
        if password:
            return password
    password = account.get_config("mail_pw")
    if password:
        return password
    raise RuntimeError(
        "account has no transport password; "
        "Delta Chat 2.63 stores it on list_transports()"
    )


def restart_accounts_io(*accounts, settle_seconds: float = 3.0) -> None:
    """Restart client I/O after a server-side madmail restart."""
    for account in accounts:
        if account is None:
            continue
        account.stop_io()
        account.start_io()
    if settle_seconds > 0:
        time.sleep(settle_seconds)


def wait_accounts_idle(*accounts, timeout: float = 60.0) -> None:
    """Wait until each account reports IMAP IDLE readiness."""
    for account in accounts:
        if account is None:
            continue
        addr = account_addr(account)
        start = time.time()
        while time.time() - start < timeout:
            event = account.wait_for_event()
            if event and event.kind == EventType.IMAP_INBOX_IDLE:
                break
            if event and event.kind == EventType.ERROR:
                raise RuntimeError(f"account error while waiting for idle: {event.msg}")
        else:
            raise RuntimeError(f"timed out waiting for IMAP_INBOX_IDLE on {addr}")