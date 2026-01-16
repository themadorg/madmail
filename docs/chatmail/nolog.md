# No Log Policy

## Overview
Chatmail is designed with a strong privacy-first approach. This policy ensures that no persistent logs containing sensitive metadata (sender/recipient addresses, timestamps, authentication attempts) are stored on the server when the policy is active.

## Technical Enforcement

### Global Configuration
Logging can be disabled globally in `maddy.conf` using the `log` directive:
```hcl
log off
```
When set to `off`, the server uses a `NopOutput` backend, which discards all log events immediately. No log files are created, and no output is sent to `stderr` or `syslog`.

### Dynamic Logging Toggle
Logging can also be toggled dynamically via the settings database. This allows administrators to suppress logging without restarting the service.

- **Toggle Status**:
  ```bash
  maddy creds logging off  # Disables logging immediately
  maddy creds logging on   # Enables logging (requires service restart to take full effect)
  ```
- **Database Backend**: The status is stored in the `settings` table under the key `__LOG_DISABLED__`. 

### Privacy Safeguards
1.  **Zero Persistence**: When logging is disabled, the system does not open any file handles for logging purposes.
2.  **Metadata Protection**: This prevents the accumulation of long-term audit trails that could be used to reconstruct user communication patterns.
3.  **Boot Phase**: Only critical initialization errors that prevent the server from starting are reported to the system journal (stdout/stderr) during the boot phase. Once initialized, the "No Log" policy takes over.

## Verification
To verify that no logs are being generated:
- Check that `/var/log/maddy/` (or your configured log directory) remains empty.
- Run `journalctl -u maddy` and confirm that no new event logs are appearing after the "Listening for incoming connections..." message.
