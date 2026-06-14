"""Remote madmail control via SSH (config, limits, logging)."""

from __future__ import annotations

import time

from utils.remote_service import MAD_CONFIG_PATHS, MAD_SERVICE
from utils.ssh import run_ssh_command


def _config_sed(remote: str, pattern: str) -> None:
    for path in MAD_CONFIG_PATHS:
        run_ssh_command(
            remote,
            f"test -f {path} && sed -i '{pattern}' {path} || true",
        )


def restart_service(remote: str, *, wait_seconds: float = 3.0) -> None:
    rc, _, err = run_ssh_command(remote, f"systemctl restart {MAD_SERVICE}")
    if rc != 0:
        raise RuntimeError(f"Failed to restart {MAD_SERVICE} on {remote}: {err}")
    if wait_seconds > 0:
        time.sleep(wait_seconds)


def apply_config(remote: str) -> None:
    """Prefer hot reload; fall back to systemd restart."""
    rc, _, _ = run_ssh_command(
        remote,
        "madmail reload --insecure 2>/dev/null || systemctl restart madmail.service",
    )
    if rc != 0:
        restart_service(remote)


def set_message_limits(remote: str, limit: str) -> None:
    print(f"  Setting limits to {limit} on {remote}...")
    rc, out, err = run_ssh_command(remote, f"madmail message-size set {limit}")
    if rc != 0:
        for pat in (
            f"s/appendlimit [0-9A-Za-z]*/appendlimit {limit}/g",
            f"s/max_message_size [0-9A-Za-z]*/max_message_size {limit}/g",
        ):
            _config_sed(remote, pat)
        restart_service(remote)
        return
    if out.strip():
        print(f"    {out.strip()}")
    apply_config(remote)


def disable_logging(remote: str) -> None:
    print(f"  Disabling logging on {remote}...")
    _config_sed(remote, "s/^log .*/log off/")
    _config_sed(remote, "s/debug yes/debug no/g")
    _config_sed(remote, "s/debug true/debug false/g")
    print(f"  Restarting {MAD_SERVICE} on {remote}...")
    restart_service(remote, wait_seconds=5)
    print(f"  Logging disabled on {remote}")


def enable_logging(remote: str) -> None:
    print(f"  Re-enabling logging on {remote}...")
    _config_sed(remote, "s/^log off/log stderr/")
    rc, _, err = run_ssh_command(remote, f"systemctl restart {MAD_SERVICE}")
    if rc != 0:
        print(f"    Warning: Failed to restart madmail on {remote}: {err}")
    time.sleep(3)
    print(f"  Logging re-enabled on {remote}")