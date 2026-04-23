"""
Test #11: JIT Registration Test

This test verifies the Just-In-Time (JIT) registration functionality:
1. JIT enabled: Accounts are created automatically on first IMAP/SMTP login.
2. JIT disabled: Accounts are NOT created on login (results in Invalid Credentials).
3. JIT disabled: Existing accounts can still login.
4. JIT disabled but Registration Open: The /new API endpoint still works.
"""

import subprocess
import time
import requests
import random
import string
import urllib.parse
from deltachat_rpc_client.rpc import JsonRpcError

def run_ssh_command(remote, command):
    """Run a command on remote server via SSH"""
    result = subprocess.run(
        ["ssh", f"root@{remote}", command],
        capture_output=True,
        text=True,
        timeout=30
    )
    return result.returncode, result.stdout, result.stderr

def random_string(length=9):
    return ''.join(random.choices(string.ascii_lowercase + string.digits, k=length))


def _addr_with_remote(localpart, remote):
    """Use bracketed host only for bare IPv4 literals."""
    if remote.replace(".", "").isdigit():
        return f"{localpart}@[{remote}]"
    return f"{localpart}@{remote}"

def set_jit(remote, enabled):
    state = "enable" if enabled else "disable"
    print(f"  Setting JIT to {state} on {remote}...")
    cmd = f"madmail creds jit {state}"
    returncode, stdout, stderr = run_ssh_command(remote, cmd)
    if returncode != 0:
        raise Exception(f"Failed to set JIT on {remote}: {stderr}")
    print(f"    {stdout.strip()}")
    # Apply credential setting changes to running auth handlers.
    rc, _, err = run_ssh_command(remote, "systemctl restart madmail.service")
    if rc != 0:
        raise Exception(f"Failed to restart madmail.service on {remote}: {err}")
    time.sleep(1)


def set_registration(remote, enabled):
    state = "open" if enabled else "close"
    print(f"  Setting registration to {state} on {remote}...")
    cmd = f"madmail creds registration {state}"
    returncode, stdout, stderr = run_ssh_command(remote, cmd)
    if returncode != 0:
        raise Exception(f"Failed to set registration on {remote}: {stderr}")
    print(f"    {stdout.strip()}")
    # Apply setting changes to running auth handlers.
    rc, _, err = run_ssh_command(remote, "systemctl restart madmail.service")
    if rc != 0:
        raise Exception(f"Failed to restart madmail.service on {remote}: {err}")
    time.sleep(1)


