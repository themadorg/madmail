import os
import sys
import time
import json
import argparse
import threading
import datetime
import subprocess
import concurrent.futures
import shutil
import traceback
import multiprocessing
import logging
from typing import List, Dict

# Try to import matplotlib
try:
    import matplotlib
    matplotlib.use('Agg')
    import matplotlib.pyplot as plt
except ImportError:
    plt = None

# Add relative paths for imports
TEST_DIR = os.path.dirname(os.path.abspath(__file__))
MADMAIL_ROOT = os.path.abspath(os.path.join(TEST_DIR, "../.."))
LOG_DIR = os.path.join(MADMAIL_ROOT, "tmp")
os.makedirs(LOG_DIR, exist_ok=True)

sys.path.append(os.path.abspath(os.path.join(TEST_DIR, "../../chatmail-core/deltachat-rpc-client/src")))
sys.path.append(TEST_DIR)

from deltachat_rpc_client import DeltaChat, Rpc
from scenarios import test_01_account_creation, test_03_secure_join
from utils.lxc import LXCManager

RPC_SERVER_PATH = "/usr/bin/deltachat-rpc-server"

# Setup debug logger
DEBUG_LOG_PATH = os.path.join(LOG_DIR, "stress_debug.log")
debug_logger = logging.getLogger("stress_debug")
debug_logger.setLevel(logging.DEBUG)
_fh = logging.FileHandler(DEBUG_LOG_PATH, mode='w')
_fh.setFormatter(logging.Formatter('%(asctime)s - %(levelname)s - %(message)s'))
debug_logger.addHandler(_fh)

# --- Original Stress Functions (for main.py compatibility) ---

def _worker_run(queue, worker_id, remote, user_count, duration, test_dir):
    start_time = time.time()
    data_dir = os.path.join(test_dir, f"dc_data_worker_{worker_id}")
    os.makedirs(data_dir, exist_ok=True)
    accounts_path = os.path.join(data_dir, "accounts.toml")
    if not os.path.exists(accounts_path):
        with open(accounts_path, "w") as f:
            f.write('selected_account = 0\nnext_id = 1\naccounts = []\n')
    rpc_log_path = os.path.join(test_dir, f"client_debug_worker_{worker_id}.log")
    rpc_log_file = open(rpc_log_path, "w")

    env = os.environ.copy()
    env["RUST_LOG"] = "info"

    result = {
        "worker_id": worker_id,
        "users": user_count,
        "accounts_created": 0,
        "account_create_seconds": 0.0,
        "secure_join_seconds": 0.0,
        "messages_sent": 0,
        "send_seconds": 0.0,
        "errors": [],
    }

    rpc = Rpc(accounts_dir=data_dir, rpc_server_path=RPC_SERVER_PATH, stderr=rpc_log_file, env=env)

    try:
        with rpc:
            dc = DeltaChat(rpc)
            accounts = []

            create_start = time.time()
            for _ in range(user_count):
                account = test_01_account_creation.run(dc, remote)
                accounts.append(account)
            result["accounts_created"] = len(accounts)
            result["account_create_seconds"] = time.time() - create_start

            pairs = []
            for i in range(0, len(accounts) - 1, 2):
                pairs.append((accounts[i], accounts[i + 1]))

            secure_join_start = time.time()
            for acc_a, acc_b in pairs:
                test_03_secure_join.run(rpc, acc_a, acc_b)
            result["secure_join_seconds"] = time.time() - secure_join_start

            chats = []
            for acc_a, acc_b in pairs:
                acc_b_email = acc_b.get_config("addr")
                contact = acc_a.get_contact_by_addr(acc_b_email)
                if contact is None:
                    contact = acc_a.create_contact(acc_b_email)
                chats.append(contact.create_chat())

            send_start = time.time()
            msg_index = 0
            while time.time() - send_start < duration:
                for chat in chats:
                    chat.send_text(f"stress {worker_id} {msg_index}")
                    msg_index += 1
                    if time.time() - send_start >= duration:
                        break
            result["messages_sent"] = msg_index
            result["send_seconds"] = time.time() - send_start

    except Exception as exc:
        result["errors"].append(str(exc))
        result["errors"].append(traceback.format_exc())
    finally:
        rpc_log_file.close()
        result["worker_seconds"] = time.time() - start_time
        queue.put(result)


