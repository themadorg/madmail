import random
import socket
import string
import time
import ipaddress
import urllib.parse
from deltachat_rpc_client import EventType

def random_string(length=9):
    chars = string.ascii_lowercase + string.digits
    return ''.join(random.choices(chars, k=length))

def is_ip_address(host):
    try:
        ipaddress.ip_address(host)
        return True
    except ValueError:
        return False

def run(dc, domain):
    print(f"Creating account on {domain}...")
    account = dc.add_account()
    
    if is_ip_address(domain):
        username = random_string(12)
        password = random_string(20)
        encoded_password = urllib.parse.quote(password, safe="")
        # Format: dclogin:username@host/?p=password&v=1&ip=993&sp=465&ic=3&ss=default
        login_uri = (
            f"dclogin:{username}@{domain}/?"
            f"p={encoded_password}&v=1&ip=993&sp=465&ic=3&ss=default"
        )
    elif domain.endswith(".localchat"):
        # cmlxc: dclogin with IMAP/SMTP to public IP; same /?... form as the IP branch
        # above and relay_minitest/support.py (required for correct DC dclogin parse).
        ip = socket.gethostbyname(domain)
        username = random_string(12)
        password = random_string(20)
        encoded_password = urllib.parse.quote(password, safe="")
        addr = f"{username}@{domain}"
        login_uri = (
            f"dclogin:{addr}/?"
            f"p={encoded_password}&v=1"
            f"&ih={ip}&ip=993&sh={ip}&sp=465&ic=3&ss=default"
        )
    else:
        login_uri = f"dcaccount:{domain}"
    
    print(f"  Configuring from QR...")
    account.set_config_from_qr(login_uri)
    account.set_config("displayname", f"Test User {random_string(4)}")
    
    print(f"  Starting I/O...")
    account.start_io()
    
    # Wait for IMAP_INBOX_IDLE indicating readiness
    print(f"  Waiting for IMAP_INBOX_IDLE...")
    max_wait = 60
    start_time = time.time()
    while time.time() - start_time < max_wait:
        event = account.wait_for_event()
        if event and event.kind == EventType.IMAP_INBOX_IDLE:
            addr = account.get_config("addr")
            print(f"✓ Account {addr} is now idle and ready.")
            return account
        elif event and event.kind == EventType.ERROR:
            print(f"✗ ERROR during setup: {event.msg}")
    
    raise Exception(f"Failed to reach IMAP_INBOX_IDLE on {domain} within {max_wait}s")
