import os
import hashlib
import time
import shutil

def run(sender, receiver, test_dir):
    print("Testing file transfer (1MB)...")
    # Generate 1MB random file in the test_dir
    file_path = os.path.join(test_dir, "large_file_sent.bin")
        
    random_data = os.urandom(1024 * 1024) # 1MB
    original_hash = hashlib.sha256(random_data).hexdigest()
    
    with open(file_path, "wb") as f:
        f.write(random_data)
        
    print(f"File created, hash: {original_hash}")
    
    # Get or create the chat with receiver (should already exist after secure join)
    receiver_email = receiver.get_config("addr")
    receiver_contact = sender.get_contact_by_addr(receiver_email)
    
    if receiver_contact is None:
        raise Exception(f"Receiver contact {receiver_email} not found - did previous tests complete?")
    
    chat = receiver_contact.create_chat()
    print(f"Sending file to chat ID: {chat.id}")
    
    # Send file
    chat.send_file(os.path.abspath(file_path))
    
    # Wait for receipt
    max_wait = 180 # Large files take longer and federation can be slow
    start_time = time.time()
    
    sender_email = sender.get_config("addr")
    print(f"Waiting for receiver to get the file from {sender_email}...")
    
    while time.time() - start_time < max_wait:
        # Check all chats for binary file messages that match our expected size
        chatlist = receiver.get_chatlist()
        for c in chatlist:
            msgs = c.get_messages()
            for m in msgs:
                try:
                    snap = m.get_snapshot()
                    # Check if this message has a file with the right size (around 1MB)
                    if snap.file:
                        received_file_path = snap.file
                        if os.path.exists(received_file_path):
                            file_size = os.path.getsize(received_file_path)
                            # Must be close to 1MB (allow some variability for encryption overhead)
                            if file_size >= 1024 * 1024 * 0.9 and file_size <= 1024 * 1024 * 1.5:
                                with open(received_file_path, "rb") as f:
                                    received_data = f.read()
                                    received_hash = hashlib.sha256(received_data).hexdigest()
                                    
                                # Copy received file to test_dir for record keeping
                                dest_received = os.path.join(test_dir, "large_file_received.bin")
                                shutil.copy2(received_file_path, dest_received)
                                
                                print(f"Found file: {received_file_path}")
                                print(f"Original size: {len(random_data)}, Received size: {len(received_data)}")
                                
                                if received_hash == original_hash:
                                    print(f"File transfer successful! Hash matches: {received_hash}")
                                    return True
                                else:
                                    # Keep trying, might be wrong file
                                    print(f"Hash mismatch for {received_file_path}, continuing search...")
                except Exception as e:
                    # Skip messages that can't be read
                    pass
        time.sleep(5)
        
    raise Exception("File transfer timed out - no matching file found")
