import time
import smtplib
from email.mime.text import MIMEText


def run_encrypted_message(sender, receiver, text):
    """Test that encrypted messages work after secure join"""
    print(f"Sending P2P encrypted message: '{text}'")
    chat = sender.create_chat(receiver)
    chat.send_text(text)
    
    # Wait for receipt
    max_wait = 45
    start_time = time.time()
    
    print(f"Waiting for receiver to get the message...")
    while time.time() - start_time < max_wait:
        # Check all chats on receiver
        chats = receiver.get_chatlist()
        for c in chats:
            msgs = c.get_messages()
            for msg in msgs:
                snap = msg.get_snapshot()
                if snap.text == text:
                    print(f"Message received: {text}")
                    return True
        time.sleep(2)
    
    raise Exception(f"P2P message '{text}' not received within {max_wait}s")


def run_unencrypted_rejection_test(sender_email, sender_password, receiver_email, smtp_host):
    """Test that unencrypted messages are rejected by the server.
    
    This test sends a plain text (unencrypted) email via SMTP and expects
    the server to reject it with a 523 error code.
    """
    print(f"Testing unencrypted message rejection...")
    print(f"  From: {sender_email}")
    print(f"  To: {receiver_email}")
    print(f"  Server: {smtp_host}")
    
    # Create a simple unencrypted plain text message
    msg = MIMEText("This is an unencrypted test message that should be rejected.")
    msg['Subject'] = 'Unencrypted Test'
    msg['From'] = sender_email
    msg['To'] = receiver_email
    
    try:
        # Connect to SMTP server
        smtp = smtplib.SMTP(smtp_host, 587)
        smtp.starttls()
        smtp.login(sender_email, sender_password)
        
        # Try to send unencrypted message
        smtp.sendmail(sender_email, receiver_email, msg.as_string())
        smtp.quit()
        
        # If we get here, the message was accepted (which is a failure!)
        raise Exception("FAILED: Unencrypted message was accepted, but should have been rejected!")
        
    except smtplib.SMTPDataError as e:
        # We expect error code 523 "Encryption Needed"
        if e.smtp_code == 523:
            print(f"SUCCESS: Unencrypted message correctly rejected with 523: {e.smtp_error}")
            return True
        else:
            raise Exception(f"FAILED: Got unexpected SMTP error {e.smtp_code}: {e.smtp_error}")
    except smtplib.SMTPRecipientsRefused as e:
        # Check if the rejection is for encryption requirement
        for recipient, (code, message) in e.recipients.items():
            if code == 523:
                print(f"SUCCESS: Unencrypted message correctly rejected with 523: {message}")
                return True
        raise Exception(f"FAILED: Recipients refused with unexpected error: {e.recipients}")
    except Exception as e:
        raise Exception(f"FAILED: Unexpected error: {type(e).__name__}: {e}")


def run(sender, receiver, text):
    """Main test function - tests encrypted message delivery
    
    For backwards compatibility, this runs the encrypted message test.
    Use run_unencrypted_rejection_test for testing rejection of unencrypted mails.
    """
    return run_encrypted_message(sender, receiver, text)
