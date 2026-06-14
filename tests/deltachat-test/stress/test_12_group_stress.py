import os
import sys
import time
import datetime
import threading
import json
import matplotlib
matplotlib.use('Agg') # Thread-safe non-interactive backend
import matplotlib.pyplot as plt
import concurrent.futures
import queue
from statistics import stdev
from typing import List, Dict
import contextlib
import io
import logging

# Configure logging for RPC client debugging
logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(name)s - %(levelname)s - %(message)s',
    filename='/tmp/madmail_stress_internal.log',
    filemode='w'
)
logger = logging.getLogger("stress_test")

# --- Magic Numbers / Constants ---
USER_COUNT_DEFAULT = 2
MSG_COUNT_DEFAULT = 10
ACCOUNTS_WORKERS = 8
SECURE_JOIN_WORKERS = 1 # Sequential is more reliable on 1-core
PROPAGATION_WAIT = 15
VERIFY_TIMEOUT = 300
MONITOR_INTERVAL = 1.0
CHART_INTERVAL = 15.0 # Incremental chart generation interval
RPC_SERVER_PATH = "/usr/bin/deltachat-rpc-server"
# ---------------------------------

# Add necessary paths
TEST_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.append(os.path.join(os.path.dirname(TEST_DIR), "chatmail-core/deltachat-rpc-client/src"))
sys.path.append(TEST_DIR)

from deltachat_rpc_client import DeltaChat, Rpc, EventType
from scenarios import test_01_account_creation, test_03_secure_join
from utils.lxc import LXCManager

class StressTestResult:
    def __init__(self):
        self.timestamps = []
        self.cpu_usage = [] 
        self.mem_usage = [] 
        self.account_creation_cpu = [] 
        self.delivery_stats = {} # text -> {member_idx -> timestamp}
        self.total_expected = 0
        self.total_received = 0
        self.sent_timestamps = [] # List of timestamps when messages were sent

