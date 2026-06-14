import time

import requests

from scenarios import test_03_secure_join
from utils.ssh import run_ssh_command


def _get_admin_token(remote):
    for config_path in ("/etc/madmail/madmail.conf", "/etc/maddy/maddy.conf"):
        rc, stdout, _ = run_ssh_command(
            remote,
            f"grep -m1 'admin_token' {config_path} 2>/dev/null "
            r"| grep -v '^\s*#' | awk '{print $2}'",
        )
        token = stdout.strip()
        if token and token != "disabled":
            return token
        if token == "disabled":
            raise Exception(f"admin_token is set to 'disabled' in {config_path}")

    for token_path in ("/var/lib/madmail/admin_token", "/var/lib/maddy/admin_token"):
        rc, stdout, _ = run_ssh_command(remote, f"cat {token_path} 2>/dev/null")
        token = stdout.strip()
        if token:
            return token

    rc, stdout, _ = run_ssh_command(remote, "madmail admin-token --raw 2>/dev/null")
    token = stdout.strip()
    if token:
        return token

    raise Exception(
        f"Admin token not found on {remote}. "
        "Check /var/lib/madmail/admin_token or admin_token in madmail.conf."
    )


def _admin_api(remote, resource, *, method="GET", body=None, token=None):
    payload = {"method": method, "resource": resource, "headers": {}}
    if token:
        payload["headers"]["Authorization"] = f"Bearer {token}"
    if body is not None:
        payload["body"] = body
    resp = requests.post(f"http://{remote}/api/admin", json=payload, timeout=30)
    try:
        return resp.json()
    except Exception:
        return {"raw": resp.text, "status": resp.status_code}


def _queue_action(remote, token, action, **extra):
    body = {"action": action, **extra}
    data = _admin_api(
        remote,
        "/admin/queue",
        method="POST",
        body=body,
        token=token,
    )
    assert data.get("status") == 200, f"{action} failed: {data}"
    resp_body = data.get("body", {})
    deleted = int(resp_body.get("deleted", 0))
    print(f"  {action}: deleted {deleted} item(s)")
    return deleted


def _seed_notices(remote, token, recipient, count=5):
    for i in range(count):
        data = _admin_api(
            remote,
            "/admin/notice",
            method="POST",
            body={
                "recipient": recipient,
                "subject": f"Purge seed {i}",
                "body": f"Purge test message {i}",
            },
            token=token,
        )
        assert data.get("status") == 200, f"notice seed {i} failed: {data}"


def _maildir_message_count(remote, username):
    safe = username.replace("/", "_").replace("\\", "_")
    cmd = (
        f"find /var/lib/madmail/mail/{safe} "
        r"\( -path '*/new/*' -o -path '*/cur/*' \) -type f 2>/dev/null | wc -l"
    )
    rc, stdout, stderr = run_ssh_command(remote, cmd)
    if rc != 0:
        raise Exception(f"maildir scan failed on {remote}: {stderr or stdout}")
    return int(stdout.strip() or "0")


def _mark_some_messages_seen_on_server(remote, username, count=2):
    safe = username.replace("/", "_").replace("\\", "_")
    cmd = (
        f"new='/var/lib/madmail/mail/{safe}/Maildir/new'; "
        f"cur='/var/lib/madmail/mail/{safe}/Maildir/cur'; "
        "mkdir -p \"$cur\"; "
        "n=0; "
        "for f in \"$new\"/*; do "
        '[ -f "$f" ] || continue; '
        'base=$(basename "$f"); '
        'mv "$f" "$cur/${base}:2,S"; '
        "n=$((n+1)); "
        f"[ \"$n\" -ge {count} ] && break; "
        "done; "
        "echo moved:$n"
    )
    rc, stdout, stderr = run_ssh_command(remote, cmd)
    if rc != 0:
        raise Exception(f"failed to move messages to cur/: {stderr or stdout}")
    moved = int(stdout.strip().split(":")[-1] or "0")
    print(f"  moved {moved} message(s) to maildir cur/")
    return moved


def _wait_for_messages(receiver, count=5, timeout=90):
    receiver.configure()
    deadline = time.time() + timeout
    while time.time() < deadline:
        for chat in receiver.get_chatlist():
            purge_msgs = [
                m
                for m in chat.get_messages()
                if "Purge test message" in (m.get_snapshot().text or "")
            ]
            if len(purge_msgs) >= count:
                return purge_msgs
        time.sleep(2)
    raise Exception(
        f"Expected at least {count} encrypted purge test messages within {timeout}s"
    )


def run(rpc, dc, acc_sender, acc_receiver, remote1, remote2):
    print("\n" + "=" * 50)
    print("TEST #14: Purge Messages Test")
    print("=" * 50)

    sender_addr = acc_sender.get_config("addr")
    receiver_addr = acc_receiver.get_config("addr")

    contact = acc_sender.get_contact_by_addr(receiver_addr)
    if not contact or not contact.get_snapshot().is_verified:
        print(f"Secure-joining {sender_addr} <-> {receiver_addr}...")
        test_03_secure_join.run(rpc, acc_sender, acc_receiver)
        time.sleep(2)
        contact = acc_sender.get_contact_by_addr(receiver_addr)
    else:
        print(f"Using existing verified contact {sender_addr} -> {receiver_addr}")

    chat = contact.create_chat() if contact else acc_sender.create_chat(acc_receiver)

    print(f"Sending encrypted messages from {sender_addr} to {receiver_addr}...")
    for i in range(5):
        chat.send_text(f"Purge test message {i}")
        time.sleep(1)

    print("Waiting for encrypted delivery in Delta Chat...")
    _wait_for_messages(acc_receiver)
    print("✓ Encrypted messages delivered")

    token = _get_admin_token(remote2)

    print(f"Seeding server maildir for {receiver_addr} via admin notices...")
    _seed_notices(remote2, token, receiver_addr)

    files_before = _maildir_message_count(remote2, receiver_addr)
    assert files_before >= 5, (
        f"Expected seeded maildir files for {receiver_addr}, found {files_before}"
    )
    print(f"  {receiver_addr}: {files_before} maildir file(s) before purge")

    moved = _mark_some_messages_seen_on_server(remote2, receiver_addr, count=2)
    assert moved > 0, "Expected to move at least one message into cur/"

    print("\nTesting 'purge-read'...")
    deleted_read = _queue_action(remote2, token, "purge_read")
    files_after_read = _maildir_message_count(remote2, receiver_addr)
    assert deleted_read > 0, "purge-read should delete cur/ messages"
    assert files_after_read < files_before, "purge-read should reduce maildir file count"

    print("\nTesting 'purge-all' (purge_user on receiver)...")
    deleted_user = _queue_action(
        remote2, token, "purge_user", username=receiver_addr
    )
    files_after_all = _maildir_message_count(remote2, receiver_addr)
    assert deleted_user > 0, "purge_user should delete remaining maildir files"
    assert files_after_all == 0, (
        f"receiver still has {files_after_all} maildir file(s) after purge_user"
    )

    print("✓ TEST #14 PASSED: Purge commands executed and verified via stats")