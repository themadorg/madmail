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
"""

import os
import sys
import shutil
import time
import datetime
import subprocess
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
                    ["ssh", "-o", "StrictHostKeyChecking=no", "-o", "UserKnownHostsFile=/dev/null", f"root@{remote}", "journalctl -u maddy.service -n 1000 --no-pager"],
                    timeout=15
                ).decode('utf-8', errors='ignore')
                with open(os.path.join(test_dir, f"server{i}_debug.log"), "w") as f:
                    f.write(log)
            except Exception as e:
                print(f"Failed to collect logs from server{i}: {e}")
    except Exception as e:
        print(f"Failed to collect logs: {e}")


import argparse

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
    parser.add_argument("--domain", help="Specify domain/IP for tests (updates REMOTE1/REMOTE2)")
    parser.add_argument("--lxc", action="store_true", help="Run tests in local LXC containers")
    parser.add_argument("--keep-lxc", action="store_true", help="Keep LXC containers alive after test")
    parser.add_argument("--all", action="store_true", help="Run all tests (default)")
    parser.add_argument("--stress", action="store_true", help="Run stress test against a remote server")
    parser.add_argument("--stress-users", type=int, default=50, help="Total users to create (default: 50)")
    parser.add_argument("--stress-workers", type=int, default=8, help="Worker processes to use (default: 8)")
    parser.add_argument("--stress-duration", type=int, default=60, help="Send duration per worker in seconds (default: 60)")
    parser.add_argument("--stress-report", default="", help="Path to write stress report JSON")
    
    args = parser.parse_args()
    
    # If no specific tests selected, run all
    run_all = args.all or not any([
        args.test_1, args.test_2, args.test_3, args.test_4, 
        args.test_5, args.test_6, args.test_7, args.test_8, args.test_9, 
        args.test_10, args.test_11, args.test_12, args.test_13, args.test_14, 
        args.test_15, args.test_16, args.test_17
    ])

    timestamp = datetime.datetime.now().strftime("%Y%m%d_%H%M%S")
    tmp_root = os.path.join(ROOT_DIR, "tmp")
    test_dir = os.path.abspath(os.path.join(tmp_root, f"test_run_{timestamp}"))
    os.makedirs(test_dir, exist_ok=True)
    
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
        lxc = LXCManager()
        ips = lxc.setup()
        remote1 = ips[0] if len(ips) > 0 else remote1
        remote2 = ips[1] if len(ips) > 1 else remote2


    try:
        with rpc:
            dc = DeltaChat(rpc)
            
            # ==========================================
            # PRE-REQUISITE: Accounts needed for almost all tests
            # ==========================================
            print("\n" + "="*50)
            print("INITIALIZING: Account Creation")
            print("="*50)
            acc1 = test_01_account_creation.run(dc, remote1)
            acc2 = test_01_account_creation.run(dc, remote2)
            
            if run_all or args.test_1:
                print("✓ TEST #1 PASSED: Accounts created successfully")
            
            # Store credentials for SMTP test
            acc1_email = acc1.get_config("addr")
            acc1_password = acc1.get_config("mail_pw")
            acc2_email = acc2.get_config("addr")
            
            # ==========================================
            # TEST #2: Unencrypted Message Rejection
            # ==========================================
            if run_all or args.test_2:
                print("\n" + "="*50)
                print("TEST #2: Unencrypted Message Rejection")
                print("="*50)
                test_02_unencrypted_rejection.run_unencrypted_rejection_test(
                    sender_email=acc1_email,
                    sender_password=acc1_password,
                    receiver_email=acc2_email,
                    smtp_host=remote1
                )
                print("✓ TEST #2 PASSED: Unencrypted messages correctly rejected")
            
            # ==========================================
            # TEST #3: Secure Join
            # ==========================================
            # Always run Secure Join if subsequent tests depend on it
            if run_all or args.test_3 or args.test_4 or args.test_5 or args.test_6 or args.test_8 or args.test_9:
                print("\n" + "="*50)
                print("TEST #3: Secure Join (acc1 <-> acc2)")
                print("="*50)
                test_03_secure_join.run(rpc, acc1, acc2)
                print("✓ TEST #3 PASSED: Secure join completed successfully")
            
            # ==========================================
            # TEST #4: P2P Encrypted Message
            # ==========================================
            if run_all or args.test_4:
                print("\n" + "="*50)
                print("TEST #4: P2P Encrypted Message")
                print("="*50)
                test_02_unencrypted_rejection.run(acc1, acc2, f"P2P Test Message {timestamp}")
                print("✓ TEST #4 PASSED: P2P encrypted message delivered")
            
            # ==========================================
            # TEST #5: Group Creation & Message
            # ==========================================
            if run_all or args.test_5 or args.test_8:
                print("\n" + "="*50)
                print("TEST #5: Group Creation & Message")
                print("="*50)
                group_chat = test_05_group_message.run(acc1, acc2, f"Group {timestamp}")
                print("✓ TEST #5 PASSED: Group created and message delivered")
            
            # ==========================================
            # TEST #6: File Transfer
            # ==========================================
            if run_all or args.test_6:
                print("\n" + "="*50)
                print("TEST #6: File Transfer (1MB)")
                print("="*50)
                test_06_file_transfer.run(acc1, acc2, test_dir)
                print("✓ TEST #6 PASSED: File transfer completed with matching hash")
            
            # ==========================================
            # TEST #7: Federation
            # ==========================================
            if run_all or args.test_7:
                print("\n" + "="*50)
                print("TEST #7: Federation (Cross-Server Messaging)")
                print("="*50)
                acc3 = test_07_federation.run(rpc, dc, acc1, acc2, remote2, timestamp)
                print("✓ TEST #7 PASSED: Federation test completed successfully")
            
            # ==========================================
            # TEST #8: No Logging Test
            # ==========================================
            if run_all or args.test_8:
                print("\n" + "="*50)
                print("TEST #8: No Logging Test")
                print("="*50)
                
                # If we skipped test 7 and need acc3, create it on demand
                if acc3 is None:
                    print("  Initializing acc3 for No Logging test...")
                    acc3 = test_01_account_creation.run(dc, remote2)
                    # We might need to do secure join with acc1 if federation messages are tested
                    # but test_08 handles some creation. Let's ensure it has what it needs.
                
                test_08_no_logging.run(acc1, acc2, acc3, group_chat, (remote1, remote2))
                print("✓ TEST #8 PASSED: No logs generated with logging disabled")
            
            # ==========================================
            # TEST #9: Big File Test
            # ==========================================
            if run_all or args.test_9:
                test_09_send_bigfile.run(acc1, acc2, test_dir, (remote1, remote2))
                print("✓ TEST #9 PASSED: Big file transfer timing completed")
            
            # ==========================================
            # TEST #10: Upgrade Mechanism
            # ==========================================
            if run_all or args.test_10:
                print("\n" + "="*50)
                print("TEST #10: Upgrade Mechanism")
                print("="*50)
                test_10_upgrade_mechanism.run(dc, remote1, test_dir)
                print("✓ TEST #10 PASSED: Upgrade/Update signature verification verified")

            # ==========================================
            # TEST #11: JIT Registration
            # ==========================================
            if run_all or args.test_11:
                test_11_jit_registration.run(dc, (remote1, remote2))
                print("✓ TEST #11 PASSED: JIT registration verified")

            # ==========================================
            # TEST #12: SMTP/IMAP IDLE Test (local server)
            # ==========================================
            if run_all or args.test_12:
                # This test runs its own local maddy server
                test_12_smtp_imap_idle.run(test_dir=test_dir)
                print("✓ TEST #12 PASSED: SMTP/IMAP IDLE test verified")

            # ==========================================
            # TEST #13: Concurrent Profiles
            # ==========================================
            if run_all or args.test_13:
                test_13_concurrent_profiles.run(test_dir=test_dir)
                print("✓ TEST #13 PASSED: Concurrent profiles verified")

            # ==========================================
            # TEST #14: Purge Messages
            # ==========================================
            if run_all or args.test_14:
                test_14_purge_messages.run(rpc, dc, acc1, acc2, remote1)
                print("✓ TEST #14 PASSED: Purge messages verified")

            # ==========================================
            # TEST #15: Iroh Discovery
            # ==========================================
            if run_all or args.test_15:
                test_15_iroh_discovery.run(dc, remote1)
                print("✓ TEST #15 PASSED: Iroh discovery verified")

            # ==========================================
            # TEST #16: WebXDC Realtime
            # ==========================================
            if run_all or args.test_16:
                test_16_webxdc_realtime.run(dc, remote1)
                print("✓ TEST #16 PASSED: WebXDC Realtime P2P verified")

            # ==========================================
            # TEST #17: Admin API
            # ==========================================
            if run_all or args.test_17:
                test_17_admin_api.run(dc, remote1, test_dir)
                print("✓ TEST #17 PASSED: Admin API verified")

            # ==========================================
            # ALL TESTS COMPLETE
            # ==========================================
            print("\n" + "="*60)
            print("🎉 SELECTED TESTS PASSED! 🎉")
            print("="*60)
            success = True
            
    except Exception as e:
        print(f"\n❌ TEST FAILED: {e}")
        import traceback
        traceback.print_exc()
        # Save error to file
        with open(os.path.join(test_dir, "error.txt"), "w") as f:
            f.write(str(e))
            f.write("\n\n")
            f.write(traceback.format_exc())
    finally:
        rpc_log_file.close()
        # Collect server logs regardless of success
        collect_server_logs(test_dir, remote1, remote2)
        
        if lxc:
            if args.keep_lxc:
                print("\nKeeping LXC containers alive as requested.")
                print(f"  Server 1: {remote1}")
                print(f"  Server 2: {remote2}")
            else:
                lxc.cleanup()
        
        print(f"\nTest finished. Results in {test_dir}")
        if not success:
            sys.exit(1)

if __name__ == "__main__":
    main()
