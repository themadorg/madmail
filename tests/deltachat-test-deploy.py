#!/usr/bin/env python3
"""Deploy a pre-built madmail-v2 binary into cmlxc Incus relay containers."""

from __future__ import annotations

import argparse
import os
import subprocess
import sys
from pathlib import Path

TESTS_DIR = Path(__file__).resolve().parent
ROOT = TESTS_DIR.parent
ENV_FILE = TESTS_DIR / ".deltachat-test-env"
MADMAIL_DRIVER = "madmail"


def _setup_cmlxc_import() -> None:
    submodule_src = TESTS_DIR / "cmlxc" / "src"
    if submodule_src.is_dir():
        sys.path.insert(0, str(submodule_src))
        return
    try:
        import cmlxc  # noqa: F401
    except ImportError as exc:
        raise SystemExit(
            "cmlxc is required for deltachat-test deploy.\n"
            "Run: git submodule update --init tests/cmlxc\n"
            "Or install: uv pip install cmlxc"
        ) from exc


def _require_binary(path: Path) -> Path:
    binary = path.resolve()
    if not binary.is_file():
        raise SystemExit(f"CHATMAIL_BIN not found: {binary}")
    if not os.access(binary, os.X_OK):
        binary.chmod(0o755)
    return binary


def _ensure_init(ix, out) -> None:
    from cmlxc.container import DNS_CONTAINER_NAME, BuilderContainer, DNSContainer
    from cmlxc.incus import check_cgroup_compat

    check_cgroup_compat()
    data = ix.run_json(["list", DNS_CONTAINER_NAME], check=False) or []
    if data and data[0].get("status") == "Running":
        bld = ix.run_json(["list", BuilderContainer(ix).name], check=False) or []
        if bld and bld[0].get("status") == "Running":
            out.print("cmlxc environment already initialized")
            return

    out.print("cmlxc not initialized — run: make test-deltachat DC_TEST_ARGS='--init'")
    raise SystemExit(1)


def _deploy_relay(ix, out, name: str, binary: Path, with_admin: bool) -> str:
    from cmlxc.container import DNSContainer, SetupError
    from cmlxc.driver_base import parse_source

    ct = ix.get_relay_container(name)
    ct.out = out

    try:
        ct.check_deploy_lock(MADMAIL_DRIVER)
    except SetupError as exc:
        out.red(str(exc))
        raise SystemExit(1) from exc

    ix.write_ssh_config()
    ct.ensure(ipv4_only=True)
    ct.wait_ready()
    ip = ct.ipv4
    if not ip:
        raise SystemExit(f"relay {name!r} has no IPv4 address")

    dns_ct = DNSContainer(ix)
    if not dns_ct.ipv4:
        dns_ct.wait_ready()
    ct.setup_resolvconf_localchat_nameserver(dns_ct.ipv4)

    out.print(f"Deploying {binary.name} to {ct.shortname} ({ip}) ...")
    ix.run(
        [
            "file",
            "push",
            str(binary),
            f"{ct.name}/tmp/madmail",
            "--mode=755",
            "--uid=0",
            "--gid=0",
        ]
    )

    install_flags = (
        f"--simple --ip {ip}"
        " --tls-mode self_signed"
        " --enable-chatmail"
        " --enable-iroh"
        " --non-interactive"
    )
    ct.bash("systemctl stop madmail || true")
    ct.bash(f"/tmp/madmail install {install_flags}")
    ct.bash("sed -i 's/^log off$/log syslog/' /etc/madmail/madmail.conf || true")
    ct.bash("systemctl daemon-reload")
    ct.bash("systemctl enable madmail")
    ct.bash("systemctl start madmail")

    if with_admin:
        ct.bash("madmail admin-web path /admin")
        ct.bash("madmail admin-web enable")
        ct.bash("systemctl restart madmail")
    else:
        ct.bash("madmail admin-web disable || true")

    ct.bash("rm -f /tmp/madmail")
    source = parse_source(str(ROOT), "https://github.com/themadorg/madmail.git")
    ct.write_deploy_state(MADMAIL_DRIVER, source=source, deploy_type="ipv4")
    out.green(f"madmail deployed to {ct.shortname} ({ip})")
    return ip


def _write_env(remote1: str, remote2: str) -> None:
    ENV_FILE.write_text(
        f"REMOTE1={remote1}\nREMOTE2={remote2}\n",
        encoding="utf-8",
    )


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Deploy madmail-v2 into cmlxc Incus relays for deltachat-test"
    )
    parser.add_argument(
        "--binary",
        default=os.environ.get("CHATMAIL_BIN", str(ROOT / "target/release/madmail")),
        help="Path to the madmail binary (default: CHATMAIL_BIN or target/release/madmail)",
    )
    parser.add_argument(
        "--relay",
        action="append",
        dest="relays",
        metavar="NAME",
        help="Relay short name (default: mad0 and mad1)",
    )
    parser.add_argument(
        "--with-webadmin",
        action="store_true",
        help="Enable embedded admin web at /admin on deployed relays",
    )
    args = parser.parse_args()

    _setup_cmlxc_import()
    from cmlxc.incus import Incus
    from cmlxc.output import Out

    binary = _require_binary(Path(args.binary))
    relays = args.relays or ["mad0", "mad1"]

    out = Out()
    ix = Incus(out)
    _ensure_init(ix, out)

    ips: list[str] = []
    for relay in relays:
        ips.append(_deploy_relay(ix, out, relay, binary, args.with_webadmin))

    remote1 = ips[0]
    remote2 = ips[1] if len(ips) > 1 else ips[0]
    _write_env(remote1, remote2)
    out.print(f"REMOTE1={remote1} REMOTE2={remote2} (written to {ENV_FILE})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())