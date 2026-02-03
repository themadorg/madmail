import time
import os
import zipfile
import tempfile
from deltachat_rpc_client import EventType, ViewType
from scenarios.test_01_account_creation import run as create_account

def create_dummy_xdc():
    tmpdir = tempfile.mkdtemp()
    manifest_path = os.path.join(tmpdir, "manifest.toml")
    with open(manifest_path, "w") as f:
        f.write('name = "Test App"\n')
    
    index_path = os.path.join(tmpdir, "index.html")
    with open(index_path, "w") as f:
        f.write("<html><body>Test</body></html>\n")
        
    xdc_path = os.path.join(tmpdir, "test.xdc")
    with zipfile.ZipFile(xdc_path, "w") as z:
        z.write(manifest_path, "manifest.toml")
        z.write(index_path, "index.html")
    return xdc_path

def run(dc, domain):
    print(f"--- Running WebXDC Realtime Test on {domain} ---")
    
    # Create two accounts on the same domain
    print("Creating Acc1...")
    acc1 = create_account(dc, domain)
    print("Creating Acc2...")
    acc2 = create_account(dc, domain)
    
    addr1 = acc1.get_config("addr")
    addr2 = acc2.get_config("addr")
    
    print(f"Acc1: {addr1}")
    print(f"Acc2: {addr2}")
    # Acc1 and Acc2 need to exchange keys. 
    # Since the server rejects unencrypted mail, we use Secure Join.
    print("Acc1 generating Secure Join QR code...")
    qr_data = acc1.get_qr_code()
    print("Acc2 joining via Secure Join...")
    acc2.secure_join(qr_data)

    # Wait for mutual verification
    print("Waiting for mutual verification (Acc1 <-> Acc2)...")
    start_time = time.time()
    verified1 = False
    verified2 = False
    while time.time() - start_time < 60:
        if not verified2:
            c2 = acc2.get_contact_by_addr(addr1)
            if c2 and c2.get_snapshot().is_verified:
                print("Acc2 verified Acc1.")
                verified2 = True
        
        if not verified1:
            c1 = acc1.get_contact_by_addr(addr2)
            if c1 and c1.get_snapshot().is_verified:
                print("Acc1 verified Acc2.")
                verified1 = True
        
        if verified1 and verified2:
            break
        time.sleep(2)
    else:
        raise Exception(f"Secure Join failed: verified1={verified1}, verified2={verified2}")

    # Acc1 adds Acc2 as contact in its database
    print("Acc1 ensuring Acc2 is a contact...")
    acc1.create_contact(addr2)
    contact2 = acc1.get_contact_by_addr(addr2)
    chat_acc1 = acc1.get_chat_by_contact(contact2)
    if not chat_acc1:
        chat_acc1 = acc1.create_chat_by_contact_id(contact2.id)
    
    # Send WebXDC from Acc1 to Acc2
    xdc_path = create_dummy_xdc()
    print(f"Sending WebXDC from Acc1 to Acc2 using {xdc_path}...")
    msg_acc1 = chat_acc1.send_message(viewtype=ViewType.WEBXDC, file=xdc_path)
    
    print("Waiting for Acc2 to receive the message...")
    msg_acc2 = None
    timeout = 60
    start_time = time.time()
    while time.time() - start_time < timeout:
        fresh = acc2.get_fresh_messages()
        for m in fresh:
            if m.get_snapshot().view_type == ViewType.WEBXDC:
                msg_acc2 = m
                break
        if msg_acc2:
            break
        time.sleep(2)
        
    if not msg_acc2:
        # Check if it arrived but is not in "fresh"
        for chat in acc2.get_chatlist():
            for m in chat.get_messages():
                if m.get_snapshot().view_type == ViewType.WEBXDC:
                    msg_acc2 = m
                    break
            if msg_acc2: break

    if not msg_acc2:
        raise Exception("Acc2 did not receive WebXDC message in time")
        
    print(f"Acc2 received WebXDC message, id={msg_acc2.id}")
    
    # Both join realtime
    print("Acc1 sending realtime advertisement...")
    msg_acc1.send_webxdc_realtime_advertisement()
    
    print("Acc2 sending realtime advertisement...")
    msg_acc2.send_webxdc_realtime_advertisement()
    
    # Give it some time to establish Iroh connection
    print(f"Waiting for Iroh connection (10s) for msg_id={msg_acc2.id}...")
    time.sleep(10)
    
    # Acc1 sends data
    test_data = b"Hello Iroh P2P"
    print(f"Acc1 sending realtime data: {test_data}")
    msg_acc1.send_webxdc_realtime_data(test_data)
    
    # Acc2 waits for data
    print("Acc2 waiting for WebXDC real-time data...")
    received_data = None
    start_time = time.time()
    timeout = 60
    
    # We need to listen to events for acc2
    while time.time() - start_time < timeout:
        event = acc2.wait_for_event(EventType.WEBXDC_REALTIME_DATA, timeout=5)
        if event:
            if event.msg_id == msg_acc2.id:
                received_data = bytes(event.data)
                print(f"Acc2 received {len(received_data)} bytes of real-time data.")
                if received_data == test_data:
                    break
        
        # Keep connection alive if needed
        msg_acc1.send_webxdc_realtime_data(test_data)
        time.sleep(2)
        
    if received_data == test_data:
        print("✓ WebXDC Realtime P2P connection SUCCESSFUL!")
    else:
        print(f"✗ WebXDC Realtime P2P connection FAILED. Received: {received_data}")
        raise Exception("WebXDC Realtime P2P connection failed")

    print(f"--- WebXDC Realtime Test PASSED for {domain} ---")
    return acc1, acc2
