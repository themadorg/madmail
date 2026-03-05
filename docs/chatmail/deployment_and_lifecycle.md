# Deployment and Runtime Lifecycle

This document describes how Madmail is installed, how it manages its runtime configuration, and how it handles service restarts in a production environment.

## Installation via CLI

The primary method for deploying Madmail is the `maddy install` command. It is designed to handle all system-level requirements including user creation, directory setup, and configuration generation.

### Installation Modes

| Mode | Command Flag | Description |
|------|--------------|-------------|
| **Interactive** | *(default)* | Prompts for domain, TLS method, and optional services. |
| **Simple** | `--simple` | Automatically enables Chatmail and contact sharing. Ideal for quick setups with `--ip`. |
| **Non-interactive** | `--non-interactive` | Uses defaults for all values. Requires flags for any deviation. |

### Camouflage (Stealth) Mode

To evade automated detection in restricted network environments, Madmail supports a "Camouflage Mode." By renaming the binary before installation, all system artifacts (directories, services, process names) are automatically renamed to match.

1.  **Rename binary:** `cp madmail sysmond`
2.  **Install:** `sudo ./sysmond install --simple --ip 1.2.3.4`

**Resulting System View:**
- **Process List:** `sysmond --config /etc/sysmond/sysmond.conf run`
- **Systemd Unit:** `sysmond.service`
- **Config Path:** `/etc/sysmond/`
- **Data Path:** `/var/lib/sysmond/`

## Runtime Configuration Management

Madmail supports dynamic updates to core settings (ports, hostnames, credentials) via the Admin API/Dashboard. These updates are synchronized from the database to the filesystem to ensure persistence across reboots.

### The Reload Mechanism

When a configuration change is triggered via the Admin API:

1.  **DB Lookup:** The system reads the override value from the `settings` database table.
2.  **Config Patching:** The `reloadConfig` internal handler reads the existing `maddy.conf`. It uses predefined regular expressions (Regex) to locate and replace specific directives while preserving comments and other settings.
3.  **Atomic Write:**
    - Since `/etc/` is typically root-owned, the service cannot overwrite the main config directly.
    - Instead, it writes a `.conf.pending` file to the state directory (`/var/lib/<binary>/`).
4.  **Pending Application:** An `ExecStartPre` script in the systemd unit automatically copies the `.pending` file to the actual configuration directory during the next startup.

### Zero-Downtime Restart

After patching the configuration, the service must restart to bind to new ports or apply logic changes.

- **SIGTERM Signal:** The process sends a `SIGTERM` to itself after a 500ms delay (allowing the API response to be sent successfully).
- **Systemd Orchestration:** The systemd unit is configured with `Restart=always`. Systemd detects the exit, runs `ExecStartPre` to apply pending configs, and restarts the service within milliseconds.

## Security and Integrity

### Input Sanitization
The config replacer (in `reload.go`) implements a defense-in-depth strategy. It rejects any value containing newlines (`\n`), null bytes, or backslashes to prevent **Configuration Injection** attacks, where a malicious DB entry could insert new server directives.

### Path Verification
The system dynamically resolves configuration paths by checking:
1. The path provided via `--config` flag.
2. Paths derived from the running binary name in `/etc/`.
3. Relative paths in the state directory.

### Port Reconciliation
Some settings (like Shadowsocks cipher or password) take effect immediately in-memory *before* the restart, ensuring the shortest possible window of inconsistency between the database state and the running service.
