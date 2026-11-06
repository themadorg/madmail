#!/usr/bin/env python3
"""Execute the Persian website guide against real SQLite and PostgreSQL.

Build first: CARGO_PROFILE_DEV_DEBUG=0 cargo build --locked -p chatmail \
    -p chatmail-www --bin madmail --features rust-embed/debug-embed
Run: python3 tests/database-guide-docker.py [--image ubuntu:24.04|debian:13|fedora:44] [--keep]
Requires Docker, Python 3, OpenSSL and a Linux binary compatible with the image.
Containers have no systemd: start PostgreSQL with pg_ctlcluster / pg_ctl and Madmail
with its real `run` command instead of the guide's systemctl commands.
"""

import argparse
import shlex
import subprocess
import tempfile
import time
import urllib.request
from html.parser import HTMLParser
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


class Guide(HTMLParser):
    def __init__(self, source):
        super().__init__()
        self.blocks = []
        self.named_blocks = {}
        self.block_id = None
        self.in_pre = False
        self.feed(source)

    def handle_starttag(self, tag, attrs):
        if tag == "pre":
            self.in_pre = True
            self.blocks.append("")
            self.block_id = dict(attrs).get("id")

    def handle_endtag(self, tag):
        if tag == "pre":
            if self.block_id:
                self.named_blocks[self.block_id] = self.blocks[-1]
            self.in_pre = False

    def handle_data(self, data):
        if self.in_pre:
            self.blocks[-1] += data


