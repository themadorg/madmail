import os
import sys
import time
import json
import subprocess
import threading
import datetime
import multiprocessing
from multiprocessing import Process, Queue

# Set matplotlib backend before importing it
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt

# Add project paths
TEST_DIR = os.path.dirname(os.path.abspath(__file__))
sys.path.append(os.path.abspath("chatmail-core/deltachat-rpc-client/src"))
sys.path.append(TEST_DIR)

from deltachat_rpc_client import DeltaChat, Rpc
from scenarios import test_01_account_creation
from utils.lxc import LXCManager

RPC_SERVER_PATH = "/usr/bin/deltachat-rpc-server"

# Test Magic Numbers
USERS_TOTAL = 10
DURATION_S = 60
WORKERS_COUNT = 2
TELEMETRY_INTERVAL = 1
SYNC_INTERVAL = "20m"

def wait_for_port(ip, port, timeout=30):
    start = time.time()
    while time.time() - start < timeout:
        try:
            with subprocess.Popen(["nc", "-zv", ip, str(port)], stdout=subprocess.PIPE, stderr=subprocess.PIPE) as p:
                p.wait(timeout=1)
                if p.returncode == 0:
                    return True
        except:
            pass
        time.sleep(1)
    return False

class TelemetryCollector(threading.Thread):
    def __init__(self, ip, interval=TELEMETRY_INTERVAL):
        super().__init__()
        self.ip = ip
        self.interval = interval
        self.stop_event = threading.Event()
        self.stats = []

    def get_stats(self):
        try:
            # Get CPU %, Mem % and RSS for maddy process
            cmd = ["ssh", "-o", "StrictHostKeyChecking=no", "-o", "BatchMode=yes", f"root@{self.ip}", "ps -C maddy -o %cpu,%mem,rss --no-headers"]
            out = subprocess.check_output(cmd, stderr=subprocess.DEVNULL).decode().strip()
            if out:
                parts = out.split()
                return {
                    "time": time.time(),
                    "cpu": float(parts[0]),
                    "mem_p": float(parts[1]),
                    "rss_kb": int(parts[2])
                }
        except Exception:
            pass
        return None

    def run(self):
        while not self.stop_event.is_set():
            stat = self.get_stats()
            if stat:
                self.stats.append(stat)
            time.sleep(self.interval)

    def stop(self):
        self.stop_event.set()

def _worker_run(queue, worker_id, remote, user_count, duration, test_dir):
    print(f"[Worker {worker_id}] Starting...")
    data_dir = os.path.join(test_dir, f"dc_data_worker_{worker_id}")
    os.makedirs(data_dir, exist_ok=True)
    
    # DeltaChat RPC server requires accounts.toml to exist
    accounts_path = os.path.join(data_dir, "accounts.toml")
    if not os.path.exists(accounts_path):
        with open(accounts_path, "w") as f:
            f.write('selected_account = 0\nnext_id = 1\naccounts = []\n')

    rpc_log_path = os.path.join(test_dir, f"client_debug_worker_{worker_id}.log")
    
    # Use different RPC socket path for each worker to avoid collision
    # DeltaChat Rpc client uses stdin/stdout by default, so it's fine for multiple processes
    rpc_log_file = open(rpc_log_path, "w")

    try:
        rpc = Rpc(accounts_dir=data_dir, rpc_server_path=RPC_SERVER_PATH, stderr=rpc_log_file)
        messages_timeline = []

        with rpc:
            dc = DeltaChat(rpc)
            accounts = []
            for i in range(user_count):
                print(f"[Worker {worker_id}] Creating account {i+1}/{user_count} on {remote}...")
                acc = test_01_account_creation.run(dc, remote)
                accounts.append(acc)

            print(f"[Worker {worker_id}] All accounts created. Setting up chats...")
            chats = []
            for i, acc_a in enumerate(accounts):
                for j, acc_b in enumerate(accounts):
                    if i == j: continue
                    email_b = acc_b.get_config("addr")
                    contact = acc_a.create_contact(email_b)
                    chats.append(contact.create_chat())

            print(f"[Worker {worker_id}] Starting message flood for {duration} seconds...")
            start_time = time.time()
            msg_count = 0
            while time.time() - start_time < duration:
                for chat in chats:
                    chat.send_text(f"stress {worker_id} {msg_count}")
                    msg_count += 1
                    messages_timeline.append(time.time())
                    if time.time() - start_time >= duration:
                        break
            
            print(f"[Worker {worker_id}] Flood complete. Sent {msg_count} messages.")
            queue.put({"worker_id": worker_id, "sent": msg_count, "timeline": messages_timeline})
    except Exception as e:
        print(f"[Worker {worker_id}] CRITICAL ERROR: {e}")
        import traceback
        traceback.print_exc()
        queue.put({"worker_id": worker_id, "sent": 0, "timeline": [], "error": str(e)})
    finally:
        rpc_log_file.close()

