"""
Delta Chat E2E Test Suite for Madmail Server

Tests:
  #1 - Account Creation
  #2 - Unencrypted Message Rejection
  #3 - Secure Join
  #4 - P2P Encrypted Message
  #5 - Group Creation & Message
  #6 - File Transfer
  #7 - Federation (cross-server messaging)
  #8 - No Logging Test (30 messages with logging disabled)
  #9-22 - see TEST_NAMES; #23 is big file roundtrip (SHA-256 verify)
"""

import os
import sys
import shutil
import time
import datetime
import subprocess
import io
import threading
from dotenv import load_dotenv

# Load environment variables from .env file
load_dotenv()

# Path to the rpc client
TEST_DIR = os.path.dirname(os.path.abspath(__file__))
PROJECT_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(TEST_DIR)))

sys.path.append(TEST_DIR)

from deltachat_rpc_client import DeltaChat, Rpc
from scenarios import (
    test_01_account_creation,
    test_02_unencrypted_rejection,
    test_03_secure_join,
    test_05_group_message,
    test_06_file_transfer,
    test_07_federation,
    test_08_no_logging,
    test_09_send_bigfile,
    test_10_upgrade_mechanism,
    test_11_jit_registration,
    test_12_smtp_imap_idle,
    test_13_concurrent_profiles,
    test_14_purge_messages,
    test_15_iroh_discovery,
    test_16_webxdc_realtime,
    test_17_admin_api,
    test_18_stealth_mode,
    test_19_login_validation,
    test_20_exchanger,
    test_21_exchanger_php,
    test_22_mxdeliv_security,
    test_23_bigfile_roundtrip,
)
from utils.lxc import LXCManager
from stress import run_stress

REMOTE1 = os.getenv("REMOTE1", "127.0.0.1")
REMOTE2 = os.getenv("REMOTE2", "127.0.0.1")
ROOT_DIR = PROJECT_ROOT

def collect_server_logs(test_dir, remote1, remote2):
    print("Collecting server logs...")
    try:
        for i, remote in enumerate([remote1, remote2], 1):
            try:
                log = subprocess.check_output(
                    [
                        "ssh",
                        "-o",
                        "StrictHostKeyChecking=no",
                        "-o",
                        "UserKnownHostsFile=/dev/null",
                        f"root@{remote}",
                        "journalctl -u maddy.service -u madmail.service -n 1000 --no-pager",
                    ],
                    timeout=15
                ).decode('utf-8', errors='ignore')
                with open(os.path.join(test_dir, f"server{i}_debug.log"), "w") as f:
                    f.write(log)
            except Exception as e:
                print(f"Failed to collect logs from server{i}: {e}")
    except Exception as e:
        print(f"Failed to collect logs: {e}")


import argparse

# ── ANSI color helpers ──────────────────────────────────
class _C:
    RST   = '\033[0m'
    BOLD  = '\033[1m'
    DIM   = '\033[2m'
    RED   = '\033[91m'
    GREEN = '\033[92m'
    CYAN  = '\033[96m'
    YELLOW = '\033[93m'
    MAG   = '\033[95m'
    WHITE = '\033[97m'
    BG_RED   = '\033[41m'
    BG_GREEN = '\033[42m'
    CLR_LINE = '\033[2K'

SPIN = ['⠋','⠙','⠹','⠸','⠼','⠴','⠦','⠧','⠇','⠏']

# Test registry: (number, short_name)
TEST_NAMES = {
    0:  'LXC Setup',
    1:  'Account Creation',
    2:  'Unencrypted Rejection',
    3:  'Secure Join',
    4:  'P2P Encrypted Msg',
    5:  'Group Message',
    6:  'File Transfer',
    7:  'Federation',
    8:  'No Logging',
    9:  'Big File',
    10: 'Upgrade Mechanism',
    11: 'JIT Registration',
    12: 'SMTP/IMAP IDLE',
    13: 'Concurrent Profiles',
    14: 'Purge Messages',
    15: 'Iroh Discovery',
    16: 'WebXDC Realtime',
    17: 'Admin API',
    18: 'Stealth Mode',
    19: 'Login Validation',
    20: 'Exchanger E2E',
    21: 'PHP Exchanger E2E',
    22: 'MxDeliv Security',
    23: 'Bigfile roundtrip (hash)',
}


