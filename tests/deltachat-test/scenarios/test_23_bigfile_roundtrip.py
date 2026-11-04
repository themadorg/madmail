"""
Big encrypted file E2E: send a multi-megabyte file and assert the
receiver can download it with a matching SHA-256 (same goal as
test_09_send_bigfile, but that scenario measures SMTP timing; this one
is an integrity/roundtrip check for madmail PGP + IMAP/attachment path).

The default size is 5 MB so full-suite runs still finish in a
reasonable time. Override with the BIGFILE_E2E_MB environment variable
(e.g. BIGFILE_E2E_MB=12) for a heavier run on a beefy relay.
"""

import hashlib
import os
import shutil
import time

from deltachat_rpc_client.const import MessageState

from utils.server_ctl import enable_logging
from utils.accounts import account_addr

_DOWNLOAD_LIMIT = "268435456"
_MAX_WAIT_S = 60
_MAX_SERVER_S = 120


def _size_mb() -> int:
    raw = os.environ.get("BIGFILE_E2E_MB", "5")
    try:
        n = int(raw)
    except ValueError:
        n = 5
    if n < 1:
        n = 1
    if n > 80:
        n = 80
    return n


def _wait_sender_delivered(msg, timeout: float) -> None:
    start = time.time()
    while time.time() - start < timeout:
        snap = msg.get_snapshot()
        if snap.state >= MessageState.OUT_DELIVERED:
            return
        if snap.state == MessageState.OUT_FAILED:
            raise Exception("Sender failed to deliver the big file (OUT_FAILED)")
        time.sleep(0.25)
    raise Exception(f"Sender did not report delivery within {timeout:.0f}s")


def run(sender, receiver, test_dir) -> bool:
    remote1 = os.environ.get("REMOTE1")
    remote2 = os.environ.get("REMOTE2")
    if remote1 and remote2:
        print("Enabling server logging for bigfile roundtrip …")
        enable_logging(remote1)
        enable_logging(remote2)

    size_mb = _size_mb()
    print(f"Bigfile roundtrip: generating {size_mb} MiB random file …")

    # Large attachments use Delta Chat pre/post split; auto-download needs a high limit.
    receiver.set_config("download_limit", _DOWNLOAD_LIMIT)

    file_path = os.path.join(test_dir, f"bigfile_roundtrip_{size_mb}mb.bin")
    data = os.urandom(size_mb * 1024 * 1024)
    h_expected = hashlib.sha256(data).hexdigest()
    with open(file_path, "wb") as f:
        f.write(data)

    receiver_email = account_addr(receiver)
    rc = sender.get_contact_by_addr(receiver_email)
    if rc is None:
        raise Exception(
            f"Receiver contact {receiver_email} not found — run account creation and secure-join first."
        )
    chat = rc.create_chat()

    print(f"  Sending to {receiver_email} …")
    msg = chat.send_file(os.path.abspath(file_path))
    expect_bytes = size_mb * 1024 * 1024

    # Wait until pre- and post-message SMTP completes before polling the receiver.
    _wait_sender_delivered(msg, min(45, _MAX_WAIT_S - 5))

    start = time.time()
    last_diag = 0.0

    while time.time() - start < _MAX_WAIT_S:
        for c in receiver.get_chatlist():
            for m in c.get_messages():
                try:
                    snap = m.get_snapshot()
                    if not snap.file:
                        continue
                    p = snap.file
                    if not os.path.exists(p):
                        continue
                    sz = os.path.getsize(p)
                    if sz < int(expect_bytes * 0.85) or sz > int(expect_bytes * 1.25):
                        continue
                    with open(p, "rb") as f:
                        got = f.read()
                    h = hashlib.sha256(got).hexdigest()
                    if h == h_expected:
                        dest = os.path.join(
                            test_dir, f"bigfile_roundtrip_{size_mb}mb_received.bin"
                        )
                        shutil.copy2(p, dest)
                        elapsed = time.time() - start
                        print(
                            f"  OK: {size_mb} MiB file received, SHA-256 matches "
                            f"({h[:12]}…) in {elapsed:.1f}s"
                        )
                        return True
                except Exception:
                    pass
        now = time.time()
        if now - last_diag >= 10:
            print(f"  … still waiting for {size_mb} MiB file ({now - start:.0f}s)")
            last_diag = now
        time.sleep(1)

    elapsed = time.time() - start
    if elapsed > _MAX_SERVER_S:
        raise Exception(
            f"Server-side delivery exceeded {_MAX_SERVER_S}s "
            f"(waited {elapsed:.0f}s for {size_mb} MiB file)"
        )
    raise Exception(
        f"Timed out after {_MAX_WAIT_S}s waiting for a {size_mb} MiB file with matching hash"
    )