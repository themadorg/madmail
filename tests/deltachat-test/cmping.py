"""
chatmail ping aka "cmping" transmits messages between relays.

Message Flow:
=============
1. ACCOUNT SETUP: Create sender and receiver accounts on specified relay domains
   - Each account connects to its relay's IMAP/SMTP servers
   - Accounts wait for IMAP_INBOX_IDLE state indicating readiness

2. GROUP CREATION: Sender creates a group chat and adds all receivers
   - An initialization message is sent to promote the group
   - All receivers must accept the group invitation before ping begins

3. PING SEND: Sender transmits messages to the group at specified intervals
   - Messages contain: unique-id timestamp sequence-number
   - Messages flow: Sender -> relay1 SMTP -> relay2 IMAP -> Receivers

4. PING RECEIVE: Each receiver waits for incoming messages
   - On receipt, round-trip time is calculated from embedded timestamp
   - Progress is tracked per-sequence across all receivers
   - Stats are accumulated for final report
"""

import argparse
import ipaddress
import os
import queue
import random
import shutil
import signal
import string
import sys
import threading
import time
import urllib.parse
from dataclasses import dataclass
from statistics import stdev

from deltachat_rpc_client import DeltaChat, EventType, Rpc
from xdg_base_dirs import xdg_cache_home

# Spinner characters for progress display
SPINNER_CHARS = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]


@dataclass
class RelayContext:
    """Context for a relay including its RPC connection, DeltaChat instance, and account maker."""

    rpc: Rpc
    dc: DeltaChat
    maker: "AccountMaker"


def log_event_verbose(event, addr, verbose_level=3):
    """Helper function to log events at specified verbose level."""
    if hasattr(event, "msg") and event.msg:
        print(f"  [{addr}] {event.kind}: {event.msg}")
    else:
        print(f"  [{addr}] {event.kind}")


def is_ip_address(host):
    """Check if the given host is an IP address."""
    try:
        ipaddress.ip_address(host)
        return True
    except ValueError:
        return False


def generate_credentials():
    """Generate random username and password for IP-based login.

    Returns:
        tuple: (username, password) where username is 12 chars and password is 20 chars
    """
    chars = string.ascii_lowercase + string.digits
    username = "".join(random.choices(chars, k=12))
    password = "".join(random.choices(chars, k=20))
    return username, password


def create_qr_url(domain_or_ip):
    """Create either a dcaccount or dclogin URL based on input type.

    Args:
        domain_or_ip: Either a domain name or an IP address

    Returns:
        str: Either dcaccount:domain or dclogin:username@ip/?p=password&v=1&ip=993&sp=465&ic=3&ss=default
    """
    if is_ip_address(domain_or_ip):
        # Generate credentials for IP address
        username, password = generate_credentials()

        # Build dclogin URL according to spec
        # dclogin:username@ip/?p=password&v=1&ip=993&sp=465&ic=3&ss=default
        encoded_password = urllib.parse.quote(password, safe="")

        # Format: dclogin:username@host/?query
        qr_url = (
            f"dclogin:{username}@{domain_or_ip}/?"
            f"p={encoded_password}&v=1&ip=993&sp=465&ic=3&ss=default"
        )
        return qr_url
    else:
        # Use dcaccount for domain names
        return f"dcaccount:{domain_or_ip}"


def print_progress(message, current=None, total=None, spinner_idx=0, done=False):
    """Print progress with optional spinner and counter.

    Args:
        message: The progress message to display
        current: Current count (optional)
        total: Total count (optional)
        spinner_idx: Index into SPINNER_CHARS for spinner animation
        done: If True, print 'Done!' and newline
    """
    if done:
        print(f"\r# {message}... Done!".ljust(60))
    elif current is not None and total is not None:
        spinner = SPINNER_CHARS[spinner_idx % len(SPINNER_CHARS)]
        print(f"\r# {message} {spinner} {current}/{total}", end="", flush=True)
    else:
        spinner = SPINNER_CHARS[spinner_idx % len(SPINNER_CHARS)]
        print(f"\r# {message} {spinner}", end="", flush=True)


def format_duration(seconds):
    """Format a duration in seconds to a human-readable string.

    Args:
        seconds: Duration in seconds

    Returns:
        str: Formatted duration (e.g., "1.23s" or "45.67ms")
    """
    if seconds >= 1:
        return f"{seconds:.2f}s"
    else:
        return f"{seconds * 1000:.2f}ms"