class CoolReporter:
    """Minimal colored output for --cool mode."""

    def __init__(self, tests_to_run):
        self.tests = tests_to_run          # ordered list of test numbers
        self.status = {}                   # num -> 'pass' | 'fail' | 'run' | 'skip'
        self.errors = {}                   # num -> error string
        self._spin = 0
        self._current = None
        self._stop = False
        self._thread = None
        self._real_stdout = sys.stdout
        self._real_stderr = sys.stderr
        self._devnull = open(os.devnull, 'w')
        # initialize all as skip
        for n in TEST_NAMES:
            self.status[n] = 'skip'

    # ── rendering ───────────────────────────────────────
    def _badge(self, n):
        s = self.status.get(n, 'skip')
        name = TEST_NAMES.get(n, f'#{n}')
        if s == 'pass':
            return f"{_C.GREEN}✓{_C.RST}"
        elif s == 'fail':
            return f"{_C.RED}✗{_C.RST}"
        elif s == 'run':
            sp = SPIN[self._spin % len(SPIN)]
            return f"{_C.CYAN}{sp}{_C.RST}"
        else:
            return f"{_C.DIM}·{_C.RST}"

    def _render(self):
        parts = []
        for n in sorted(TEST_NAMES.keys()):
            if n == 0 and n not in self.tests:
                continue
            parts.append(self._badge(n))
        bar = ' '.join(parts)
        # current test label
        label = ''
        if self._current:
            name = TEST_NAMES.get(self._current, f'#{self._current}')
            label = f"  {_C.CYAN}{_C.BOLD}{name}{_C.RST}"
        self._real_stdout.write(f"{_C.CLR_LINE}\r  {bar}{label}")
        self._real_stdout.flush()

    def _spinner_loop(self):
        while not self._stop:
            self._spin += 1
            self._render()
            time.sleep(0.12)

    # ── public API ──────────────────────────────────────
    def start(self):
        """Print header and start spinner thread."""
        hdr = f"\n  {_C.MAG}{_C.BOLD}madmail{_C.RST}{_C.DIM} e2e{_C.RST}\n"
        self._real_stdout.write(hdr)
        self._stop = False
        self._thread = threading.Thread(target=self._spinner_loop, daemon=True)
        self._thread.start()

    def begin_test(self, n):
        """Mark test n as running and suppress stdout."""
        self._current = n
        self.status[n] = 'run'
        sys.stdout = self._devnull
        sys.stderr = self._devnull

    def end_test(self, n, passed, error=None):
        """Mark test n result and restore stdout."""
        sys.stdout = self._real_stdout
        sys.stderr = self._real_stderr
        self.status[n] = 'pass' if passed else 'fail'
        if error:
            self.errors[n] = str(error)
        self._current = None

    def finish(self):
        """Stop spinner and print summary."""
        self._stop = True
        if self._thread:
            self._thread.join(timeout=1)
        sys.stdout = self._real_stdout
        sys.stderr = self._real_stderr
        # final bar
        self._render()
        self._real_stdout.write('\n\n')

        relevant = {n: s for n, s in self.status.items() if n != 0 or n in self.tests}
        passed  = sum(1 for s in relevant.values() if s == 'pass')
        failed  = sum(1 for s in relevant.values() if s == 'fail')
        skipped = sum(1 for s in relevant.values() if s == 'skip')

        # legend
        self._real_stdout.write(f"  {_C.GREEN}{_C.BOLD}{passed} passed{_C.RST}")
        if failed:
            self._real_stdout.write(f"  {_C.RED}{_C.BOLD}{failed} failed{_C.RST}")
        if skipped:
            self._real_stdout.write(f"  {_C.DIM}{skipped} skipped{_C.RST}")
        self._real_stdout.write('\n')

        # show failures
        if self.errors:
            self._real_stdout.write(f"\n")
            for n, err in self.errors.items():
                name = TEST_NAMES.get(n, f'#{n}')
                # take first line of error only
                first_line = err.strip().split('\n')[0][:120]
                self._real_stdout.write(
                    f"  {_C.RED}✗ #{n} {name}{_C.RST}  {_C.DIM}{first_line}{_C.RST}\n"
                )

        if failed:
            self._real_stdout.write(f"\n  {_C.BG_RED}{_C.WHITE}{_C.BOLD} FAIL {_C.RST}\n")
        else:
            self._real_stdout.write(f"\n  {_C.BG_GREEN}{_C.WHITE}{_C.BOLD} PASS {_C.RST}\n")
        self._real_stdout.write('\n')
        self._devnull.close()

