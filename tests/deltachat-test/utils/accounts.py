"""Delta Chat account helpers for deltachat-test."""

from __future__ import annotations

import time

from deltachat_rpc_client import EventType


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
        addr = account.get_config("addr")
        start = time.time()
        while time.time() - start < timeout:
            event = account.wait_for_event()
            if event and event.kind == EventType.IMAP_INBOX_IDLE:
                break
            if event and event.kind == EventType.ERROR:
                raise RuntimeError(f"account error while waiting for idle: {event.msg}")
        else:
            raise RuntimeError(f"timed out waiting for IMAP_INBOX_IDLE on {addr}")