class AccountMaker:
    def __init__(self, dc, verbose=0):
        self.dc = dc
        self.online = []
        self.verbose = verbose

    def _log_event(self, event, addr):
        """Helper method to log events at verbose level 3."""
        if self.verbose >= 3:
            if hasattr(event, "msg") and event.msg:
                print(f"  {event.kind}: {event.msg} [{addr}]")
            else:
                print(f"  {event.kind} [{addr}]")

    def wait_all_online(self):
        remaining = list(self.online)
        while remaining:
            ac = remaining.pop()
            while True:
                event = ac.wait_for_event()
                if event.kind == EventType.IMAP_INBOX_IDLE:
                    if self.verbose >= 3:
                        addr = ac.get_config("addr")
                        print(f"✓ IMAP_INBOX_IDLE: {addr} is now idle and ready")
                    break
                elif event.kind == EventType.ERROR and self.verbose >= 1:
                    print(f"✗ ERROR during profile setup: {event.msg}")
                elif self.verbose >= 3:
                    # Show all events during online phase when verbose level 3
                    addr = ac.get_config("addr")
                    self._log_event(event, addr)

    def _add_online(self, account):
        if self.verbose >= 3:
            addr = account.get_config("addr")
            print(f"  Starting I/O for account: {addr}")
        account.start_io()
        self.online.append(account)

    def get_relay_account(self, domain):
        # Try to find an existing account for this domain/IP
        for account in self.dc.get_all_accounts():
            addr = account.get_config("configured_addr")
            if addr is not None:
                # Extract the domain/IP from the configured address
                addr_domain = addr.split("@")[1] if "@" in addr else None
                if addr_domain == domain:
                    if account not in self.online:
                        if self.verbose >= 3:
                            print(f"  Reusing existing account: {addr}")
                        break
        else:
            account = self.dc.add_account()
            if self.verbose >= 3:
                print(f"  Creating new account for domain: {domain}")
            qr_url = create_qr_url(domain)
            try:
                if self.verbose >= 3:
                    print(f"  Configuring account from QR: {domain}")
                account.set_config_from_qr(qr_url)
                if self.verbose >= 3:
                    addr = account.get_config("addr")
                    print(f"  Account configured: {addr}")
            except Exception as e:
                print(f"✗ Failed to configure profile on {domain}: {e}")
                raise

        try:
            self._add_online(account)
        except Exception as e:
            print(f"✗ Failed to bring profile online for {domain}: {e}")
            raise

        return account


def setup_accounts(args, sender_maker, receiver_maker):
    """Set up sender and receiver accounts with progress display.

    Timing: This function's duration is tracked as 'account_setup_time'.

    Args:
        args: Command line arguments
        sender_maker: AccountMaker for the sender's relay
        receiver_maker: AccountMaker for the receiver's relay

    Returns:
        tuple: (sender_account, list_of_receiver_accounts)
    """
    # Calculate total profiles needed
    total_profiles = 1 + args.numrecipients
    profiles_created = 0

    # Create sender and receiver accounts with spinner
    print_progress("Setting up profiles", profiles_created, total_profiles, 0)

    try:
        sender = sender_maker.get_relay_account(args.relay1)
        profiles_created += 1
        print_progress("Setting up profiles", profiles_created, total_profiles, profiles_created)
    except Exception as e:
        print(f"\r✗ Failed to setup sender profile on {args.relay1}: {e}")
        sys.exit(1)

    # Create receiver accounts
    receivers = []
    for i in range(args.numrecipients):
        try:
            receiver = receiver_maker.get_relay_account(args.relay2)
            receivers.append(receiver)
            profiles_created += 1
            print_progress("Setting up profiles", profiles_created, total_profiles, profiles_created)
        except Exception as e:
            print(f"\r✗ Failed to setup receiver profile {i+1} on {args.relay2}: {e}")
            sys.exit(1)

    # Profile setup complete
    print_progress("Setting up profiles", done=True)

    return sender, receivers


