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

# Test Magic Numbers - Higher stress to see memory effects
USERS_TOTAL = 30
DURATION_S = 180
WORKERS_COUNT = 3
TELEMETRY_INTERVAL = 1

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
    data_dir = os.path.join(test_dir, f"dc_data_worker_{worker_id}")
    os.makedirs(data_dir, exist_ok=True)
    accounts_path = os.path.join(data_dir, "accounts.toml")
    if not os.path.exists(accounts_path):
        with open(accounts_path, "w") as f:
            f.write('selected_account = 0\nnext_id = 1\naccounts = []\n')

    rpc_log_path = os.path.join(test_dir, f"client_debug_worker_{worker_id}.log")
    rpc_log_file = open(rpc_log_path, "w")

    try:
        rpc = Rpc(accounts_dir=data_dir, rpc_server_path=RPC_SERVER_PATH, stderr=rpc_log_file)
        messages_timeline = []

        with rpc:
            dc = DeltaChat(rpc)
            accounts = []
            for i in range(user_count):
                acc = test_01_account_creation.run(dc, remote)
                accounts.append(acc)

            chats = []
            for acc in accounts:
                contact = acc.create_contact(acc.get_config("addr"))
                chats.append(contact.create_chat())

            start_time = time.time()
            msg_count = 0
            while time.time() - start_time < duration:
                for chat in chats:
                    chat.send_text(f"stress {worker_id} {msg_count}")
                    msg_count += 1
                    messages_timeline.append(time.time())
                    if time.time() - start_time >= duration:
                        break
            
            queue.put({"worker_id": worker_id, "sent": msg_count, "timeline": messages_timeline})
    except Exception as e:
        queue.put({"worker_id": worker_id, "sent": 0, "timeline": [], "error": str(e)})
    finally:
        rpc_log_file.close()

def run_scenario(ip, name, binary_path, require_pgp=True, duration=DURATION_S, users=USERS_TOTAL, workers=WORKERS_COUNT):
    print(f"\n" + "="*60)
    print(f"SCENARIO: {name} (PGP Required: {require_pgp})")
    print("="*60)
    
    timestamp = datetime.datetime.now().strftime("%Y%m%d_%H%M%S")
    scenario_dir = os.path.join(TEST_DIR, "../../tmp", f"stress_final_{name}_{timestamp}")
    os.makedirs(scenario_dir, exist_ok=True)

    # Stop maddy, push new binary, configure, restart
    print(f"Pushing {binary_path} to container...")
    subprocess.run(f"ssh -o StrictHostKeyChecking=no root@{ip} \"systemctl stop maddy\"", shell=True)
    subprocess.run(f"scp -o StrictHostKeyChecking=no {binary_path} root@{ip}:/usr/local/bin/maddy", shell=True, check=True)
    
    # Configure require_encryption
    pgp_val = "yes" if require_pgp else "no"
    print(f"Configuring maddy with require_encryption {pgp_val}...")
    sed_cmd = f"ssh -o StrictHostKeyChecking=no root@{ip} \"sed -i '/require_encryption/d' /etc/maddy/maddy.conf && sed -i '/endpoint imap/a \\    require_encryption {pgp_val}' /etc/maddy/maddy.conf && sed -i '/submission {ip}/a \\    require_encryption {pgp_val}' /etc/maddy/maddy.conf && systemctl start maddy\""
    subprocess.run(sed_cmd, shell=True, check=True)
    
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

def plot_final_results(results_list):
    print("Generating optimization comparison graphs...")
    fig, (ax1, ax2) = plt.subplots(2, 1, figsize=(10, 10))

    colors = ["red", "blue", "green"]
    for i, res in enumerate(results_list):
        if res["telemetry"]:
            start = res["telemetry"][0]["time"]
            t = [s["time"] - start for s in res["telemetry"]]
            
            # CPU
            v_cpu = [s["cpu"] for s in res["telemetry"]]
            ax1.plot(t, v_cpu, label=res["name"], color=colors[i % len(colors)], alpha=0.8)
            
            # RAM
            v_mem = [s["rss_kb"] / 1024.0 for s in res["telemetry"]]
            ax2.plot(t, v_mem, label=res["name"], color=colors[i % len(colors)], alpha=0.8)

    ax1.set_title("CPU Usage Comparison")
    ax1.set_ylabel("CPU %")
    ax1.legend()
    ax1.grid(True, alpha=0.3)

    ax2.set_title("Memory Usage (RSS) Comparison")
    ax2.set_ylabel("MB")
    ax2.legend()
    ax2.grid(True, alpha=0.3)

    plt.tight_layout()
    report_img = os.path.join(TEST_DIR, "../../tmp", f"pgp_optimization_{datetime.datetime.now().strftime('%Y%m%d_%H%M%S')}.png")
    plt.savefig(report_img)
    print(f"Comparison graph saved to: {report_img}")
    return report_img

def main():
    # Keep the 1 core / 1GB limit to see impact clearly
    lxc = LXCManager(cpu_limit=1, mem_limit="1G")
    
    print(f"Initializing Final Optimization Stress Test: {USERS_TOTAL} users, {DURATION_S}s duration.")
    remote1, remote2 = lxc.setup()
    
    try:
        old_bin = os.path.abspath("build/maddy_pgp_enabled") # Version with ReadAll bug
        new_bin = os.path.abspath("build/maddy_pgp_optimized") # Version with Streaming fix

        scenarios = []

        # 1. Old Version (Buffers everything)
        res_old = run_scenario(remote1, "PGP_Old_Enabled", old_bin, require_pgp=True)
        if res_old: scenarios.append(res_old)

        # 2. Optimized Version (Streaming)
        res_opt_en = run_scenario(remote1, "PGP_Opt_Enabled", new_bin, require_pgp=True)
        if res_opt_en: scenarios.append(res_opt_en)

        # 3. Optimized Version (Disabled)
        res_opt_dis = run_scenario(remote1, "PGP_Opt_Disabled", new_bin, require_pgp=False)
        if res_opt_dis: scenarios.append(res_opt_dis)

        img_path = plot_final_results(scenarios)
        
        summary_path = os.path.join(TEST_DIR, "../../tmp", "pgp_optimization_summary.md")
        with open(summary_path, "w") as f:
            f.write("# PGP Streaming Optimization Report\n\n")
            f.write("| Scenario | Total Messages | Peak RAM (MB) |\n")
            f.write("|----------|----------------|---------------|\n")
            for s in scenarios:
                peak_ram = max(st["rss_kb"] for st in s["telemetry"]) / 1024.0 if s["telemetry"] else 0
                f.write(f"| {s['name']} | {s['total_sent']} | {peak_ram:.2f} |\n")
            
            f.write(f"\n![Comparison Output]({os.path.basename(img_path)})\n")
        
        print(f"Summary report written to: {summary_path}")

    except Exception as e:
        print(f"Main suite error: {e}")
        import traceback
        traceback.print_exc()
    finally:
        lxc.cleanup()

if __name__ == "__main__":
    multiprocessing.set_start_method('spawn', force=True)
    main()
