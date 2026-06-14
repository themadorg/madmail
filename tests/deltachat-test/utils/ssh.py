"""SSH helpers for deltachat-test against cmlxc Incus relays."""

from __future__ import annotations

import os
import shutil
import subprocess
from typing import Sequence


def _expand(path: str) -> str:
    return os.path.expanduser(path)


def ssh_command_prefix() -> list[str]:
    """Build ssh argv prefix (cmlxc key/config when available)."""
    ssh = shutil.which("ssh") or "/usr/bin/ssh"
    config = _expand(
        os.getenv("DELTACHAT_TEST_SSH_CONFIG", "~/.config/cmlxc/ssh-config")
    )
    identity = _expand(
        os.getenv("DELTACHAT_TEST_SSH_IDENTITY", "~/.config/cmlxc/id_localchat")
    )
    if os.path.isfile(config) and os.path.isfile(identity):
        return [
            ssh,
            "-F",
            config,
            "-i",
            identity,
            "-o",
            "IdentitiesOnly=yes",
            "-o",
            "BatchMode=yes",
        ]
    return [
        ssh,
        "-o",
        "StrictHostKeyChecking=no",
        "-o",
        "UserKnownHostsFile=/dev/null",
    ]


def run_ssh_command(
    remote: str,
    command: str,
    *,
    timeout: int = 30,
) -> tuple[int, str, str]:
    """Run a command on *remote* as root via SSH."""
    user_host = remote if "@" in remote else f"root@{remote}"
    cmd: Sequence[str] = [*ssh_command_prefix(), user_host, command]
    result = subprocess.run(
        cmd,
        capture_output=True,
        text=True,
        timeout=timeout,
    )
    return result.returncode, result.stdout, result.stderr


def journal_cursor_command(service: str) -> str:
    """Shell snippet returning the latest journal cursor (no jq required)."""
    return (
        f"journalctl -u {service} -n 1 --show-cursor --no-pager 2>/dev/null "
        r"| sed -n 's/^-- cursor: //p' | head -1"
    )