def run_stress(remote, test_dir, users, workers, duration, report_path):
    os.makedirs(test_dir, exist_ok=True)
    queue = multiprocessing.Queue()
    processes = []

    per_worker = [users // workers] * workers
    for i in range(users % workers):
        per_worker[i] += 1

    for worker_id, user_count in enumerate(per_worker, start=1):
        proc = multiprocessing.Process(
            target=_worker_run,
            args=(queue, worker_id, remote, user_count, duration, test_dir),
        )
        proc.start()
        processes.append(proc)

    results = []
    for _ in processes:
        results.append(queue.get())

    for proc in processes:
        proc.join()

    total_messages = sum(r["messages_sent"] for r in results)
    total_send_seconds = max((r["send_seconds"] for r in results), default=0.0)
    send_rate = total_messages / total_send_seconds if total_send_seconds > 0 else 0.0

    report = {
        "remote": remote,
        "users": users,
        "workers": workers,
        "duration_seconds": duration,
        "messages_sent": total_messages,
        "send_rate_mps": send_rate,
        "workers_results": results,
    }

    with open(report_path, "w") as f:
        json.dump(report, f, indent=2)

    report_md_path = report_path.rsplit(".", 1)[0] + ".md"
    _write_report_md(report_md_path, report)

    return report_path, report_md_path, report


def _write_report_md(report_md_path, report):
    users = report.get("users", 0)
    workers = report.get("workers", 0)
    duration = report.get("duration_seconds", 0)
    total_messages = report.get("messages_sent", 0)
    send_rate = report.get("send_rate_mps", 0.0)
    per_user_rate = (send_rate / users) if users else 0.0

    lines = [
        "# Madmail Stress Test Report",
        "",
        "## Goal",
        "Validate Madmail capacity under simulated multi-user load on low-resource hardware.",
        "",
        "## Test Setup",
        f"- Target server: {report.get('remote', 'unknown')}",
        f"- Users: {users}",
        f"- Workers: {workers}",
        f"- Send window: {duration}s",
        "",
        "## Key Results",
        f"- Total send attempts: {total_messages}",
        f"- Aggregate send rate: {send_rate:.2f} msg/sec",
        f"- Avg per-user send rate: {per_user_rate:.2f} msg/sec",
        "",
        "## Notes",
        "- Send rate reflects client-side send attempts only (not confirmed deliveries).",
        "- No server CPU/RAM telemetry captured in this run.",
        "",
        "## Raw Data",
        f"- JSON: {os.path.basename(report_md_path).rsplit('.', 1)[0]}.json",
        "",
    ]

    with open(report_md_path, "w") as f:
        f.write("\n".join(lines))

# --- New LXC-based Stress Functions ---

class StressStats:
    def __init__(self):
        self.timestamps = []
        self.cpu_usage = []
        self.mem_usage = []
        self.messages_sent = 0
        self.errors = []
        self.start_time = time.time()

class LXCMonitor:
    def __init__(self, lxc, container_name, interval=1.0):
        self.lxc = lxc
        self.container_name = container_name
        self.interval = interval
        self.stop_event = threading.Event()
        self.stats = StressStats()

    def run(self):
        last_cpu_time = 0
        last_check_time = time.time()
        debug_logger.info(f"Monitor started for {self.container_name}")
        
        while not self.stop_event.is_set():
            try:
                info = self.lxc.get_stats(self.container_name)
                current_time = time.time()
                
                if info["cpu_seconds"] == 0 and info["mem_mb"] == 0:
                    # Likely container is stopped or starting
                    time.sleep(self.interval)
                    continue

                if last_cpu_time > 0:
                    delta_cpu = info["cpu_seconds"] - last_cpu_time
                    delta_time = current_time - last_check_time
                    if delta_time > 0:
                        cpu_percent = (delta_cpu / delta_time) * 100.0
                        self.stats.timestamps.append(current_time - self.stats.start_time)
                        self.stats.cpu_usage.append(cpu_percent)
                        self.stats.mem_usage.append(info["mem_mb"])
                
                last_cpu_time = info["cpu_seconds"]
                last_check_time = current_time
            except Exception as e:
                debug_logger.error(f"Monitor error: {e}")
            
            self.stop_event.wait(self.interval)
        debug_logger.info(f"Monitor stopped for {self.container_name}")

    def stop(self):
        self.stop_event.set()


def generate_charts(stats, output_dir):
    if not plt:
        print("Matplotlib not installed, skipping chart generation.")
        return

    os.makedirs(output_dir, exist_ok=True)
    
    if not stats.timestamps:
        print("No telemetry data collected.")
        return

    # Resource Usage Chart
    plt.figure(figsize=(12, 7))
    fig, ax1 = plt.subplots(figsize=(12, 7))

    ax1.set_xlabel('Seconds from Start', fontsize=12)
    ax1.set_ylabel('CPU Usage (%)', color='#e67e22', fontsize=12)
    ax1.plot(stats.timestamps, stats.cpu_usage, color='#e67e22', linewidth=2, label='CPU Usage')
    ax1.tick_params(axis='y', labelcolor='#e67e22')
    ax1.set_ylim(0, max(stats.cpu_usage + [100]) * 1.1)

    ax2 = ax1.twinx()
    ax2.set_ylabel('Memory Usage (MB)', color='#3498db', fontsize=12)
    ax2.plot(stats.timestamps, stats.mem_usage, color='#3498db', linewidth=2, label='Memory Usage')
    ax2.tick_params(axis='y', labelcolor='#3498db')
    ax2.set_ylim(0, max(stats.mem_usage + [512]) * 1.2)

    plt.title('Madmail Binary Stress: Resource Telemetry', fontsize=14)
    fig.tight_layout()
    plt.grid(True, alpha=0.3)
    
    chart_path = os.path.join(output_dir, "resource_usage.png")
    plt.savefig(chart_path, dpi=150)
    print(f"📊 Chart generated: {chart_path}")
    plt.close('all')

def run_cmping_worker(server_ip, worker_id, count, results, debug=False):
    # Run internal cmping.py directly to save uv overhead, always use -v for heartbeat visibility
    cmping_path = os.path.join(MADMAIL_ROOT, "cmping", "cmping.py")
    cmd = [sys.executable, cmping_path, "-v", "-c", str(count), server_ip]
    cache_dir = f"/tmp/cmping_cache_{worker_id}"
    env = os.environ.copy()
    env["XDG_CACHE_HOME"] = cache_dir
    
    # Ensure worker uses the local modified rpc-client library
    rpc_client_src = os.path.abspath(os.path.join(MADMAIL_ROOT, "chatmail-core/deltachat-rpc-client/src"))
    current_pythonpath = env.get("PYTHONPATH", "")
    if current_pythonpath:
        env["PYTHONPATH"] = f"{rpc_client_src}:{current_pythonpath}"
    else:
        env["PYTHONPATH"] = rpc_client_src
    
    debug_logger.info(f"Worker {worker_id}: Starting...")
    
    try:
        start_time = time.time()
        # Use Popen to read real-time output, merging stderr into stdout
        process = subprocess.Popen(
            cmd, 
            stdout=subprocess.PIPE, 
            stderr=subprocess.STDOUT, 
            text=True, 
            env=env,
            bufsize=1
        )
        
        success_pings = 0
        last_output_time = time.time()
        
        # Read stdout in real-time to track pings
        while True:
            line = process.stdout.readline()
            if not line and process.poll() is not None:
                break
            if line:
                last_output_time = time.time()
                debug_logger.debug(f"Worker {worker_id} output: {line.strip()}")
                # Force flush of the file handler to ensure it's readable during the hang
                for handler in debug_logger.handlers:
                    handler.flush()
                
                if "seq=" in line:
                    success_pings += 1
                    # Update progress or just log it
                    if success_pings % 5 == 0:
                        debug_logger.info(f"Worker {worker_id}: {success_pings}/{count} pings...")
                
                # Check for CMPING statistics to mark completion
                if "--- Statistics:" in line:
                    debug_logger.info(f"Worker {worker_id}: Reached summary line.")

        stdout_remainder, _ = process.communicate(timeout=5)
        duration = time.time() - start_time
        
        if process.returncode == 0:
            results.append({"success": True, "duration": duration, "count": count, "worker_id": worker_id})
            print(f"  ✓ Worker {worker_id}: Complete ({duration:.1f}s)")
            debug_logger.info(f"Worker {worker_id}: SUCCESS")
        else:
            error_msg = stdout_remainder[:200] if stdout_remainder else "Process failed"
            results.append({"success": False, "error": error_msg, "duration": duration, "worker_id": worker_id})
            print(f"  ✗ Worker {worker_id}: FAILED")
            debug_logger.error(f"Worker {worker_id}: FAILED - {error_msg}")
    except Exception as e:
        results.append({"success": False, "error": str(e), "worker_id": worker_id})
        print(f"  ✗ Worker {worker_id}: EXCEPTION")
        debug_logger.exception(f"Worker {worker_id}: EXCEPTION - {e}")
    finally:
        shutil.rmtree(cache_dir, ignore_errors=True)

def perform_stress_test(server_ip, duration=60, workers=10, debug=False):
    print(f"\n🚀 STRESSING SERVER: {server_ip}")
    print(f"🔹 Workers: {workers} | Pings per worker: 20")
    print(f"📝 Debug log: {DEBUG_LOG_PATH}")
    
    debug_logger.info(f"Starting stress test: server={server_ip}, workers={workers}")
    
    test_results = []
    start_time = time.time()
    
    with concurrent.futures.ThreadPoolExecutor(max_workers=workers) as executor:
        futures = {}
        for i in range(workers):
            # Increase stagger to 4s for 1-core stability
            if i > 0:
                time.sleep(4.0)
            
            future = executor.submit(run_cmping_worker, server_ip, f"w{i}", 20, test_results, debug)
            futures[future] = i
            print(f"  ➜ Worker w{i} launched...")
        
        print("\n⏳ All workers launched. Waiting for pings to complete...")
        print("   (Check tmp/stress_debug.log for real-time heartbeat)")
        
        # Total timeout: registration (60s) + pings (interval*count) + buffer (120s)
        total_timeout = 60 + (1.1 * 20) + 120
        
        # Wait for all to finish with a safety timeout
        _, not_done = concurrent.futures.wait(futures, timeout=total_timeout)
        
        if not_done:
            print(f"  ⚠️ Warning: {len(not_done)} workers timed out after {total_timeout}s")
            debug_logger.warning(f"{len(not_done)} workers timed out")
            # Try to kill remaining processes if possible (not easy with executor)

    end_time = time.time()
    total_time = end_time - start_time
    successes = [r for r in test_results if r.get("success")]
    failures = [r for r in test_results if not r.get("success")]
    total_pings = sum(r.get("count", 0) for r in successes)
    
    print(f"\n{'='*50}")
    print(f"📊 STRESS TEST SUMMARY")
    print(f"{'='*50}")
    print(f"  ⏱️  Total time: {total_time:.2f}s")
    print(f"  ✓  Successful workers: {len(successes)}/{workers}")
    print(f"  ✗  Failed workers: {len(failures)}/{workers}")
    print(f"  📨 Total pings: {total_pings}")
    if total_time > 0:
        print(f"  🚀 Rate: {total_pings / total_time:.2f} pings/sec")
    
    # Log failures to debug log
    debug_logger.info(f"Test complete: {len(successes)}/{workers} success, {total_pings} pings in {total_time:.2f}s")
    for f in failures:
        debug_logger.error(f"Failure: Worker {f.get('worker_id', '?')}: {f.get('error', 'Unknown')}")

def collect_worker_logs(output_dir):
    """Find and collect core.log files from all worker cache directories."""
    logs_dir = os.path.join(output_dir, "worker_logs")
    os.makedirs(logs_dir, exist_ok=True)
    
    # Also collect the main stress debug log
    try:
        shutil.copy(DEBUG_LOG_PATH, os.path.join(output_dir, "stress_debug.log"))
    except:
        pass

    for i in range(50): # Check up to 50 workers
        worker_cache = f"/tmp/cmping_cache_w{i}"
        if os.path.exists(worker_cache):
            worker_log_dest = os.path.join(logs_dir, f"worker_w{i}")
            os.makedirs(worker_log_dest, exist_ok=True)
            
            # Search for core.log files
            for root, dirs, files in os.walk(worker_cache):
                if "core.log" in files:
                    src = os.path.join(root, "core.log")
                    # Use a unique name if multiple accounts in one worker folder
                    sub_path = os.path.relpath(root, worker_cache).replace(os.sep, "_")
                    dest = os.path.join(worker_log_dest, f"core_{sub_path}.log")
                    try:
                        shutil.copy(src, dest)
                        debug_logger.info(f"Collected log from w{i}: {src} -> {dest}")
                    except Exception as e:
                        debug_logger.warning(f"Failed to copy log from w{i}: {e}")


def main():

    parser = argparse.ArgumentParser(description="Madmail LXC Binary Stress Tester")
    parser.add_argument("--lxc-binary-stress", type=str, help="Path to madmail binary")
    parser.add_argument("--memory", type=str, default="1G", help="RAM limit (e.g. 1G)")
    parser.add_argument("--cpu", type=str, default="1", help="CPU limit (e.g. 1)")
    parser.add_argument("--workers", type=int, default=15, help="Concurrent workers")
    parser.add_argument("--duration", type=int, default=60, help="Target duration")
    parser.add_argument("--debug", action="store_true", help="Enable debug output")
    parser.add_argument("--reuse", action="store_true", help="Reuse existing LXC container and accounts")
    
    args = parser.parse_args()
    
    if args.lxc_binary_stress:
        binary_path = os.path.abspath(args.lxc_binary_stress)
        if not os.path.exists(binary_path):
            print(f"Error: Binary {binary_path} not found.")
            sys.exit(1)

        print("\n" + "="*60)
        print("🛡️ MADMAIL BINARY STRESS TEST WORKFLOW")
        print("="*60)
        print(f"Target Binary: {binary_path}")
        print(f"Environment: Debian 12 | RAM: {args.memory} | CPU: {args.cpu}")
        if args.debug:
            print("[DEBUG] Debug mode enabled")
        
        lxc = LXCManager(memory_limit=args.memory, cpu_limit=args.cpu)
        container_name = "madmail-stress-node"
        
        try:
            # Setup
            print("🔹 Phase 1: LXC Setup & Binary Injection...")
            ips = lxc.setup(containers=[container_name], binary_path=binary_path, reuse_existing=args.reuse)
            server_ip = ips[0]
            
            # Monitoring
            monitor = LXCMonitor(lxc, container_name)
            monitor_thread = threading.Thread(target=monitor.run, daemon=True)
            monitor_thread.start()
            
            # Stress
            print("🔹 Phase 2: Running Stress Load...")
            perform_stress_test(server_ip, duration=args.duration, workers=args.workers, debug=args.debug)
            
            # Finalize
            monitor.stop()
            monitor_thread.join(timeout=2.0)
            
            print("🔹 Phase 3: Generating Performance Reports...")
            os.makedirs(output_dir, exist_ok=True)
            generate_charts(monitor.stats, output_dir)
            
            print("🔹 Phase 4: Collecting worker logs...")
            collect_worker_logs(output_dir)
            
            print(f"\n✅ STRESS TEST COMPLETE")
            print(f"📂 Results are saved in: {output_dir}")
            
        except Exception as e:
            print(f"❌ Critical Error: {e}")
            debug_logger.exception(f"Critical Error in main: {e}")
            traceback.print_exc()
        finally:
            if not args.reuse:
                print("🔹 Phase 4: Cleaning up...")
                try:
                    if 'monitor' in locals():
                        monitor.stop()
                    lxc.cleanup()
                except Exception as cleanup_err:
                    debug_logger.error(f"Cleanup error: {cleanup_err}")
                    print(f"⚠️ Cleanup partially failed: {cleanup_err}")
            else:
                print("🔹 Phase 4: Skipping cleanup (reuse enabled)...")
                # Ensure monitor is stopped even if cleanup is skipped
                if 'monitor' in locals():
                    monitor.stop()
    else:
        # Fallback to old behavior or show help
        parser.print_help()

if __name__ == "__main__":
    main()
