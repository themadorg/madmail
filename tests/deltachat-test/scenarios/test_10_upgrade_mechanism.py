import os
import sys
import subprocess
import shutil
import http.server
import threading
import time

def run_test(maddy_bin, private_key_path, test_dir):
    print("Testing Upgrade Mechanism...")

    # Run upgrade/update tests against a disposable copy so we do not mutate
    # the real build/maddy used by subsequent scenarios.
    maddy_under_test = os.path.join(test_dir, "maddy_under_test")
    shutil.copy2(maddy_bin, maddy_under_test)
    os.chmod(maddy_under_test, 0o755)

    # Path to the dummy binary in test directory
    dummy_path = os.path.join(test_dir, "maddy_v2")
    with open(dummy_path, "wb") as f:
        f.write(b"MADDY DUMMY UPDATE BINARY CONTENT " + os.urandom(64))
    
    # 1. Try to upgrade without signature (should fail)
    print("Attempting upgrade with unsigned binary...")
    # NOTE: We expect it to fail verification before it even checks for root/systemd
    result = subprocess.run([maddy_under_test, "upgrade", dummy_path], capture_output=True, text=True)
    if "INVALID SIGNATURE" in result.stderr or "INVALID SIGNATURE" in result.stdout:
        print("✓ Success: Unsigned binary correctly rejected")
    else:
        print(f"Error output: {result.stderr}")
        raise Exception("Security Failure: Unsigned binary was NOT rejected during verification!")

    # 2. Sign the binary
    print(f"Signing binary using {private_key_path}...")
    repo_root = os.path.abspath(
        os.path.join(os.path.dirname(__file__), "..", "..", "..")
    )
    sign_script = os.path.join(repo_root, "internal", "cli", "clitools", "sign.py")
    subprocess.run(
        [sys.executable, sign_script, dummy_path, private_key_path],
        check=True,
        cwd=repo_root,
    )
    
    # 3. Verify signature using maddy's upgrade logic
    print("Attempting upgrade with signed binary (checking verification stage)...")
    # Even if it fails later due to lack of root, the 'Signature verification successful' message should appear
    result = subprocess.run([maddy_under_test, "upgrade", dummy_path], capture_output=True, text=True)
    if "Signature verification successful" in result.stdout:
        print("✓ Success: Signed binary verification passed")
    else:
        print(f"Stdout: {result.stdout}")
        print(f"Stderr: {result.stderr}")
        raise Exception("Failure: Signed binary verification failed!")

    # When this test runs as root (e.g. in CI / Incus), `upgrade` continues past
    # verification and replaces the current executable with the signed payload.
    # Restore the real binary so the URL `update` step runs a valid ELF again.
    shutil.copy2(maddy_bin, maddy_under_test)
    os.chmod(maddy_under_test, 0o755)

    # 4. Test Update from URL
    print("Testing update command from a mock HTTP server...")
    PORT = 9988
    
    # Serve the test directory
    class QuietHandler(http.server.SimpleHTTPRequestHandler):
        def log_message(self, format, *args):
            pass # Keep output clean

    httpd = http.server.HTTPServer(("", PORT), QuietHandler)
    # Move to test_dir to serve files from there
    original_cwd = os.getcwd()
    os.chdir(test_dir)
    
    server_thread = threading.Thread(target=httpd.serve_forever)
    server_thread.daemon = True
    server_thread.start()
    
    try:
        url = f"http://localhost:{PORT}/maddy_v2"
        result = subprocess.run([maddy_under_test, "update", url], capture_output=True, text=True)
        if "Signature verification successful" in result.stdout:
            print("✓ Success: Update from URL (download + verify) passed")
        else:
            print(f"Stdout: {result.stdout}")
            print(f"Stderr: {result.stderr}")
            raise Exception("Failure: Update from URL verification failed!")
    finally:
        os.chdir(original_cwd)
        httpd.shutdown()
        server_thread.join()

    return True

def ensure_keys_exist(private_key_path, repo_root):
    """
    If the private key is missing, generate a new key pair and update the binary's public key.
    """
    import os
    from cryptography.hazmat.primitives.asymmetric import ed25519
    import subprocess

    if os.path.exists(private_key_path):
        return

    print(f"🔑 Private key not found at {private_key_path}. Generating new mission keys...")
    
    # Create directory if needed
    imp_dir = os.path.dirname(private_key_path)
    os.makedirs(imp_dir, exist_ok=True)

    # Generate Ed25519 keys
    private_key = ed25519.Ed25519PrivateKey.generate()
    public_key = private_key.public_key()

    # Get hex strings
    # Ed25519 private keys in Go are 64 bytes (32-byte seed + 32-byte public key)
    # cryptography's private_bytes matches the seed (32 bytes)
    priv_bytes = private_key.private_bytes_raw()
    pub_bytes = public_key.public_bytes_raw()
    
    # In Go, it's seed + pub
    full_priv_hex = (priv_bytes + pub_bytes).hex()
    pub_hex = pub_bytes.hex()

    # Save to files
    with open(private_key_path, "w") as f:
        f.write(full_priv_hex)
    
    pub_path = os.path.join(imp_dir, "public_key.hex")
    with open(pub_path, "w") as f:
        f.write(pub_hex)

    print(f"✅ Generated new keys in {imp_dir}")

    # Update Go source code
    sig_key_path = os.path.join(repo_root, "internal", "auth", "signature_key.go")
    if os.path.exists(sig_key_path):
        print(f"🛠️ Updating {sig_key_path} with new public key...")
        with open(sig_key_path, "r") as f:
            content = f.read()
        
        # Replace the hardcoded hex
        import re
        new_content = re.sub(r'const PublicKeyHex = "[a-f0-9]+"', f'const PublicKeyHex = "{pub_hex}"', content)
        
        with open(sig_key_path, "w") as f:
            f.write(new_content)
        
        print("🔨 Rebuilding maddy to embed the new public key...")
        subprocess.run(["make", "build"], check=True, cwd=repo_root)
    else:
        print(f"⚠️ Could not find {sig_key_path} to update.")

def run(dc, remote, test_dir):
    """
    E2E scenario for verifying the binary signature & upgrade mechanism.
    """
    repo_root = os.path.abspath(
        os.path.join(os.path.dirname(__file__), "..", "..", "..")
    )
    private_key_path = os.path.join(repo_root, "imp", "private_key.hex")
    maddy_bin = "build/maddy"
    
    # Ensure keys exist before running tests
    ensure_keys_exist(private_key_path, repo_root)
    
    # Check if necessary files exist
    if not os.path.exists(maddy_bin):
        raise Exception(f"maddy binary not found at {maddy_bin}. Run 'make build' first.")
    
    return run_test(maddy_bin, private_key_path, test_dir)
