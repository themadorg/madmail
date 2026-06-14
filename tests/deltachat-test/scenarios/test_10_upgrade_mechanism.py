import os
import sys
import subprocess
import shutil
import http.server
import threading
import time


def _resolve_madmail_bin(repo_root: str) -> str:
    env_bin = os.environ.get("CHATMAIL_BIN")
    if env_bin and os.path.isfile(env_bin):
        return os.path.abspath(env_bin)
    for candidate in (
        os.path.join(repo_root, "target", "release", "madmail"),
        os.path.join(repo_root, "build", "maddy"),
    ):
        if os.path.isfile(candidate):
            return candidate
    raise Exception(
        "madmail binary not found. Set CHATMAIL_BIN or run "
        "'make build-release-static' (target/release/madmail)."
    )


def _resolve_private_key(repo_root: str) -> str:
    # Default signing key location (see scripts/sign.sh).
    for candidate in (
        os.path.join(repo_root, "..", "imp", "private_key.hex"),
        os.path.join(repo_root, "imp", "private_key.hex"),
    ):
        path = os.path.abspath(candidate)
        if os.path.isfile(path):
            return path
    raise Exception(
        "Signing key not found. Expected ../imp/private_key.hex "
        "(matching the embedded upgrade public key)."
    )


def run_test(madmail_bin, private_key_path, test_dir):
    print("Testing Upgrade Mechanism...")

    madmail_under_test = os.path.join(test_dir, "madmail_under_test")
    shutil.copy2(madmail_bin, madmail_under_test)
    os.chmod(madmail_under_test, 0o755)

    dummy_path = os.path.join(test_dir, "madmail_v2")
    with open(dummy_path, "wb") as f:
        f.write(b"MADMAIL DUMMY UPDATE BINARY CONTENT " + os.urandom(64))

    print("Attempting upgrade with unsigned binary...")
    result = subprocess.run(
        [madmail_under_test, "upgrade", dummy_path],
        capture_output=True,
        text=True,
    )
    combined = result.stdout + result.stderr
    if "INVALID SIGNATURE" in combined:
        print("✓ Success: Unsigned binary correctly rejected")
    else:
        print(f"Error output: {result.stderr}")
        raise Exception(
            "Security Failure: Unsigned binary was NOT rejected during verification!"
        )

    print(f"Signing binary using {private_key_path}...")
    repo_root = os.path.abspath(
        os.path.join(os.path.dirname(__file__), "..", "..", "..")
    )
    sign_script = os.path.join(repo_root, "scripts", "publish", "sign.py")
    if not os.path.isfile(sign_script):
        raise Exception(f"Signing script not found at {sign_script}")
    subprocess.run(
        [sys.executable, sign_script, dummy_path, private_key_path],
        check=True,
        cwd=repo_root,
    )

    print("Attempting upgrade with signed binary (checking verification stage)...")
    result = subprocess.run(
        [madmail_under_test, "upgrade", dummy_path],
        capture_output=True,
        text=True,
    )
    signed_out = result.stdout + result.stderr
    if "Signature verification successful" in signed_out:
        print("✓ Success: Signed binary verification passed")
    else:
        print(f"Stdout: {result.stdout}")
        print(f"Stderr: {result.stderr}")
        raise Exception("Failure: Signed binary verification failed!")

    # Use a fresh copy for the URL step (the prior binary may still be mapped).
    madmail_for_url = os.path.join(test_dir, "madmail_for_url")
    shutil.copy2(madmail_bin, madmail_for_url)
    os.chmod(madmail_for_url, 0o755)

    print("Testing update command from a mock HTTP server...")
    port = 9988

    class QuietHandler(http.server.SimpleHTTPRequestHandler):
        def log_message(self, format, *args):
            pass

    httpd = http.server.HTTPServer(("", port), QuietHandler)
    original_cwd = os.getcwd()
    os.chdir(test_dir)

    server_thread = threading.Thread(target=httpd.serve_forever)
    server_thread.daemon = True
    server_thread.start()

    try:
        url = f"http://localhost:{port}/madmail_v2"
        result = subprocess.run(
            [madmail_for_url, "update", url],
            capture_output=True,
            text=True,
        )
        update_out = result.stdout + result.stderr
        if "Signature verification successful" in update_out:
            print("✓ Success: Update from URL (download + verify) passed")
        else:
            print(f"Stdout: {result.stdout}")
            print(f"Stderr: {result.stderr}")
            raise Exception("Failure: Update from URL verification failed!")
    finally:
        os.chdir(original_cwd)
        httpd.shutdown()
        server_thread.join(timeout=5)

    return True


def run(dc, remote, test_dir):
    """E2E scenario for verifying the binary signature & upgrade mechanism."""
    repo_root = os.path.abspath(
        os.path.join(os.path.dirname(__file__), "..", "..", "..")
    )
    madmail_bin = _resolve_madmail_bin(repo_root)
    private_key_path = _resolve_private_key(repo_root)
    print(f"  Using madmail binary: {madmail_bin}")
    print(f"  Using signing key: {private_key_path}")
    return run_test(madmail_bin, private_key_path, test_dir)