def run(*args, **kwargs):
    return subprocess.run(args, check=True, text=True, **kwargs)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--image", default="ubuntu:24.04")
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug/madmail")
    parser.add_argument(
        "--container", help="Reuse an already provisioned test container"
    )
    parser.add_argument(
        "--keep", action="store_true", help="Keep server for browser inspection"
    )
    args = parser.parse_args()
    fedora = args.image.startswith("fedora:")
    binary = args.binary.resolve(strict=True)
    source = (ROOT / "crates/chatmail-www/www-src/docs/fa/database.html").read_text()
    blocks = Guide(source).named_blocks
    install = blocks["postgres-debian-install"]
    psql = blocks["postgres-native-psql"]
    sql = blocks["postgres-create-database"]
    connect = blocks["postgres-native-connect"]
    backup = blocks["madmail-backup"]
    migrate = blocks["madmail-migrate"]
    config = blocks["madmail-postgres-config"]
    start = blocks["madmail-start"]
    name = args.container or f"madmail-guide-{time.time_ns()}"
    evidence_dir = ROOT / "target/database-guide-tests"
    evidence_dir.mkdir(parents=True, exist_ok=True)
    work = Path(tempfile.mkdtemp(prefix="run-", dir=evidence_dir))

    def shell(command, **kwargs):
        return run("docker", "exec", "-i", name, "bash", "-euc", command, **kwargs)

    def query(command):
        return shell(
            "sudo -u postgres psql -XAt -v ON_ERROR_STOP=1 -d madmail -c "
            + shlex.quote(command),
            capture_output=True,
        ).stdout.strip()

    def start_server():
        shell(
            "mkdir -p /run/madmail; "
            "nohup bash -c 'echo $$ > /run/madmail/test.pid; "
            "exec madmail run' > /var/log/madmail-test.log 2>&1 < /dev/null &"
        )
        for _ in range(60):
            result = subprocess.run(
                [
                    "docker",
                    "exec",
                    name,
                    "bash",
                    "-c",
                    "echo > /dev/tcp/127.0.0.1/8080",
                ],
                check=False,
                capture_output=True,
            )
            if result.returncode == 0:
                return
            time.sleep(1)
        raise RuntimeError("Madmail did not start")

    def stop_server():
        shell('kill -TERM "$(cat /run/madmail/test.pid)"')
        for _ in range(30):
            # Reaping may be delayed by container PID 1; zombie is stopped too.
            result = shell(
                "ps -o stat= -p $(cat /run/madmail/test.pid) || true",
                capture_output=True,
            )
            if not result.stdout.strip() or result.stdout.strip().startswith("Z"):
                return
            time.sleep(1)
        raise RuntimeError("Madmail did not stop")

    def login():
        # Run inside the container to avoid exposing mail ports.
        shell(
            "python3 - <<'CODE'\nimport imaplib, ssl\ncontext=ssl.create_default_context(cafile='/etc/madmail/cert.pem')\nc=imaplib.IMAP4_SSL('localhost',1993,ssl_context=context,timeout=10)\n"
            "assert c.login('guide@example.test','guide-test-password')[0]=='OK'\n"
            "c.logout()\nCODE"
        )

    try:
        if not args.container:
            run(
                "docker",
                "run",
                "-d",
                "--name",
                name,
                "-p",
                "127.0.0.1::8080",
                args.image,
                "sleep",
                "infinity",
            )
            # Base images have neither sudo nor systemd as PID 1.
            if fedora:
                shell(
                    blocks["postgres-fedora-install"]
                    .split("sudo postgresql-setup")[0]
                    .replace("sudo ", "")
                    + "\ndnf install -y sudo python3 procps-ng"
                )
            else:
                shell(
                    install.split("sudo systemctl")[0].replace("sudo ", "")
                    + "\nDEBIAN_FRONTEND=noninteractive apt-get install -y sudo python3 ca-certificates"
                )
        elif fedora:
            shell("dnf install -y python3 procps-ng")
        else:
            shell("DEBIAN_FRONTEND=noninteractive apt-get install -y python3")
        if fedora:
            shell(
                "test -f /var/lib/pgsql/data/PG_VERSION || sudo postgresql-setup --initdb"
            )
            # Apply the guide's instruction to replace the existing IPv4 loopback rule.
            shell(
                "python3 - <<'CODE'\nfrom pathlib import Path\np=Path('/var/lib/pgsql/data/pg_hba.conf')\n"
                "lines=p.read_text().splitlines()\n"
                f"rule={blocks['postgres-fedora-auth']!r}\n"
                "p.write_text('\\n'.join(rule if line.split()[:4]==['host','all','all','127.0.0.1/32'] else line for line in lines)+'\\n')\nCODE"
            )
            shell(
                "install -d -o postgres -g postgres /run/postgresql; "
                "pg_isready -q || sudo -u postgres pg_ctl -D /var/lib/pgsql/data -l /var/lib/pgsql/test.log start"
            )
        else:
            shell(
                "pg_isready -q || pg_ctlcluster $(ls /etc/postgresql | sort -V | tail -1) main start"
            )
        shell(psql, input=sql + "\n")
        shell("export PGPASSWORD=choose-a-secret\n" + connect.replace(" -W ", " "))
        shell("cat /etc/os-release; psql --version")
        print(
            "PASS: package installation, role/database creation, password connection",
            flush=True,
        )
        run("docker", "cp", str(binary), f"{name}:/usr/local/bin/madmail")
        shell("mkdir -p /etc/madmail /var/lib/madmail/mail /run/madmail")
        run(
            "openssl",
            "req",
            "-x509",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-days",
            "1",
            "-subj",
            "/CN=localhost",
            "-keyout",
            str(work / "key.pem"),
            "-out",
            str(work / "cert.pem"),
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        for f in ("key.pem", "cert.pem"):
            run("docker", "cp", str(work / f), f"{name}:/etc/madmail/{f}")
        prefix = """$(hostname) = localhost
$(primary_domain) = example.test
log stderr
state_dir /var/lib/madmail
runtime_dir /run/madmail
tls file /etc/madmail/cert.pem /etc/madmail/key.pem
"""
        suffix = """
smtp tcp://0.0.0.0:2525 {
 hostname localhost
}
imap tls://0.0.0.0:1993 tcp://0.0.0.0:1143 {
 auth &local_authdb
}
chatmail tcp://0.0.0.0:8080 {
 mail_domain example.test
}
"""
        sqlite = config.replace("driver postgres", "driver sqlite3").replace(
            "host=127.0.0.1 port=5432 user=madmail password=choose-a-secret dbname=madmail sslmode=disable",
            "/var/lib/madmail/credentials.db",
        )

        def write_config(blocks):
            shell("cat > /etc/madmail/madmail.conf", input=prefix + blocks + suffix)

        write_config(sqlite)
        shell(
            "madmail accounts create guide@example.test --password guide-test-password"
        )
        shell("madmail language set fa")
        start_server()
        login()
        stop_server()
        shell("echo mail-marker > /var/lib/madmail/mail/guide-marker")
        shell(backup.split("\n", 1)[1])
        shell("tar -tzf /root/madmail-before-postgres.tar.gz | grep credentials.db")
        before = query(
            "SELECT count(*) FROM information_schema.tables WHERE table_schema='public'"
        )
        dry, real = migrate.split("\n\n")
        shell(dry)
        assert (
            query(
                "SELECT count(*) FROM information_schema.tables WHERE table_schema='public'"
            )
            == before
        )
        report = shell(real, capture_output=True).stdout
        print(report, flush=True)
        counts = [
            line.split()
            for line in report.splitlines()
            if len(line.split()) == 5 and line.split()[2].isdigit()
        ]
        assert counts and all(row[2] == row[3] for row in counts), report
        assert query("SELECT username FROM passwords") == "guide@example.test"
        print(
            "PASS: SQLite login, stopped backup, dry run, password-authenticated migration",
            flush=True,
        )
        # Repeating the exact command must refuse without changing existing data.
        refusal = subprocess.run(
            ["docker", "exec", name, "bash", "-euc", real],
            check=False,
            capture_output=True,
            text=True,
        )
        assert refusal.returncode != 0, "copy into populated Postgres must fail"
        assert "--force" in refusal.stderr, refusal.stderr
        assert query("SELECT count(*) FROM passwords") == "1"
        write_config(config)
        start_server()
        login()
        shell(start.split("\n")[-1])
        shell("test $(cat /var/lib/madmail/mail/guide-marker) = mail-marker")
        shell("test -f /var/lib/madmail/credentials.db")
        ip = run(
            "docker",
            "inspect",
            "-f",
            "{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}",
            name,
            capture_output=True,
        ).stdout.strip()
        url = f"http://{ip}:8080/docs/database"
        html = urllib.request.urlopen(url, timeout=10).read().decode()
        assert 'lang="fa" dir="rtl"' in html
        assert "نصب PostgreSQL" in html and "{{" not in html
        index = (
            urllib.request.urlopen(f"http://{ip}:8080/docs", timeout=10).read().decode()
        )
        assert 'href="/docs/database"' in index
        (work / "rendered.html").write_text(html)
        print(
            "PASS: PostgreSQL startup, migrated IMAP login, account status, preserved mail/SQLite, Persian web route/link",
            flush=True,
        )
        stop_server()
        start_server()
        login()
        print("PASS: PostgreSQL restart and login", flush=True)
        # The guide also supports a fresh database, without running the copy.
        stop_server()
        query("CREATE DATABASE guide_fresh OWNER madmail")
        write_config(config.replace("dbname=madmail", "dbname=guide_fresh"))
        start_server()
        fresh = shell(
            "sudo -u postgres psql -XAt -d guide_fresh -c 'SELECT count(*) FROM passwords'",
            capture_output=True,
        ).stdout.strip()
        assert fresh == "0"
        shell(start.split("\n")[-1])
        stop_server()
        write_config(config)
        start_server()
        login()
        print(
            "PASS: fresh PostgreSQL schema created automatically; migrated database restored",
            flush=True,
        )
        shell("cat /var/log/madmail-test.log", stdout=(work / "server.log").open("w"))
        print(f"Evidence: {work}")
        if args.keep:
            port = subprocess.run(
                ["docker", "port", name, "8080/tcp"],
                check=False,
                capture_output=True,
                text=True,
            ).stdout.strip()
            preview = f"http://{port}/docs/database" if port else url
            print(f"Preview: {preview}\nContainer: {name}")
    finally:
        subprocess.run(
            ["docker", "logs", name],
            check=False,
            stdout=(work / "container.log").open("w"),
            stderr=subprocess.STDOUT,
        )
        if not args.keep:
            subprocess.run(["docker", "rm", "-fv", name], check=False)


if __name__ == "__main__":
    main()
