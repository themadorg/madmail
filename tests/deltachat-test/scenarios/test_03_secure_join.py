def run(rpc, inviter, joiner):
    print("Starting Secure Join...")

    qr_data = inviter.get_qr_code()
    print(f"Secure Join QR data: {qr_data[:50]}...")

    joiner_email = joiner.get_config("addr")
    inviter_email = inviter.get_config("addr")

    print(f"Joiner ({joiner_email}) joining Inviter ({inviter_email})...")
    joiner.secure_join(qr_data)

    # Same order as chatmail-core/deltachat-rpc-client/tests/test_securejoin.py.
    # Do not wait inviter + joiner in parallel threads: Rpc event delivery plus
    # ThreadPoolExecutor shutdown can block forever if progress events stall.
    print("Waiting for Secure Join handshakes (inviter then joiner progress)...")
    inviter.wait_for_securejoin_inviter_success()
    joiner.wait_for_securejoin_joiner_success()

    contact_on_joiner = joiner.get_contact_by_addr(inviter_email)
    if not contact_on_joiner or not contact_on_joiner.get_snapshot().is_verified:
        raise Exception(
            "Secure Join events completed but joiner does not show verified inviter contact"
        )
    print("SUCCESS: Secure Join complete!")
    return True
