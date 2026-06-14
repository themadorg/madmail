"""
Test #11: JIT Registration Test

This test verifies the Just-In-Time (JIT) registration functionality:
1. JIT enabled: Accounts are created automatically on first IMAP/SMTP login.
2. JIT disabled: Accounts are NOT created on login (results in Invalid Credentials).
3. JIT disabled: Existing accounts can still login.
4. JIT disabled but Registration Open: The /new API endpoint still works.
"""

import time
import requests
import random
import string
import urllib.parse
from deltachat_rpc_client.rpc import JsonRpcError

from utils.ssh import run_ssh_command


def random_string(length=9):
    return "".join(random.choices(string.ascii_lowercase + string.digits, k=length))


def _addr_with_remote(localpart, remote):
    """Use bracketed host only for bare IPv4 literals."""
    if remote.replace(".", "").isdigit():
        return f"{localpart}@[{remote}]"
    return f"{localpart}@{remote}"


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
    resp = requests.post(f"http://{remote}/api/admin", json=payload, timeout=15)
    try:
        return resp.json()
    except Exception:
        return {"raw": resp.text, "status": resp.status_code}


def _apply_auth_settings(remote):
    """Reload in-memory auth/settings after DB changes."""
    rc, stdout, stderr = run_ssh_command(
        remote,
        "madmail reload --insecure 2>/dev/null || systemctl restart madmail.service",
        timeout=60,
    )
    if rc != 0:
        raise Exception(f"Failed to apply settings on {remote}: {stderr or stdout}")
    time.sleep(5)


def _is_rejected_login(exc) -> bool:
    msg = str(exc).lower()
    return any(
        needle in msg
        for needle in (
            "invalid credentials",
            "cannot login",
            "connection lost",
            "please check if the email address",
        )
    )


def set_jit(remote, enabled):
    state = "enable" if enabled else "disable"
    print(f"  Setting JIT to {state} on {remote}...")
    token = _get_admin_token(remote)
    data = _admin_api(
        remote,
        "/admin/registration/jit",
        method="POST",
        body={"action": state},
        token=token,
    )
    if data.get("status") != 200:
        raise Exception(f"Failed to set JIT on {remote}: {data}")
    print(f"    JIT registration {data.get('body', {}).get('status', state)}")
    _apply_auth_settings(remote)


def set_registration(remote, enabled):
    state = "open" if enabled else "close"
    print(f"  Setting registration to {state} on {remote}...")
    rc, stdout, stderr = run_ssh_command(remote, f"madmail registration {state}")
    if rc != 0:
        raise Exception(f"Failed to set registration on {remote}: {stderr or stdout}")
    if stdout.strip():
        print(f"    {stdout.strip()}")
    _apply_auth_settings(remote)


def run(dc, remotes):
    remote1, _remote2 = remotes

    print("\n" + "=" * 50)
    print("TEST #11: JIT Registration Test")
    print("=" * 50)

    try:
        print("\nStep 1: Testing with JIT ENABLED")
        set_jit(remote1, True)

        username1 = random_string(8)
        password1 = random_string(16)
        email1 = _addr_with_remote(username1, remote1)

        print(f"  Attempting login for {email1} (JIT enabled)...")
        acc1 = dc.add_account()

        enc_pass1 = urllib.parse.quote(password1)
        login_uri1 = (
            f"dclogin:{email1}?p={enc_pass1}&v=1&ih={remote1}&ip=993"
            f"&sh={remote1}&sp=465&ic=3&ss=default"
        )

        acc1.set_config_from_qr(login_uri1)
        acc1.start_io()

        configured = False
        for _ in range(30):
            if acc1.is_configured():
                configured = True
                break
            time.sleep(1)

        if not configured:
            raise Exception(f"Failed to configure account {email1} with JIT ENABLED")
        print(f"  ✓ Success: Account {email1} created automatically via JIT.")

        print("\nStep 2: Testing with JIT DISABLED")
        set_jit(remote1, False)
        set_registration(remote1, False)

        username2 = random_string(8)
        password2 = random_string(16)
        email2 = _addr_with_remote(username2, remote1)

        print(f"  Attempting login for {email2} (JIT disabled)...")
        acc2 = dc.add_account()

        enc_pass2 = urllib.parse.quote(password2)
        login_uri2 = (
            f"dclogin:{email2}?p={enc_pass2}&v=1&ih={remote1}&ip=993"
            f"&sh={remote1}&sp=465&ic=3&ss=default"
        )

        try:
            acc2.set_config_from_qr(login_uri2)
            acc2.start_io()

            start_time = time.time()
            deadline = 20
            while time.time() - start_time < deadline:
                if acc2.is_configured():
                    raise Exception(
                        f"Account {email2} was configured even though JIT was DISABLED!"
                    )
                time.sleep(1)

            print(
                f"  ✓ Success: Account {email2} was NOT created automatically "
                f"after {deadline}s deadline."
            )

        except JsonRpcError as e:
            if _is_rejected_login(e):
                print(
                    "  ✓ Caught expected login failure with JIT disabled "
                    f"({e})"
                )
            else:
                print(f"  Caught JsonRpcError: {e}")
                raise
        except Exception as e:
            if _is_rejected_login(e):
                print(f"  ✓ Caught expected exception: {e}")
            else:
                raise

        print("\nStep 3: Verifying existing account still works with JIT DISABLED")
        if acc1.is_configured():
            print(f"  Existing account {email1} is still functional.")
        else:
            raise Exception(
                f"Existing account {email1} stopped working after JIT was disabled!"
            )

        print("\nStep 4: Testing /new API with JIT DISABLED")
        set_registration(remote1, True)

        api_url = f"http://{remote1}/new"
        print(f"  Calling {api_url}...")
        try:
            resp = requests.post(api_url, timeout=10)
            if resp.status_code == 200:
                data = resp.json()
                new_email = data.get("email")
                new_pw = data.get("password")
                print(f"  ✓ Success: /new API created account: {new_email}")

                print(f"  Verifying login for API-created account {new_email}...")
                acc3 = dc.add_account()

                enc_new_pw = urllib.parse.quote(new_pw)
                login_uri3 = (
                    f"dclogin:{new_email}?p={enc_new_pw}&v=1&ih={remote1}&ip=993"
                    f"&sh={remote1}&sp=465&ic=3&ss=default"
                )

                acc3.set_config_from_qr(login_uri3)
                acc3.start_io()

                configured = False
                for _ in range(30):
                    if acc3.is_configured():
                        configured = True
                        break
                    time.sleep(1)

                if not configured:
                    raise Exception(
                        "Failed to login with account created via /new API "
                        "while JIT is disabled"
                    )
                print("  ✓ Success: Logged into API-created account.")
            else:
                raise Exception(f"/new API returned {resp.status_code}: {resp.text}")
        except Exception as e:
            raise Exception(f"Failed to test /new API: {e}") from e

        print("\n" + "=" * 50)
        print("🎉 TEST #11 PASSED! JIT Registration verified.")
        print("=" * 50)
        return True

    finally:
        print("\nCleaning up: Restoring JIT ENABLED + registration OPEN...")
        try:
            set_jit(remote1, True)
            set_registration(remote1, True)
        except Exception as e:
            print(f"  Warning: cleanup failed: {e}")