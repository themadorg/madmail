import imaplib
import time
import sys
import os

# Add the scenarios directory to sys.path so we can import test_01_account_creation
sys.path.append(os.path.dirname(__file__))

from deltachat_rpc_client import EventType
from test_01_account_creation import run as create_account

def run(dc, domain):
    print(f"--- Running Iroh Discovery Test on {domain} ---")
    account = create_account(dc, domain)
    addr = account.get_config("addr")
    # For deltachat-core-rust, mail_pw is the config key for IMAP password
    password = account.get_config("mail_pw")
    
    if not password:
        print("✗ Could not retrieve IMAP password from account config.")
        raise Exception("IMAP password missing in config")

    print(f"Verifying Iroh discovery via IMAP for {addr}...")
    
    # The domain might be an IP address in some test environments
    host = domain
    print(f"  Connecting to IMAP_SSL at {host}:993...")
    
    # Register the GETMETADATA command in imaplib as it's not standard
    if 'GETMETADATA' not in imaplib.Commands:
        imaplib.Commands['GETMETADATA'] = ('AUTH', 'SELECTED')
    
    # Use a try-except block to handle connection errors gracefully
    try:
        # We use IMAP4_SSL because madmail uses 993 by default
        mail = imaplib.IMAP4_SSL(host, 993)
        try:
            print(f"  Logging in as {addr}...")
            mail.login(addr, password)
            
            # Capability check
            typ, cap = mail.capability()
            print(f"  Capabilities: {cap}")
            if not any(b"METADATA" in c for c in cap):
                raise Exception("METADATA capability not advertised by IMAP server")
                
            # GETMETADATA call
            # Command: GETMETADATA "" /shared/vendor/deltachat/irohrelay
            print("  Sending GETMETADATA /shared/vendor/deltachat/irohrelay ...")
            # Clear untagged responses
            mail.untagged_responses = {}
            typ, data = mail._simple_command('GETMETADATA', '""', '/shared/vendor/deltachat/irohrelay')
            print(f"  GETMETADATA response: {typ} {data}")
            print(f"  Untagged responses: {mail.untagged_responses}")
            
            if typ != 'OK':
                raise Exception(f"GETMETADATA failed with status {typ}: {data}")
                
            # Parse responses from untagged_responses
            found_url = None
            metadata_responses = mail.untagged_responses.get('METADATA', [])
            for entry in metadata_responses:
                print(f"  Processing metadata entry: {entry}")
                if b"/shared/vendor/deltachat/irohrelay" in entry:
                    try:
                        entry_str = entry.decode('utf-8')
                        import re
                        urls = re.findall(r'https?://[^\s"()]+', entry_str)
                        if urls:
                            found_url = urls[0]
                            break
                    except Exception as e:
                        print(f"  Error parsing entry {entry}: {e}")
            
            if found_url:
                print(f"✓ Successfully discovered Iroh Relay URL: {found_url}")
                # Optional: Basic validation of the URL
                if domain in found_url or "127.0.0.1" in found_url or "::1" in found_url:
                    print("✓ URL seems to point to the correct server.")
                else:
                    print(f"! Note: URL {found_url} points to a different host than {domain}")
            else:
                raise Exception("Iroh Relay URL not found in metadata response")
                
        finally:
            mail.logout()
            print("  Logged out.")
    except Exception as e:
        print(f"✗ IMAP Verification FAILED: {e}")
        raise

    print(f"--- Iroh Discovery Test PASSED for {domain} ---")
    return account

if __name__ == "__main__":
    # This block allows running the script standalone if needed, 
    # but it's designed to be called by main.py
    pass
