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


def run(sender, receiver, test_dir) -> bool:
    size_mb = _size_mb()
    print(f"Bigfile roundtrip: generating {size_mb} MiB random file …")

    file_path = os.path.join(test_dir, f"bigfile_roundtrip_{size_mb}mb.bin")
    data = os.urandom(size_mb * 1024 * 1024)
    h_expected = hashlib.sha256(data).hexdigest()
    with open(file_path, "wb") as f:
        f.write(data)

    receiver_email = receiver.get_config("addr")
    rc = sender.get_contact_by_addr(receiver_email)
    if rc is None:
        raise Exception(
            f"Receiver contact {receiver_email} not found — run account creation and secure-join first."
        )
    chat = rc.create_chat()

    print(f"  Sending to {receiver_email} …")
    chat.send_file(os.path.abspath(file_path))
    expect_bytes = size_mb * 1024 * 1024

    max_wait = max(300, 60 + size_mb * 20)
    start = time.time()

    while time.time() - start < max_wait:
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
                        print(
                            f"  OK: {size_mb} MiB file received, SHA-256 matches "
                            f"({h[:12]}…)"
                        )
                        return True
                except Exception:
                    pass
        time.sleep(3)

    raise Exception(
        f"Timed out after {max_wait}s waiting for a {size_mb} MiB file with matching hash"
    )
