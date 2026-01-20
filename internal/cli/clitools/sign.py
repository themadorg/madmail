# /// script
# dependencies = [
#   "cryptography",
# ]
# ///
import sys
import os
from cryptography.hazmat.primitives.asymmetric import ed25519

def sign_file(file_path, private_key_path):
    """
    Signs a file using Ed25519 and appends the 64-byte signature to the end of the file.
    """
    if not os.path.exists(file_path):
        print(f"Error: File {file_path} not found")
        sys.exit(1)
        
    if not os.path.exists(private_key_path):
        print(f"Error: Private key {private_key_path} not found")
        sys.exit(1)

    # Load private key (from hex string)
    with open(private_key_path, 'r') as f:
        priv_hex = f.read().strip()
    
    try:
        priv_bytes = bytes.fromhex(priv_hex)
        # Ed25519 private keys in Go are 64 bytes (32-byte seed + 32-byte public key)
        # The cryptography library in Python expects only the 32-byte seed.
        seed = priv_bytes[:32]
        private_key = ed25519.Ed25519PrivateKey.from_private_bytes(seed)
    except Exception as e:
        print(f"Error parsing private key: {e}")
        sys.exit(1)
    
    # Read file content
    with open(file_path, 'rb') as f:
        content = f.read()
    
    # Sign content
    signature = private_key.sign(content)
    
    # Append signature to file
    with open(file_path, 'ab') as f:
        f.write(signature)
    
    print(f"✅ Successfully signed {file_path} (appended {len(signature)} bytes)")

if __name__ == "__main__":
    if len(sys.argv) < 3:
        print("Usage: uv run internal/cli/clitools/sign.py <file_to_sign> <private_key_path>")
        sys.exit(1)
    
    sign_file(sys.argv[1], sys.argv[2])
