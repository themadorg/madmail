import json
import os
import time
import traceback
import multiprocessing

from deltachat_rpc_client import DeltaChat, Rpc
from scenarios import test_01_account_creation, test_03_secure_join

RPC_SERVER_PATH = "/usr/bin/deltachat-rpc-server"

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