def run_scenario(ip, name, duration=DURATION_S, users=USERS_TOTAL, workers=WORKERS_COUNT):
    print(f"\n" + "="*50)
    print(f"SCENARIO: {name}")
    print("="*50)
    
    timestamp = datetime.datetime.now().strftime("%Y%m%d_%H%M%S")
    scenario_dir = os.path.join(TEST_DIR, "../../tmp", f"stress_{name}_{timestamp}")
    os.makedirs(scenario_dir, exist_ok=True)

    # Configure Maddy
    if name == "Memory":
        print("Enabling In-Memory SQLite on remote...")
        # Use a more robust sed that handles potential missing lines or multiple occurrences
        cmd = f"ssh -o StrictHostKeyChecking=no root@{ip} \"sed -i '/sqlite_in_memory/d' /etc/maddy/maddy.conf && sed -i '/sqlite_sync_interval/d' /etc/maddy/maddy.conf && sed -i '/table_name passwords/a \\        sqlite_in_memory yes\\n        sqlite_sync_interval {SYNC_INTERVAL}' /etc/maddy/maddy.conf && sed -i '/dsn imapsql.db/a \\    sqlite_in_memory yes\\n    sqlite_sync_interval {SYNC_INTERVAL}' /etc/maddy/maddy.conf && systemctl restart maddy\""
    else:
        print("Disabling In-Memory SQLite (Disk Mode) on remote...")
        cmd = f"ssh -o StrictHostKeyChecking=no root@{ip} \"sed -i '/sqlite_in_memory/d' /etc/maddy/maddy.conf && sed -i '/sqlite_sync_interval/d' /etc/maddy/maddy.conf && systemctl restart maddy\""
    
    subprocess.run(cmd, shell=True, check=True)
    print("Waiting for ports to open...")
    if not wait_for_port(ip, 993) or not wait_for_port(ip, 465):
        print("ERROR: Maddy failed to restart properly")
        return None

    collector = TelemetryCollector(ip)
    collector.start()

    print(f"Starting {workers} workers to create {users} accounts total...")
    queue = Queue()
    procs = []
    per_worker = users // workers
    for i in range(workers):
        p = Process(target=_worker_run, args=(queue, i+1, ip, per_worker, duration, scenario_dir))
        p.start()
        procs.append(p)

    results = []
    for _ in procs:
        res = queue.get()
        results.append(res)
        if "error" in res:
            print(f"Worker report error: {res['error']}")
            
    for p in procs:
        p.join()

    collector.stop()
    collector.join()

    return {
        "name": name,
        "results": results,
        "telemetry": collector.stats,
        "total_sent": sum(r["sent"] for r in results)
    }