def main():
    parser = argparse.ArgumentParser(description="Delta Chat E2E Test Suite for Madmail Server")
    parser.add_argument("--test-1", action="store_true", help="Run Account Creation")
    parser.add_argument("--test-2", action="store_true", help="Run Unencrypted Message Rejection")
    parser.add_argument("--test-3", action="store_true", help="Run Secure Join")
    parser.add_argument("--test-4", action="store_true", help="Run P2P Encrypted Message")
    parser.add_argument("--test-5", action="store_true", help="Run Group Creation & Message")
    parser.add_argument("--test-6", action="store_true", help="Run File Transfer")
    parser.add_argument("--test-7", action="store_true", help="Run Federation")
    parser.add_argument("--test-8", action="store_true", help="Run No Logging Test")
    parser.add_argument("--test-9", action="store_true", help="Run Big File Test (10-70MB)")
    parser.add_argument("--test-10", action="store_true", help="Run Upgrade Mechanism Test")
    parser.add_argument("--test-11", action="store_true", help="Run JIT Registration Test")
    parser.add_argument("--test-12", action="store_true", help="Run SMTP/IMAP IDLE Test")
    parser.add_argument("--test-13", action="store_true", help="Run Concurrent Profiles Test")
    parser.add_argument("--test-14", action="store_true", help="Run Purge Messages Test")
    parser.add_argument("--test-15", action="store_true", help="Run Iroh Discovery Test")
    parser.add_argument("--test-16", action="store_true", help="Run WebXDC Realtime P2P Test")
    parser.add_argument("--test-17", action="store_true", help="Run Admin API Test")
    parser.add_argument("--test-18", action="store_true", help="Run Stealth / Camouflage Mode Test")
    parser.add_argument("--test-19", action="store_true", help="Run Login Domain Validation Test")
    parser.add_argument("--test-20", action="store_true", help="Run Madexchanger E2E Test (3 LXC containers)")
    parser.add_argument("--test-21", action="store_true", help="Run PHP Exchanger E2E Test (3 LXC containers)")
    parser.add_argument("--test-22", action="store_true", help="Run MxDeliv Security Validation Test")
    parser.add_argument(
        "--test-23", action="store_true", help="Run big encrypted file roundtrip (SHA-256) test"
    )
    parser.add_argument("--domain", help="Specify domain/IP for tests (updates REMOTE1/REMOTE2)")
    parser.add_argument("--lxc", action="store_true", help="Run tests in local LXC containers")
    parser.add_argument("--keep-lxc", action="store_true", help="Keep LXC containers alive after test")
    parser.add_argument("--all", action="store_true", help="Run all tests (default)")
    parser.add_argument("--no-test", type=str, default="", help="Comma-separated test numbers to skip, e.g. --no-test 13 or --no-test 12,13,14")
    parser.add_argument("--cool", action="store_true", help="Minimal colored output (show only pass/fail per test)")
    parser.add_argument(
        "--color",
        action="store_true",
        help="Colorize pass/fail result lines while keeping normal verbose output.",
    )
    parser.add_argument("--stress", action="store_true", help="Run stress test against a remote server")
    parser.add_argument("--stress-users", type=int, default=50, help="Total users to create (default: 50)")
    parser.add_argument("--stress-workers", type=int, default=8, help="Worker processes to use (default: 8)")
    parser.add_argument("--stress-duration", type=int, default=60, help="Send duration per worker in seconds (default: 60)")
    parser.add_argument("--stress-report", default="", help="Path to write stress report JSON")
    
    args = parser.parse_args()

    use_result_color = bool(args.color) and not bool(args.cool)

    def _green(text):
        if use_result_color:
            return f"{_C.GREEN}{text}{_C.RST}"
        return text

    def _red(text):
        if use_result_color:
            return f"{_C.RED}{text}{_C.RST}"
        return text

    # Parse excluded tests
    excluded_tests = set()
    if args.no_test:
        for part in args.no_test.replace(" ", "").split(","):
            try:
                excluded_tests.add(int(part))
            except ValueError:
                parser.error(f"Invalid test number in --no-test: {part}")
    
    # If no specific tests selected, run all
    run_all = args.all or not any([
        args.test_1, args.test_2, args.test_3, args.test_4,
        args.test_5, args.test_6, args.test_7, args.test_8, args.test_9,
        args.test_10, args.test_11, args.test_12, args.test_13, args.test_14,
        args.test_15, args.test_16, args.test_17, args.test_18,
        args.test_19, args.test_20, args.test_21, args.test_22, args.test_23,
    ])

    def should_run(n):
        """Return True if test #n should run, respecting --no-test exclusions."""
        if n in excluded_tests:
            return False
        return run_all or getattr(args, f"test_{n}", False)

    timestamp = datetime.datetime.now().strftime("%Y%m%d_%H%M%S")
    tmp_root = os.path.join(ROOT_DIR, "tmp")
    test_dir = os.path.abspath(os.path.join(tmp_root, f"test_run_{timestamp}"))
    os.makedirs(test_dir, exist_ok=True)

    # ── Cool mode setup ─────────────────────────────────
    cool = None
    if args.cool:
        tests_to_run = [n for n in sorted(TEST_NAMES.keys()) if should_run(n)]
        if args.lxc:
            tests_to_run = [0] + tests_to_run   # add LXC Setup phase
        cool = CoolReporter(tests_to_run)
        cool.start()

    if not cool:
        print(f"Starting E2E tests. Results will be stored in: {test_dir}")
        print("="*60)
    
    success = False
    acc1 = None
    acc2 = None
    acc3 = None
    group_chat = None
    lxc = None
    
    remote1 = args.domain or REMOTE1
    remote2 = args.domain or REMOTE2

    if args.stress:
        report_path = args.stress_report or os.path.join(test_dir, "stress_report.json")
        print("\n" + "="*60)
        print("STRESS TEST")
        print("="*60)
        report_path, report_md_path, report = run_stress(
            remote=remote1,
            test_dir=test_dir,
            users=args.stress_users,
            workers=args.stress_workers,
            duration=args.stress_duration,
            report_path=report_path,
        )
        print(f"Stress report written to {report_path}")
        print(f"Stakeholder report written to {report_md_path}")
        print(f"Messages sent: {report['messages_sent']}")
        print(f"Send rate (messages/sec): {report['send_rate_mps']:.2f}")
        return

    data_dir = os.path.join(test_dir, "dc_data")
    
    # RPC server logs
    rpc_log_path = os.path.join(test_dir, "client_debug.log")
    
    # Try to find the built rpc server first
    rpc_server_path = os.getenv("RPC_SERVER_PATH")
    if not rpc_server_path:
        local_rpc_bin = os.path.join(PROJECT_ROOT, "chatmail-core/target/debug/deltachat-rpc-server")
        if os.path.exists(local_rpc_bin):
            rpc_server_path = local_rpc_bin
        else:
            rpc_server_path = "/usr/bin/deltachat-rpc-server"
    
    rpc_log_file = open(rpc_log_path, "w")
    
    # Enable debug mode for JSON-RPC and Core
    env = os.environ.copy()
    env["RUST_LOG"] = "deltachat=trace,deltachat_rpc_server=trace,deltachat_jsonrpc=trace,deltachat_rpc_client=trace,info"
    
    rpc = Rpc(accounts_dir=data_dir, rpc_server_path=rpc_server_path, stderr=rpc_log_file, env=env)
    
    if args.lxc:
        if cool:
            cool.begin_test(0)
            try:
                _silent = lambda *a, **kw: None
                lxc = LXCManager(logger=_silent)
                ips = lxc.setup()
                remote1 = ips[0] if len(ips) > 0 else remote1
                remote2 = ips[1] if len(ips) > 1 else remote2
                cool.end_test(0, True)
            except Exception as e:
                cool.end_test(0, False, e)
                raise
        else:
            lxc = LXCManager()
            ips = lxc.setup()
            remote1 = ips[0] if len(ips) > 0 else remote1
            remote2 = ips[1] if len(ips) > 1 else remote2

    try:
        with rpc:
            dc = DeltaChat(rpc)

            # ==========================================
            # TEST #20: Madexchanger E2E (self-contained, own LXC)
            # ==========================================
            if args.test_20:
                if cool:
                    cool.begin_test(20)
                    try:
                        test_20_exchanger.run(
                            rpc, dc, remote1, remote2, test_dir, timestamp,
                            keep_lxc=args.keep_lxc,
                        )
                        cool.end_test(20, True)
                    except Exception as e:
                        cool.end_test(20, False, e)
                else:
                    print("\n" + "="*50)
                    print("TEST #20: Madexchanger E2E (3 LXC containers)")
                    print("="*50)
                    test_20_exchanger.run(
                        rpc, dc, remote1, remote2, test_dir, timestamp,
                        keep_lxc=args.keep_lxc,
                    )
                    print(_green("✓ TEST #20 PASSED: Exchanger E2E verified"))
                # If test_20 is the only test, we're done
                only_test_20 = not run_all and not any([
                    args.test_1, args.test_2, args.test_3, args.test_4,
                    args.test_5, args.test_6, args.test_7, args.test_8,
                    args.test_9, args.test_10, args.test_11, args.test_12,
                    args.test_13, args.test_14, args.test_15, args.test_16,
                    args.test_17, args.test_18, args.test_19, args.test_22,
                    args.test_23,
                ])
                if only_test_20:
                    if not cool:
                        print("\n" + "="*60)
                        print(_green("🎉 TEST #20 PASSED! 🎉"))
                        print("="*60)
                    success = True

            # ==========================================
            # TEST #21: PHP Exchanger E2E (self-contained, own LXC)
            # ==========================================
            if args.test_21:
                if cool:
                    cool.begin_test(21)
                    try:
                        test_21_exchanger_php.run(
                            rpc, dc, remote1, remote2, test_dir, timestamp,
                            keep_lxc=args.keep_lxc,
                        )
                        cool.end_test(21, True)
                    except Exception as e:
                        cool.end_test(21, False, e)
                else:
                    print("\n" + "="*50)
                    print("TEST #21: PHP Exchanger E2E (3 LXC containers)")
                    print("="*50)
                    test_21_exchanger_php.run(
                        rpc, dc, remote1, remote2, test_dir, timestamp,
                        keep_lxc=args.keep_lxc,
                    )
                    print(_green("✓ TEST #21 PASSED: PHP Exchanger E2E verified"))
                # If test_21 is the only test, we're done
                only_test_21 = not run_all and not any([
                    args.test_1, args.test_2, args.test_3, args.test_4,
                    args.test_5, args.test_6, args.test_7, args.test_8,
                    args.test_9, args.test_10, args.test_11, args.test_12,
                    args.test_13, args.test_14, args.test_15, args.test_16,
                    args.test_17, args.test_18, args.test_19, args.test_20, args.test_22,
                    args.test_23,
                ])
                if only_test_21:
                    if not cool:
                        print("\n" + "="*60)
                        print(_green("🎉 TEST #21 PASSED! 🎉"))
                        print("="*60)
                    success = True

            # ==========================================
            # TEST #22: MxDeliv Security (standalone, no accounts needed)
            # ==========================================
            if should_run(22):
                if cool:
                    cool.begin_test(22)
                    try:
                        test_22_mxdeliv_security.run(dc, (remote1, remote2))
                        cool.end_test(22, True)
                    except Exception as e:
                        cool.end_test(22, False, e)
                else:
                    print("\n" + "="*50)
                    print("TEST #22: MxDeliv Security Validation")
                    print("="*50)
                    test_22_mxdeliv_security.run(dc, (remote1, remote2))
                    print(_green("✓ TEST #22 PASSED: MxDeliv security validation verified"))
                # If test_22 is the only test, we're done
                only_test_22 = not run_all and not any([
                    args.test_1, args.test_2, args.test_3, args.test_4,
                    args.test_5, args.test_6, args.test_7, args.test_8,
                    args.test_9, args.test_10, args.test_11, args.test_12,
                    args.test_13, args.test_14, args.test_15, args.test_16,
                    args.test_17, args.test_18, args.test_19, args.test_20, args.test_21,
                    args.test_23,
                ])
                if only_test_22:
                    if not cool:
                        print("\n" + "="*60)
                        print(_green("🎉 TEST #22 PASSED! 🎉"))
                        print("="*60)
                    success = True

            if success:
                pass  # standalone test was the only test and it passed
            else:
                # ==========================================
                # PRE-REQUISITE: Accounts needed for almost all tests
                # ==========================================
                if cool:
                    cool.begin_test(1)
                    try:
                        acc1 = test_01_account_creation.run(dc, remote1)
                        acc2 = test_01_account_creation.run(dc, remote2)
                        cool.end_test(1, True)
                    except Exception as e:
                        cool.end_test(1, False, e)
                        raise
                else:
                    print("\n" + "="*50)
                    print("INITIALIZING: Account Creation")
                    print("="*50)
                    acc1 = test_01_account_creation.run(dc, remote1)
                    acc2 = test_01_account_creation.run(dc, remote2)
                    if should_run(1):
                        print(_green("✓ TEST #1 PASSED: Accounts created successfully"))
                
                # Store credentials for SMTP test
                acc1_email = acc1.get_config("addr")
                acc1_password = acc1.get_config("mail_pw")
                acc2_email = acc2.get_config("addr")
                
                # ── Helper: run a test with cool-mode wrapping ──
                def _run_cool(num, fn):
                    """Run test function fn under cool mode, or normally."""
                    if cool:
                        cool.begin_test(num)
                        try:
                            result = fn()
                            cool.end_test(num, True)
                            return result
                        except Exception as e:
                            cool.end_test(num, False, e)
                            return None
                    else:
                        return fn()

                # ==========================================
                # TEST #2: Unencrypted Message Rejection
                # ==========================================
                if should_run(2):
                    def _t2():
                        if not cool:
                            print("\n" + "="*50)
                            print("TEST #2: Unencrypted Message Rejection")
                            print("="*50)
                        test_02_unencrypted_rejection.run_unencrypted_rejection_test(
                            sender_email=acc1_email,
                            sender_password=acc1_password,
                            receiver_email=acc2_email,
                            smtp_host=remote1
                        )
                        if not cool:
                            print(_green("✓ TEST #2 PASSED: Unencrypted messages correctly rejected"))
                    _run_cool(2, _t2)
                
                # ==========================================
                # TEST #3: Secure Join
                # ==========================================
                if should_run(3) or should_run(4) or should_run(5) or should_run(6) or should_run(8) or should_run(9) or should_run(23):
                    def _t3():
                        if not cool:
                            print("\n" + "="*50)
                            print("TEST #3: Secure Join (acc1 <-> acc2)")
                            print("="*50)
                        test_03_secure_join.run(rpc, acc1, acc2)
                        if not cool:
                            print(_green("✓ TEST #3 PASSED: Secure join completed successfully"))
                    _run_cool(3, _t3)
                
                # ==========================================
                # TEST #4: P2P Encrypted Message
                # ==========================================
                if should_run(4):
                    def _t4():
                        if not cool:
                            print("\n" + "="*50)
                            print("TEST #4: P2P Encrypted Message")
                            print("="*50)
                        test_02_unencrypted_rejection.run(acc1, acc2, f"P2P Test Message {timestamp}")
                        if not cool:
                            print(_green("✓ TEST #4 PASSED: P2P encrypted message delivered"))
                    _run_cool(4, _t4)
                
                # ==========================================
                # TEST #5: Group Creation & Message
                # ==========================================
                if should_run(5) or should_run(8):
                    def _t5():
                        nonlocal group_chat
                        if not cool:
                            print("\n" + "="*50)
                            print("TEST #5: Group Creation & Message")
                            print("="*50)
                        group_chat = test_05_group_message.run(acc1, acc2, f"Group {timestamp}")
                        if not cool:
                            print(_green("✓ TEST #5 PASSED: Group created and message delivered"))
                    _run_cool(5, _t5)
                
                # ==========================================
                # TEST #6: File Transfer
                # ==========================================
                if should_run(6):
                    def _t6():
                        if not cool:
                            print("\n" + "="*50)
                            print("TEST #6: File Transfer (1MB)")
                            print("="*50)
                        test_06_file_transfer.run(acc1, acc2, test_dir)
                        if not cool:
                            print(_green("✓ TEST #6 PASSED: File transfer completed with matching hash"))
                    _run_cool(6, _t6)
                
                # ==========================================
                # TEST #7: Federation
                # ==========================================
                if should_run(7):
                    def _t7():
                        nonlocal acc3
                        if not cool:
                            print("\n" + "="*50)
                            print("TEST #7: Federation (Cross-Server Messaging)")
                            print("="*50)
                        server_info = lxc.get_server_info() if args.lxc else None
                        acc3 = test_07_federation.run(rpc, dc, acc1, acc2, remote1, remote2, timestamp, server_info=server_info)
                        if not cool:
                            print(_green("✓ TEST #7 PASSED: Federation test completed successfully"))
                    _run_cool(7, _t7)
                
                # ==========================================
                # TEST #8: No Logging Test
                # ==========================================
                if should_run(8):
                    def _t8():
                        nonlocal acc3
                        if not cool:
                            print("\n" + "="*50)
                            print("TEST #8: No Logging Test")
                            print("="*50)
                        if acc3 is None:
                            print("  Initializing acc3 for No Logging test...")
                            acc3 = test_01_account_creation.run(dc, remote2)
                        test_08_no_logging.run(acc1, acc2, acc3, group_chat, (remote1, remote2))
                        if not cool:
                            print(_green("✓ TEST #8 PASSED: No logs generated with logging disabled"))
                    _run_cool(8, _t8)
                
                # ==========================================
                # TEST #9: Big File Test
                # ==========================================
                if should_run(9):
                    def _t9():
                        test_09_send_bigfile.run(acc1, acc2, test_dir, (remote1, remote2))
                        if not cool:
                            print(_green("✓ TEST #9 PASSED: Big file transfer timing completed"))
                    _run_cool(9, _t9)
                
                # ==========================================
                # TEST #10: Upgrade Mechanism
                # ==========================================
                if should_run(10):
                    def _t10():
                        if not cool:
                            print("\n" + "="*50)
                            print("TEST #10: Upgrade Mechanism")
                            print("="*50)
                        test_10_upgrade_mechanism.run(dc, remote1, test_dir)
                        if not cool:
                            print(_green("✓ TEST #10 PASSED: Upgrade/Update signature verification verified"))
                    _run_cool(10, _t10)

                # ==========================================
                # TEST #11: JIT Registration
                # ==========================================
                if should_run(11):
                    def _t11():
                        test_11_jit_registration.run(dc, (remote1, remote2))
                        if not cool:
                            print(_green("✓ TEST #11 PASSED: JIT registration verified"))
                    _run_cool(11, _t11)

                # ==========================================
                # TEST #12: SMTP/IMAP IDLE Test (local server)
                # ==========================================
                if should_run(12):
                    def _t12():
                        test_12_smtp_imap_idle.run(test_dir=test_dir)
                        if not cool:
                            print(_green("✓ TEST #12 PASSED: SMTP/IMAP IDLE test verified"))
                    _run_cool(12, _t12)

                # ==========================================
                # TEST #13: Concurrent Profiles
                # ==========================================
                if should_run(13):
                    def _t13():
                        test_13_concurrent_profiles.run(test_dir=test_dir)
                        if not cool:
                            print(_green("✓ TEST #13 PASSED: Concurrent profiles verified"))
                    _run_cool(13, _t13)

                # ==========================================
                # TEST #14: Purge Messages
                # ==========================================
                if should_run(14):
                    def _t14():
                        test_14_purge_messages.run(rpc, dc, acc1, acc2, remote1)
                        if not cool:
                            print(_green("✓ TEST #14 PASSED: Purge messages verified"))
                    _run_cool(14, _t14)

                # ==========================================
                # TEST #15: Iroh Discovery
                # ==========================================
                if should_run(15):
                    def _t15():
                        test_15_iroh_discovery.run(dc, remote1)
                        if not cool:
                            print(_green("✓ TEST #15 PASSED: Iroh discovery verified"))
                    _run_cool(15, _t15)

                # ==========================================
                # TEST #16: WebXDC Realtime
                # ==========================================
                if should_run(16):
                    def _t16():
                        test_16_webxdc_realtime.run(dc, remote1)
                        if not cool:
                            print(_green("✓ TEST #16 PASSED: WebXDC Realtime P2P verified"))
                    _run_cool(16, _t16)

                # ==========================================
                # TEST #17: Admin API
                # ==========================================
                if should_run(17):
                    def _t17():
                        test_17_admin_api.run(dc, remote1, test_dir)
                        if not cool:
                            print(_green("✓ TEST #17 PASSED: Admin API verified"))
                    _run_cool(17, _t17)

                # ==========================================
                # TEST #18: Stealth / Camouflage Mode
                # ==========================================
                if should_run(18):
                    def _t18():
                        if not cool:
                            print("\n" + "="*50)
                            print("TEST #18: Stealth / Camouflage Mode")
                            print("="*50)
                        test_18_stealth_mode.run(test_dir=test_dir)
                        if not cool:
                            print(_green("✓ TEST #18 PASSED: Stealth / Camouflage Mode verified"))
                    _run_cool(18, _t18)

                # ==========================================
                # TEST #19: Login Domain Validation
                # ==========================================
                if should_run(19):
                    def _t19():
                        if not cool:
                            print("\n" + "="*50)
                            print("TEST #19: Login Domain Validation")
                            print("="*50)
                        test_19_login_validation.run(dc, (remote1, remote2))
                        if not cool:
                            print(_green("✓ TEST #19 PASSED: Login domain validation verified"))
                    _run_cool(19, _t19)

                # ==========================================
                # TEST #23: Big encrypted file roundtrip (hash)
                # ==========================================
                if should_run(23):
                    def _t23():
                        if not cool:
                            print("\n" + "="*50)
                            print("TEST #23: Big file roundtrip (SHA-256)")
                            print("="*50)
                        test_23_bigfile_roundtrip.run(acc1, acc2, test_dir)
                        if not cool:
                            print(_green("✓ TEST #23 PASSED: Big file received with matching hash"))
                    _run_cool(23, _t23)

                # ==========================================
                # ALL TESTS COMPLETE
                # ==========================================
                if not cool:
                    print("\n" + "="*60)
                    print(_green("🎉 SELECTED TESTS PASSED! 🎉"))
                    print("="*60)
                success = True
            
    except Exception as e:
        if cool:
            # Restore stdout so we can print the summary
            sys.stdout = cool._real_stdout
            sys.stderr = cool._real_stderr
        if not cool:
            print(_red(f"\n❌ TEST FAILED: {e}"))
            import traceback
            traceback.print_exc()
        # Save error to file
        with open(os.path.join(test_dir, "error.txt"), "w") as f:
            f.write(str(e))
            f.write("\n\n")
            import traceback as tb
            f.write(tb.format_exc())
    finally:
        if cool:
            cool.finish()
        rpc_log_file.close()
        # Collect server logs regardless of success
        if not cool:
            collect_server_logs(test_dir, remote1, remote2)
        else:
            # still collect but silently
            _orig_out, _orig_err = sys.stdout, sys.stderr
            sys.stdout = open(os.devnull, 'w')
            sys.stderr = open(os.devnull, 'w')
            try:
                collect_server_logs(test_dir, remote1, remote2)
            finally:
                sys.stdout.close()
                sys.stderr.close()
                sys.stdout, sys.stderr = _orig_out, _orig_err
        
        if lxc:
            if args.keep_lxc:
                if not cool:
                    print("\nKeeping LXC containers alive as requested.")
                    print(f"  Server 1: {remote1}")
                    print(f"  Server 2: {remote2}")
            else:
                lxc.cleanup()
        
        if not cool:
            print(f"\nTest finished. Results in {test_dir}")
        if not success:
            sys.exit(1)

if __name__ == "__main__":
    main()
