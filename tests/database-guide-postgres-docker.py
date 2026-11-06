#!/usr/bin/env python3
"""Test the Persian guide's official PostgreSQL Docker setup and migration.

Build the embedded binary as described in database-guide-docker.py, then run:
python3 tests/database-guide-postgres-docker.py
Docker uses an ephemeral localhost port and unique test container/volume names.
"""

import runpy
import subprocess
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
Guide = runpy.run_path(str(ROOT / "tests/database-guide-docker.py"))["Guide"]


def run(*args, **kwargs):
    return subprocess.run(args, check=True, text=True, **kwargs)


def main():
    blocks = Guide(
        (ROOT / "crates/chatmail-www/www-src/docs/fa/database.html").read_text()
    ).named_blocks
    binary = ROOT / "target/debug/madmail"
    name = f"madmail-guide-docker-{time.time_ns()}"
    volume = name + "-data"
    command = (
        blocks["postgres-docker-install"]
        .replace("sudo ", "")
        .replace("madmail-postgres-data", volume)
        .replace("madmail-postgres", name)
        .replace("127.0.0.1:5432:5432", "127.0.0.1::5432")
    )
    create_volume, create_container = command.split("\n", 1)

    def shell(command, **kwargs):
        return run("bash", "-euc", command, **kwargs)

    def ready():
        command = (
            blocks["postgres-docker-ready"]
            .replace("sudo ", "")
            .replace("madmail-postgres", name)
        )
        for _ in range(60):
            result = subprocess.run(
                ["bash", "-c", command], capture_output=True, check=False
            )
            if result.returncode == 0:
                # The image's temporary initialization server uses a Unix socket.
                result = subprocess.run(
                    [
                        "docker",
                        "exec",
                        name,
                        "pg_isready",
                        "-h",
                        "127.0.0.1",
                        "-U",
                        "postgres",
                    ],
                    capture_output=True,
                    check=False,
                )
                if result.returncode == 0:
                    return
            time.sleep(1)
        raise RuntimeError("PostgreSQL did not become ready")

    def query(sql):
        return run(
            "docker",
            "exec",
            name,
            "psql",
            "-U",
            "postgres",
            "-d",
            "madmail",
            "-Atc",
            sql,
            capture_output=True,
        ).stdout.strip()

    try:
        shell(create_volume)
        shell(create_container)
        ready()
        shell(
            blocks["postgres-docker-psql"]
            .replace("sudo ", "")
            .replace("-it ", "-i ")
            .replace("madmail-postgres", name),
            input=blocks["postgres-create-database"] + "\n",
        )
        connect = (
            blocks["postgres-docker-connect"]
            .replace("sudo ", "")
            .replace("-it ", "-e PGPASSWORD=choose-a-secret ")
            .replace("madmail-postgres", name)
            .replace(" -W ", " ")
        )
        shell(connect)
        port = (
            run("docker", "port", name, "5432/tcp", capture_output=True)
            .stdout.strip()
            .rsplit(":", 1)[1]
        )
        with tempfile.TemporaryDirectory(prefix="madmail-docker-guide-") as tmp:
            state = Path(tmp)
            sqlite = state / "credentials.db"
            config = state / "madmail.conf"
            config.write_text(
                f"state_dir {state}\nauth.pass_table local_authdb {{\n table sql_table {{\n  driver sqlite3\n  dsn {sqlite}\n  table_name passwords\n }}\n}}\n"
            )
            run(
                str(binary),
                "--config",
                str(config),
                "--state-dir",
                str(state),
                "accounts",
                "create",
                "guide@example.test",
                "--password",
                "guide-test-password",
            )
            migration = (
                blocks["madmail-migrate"]
                .replace("sudo madmail", str(binary))
                .replace("/etc/madmail/madmail.conf", str(config))
                .replace("/var/lib/madmail/credentials.db", str(sqlite))
                .replace("port=5432", f"port={port}")
            )
            shell(migration)
            assert query("SELECT username FROM passwords") == "guide@example.test"
        run("docker", "rm", "-f", name)
        shell(create_container)
        ready()
        assert query("SELECT username FROM passwords") == "guide@example.test"
        shell(connect)
        print(
            "PASS: Docker setup, role/database creation, password connection, SQLite migration and volume persistence after container recreation"
        )
    finally:
        subprocess.run(["docker", "rm", "-fv", name], check=False)
        subprocess.run(["docker", "volume", "rm", volume], check=False)


if __name__ == "__main__":
    main()