def plot_results(disk_res, mem_res):
    print("Generating comparison graphs...")
    fig, (ax1, ax2, ax3) = plt.subplots(3, 1, figsize=(10, 15))

    def get_timeline_buckets(scen_res):
        all_times = []
        for r in scen_res["results"]:
            all_times.extend(r["timeline"])
        if not all_times: return [], []
        start = min(all_times)
        end = max(all_times)
        bucket_size = 2.0 # 2 seconds resolution
        num_buckets = int((end - start) / bucket_size) + 1
        buckets = [0] * num_buckets
        for t in all_times:
            idx = int((t - start) / bucket_size)
            if idx < num_buckets:
                buckets[idx] += 1
        return [i * bucket_size for i in range(num_buckets)], [b / bucket_size for b in buckets]

    t_disk, b_disk = get_timeline_buckets(disk_res)
    t_mem, b_mem = get_timeline_buckets(mem_res)

    if t_disk: ax1.plot(t_disk, b_disk, label="Disk", color="blue", linewidth=2)
    if t_mem: ax1.plot(t_mem, b_mem, label="Memory", color="green", linewidth=2)
    ax1.set_title("Messages Sent Per Second")
    ax1.set_xlabel("Time (s)")
    ax1.set_ylabel("Msg/s")
    ax1.grid(True, alpha=0.3)
    ax1.legend()

    # Telemetry plotting
    def plot_telemetry(ax, key, title, ylabel, unit_div=1.0):
        if disk_res["telemetry"]:
            start = disk_res["telemetry"][0]["time"]
            t = [s["time"] - start for s in disk_res["telemetry"]]
            v = [s[key] / unit_div for s in disk_res["telemetry"]]
            ax.plot(t, v, label="Disk", color="blue", alpha=0.7)
        if mem_res["telemetry"]:
            start = mem_res["telemetry"][0]["time"]
            t = [s["time"] - start for s in mem_res["telemetry"]]
            v = [s[key] / unit_div for s in mem_res["telemetry"]]
            ax.plot(t, v, label="Memory", color="green", alpha=0.7)
        ax.set_title(title)
        ax.set_xlabel("Time (s)")
        ax.set_ylabel(ylabel)
        ax.grid(True, alpha=0.3)
        ax.legend()

    plot_telemetry(ax2, "cpu", "CPU Usage", "CPU %")
    plot_telemetry(ax3, "rss_kb", "Memory Usage (RSS)", "MB", unit_div=1024.0)

    plt.tight_layout()
    report_img = os.path.join(TEST_DIR, "../../tmp", f"stress_comparison_{datetime.datetime.now().strftime('%Y%m%d_%H%M%S')}.png")
    plt.savefig(report_img)
    print(f"Comparison graph saved to: {report_img}")
    return report_img

def main():
    lxc = LXCManager()
    
    print(f"Initializing Stress Test: {USERS_TOTAL} users, {DURATION_S}s duration.")
    remote1, remote2 = lxc.setup()
    
    try:
        disk_results = run_scenario(remote1, "Disk")
        if not disk_results: return
        
        mem_results = run_scenario(remote1, "Memory")
        if not mem_results: return

        print("\n" + "="*40)
        print("FINAL COMPARISON")
        print("="*40)
        print(f"Disk Total Messages:   {disk_results['total_sent']}")
        print(f"Memory Total Messages: {mem_results['total_sent']}")
        
        diff = mem_results['total_sent'] - disk_results['total_sent']
        perc = (diff / disk_results['total_sent'] * 100) if disk_results['total_sent'] > 0 else 0
        print(f"Improvement: {perc:+.2f}%")
        
        img_path = plot_results(disk_results, mem_results)
        
        summary_path = os.path.join(TEST_DIR, "../../tmp", "stress_comparison_summary.md")
        with open(summary_path, "w") as f:
            f.write("# Stress Test Comparison: Disk vs In-Memory SQLite\n\n")
            f.write(f"## Aggregate Performance\n")
            f.write(f"- **Disk Mode**: {disk_results['total_sent']} messages sent\n")
            f.write(f"- **Memory Mode**: {mem_results['total_sent']} messages sent\n\n")
            f.write(f"### Improvement: **{perc:+.2f}%**\n\n")
            f.write(f"## Telemetry and Throughput\n")
            f.write(f"![Comparison Output]({os.path.basename(img_path)})\n")
        
        print(f"Summary report written to: {summary_path}")

    except Exception as e:
        print(f"Main suite error: {e}")
        import traceback
        traceback.print_exc()
    finally:
        lxc.cleanup()

if __name__ == "__main__":
    # Fix for multiprocessing on some systems
    multiprocessing.set_start_method('spawn', force=True)
    main()
