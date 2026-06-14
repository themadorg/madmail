import subprocess
import concurrent.futures
import time
import sys
import os
import shutil

SERVERS = {
    "md_v1": "",
    "md_v2": ""
}

def run_cmping(server_ip, chat_id, count=5):
    # Use a unique cache directory for each concurrent session to avoid account conflicts
    cache_dir = f"/tmp/cmping_cache_{chat_id}"
    if os.path.exists(cache_dir):
        shutil.rmtree(cache_dir)
    os.makedirs(cache_dir, exist_ok=True)
    
    env = os.environ.copy()
    env["XDG_CACHE_HOME"] = cache_dir
    
    # We use a relatively small count per chat to speed up the ramp-up
    # but concurrent instances provide the real stress.
    cmd = ["uv", "run", "cmping", "-c", str(count), server_ip]
    try:
        # Timeout 180s to account for registration + pings
        result = subprocess.run(cmd, capture_output=True, text=True, timeout=180, env=env)
        # Cleanup
        shutil.rmtree(cache_dir, ignore_errors=True)
        if result.returncode == 0:
            return True, result.stdout
        else:
            return False, result.stderr
    except Exception as e:
        shutil.rmtree(cache_dir, ignore_errors=True)
        return False, str(e)

def stress_test(server_name, server_ip):
    print(f"\n" + "="*60)
    print(f"🚀 STRESS TESTING {server_name} ({server_ip})")
    print("="*60)
    
    num_chats = 1
    max_successful = 0
    
    # Ramp up schedule
    ramp_up = [1, 2, 4, 8, 12, 16, 20, 24, 28, 32, 40]
    
    for count in ramp_up:
        print(f"\n🔹 Testing with {count} concurrent chats...")
        start_time = time.time()
        
        # Run concurrent instances
        with concurrent.futures.ThreadPoolExecutor(max_workers=count) as executor:
            # unique chat_id for each instance
            futures = [executor.submit(run_cmping, server_ip, f"stress_{server_name}_{count}_{i}", count=5) for i in range(count)]
            results = [f.result() for f in concurrent.futures.as_completed(futures)]
            
        successes = [r[0] for r in results]
        duration = time.time() - start_time
        
        success_count = sum(successes)
        print(f"  📊 Result: {success_count}/{count} successful in {duration:.2f}s")
        
        if all(successes):
            max_successful = count
        else:
            # Find the first error to report
            error_msg = "Unknown error"
            for s, msg in results:
                if not s:
                    error_msg = msg
                    break
            print(f"  ❌ FAILURE DETECTED at {count} concurrent chats.")
            print(f"  Sample Error Trace:\n{'-'*20}\n{error_msg[:500]}...\n{'-'*20}")
            break
            
    return max_successful

if __name__ == "__main__":
    # Ensure uv is in the path or run from right dir
    if not os.path.exists("cmping"):
        print("Error: Please run this script from the madmail root where 'cmping' folder is located.")
        sys.exit(1)

    final_results = {}
    for name, ip in SERVERS.items():
        try:
            max_c = stress_test(name, ip)
            final_results[name] = max_c
        except KeyboardInterrupt:
            print("\n⚠️ Interrupted by user.")
            break
    
    print("\n" + "#"*60)
    print("      FINAL STRESS TEST COMPARISON SUMMARY")
    print("#"*60)
    for name, val in final_results.items():
        server_ip = SERVERS.get(name, "unknown")
        print(f"  {name:6} ({server_ip:15}): {val:2} max concurrent chats")
    
    if len(final_results) == 2:
        v1 = final_results["md_v1"]
        v2 = final_results["md_v2"]
        if v1 == v2:
            print("\n  ⚖️ RESULT: Both versions performed identically up to the failure point.")
        elif v2 > v1:
            print(f"\n  🏆 WINNER: md_v2 (New/Merged) is more stable ({v2} vs {v1}).")
        else:
            print(f"\n  🏆 WINNER: md_v1 (Old/Clean) is more stable ({v1} vs {v2}).")
    
    print("#"*60)