def create_and_promote_group(sender, receivers, verbose=0):
    """Create a group chat and send initial message to promote it.

    Returns:
        group: The created group chat object
    """
    # Create a group chat from sender and add all receivers
    if verbose >= 3:
        print("  Creating group chat 'cmping'")
    group = sender.create_group("cmping")
    for receiver in receivers:
        # Create a contact for the receiver account and add to group
        contact = sender.create_contact(receiver)
        if verbose >= 3:
            receiver_addr = receiver.get_config("addr")
            print(f"  Adding {receiver_addr} to group")
        group.add_contact(contact)

    # Send an initial message to promote the group
    # This sends invitations to all members; progress is shown in wait_for_receivers_to_join()
    if verbose >= 3:
        print("  Sending group initialization message")
    group.send_text("cmping group chat initialized")

    return group


def wait_for_receivers_to_join(args, sender, receivers, timeout_seconds=60):
    """Wait concurrently for all receivers to join the group with progress display.

    Timing: This function's duration is tracked as 'group_join_time'.

    Args:
        args: Command line arguments (for verbose flag)
        sender: Sender account
        receivers: List of receiver accounts
        timeout_seconds: Maximum time to wait for all receivers

    Returns:
        int: Number of receivers that successfully joined
    """
    print("# Waiting for receivers to come online", end="", flush=True)
    sender_addr = sender.get_config("addr")
    start_time = time.time()

    # Track which receivers have joined
    joined_receivers = set()
    joined_addrs = []  # Track addresses in order they joined
    receiver_threads_queue = queue.Queue()

    def wait_for_receiver_join(idx, receiver, deadline):
        """Thread function to wait for a single receiver to join.

        Args:
            idx: Index of the receiver
            receiver: Receiver account object
            deadline: Timestamp when timeout should occur
        """
        try:
            while time.time() < deadline:
                event = receiver.wait_for_event()
                if args.verbose >= 3:
                    # Log all events during group joining phase
                    receiver_addr = receiver.get_config("addr")
                    log_event_verbose(event, receiver_addr)

                if event.kind == EventType.INCOMING_MSG:
                    msg = receiver.get_message_by_id(event.msg_id)
                    snapshot = msg.get_snapshot()
                    sender_contact = msg.get_sender_contact()
                    sender_contact_snapshot = sender_contact.get_snapshot()
                    if (
                        sender_contact_snapshot.address == sender_addr
                        and "cmping group chat initialized" in snapshot.text
                    ):
                        chat_id = snapshot.chat_id
                        receiver_group = receiver.get_chat_by_id(chat_id)
                        receiver_group.accept()
                        receiver_threads_queue.put(
                            ("joined", idx, receiver.get_config("addr"))
                        )
                        return
                elif event.kind == EventType.ERROR and args.verbose >= 1:
                    receiver_threads_queue.put(("error", idx, event.msg))
            # Timeout occurred
            receiver_threads_queue.put(("timeout", idx, None))
        except Exception as e:
            receiver_threads_queue.put(("exception", idx, str(e)))

    # Start a thread for each receiver
    deadline = start_time + timeout_seconds
    threads = []
    for idx, receiver in enumerate(receivers):
        t = threading.Thread(
            target=wait_for_receiver_join, args=(idx, receiver, deadline)
        )
        t.start()
        threads.append(t)

    # Monitor progress
    total_receivers = len(receivers)
    while len(joined_receivers) < total_receivers and time.time() < deadline:
        try:
            event_type, idx, data = receiver_threads_queue.get(timeout=0.5)
            if event_type == "joined":
                joined_receivers.add(idx)
                joined_addrs.append(data)
                print(
                    f"\r# Waiting for receivers to come online {len(joined_receivers)}/{total_receivers}",
                    end="",
                    flush=True,
                )
            elif event_type == "error":
                if args.verbose >= 1:
                    print(f"\n✗ ERROR during group joining for receiver {idx}: {data}")
            elif event_type == "timeout":
                pass # Handled by loop condition
            elif event_type == "exception":
                print(f"\n# ERROR: receiver {idx} encountered exception: {data}")
        except queue.Empty:
            pass

    # Final status
    print(
        f"\r# Waiting for receivers to come online {len(joined_receivers)}/{total_receivers} - Complete!"
    )

    return len(joined_receivers)


