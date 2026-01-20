# Digital Signature and Auto-Upgrade Mechanism
This document explains the plan for adding a digital signature mechanism to the `maddy` binary and implementing the `upgrade` and `update` commands.

## Goals
- Ensure the authenticity of files received for updates.
- Prevent the execution of malicious or unofficial binaries during upgrades.
- Ease of use for both developers and end-users.

## Technical Details

### 1. Encryption Algorithm
We use **Ed25519** for generating keys and digital signatures.
- **Advantages**: High speed, modern security, and very short key/signature length (64 bytes for signature).

### 2. Key Management
- **Private Key**: Stays only on the developer's system. It is used to sign new binaries for each release.
- **Public Key**: Hardcoded as a byte array inside the binary source code (in `internal/auth/signature_key.go`).

### 3. Signed File Structure
To keep it simple (especially for direct link downloads), the signature is appended to the end of the binary file:
- Final binary = `[Original binary content] + [64 bytes Ed25519 signature]`

The developer builds the binary, calculates the signature, and adds it to the end of the file ($64$ bytes).

### 4. New CLI Commands

#### Command: `maddy upgrade <path>`
Used for upgrading from a local file.
- **Input**: Path to the new binary file.
- **Steps**:
    1. Read the public key from the running binary.
    2. Read the last 64 bytes of the new file as the signature.
    3. Verify the signature on the rest of the file content.
    4. If valid, replace the current binary using the systemd service management.

#### Command: `maddy update <url>`
Used for downloading and upgrading directly from a URL (e.g., GitHub).
- **Input**: URL of the binary file.
- **Steps**:
    1. Download the file to a temporary directory.
    2. Perform the same verification steps as the `upgrade` command.
    3. If authentic, replace the current binary.

## Example Usage

```bash
# Update from a local file
sudo maddy upgrade ./maddy-new

# Update from a direct link
sudo maddy update https://github.com/themadorg/madmail/releases/download/v1.1.0/maddy-linux-amd64
```

## Security Considerations
- **Binary Replacement**: The replacement process is managed via `systemd`:
    1. Stop service: `systemctl stop maddy`
    2. Replace the binary file in the destination path (usually `/usr/local/bin/maddy`).
    3. Restart service: `systemctl start maddy`
- **Permissions**: The upgrade command must be run with `root` or `sudo` to manage services and replace files in system paths.

## Release Process (Developer)
Because the private key is sensitive, the release and signing process **must only be done on the developer’s local system**. The private key should never be uploaded to GitHub or CI/CD.

### Steps to release a new version:
1. **Prepare Binary**: Build the program for the target platform:
   ```bash
   GOOS=linux GOARCH=amd64 go build -o maddy-linux-amd64
   ```
2. **Sign Binary**: Use the local script `internal/cli/clitools/sign.py` to create a signature using the private key in `../imp/private_key.hex`.
3. **Append Signature**: The script adds the signature to the end of the binary:
   ```bash
   uv run internal/cli/clitools/sign.py maddy-linux-amd64 ../imp/private_key.hex
   ```
4. **Publish**: Upload the final file (binary + signature) to GitHub Releases or your update server.

---
**Security Note**: The `imp/` folder should be kept outside the repository (one level up) to prevent accidental key uploads.
---

## Automated Release with Make
To make it easier, we added commands to the `Makefile`.

### Process: `make publish`
1. **Changelog**: The script `publish/main.py` checks changes between the last tag and `HEAD` to create `changes.txt`.
2. **Local Signing**: After building, the `sign_binary` target is called to sign the binary locally.
3. **Publishing**: The signed binary and `changes.txt` are uploaded to GitHub (using `gh` CLI) and sent to Telegram.

## Affected Files
### New Files:
- `internal/auth/signature_key.go`: Holds the hardcoded Public Key.
- `internal/cli/clitools/sign.py`: Python script for local signing.
- `internal/cli/ctl/upgrade.go`: Implementation of `upgrade` and `update` commands.

### Modified Files:
- `internal/cli/clitools/clitools.go`: Added `VerifySignature` logic.
- `Makefile`: Added `sign_binary` and updated `publish`.
- `publish.sh`: Added GitHub release logic.
- `publish/main.py`: Improved changelog generation using tags.
- `.gitignore`: Removed `imp/` as it is now outside the repository.

## Testing Plan
A new E2E test was added: `tests/deltachat-test/scenarios/test_10_upgrade_mechanism.py`.
- **Valid Signature**: Checks if a correctly signed binary is accepted.
- **Invalid Signature**: Checks if unsigned or modified binaries are rejected.
- **Online Update**: Simulates a download from a URL and verifies the process.
