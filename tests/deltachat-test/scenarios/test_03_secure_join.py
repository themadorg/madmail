import time
from utils.accounts import account_addr


def run(rpc, inviter, joiner):
    print("Starting Secure Join...")

    # Let IMAP/SMTP key sync settle after account creation (cross-relay joins).
    time.sleep(2)

    qr_data = inviter.get_qr_code()
    print(f"Secure Join QR data: {qr_data[:50]}...")

    joiner_email = account_addr(joiner)
    inviter_email = account_addr(inviter)

    print(f"Joiner ({joiner_email}) joining Inviter ({inviter_email})...")
    joiner.secure_join(qr_data)

    # Same order as chatmail-core/deltachat-rpc-client/tests/test_securejoin.py.
    # Do not wait inviter + joiner in parallel threads: Rpc event delivery plus
    # ThreadPoolExecutor shutdown can block forever if progress events stall.
    print("Waiting for Secure Join handshakes (inviter then joiner progress)...")
    inviter.wait_for_securejoin_inviter_success()
    joiner.wait_for_securejoin_joiner_success()

    contact_on_joiner = joiner.get_contact_by_addr(inviter_email)
    if not contact_on_joiner or not contact_on_joiner.get_snapshot().e2ee_avail:
        raise Exception(
            "Secure Join events completed but joiner does not show verified inviter contact"
        )

    contact_on_inviter = inviter.get_contact_by_addr(joiner_email)
    if not contact_on_inviter or not contact_on_inviter.get_snapshot().e2ee_avail:
        raise Exception(
            "Secure Join events completed but inviter does not show verified joiner contact"
        )

    print("SUCCESS: Secure Join complete!")
    return True
