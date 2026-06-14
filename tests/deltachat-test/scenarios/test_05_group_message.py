import time

def run(admin, member, group_name):
    print(f"Creating group: {group_name}")
    
    # Get the member's email address
    member_email = member.get_config("addr")
    
    # Look up the existing contact (which has a key after secure join)
    existing_contact = admin.get_contact_by_addr(member_email)
    
    if existing_contact is None:
        # Fallback to creating contact if not found
        print(f"  Warning: Contact {member_email} not found, creating it...")
        existing_contact = admin.create_contact(member_email)
    
    snap = existing_contact.get_snapshot()
    print(f"  Member contact ID: {existing_contact.id}")
    print(f"  Contact address: {snap.address}")
    print(f"  is_verified: {snap.is_verified}")
    
    # Create the group
    group = admin.create_group(group_name)
    
    # Add the existing contact to the group
    group.add_contact(existing_contact)
    
    print("Waiting for group to propagate...")
    time.sleep(10)
    
    msg_text = f"Hello group from {admin.get_config('addr')}"
    group.send_text(msg_text)
    
    # Wait for receipt on member
    max_wait = 60
    start_time = time.time()
    
    while time.time() - start_time < max_wait:
        chatlist = member.get_chatlist()
        for chat in chatlist:
            if group_name in chat.get_basic_snapshot().name:
                msgs = chat.get_messages()
                for m in msgs:
                    if m.get_snapshot().text == msg_text:
                        print(f"Group message received: {msg_text}")
                        return group  # Return the group chat for reuse
        time.sleep(2)
        
    raise Exception(f"Group message not received by member within {max_wait}s")