def wait_profiles_online_multi(makers):
    """Wait for all profiles to be online with spinner progress."""
    online_errors = []

    def wait_online_thread(maker):
        try:
            maker.wait_all_online()
        except Exception as e:
            online_errors.append(e)

    threads = []
    for maker in makers:
        wait_thread = threading.Thread(target=wait_online_thread, args=(maker,))
        wait_thread.start()
        threads.append(wait_thread)

    spinner_idx = 0
    while any(t.is_alive() for t in threads):
        print_progress("Waiting for profiles to be online", spinner_idx=spinner_idx)
        spinner_idx += 1
        time.sleep(0.1)

    for t in threads:
        t.join()

    if online_errors:
        print(f"\n✗ Timeout or error waiting for profiles to be online: {online_errors[0]}")
        sys.exit(1)

    print_progress("Waiting for profiles to be online", done=True)


class Pinger:
    def __init__(self, args, sender, group, receivers):
        self.args = args
        self.sender = sender
        self.group = group
        self.receivers = receivers
        self.addr1 = sender.get_config("addr")
        self.receivers_addrs = [receiver.get_config("addr") for receiver in receivers]
        self.receivers_addrs_str = ", ".join(self.receivers_addrs)
        self.relay1 = self.addr1.split("@")[1]
        self.relay2 = self.receivers_addrs[0].split("@")[1]

        print(
            f"CMPING {self.relay1}({self.addr1}) -> {self.relay2}(group with {len(receivers)} receivers) count={args.count} interval={args.interval}s"
        )
        ALPHANUMERIC = string.ascii_lowercase + string.digits
        self.tx = "".join(random.choices(ALPHANUMERIC, k=30))
        t = threading.Thread(target=self.send_pings, daemon=True)
        self.sent = 0
        self.received = 0
        t.start()

    @property
    def loss(self):
        expected_total = self.sent * len(self.receivers)
        return 0.0 if expected_total == 0 else (1 - self.received / expected_total) * 100

    def send_pings(self):
        for seq in range(self.args.count):
            text = f"{self.tx} {time.time():.4f} {seq:17}"
            self.group.send_text(text)
            self.sent += 1
            time.sleep(self.args.interval)
        time.sleep(60)
        os.kill(os.getpid(), signal.SIGINT)

    def receive(self):
        num_pending = self.args.count * len(self.receivers)
        start_clock = time.time()
        received_by_receiver = {}
        event_queue = queue.Queue()

        def receiver_thread(receiver_idx, receiver):
            while True:
                try:
                    event = receiver.wait_for_event()
                    event_queue.put((receiver_idx, receiver, event))
                except Exception:
                    event_queue.put((receiver_idx, receiver, None))
                    break

        threads = []
        for idx, receiver in enumerate(self.receivers):
            t = threading.Thread(target=receiver_thread, args=(idx, receiver), daemon=True)
            t.start()
            threads.append(t)

        while num_pending > 0:
            try:
                receiver_idx, receiver, event = event_queue.get(timeout=1.0)
                if event is None: continue

                if event.kind == EventType.INCOMING_MSG:
                    msg = receiver.get_message_by_id(event.msg_id)
                    text = msg.get_snapshot().text
                    parts = text.strip().split()
                    if len(parts) == 3 and parts[0] == self.tx:
                        seq = int(parts[2])
                        if seq not in received_by_receiver:
                            received_by_receiver[seq] = set()
                        if receiver_idx not in received_by_receiver[seq]:
                            ms_duration = (time.time() - float(parts[1])) * 1000
                            self.received += 1
                            num_pending -= 1
                            received_by_receiver[seq].add(receiver_idx)
                            yield seq, ms_duration, len(text), receiver_idx
                            start_clock = time.time()
                elif event.kind == EventType.ERROR and self.args.verbose >= 1:
                    print(f"✗ ERROR: {event.msg}")
            except queue.Empty:
                continue