class StressTest:
    def __init__(self, remote, test_dir, user_count=USER_COUNT_DEFAULT, msg_count=MSG_COUNT_DEFAULT):
        self.remote = remote
        self.test_dir = test_dir
        self.user_count = user_count
        self.msg_count = msg_count
        self.results = StressTestResult()
        self.lxc = LXCManager()
        self.stop_monitoring = False
        
        self.data_dir = os.path.join(test_dir, "dc_data_stress")
        os.makedirs(self.data_dir, exist_ok=True)
        
        # Initialize accounts.toml
        accounts_path = os.path.join(self.data_dir, "accounts.toml")
        if not os.path.exists(accounts_path):
            with open(accounts_path, "w") as f:
                f.write('selected_account = 0\nnext_id = 1\naccounts = []\n')
        
        self.rpc_log_path = os.path.join(test_dir, "client_stress_debug.log")
        self.rpc_log_file = open(self.rpc_log_path, "w")
        
        env = os.environ.copy()
        env["RUST_LOG"] = "info"
        self.rpc = Rpc(accounts_dir=self.data_dir, rpc_server_path=RPC_SERVER_PATH, stderr=self.rpc_log_file, env=env)
        
    def monitor_stats(self, container_name):
        last_cpu = 0.0
        last_time = time.time()
        
        while not self.stop_monitoring:
            try:
                stats = self.lxc.get_stats(container_name)
                current_time = time.time()
                
                if last_cpu > 0:
                    cpu_diff = stats["cpu_seconds"] - last_cpu
                    time_diff = current_time - last_time
                    if time_diff > 0:
                        cpu_percent = (cpu_diff / time_diff) * 100.0
                        self.results.timestamps.append(current_time)
                        self.results.cpu_usage.append(cpu_percent)
                        self.results.mem_usage.append(stats["mem_mb"])
                
                last_cpu = stats["cpu_seconds"]
                last_time = current_time
            except:
                pass
            time.sleep(MONITOR_INTERVAL)

    def live_charts(self):
        """Periodically generate charts while the test is running."""
        while not self.stop_monitoring:
            time.sleep(CHART_INTERVAL)
            try:
                # Silently generate charts in the background
                with contextlib.redirect_stdout(io.StringIO()):
                    self.generate_charts(quiet=True)
            except:
                pass

    def run(self):
        print("\n" + "="*50)
        print(f"🚀 MADMAIL GROUP STRESS TEST: {self.user_count} Users | {self.msg_count} Msgs")
        print("="*50 + "\n")
        
        ips = self.lxc.setup()
        remote1 = ips[0] if ips else "127.0.0.1"
        container_name = "madmail-server1"
        
        monitor_thread = threading.Thread(target=self.monitor_stats, args=(container_name,))
        monitor_thread.start()
        
        live_chart_thread = threading.Thread(target=self.live_charts,)
        live_chart_thread.start()
        
        try:
            with self.rpc:
                dc = DeltaChat(self.rpc)
                
                # Phase 1: Account Creation
                print("🔹 Phase 1: Creating Accounts...")
                accounts = []
                rpc_lock = threading.Lock()
                
                def create_account(idx):
                    f = io.StringIO()
                    try:
                        with contextlib.redirect_stdout(f), contextlib.redirect_stderr(f):
                            with rpc_lock:
                                # We add a small delay to ensure the server handles requests correctly
                                time.sleep(idx * 0.5) 
                                acc = test_01_account_creation.run(dc, remote1)
                            return acc, idx, 0.0
                    except Exception as e:
                        logger.exception(f"Account {idx} creation failed")
                        return None, idx, str(e)

                with concurrent.futures.ThreadPoolExecutor(max_workers=ACCOUNTS_WORKERS) as executor:
                    futures = [executor.submit(create_account, i) for i in range(self.user_count)]
                    completed = 0
                    for future in concurrent.futures.as_completed(futures):
                        res_acc, idx, err = future.result()
                        completed += 1
                        if res_acc:
                            accounts.append(res_acc)
                            sys.stdout.write(f"\r  ✓ Ready: {len(accounts)}/{self.user_count} ")
                        else:
                            sys.stdout.write(f"\n  ✗ Failed: Account {idx} ({err})\n")
                        sys.stdout.flush()
                print("\n")
                
                if len(accounts) < 2:
                    raise Exception("No members created. Aborting.")

                admin = accounts[0]
                members = accounts[1:]
                
                # Phase 2: Secure Join
                print("🔹 Phase 2: Establishing PGP Connections (Sequentially for 1-Core reliability)...")
                successful_members = []
                def do_secure_join(member):
                    addr = member.get_config('addr')
                    try:
                        with contextlib.redirect_stdout(io.StringIO()):
                            test_03_secure_join.run(self.rpc, admin, member)
                        return member, True
                    except Exception:
                        # Fallback for stress test continuity
                        try:
                            with contextlib.redirect_stdout(io.StringIO()):
                                admin.create_contact(addr)
                            return member, False
                        except:
                            return None, False

                with concurrent.futures.ThreadPoolExecutor(max_workers=SECURE_JOIN_WORKERS) as executor:
                    futures = [executor.submit(do_secure_join, m) for m in members]
                    for future in concurrent.futures.as_completed(futures):
                        res, verified = future.result()
                        if res:
                            successful_members.append(res)
                            status = "✓" if verified else "!"
                            sys.stdout.write(f"\r  {status} Handshakes: {len(successful_members)}/{len(members)} ")
                            sys.stdout.flush()
                print("\n")
                
                if not successful_members:
                    raise Exception("No members successfully joined.")
                
                members = successful_members

                # Phase 3: Group Creation
                group_name = f"Stress Group {datetime.datetime.now().strftime('%H%M%S')}"
                group = admin.create_group(group_name)
                print(f"🔹 Phase 3: Setup Group '{group_name}'...")
                final_members = []
                for i, member in enumerate(members):
                    member_addr = member.get_config("addr")
                    contact = admin.get_contact_by_addr(member_addr)
                    if not contact:
                        contact = admin.create_contact(member_addr)
                    try:
                        group.add_contact(contact)
                        final_members.append(member)
                    except Exception as e:
                        # "Only key-contacts can be added to encrypted chats"
                        pass
                    sys.stdout.write(f"\r  ✓ Ready: {len(final_members)}/{len(members)} ")
                    sys.stdout.flush()
                print("\n")
                
                if not final_members:
                    raise Exception("No members could be added to the group.")
                
                members = final_members
                
                # Phase 3.5: Accept Group
                print("🔹 Phase 3.5: Joining Group Members (Event-based)...")
                def accept_groups(member):
                    start = time.time()
                    while time.time() - start < 60:
                        event = member.wait_for_event(timeout=5.0)
                        if event and event.kind == EventType.INCOMING_MSG:
                            msg = member.get_message_by_id(event.msg_id)
                            snap = msg.get_snapshot()
                            if group_name in snap.chat_name:
                                chat = member.get_chat_by_id(snap.chat_id)
                                chat.accept()
                                return True
                        elif event and event.kind == EventType.MEMBER_ADDED:
                            # Sometimes MEMBER_ADDED comes first depending on timing
                            # But accepting via INCOMING_MSG is more standard for group invites
                            pass
                    # Final fallback: check chatlist once
                    for chat in member.get_chatlist():
                        if group_name in chat.get_basic_snapshot().name:
                            chat.accept()
                            return True
                    return False

                with concurrent.futures.ThreadPoolExecutor(max_workers=min(len(members), 16)) as executor:
                    futures = [executor.submit(accept_groups, m) for m in members]
                    accepted_count = 0
                    for future in concurrent.futures.as_completed(futures):
                        if future.result():
                            accepted_count += 1
                        sys.stdout.write(f"\r  ✓ Joined: {accepted_count}/{len(members)} ")
                        sys.stdout.flush()
                print("\n")

                # Phase 4: Messaging Statistics
                print(f"🔹 Phase 4: Sending {self.msg_count} Messages & Verifying...")
                self.results.total_expected = self.msg_count * len(members)
                
                event_queue = queue.Queue()
                def receiver_worker(idx, member):
                    while not self.stop_monitoring:
                        try:
                            event = member.wait_for_event(timeout=1.0)
                            if event:
                                event_queue.put((idx, event))
                        except:
                            continue

                for idx, member in enumerate(members, 1):
                    threading.Thread(target=receiver_worker, args=(idx, member), daemon=True).start()

                for i in range(self.msg_count):
                    text = f"Stress Msg {i} - {datetime.datetime.now().isoformat()}"
                    try:
                        group.send_text(text)
                        self.results.sent_timestamps.append(time.time())
                        self.results.delivery_stats[text] = {}
                        sys.stdout.write(f"\r  ➜ Sent: {i+1}/{self.msg_count} msgs ")
                        sys.stdout.flush()
                    except:
                        pass
                    time.sleep(1)
                print()

                start_verify = time.time()
                while self.results.total_received < self.results.total_expected and (time.time() - start_verify < VERIFY_TIMEOUT):
                    try:
                        member_idx, event = event_queue.get(timeout=1.0)
                        if event.kind == EventType.INCOMING_MSG:
                            member = members[member_idx - 1]
                            snap = member.get_message_by_id(event.msg_id).get_snapshot()
                            if snap.text in self.results.delivery_stats:
                                if member_idx not in self.results.delivery_stats[snap.text]:
                                    self.results.delivery_stats[snap.text][member_idx] = time.time()
                                    self.results.total_received += 1
                                    sys.stdout.write(f"\r  ➜ Delivery: {self.results.total_received}/{self.results.total_expected} ")
                                    sys.stdout.flush()
                    except queue.Empty:
                        continue
                print("\n")
                
                if self.results.total_received < self.results.total_expected:
                    print(f"  🏁 Partial Success: {self.results.total_received}/{self.results.total_expected} deliveries.")
                else:
                    print("  ✨ SUCCESS: All deliveries completed!")

        except KeyboardInterrupt:
            print("\n\n⚠️ Interrupted. Finalizing partial reports...")
        except Exception as e:
            print(f"\n❌ Error: {e}")
            import traceback
            traceback.print_exc()
        finally:
            self.stop_monitoring = True
            if 'monitor_thread' in locals(): monitor_thread.join()
            if 'live_chart_thread' in locals(): live_chart_thread.join()
            self.lxc.cleanup()
            self.rpc_log_file.close()
            self.generate_charts()

    def generate_charts(self, quiet=False):
        if not quiet: print("📊 Finalizing Charts...")
        
        # 1. Reception Chart
        plt.figure(figsize=(12, 7))
        events = []
        for d in self.results.delivery_stats.values():
            events.extend(d.values())
        events.sort()
        
        base_time = self.results.sent_timestamps[0] if self.results.sent_timestamps else (events[0] if events else time.time())
            
        if self.results.sent_timestamps:
            sent_rel = [t - base_time for t in self.results.sent_timestamps]
            actual_recipients = self.results.total_expected // self.msg_count if self.msg_count > 0 else 0
            sent_counts = [(i+1) * actual_recipients for i in range(len(sent_rel))]
            plt.step(sent_rel, sent_counts, label="Directly Sent (Expected)", color="#3498db", where='post', alpha=0.6)

        if events:
            relative_times = [t - base_time for t in events]
            counts = list(range(1, len(events) + 1))
            plt.plot(relative_times, counts, label="Received", color="#2ecc71", linewidth=2.5)
            
        plt.axhline(y=self.results.total_expected, color='#34495e', linestyle=':', label="Target")
        plt.title(f"Message Reception Progress ({self.results.total_received}/{self.results.total_expected})", fontsize=14)
        plt.xlabel("Seconds from Start", fontsize=12)
        plt.ylabel("Message Count", fontsize=12)
        plt.legend(); plt.grid(True, alpha=0.3)
        plt.savefig(os.path.join(self.test_dir, "reception_chart.png"), dpi=150)
        plt.clf(); plt.close('all')

        # 2. Resource Chart
        if self.results.timestamps:
            fig, ax1 = plt.subplots(figsize=(13, 7))
            rel_times = [t - self.results.timestamps[0] for t in self.results.timestamps]
            ax1.set_xlabel('Seconds'); ax1.set_ylabel('CPU %', color='#e67e22')
            ax1.plot(rel_times, self.results.cpu_usage, color='#e67e22', linewidth=1.5)
            ax2 = ax1.twinx()
            ax2.set_ylabel('Mem (MiB)', color='#3498db')
            ax2.plot(rel_times, self.results.mem_usage, color='#3498db', linewidth=1.5)
            plt.title("LXC Resource Telemetery"); fig.tight_layout()
            plt.savefig(os.path.join(self.test_dir, "resource_usage_chart.png"), dpi=150)
            plt.clf(); plt.close('all')

        # 3. Account CPU Chart
        if self.results.account_creation_cpu:
            plt.figure(figsize=(11, 6))
            sorted_acc = sorted(self.results.account_creation_cpu, key=lambda x: x[0])
            plt.bar([x[0] for x in sorted_acc], [x[1] for x in sorted_acc], color="#9b59b6", alpha=0.7)
            plt.title("CPU Seconds per Account Creation"); plt.grid(axis='y', alpha=0.3)
            plt.savefig(os.path.join(self.test_dir, "account_creation_cpu.png"), dpi=150)
            plt.clf(); plt.close('all')
        
        if not quiet: print(f"✨ Test Summary Ready: {self.test_dir}")

if __name__ == "__main__":
    now = datetime.datetime.now().strftime("%Y%m%d_%H%M%S")
    test_dir = os.path.join(os.path.abspath("tmp"), f"stress_run_{now}")
    os.makedirs(test_dir, exist_ok=True)
    tester = StressTest(remote="127.0.0.1", test_dir=test_dir)
    tester.run()