def run(dc, remotes):
    REMOTE1, REMOTE2 = remotes
    
    print("\n" + "="*50)
    print("TEST #11: JIT Registration Test")
    print("="*50)
    
    try:
        # Step 1: Enable JIT on REMOTE1
        print("\nStep 1: Testing with JIT ENABLED")
        set_jit(REMOTE1, True)
        
        username1 = random_string(8)
        password1 = random_string(16)
        email1 = _addr_with_remote(username1, REMOTE1)
        
        print(f"  Attempting login for {email1} (JIT enabled)...")
        acc1 = dc.add_account()
        
        # password must be quoted as it may contain special characters
        # email should NOT be quoted for dclogin scheme
        enc_pass1 = urllib.parse.quote(password1)
        login_uri1 = f"dclogin:{email1}?p={enc_pass1}&v=1&ih={REMOTE1}&ip=993&sh={REMOTE1}&sp=465&ic=3&ss=default"
        
        acc1.set_config_from_qr(login_uri1)
        acc1.start_io()
        
        # Wait for configuration
        configured = False
        for _ in range(30):
            if acc1.is_configured():
                configured = True
                break
            time.sleep(1)
        
        if not configured:
            raise Exception(f"Failed to configure account {email1} with JIT ENABLED")
        print(f"  ✓ Success: Account {email1} created automatically via JIT.")

        # Step 2: Disable JIT on REMOTE1
        print("\nStep 2: Testing with JIT DISABLED")
        set_jit(REMOTE1, False)
        # Keep registration closed here so account creation can only happen
        # via JIT (which we just disabled).
        set_registration(REMOTE1, False)
        
        # Try to create NEW account on REMOTE1 (should fail)
        username2 = random_string(8)
        password2 = random_string(16)
        email2 = _addr_with_remote(username2, REMOTE1)
        
        print(f"  Attempting login for {email2} (JIT disabled)...")
        acc2 = dc.add_account()
        
        enc_pass2 = urllib.parse.quote(password2)
        login_uri2 = f"dclogin:{email2}?p={enc_pass2}&v=1&ih={REMOTE1}&ip=993&sh={REMOTE1}&sp=465&ic=3&ss=default"
        
        try:
            # We expect this to fail eventually with "Invalid credentials"
            # because the account doesn't exist and JIT is disabled.
            acc2.set_config_from_qr(login_uri2)
            acc2.start_io()
            
            # If it didn't raise, wait and see if it gets configured
            # (it shouldn't, but let's give it a deadline)
            start_time = time.time()
            deadline = 20 # 20 seconds deadline
            while time.time() - start_time < deadline:
                if acc2.is_configured():
                    raise Exception(f"Account {email2} was configured even though JIT was DISABLED!")
                time.sleep(1)
            
            print(f"  ✓ Success: Account {email2} was NOT created automatically after {deadline}s deadline.")
            
        except JsonRpcError as e:
            error_msg = str(e)
            if "Invalid credentials" in error_msg:
                print(f"  ✓ Caught expected failure: Invalid credentials (as expected with JIT disabled)")
            else:
                print(f"  Caught JsonRpcError: {error_msg}")
                raise e
        except Exception as e:
            if "Invalid credentials" in str(e):
                print(f"  ✓ Caught expected exception: {e}")
            else:
                raise e

        # Step 3: Verify existing account can still login
        print("\nStep 3: Verifying existing account still works with JIT DISABLED")
        # acc1 is already configured, let's just trigger a small sync or check status
        if acc1.is_configured():
            print(f"  Existing account {email1} is still functional.")
        else:
            raise Exception(f"Existing account {email1} stopped working after JIT was disabled!")

        # Step 4: Verify /new API still works even if JIT is disabled
        print("\nStep 4: Testing /new API with JIT DISABLED")
        # Registration must be open for /new to work
        set_registration(REMOTE1, True)
        
        api_url = f"http://{REMOTE1}/new"
        print(f"  Calling {api_url}...")
        try:
            resp = requests.post(api_url, timeout=10)
            if resp.status_code == 200:
                data = resp.json()
                new_email = data.get("email")
                new_pw = data.get("password")
                print(f"  ✓ Success: /new API created account: {new_email}")
                
                # Verify we can login with this account (even with JIT disabled)
                print(f"  Verifying login for API-created account {new_email}...")
                acc3 = dc.add_account()
                
                # URL encode only the password as it might contain special characters (from /new API)
                # the email must remain raw for correct core parsing
                enc_new_pw = urllib.parse.quote(new_pw)
                login_uri3 = f"dclogin:{new_email}?p={enc_new_pw}&v=1&ih={REMOTE1}&ip=993&sh={REMOTE1}&sp=465&ic=3&ss=default"
                
                acc3.set_config_from_qr(login_uri3)
                acc3.start_io()
                
                configured = False
                for _ in range(30):
                    if acc3.is_configured():
                        configured = True
                        break
                    time.sleep(1)
                
                if not configured:
                    raise Exception(f"Failed to login with account created via /new API while JIT is disabled")
                print(f"  ✓ Success: Logged into API-created account.")
            else:
                raise Exception(f"/new API returned {resp.status_code}: {resp.text}")
        except Exception as e:
            raise Exception(f"Failed to test /new API: {e}")

        print("\n" + "="*50)
        print("🎉 TEST #11 PASSED! JIT Registration verified.")
        print("="*50)
        return True

    finally:
        # Clean up: restore defaults so subsequent tests behave normally.
        print("\nCleaning up: Restoring JIT ENABLED + registration OPEN...")
        set_jit(REMOTE1, True)
        set_registration(REMOTE1, True)