def perform_ping(args):
    base_accounts_dir = xdg_cache_home().joinpath("cmping")
    relays = {args.relay1, args.relay2}
    
    if args.reset:
        for relay in relays:
            relay_dir = base_accounts_dir.joinpath(relay)
            if relay_dir.exists():
                shutil.rmtree(relay_dir)
    
    relay_contexts = {}
    for relay in relays:
        relay_dir = base_accounts_dir.joinpath(relay)
        if relay_dir.exists() and not relay_dir.joinpath("accounts.toml").exists():
            shutil.rmtree(relay_dir)
        
        rpc = Rpc(accounts_dir=relay_dir)
        rpc.__enter__()
        dc = DeltaChat(rpc)
        maker = AccountMaker(dc, verbose=args.verbose)
        relay_contexts[relay] = RelayContext(rpc=rpc, dc=dc, maker=maker)
    
    try:
        account_setup_start = time.time()
        sender_maker = relay_contexts[args.relay1].maker
        receiver_maker = relay_contexts[args.relay2].maker
        sender, receivers = setup_accounts(args, sender_maker, receiver_maker)

        all_makers = [relay_contexts[r].maker for r in relays]
        wait_profiles_online_multi(all_makers)
        account_setup_time = time.time() - account_setup_start

        group_join_start = time.time()
        group = create_and_promote_group(sender, receivers, verbose=args.verbose)
        wait_for_receivers_to_join(args, sender, receivers)
        group_join_time = time.time() - group_join_start

        message_start = time.time()
        pinger = Pinger(args, sender, group, receivers)
        received = {}
        current_seq = None
        seq_tracking = {}
        
        try:
            for seq, ms_duration, size, receiver_idx in pinger.receive():
                if seq not in received: received[seq] = []
                received[seq].append(ms_duration)
                if seq not in seq_tracking:
                    seq_tracking[seq] = {"count": 0, "first_time": ms_duration, "last_time": ms_duration, "size": size}
                seq_tracking[seq]["count"] += 1
                seq_tracking[seq]["last_time"] = ms_duration

                if current_seq != seq:
                    if current_seq is not None: print()
                    print(f"{size} bytes ME -> {pinger.relay1} -> {pinger.relay2} -> ME seq={seq} time={ms_duration:0.2f}ms", end="", flush=True)
                    current_seq = seq

                count = seq_tracking[seq]["count"]
                total = args.numrecipients
                if count > 1:
                    prev_ratio_len = len(f" {count-1}/{total}")
                    print("\b" * prev_ratio_len, end="", flush=True)
                print(f" {count}/{total}", end="", flush=True)

                if count == total:
                    elapsed = seq_tracking[seq]["last_time"] - seq_tracking[seq]["first_time"]
                    print(f" (elapsed: {elapsed:0.2f}ms)", end="", flush=True)
        except KeyboardInterrupt: pass

        message_time = time.time() - message_start
        if current_seq is not None: print()

        print(f"--- {pinger.addr1} -> {len(pinger.receivers_addrs)} receivers statistics ---")
        print(f"{pinger.sent} transmitted, {pinger.received} received, {pinger.loss:.2f}% loss")
        if received:
            all_durations = [d for durations in received.values() for d in durations]
            rmin, rmax = min(all_durations), max(all_durations)
            ravg = sum(all_durations) / len(all_durations)
            rmdev = stdev(all_durations) if len(all_durations) >= 2 else 0
            print(f"rtt min/avg/max/mdev = {rmin:.3f}/{ravg:.3f}/{rmax:.3f}/{rmdev:.3f} ms")

        print("--- timing statistics ---")
        print(f"account setup: {format_duration(account_setup_time)}")
        print(f"group join: {format_duration(group_join_time)}")
        print(f"message send/recv: {format_duration(message_time)}")
        return pinger
    finally:
        for ctx in relay_contexts.values():
            ctx.rpc.__exit__(None, None, None)

def main():
    parser = argparse.ArgumentParser(description="chatmail ping")
    parser.add_argument("relay1", help="chatmail relay domain or IP address")
    parser.add_argument("relay2", nargs="?", help="chatmail relay domain or IP address")
    parser.add_argument("-c", dest="count", type=int, default=30, help="number of message pings")
    parser.add_argument("-i", dest="interval", type=float, default=1.1, help="seconds between message sending")
    parser.add_argument("-v", dest="verbose", action="count", default=0, help="increase verbosity")
    parser.add_argument("-g", dest="numrecipients", type=int, default=1, help="number of group recipients")
    parser.add_argument("--reset", action="store_true", help="force fresh account creation")
    args = parser.parse_args()
    if not args.relay2: args.relay2 = args.relay1

    pinger = perform_ping(args)
    expected_total = pinger.sent * args.numrecipients
    raise SystemExit(0 if pinger.received == expected_total else 1)

if __name__ == "__main__":
    main()
