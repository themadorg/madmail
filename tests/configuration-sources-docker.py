#!/usr/bin/env python3
"""Live configuration-source matrix against an already-built shipping Docker image.

Run: python3 tests/configuration-sources-docker.py [--image madmail-local:test-docker]
Uses isolated disposable volumes/containers, Python's standard library, and exact
operational examples extracted from docs/guide/configuration.md.
"""
import argparse
import json
from pathlib import Path
import re
import ssl
import subprocess
import time
import urllib.error
import urllib.request
import uuid


def docker(*args, input=None, check=True):
    return subprocess.run(["docker", *args], input=input, text=True,
                          capture_output=True, check=check, timeout=120)


class Relay:
    def __init__(self, image, suffix):
        self.image = image
        self.name = "madmail-sources-" + uuid.uuid4().hex[:10]
        self.state = self.name + "-state"
        self.config = self.name + "-config"
        self.path = "/etc/madmail/check." + suffix
        self.suffix = suffix
        self.context = ssl._create_unverified_context()

    def mounts(self):
        return ["-v", self.state + ":/var/lib/madmail",
                "-v", self.config + ":/etc/madmail"]

    def install(self):
        for volume in (self.state, self.config):
            docker("volume", "create", volume)
        docker("run", "--rm", *self.mounts(), self.image, "install", "--simple",
               "--ip", "127.0.0.1", "--tls-mode", "self_signed", "--skip-systemd",
               "--skip-user", "--non-interactive")
        original = docker("run", "--rm", *self.mounts(), "--entrypoint", "cat",
                          self.image, "/etc/madmail/madmail.conf").stdout
        if self.suffix == "conf":
            self.base_config = original
        else:
            cert, key = re.search(r"(?m)^tls file (\S+) (\S+)", original).groups()
            self.base_config = "\n".join(f"{k} = {json.dumps(v)}" for k, v in {
                "hostname": "127.0.0.1", "primary_domain": "[127.0.0.1]",
                "state_dir": "/var/lib/madmail", "runtime_dir": "/run/madmail",
                "credentials_driver": "sqlite3", "credentials_dsn": "credentials.db",
                "imapsql_driver": "sqlite3", "imapsql_dsn": "imapsql.db",
                "tls_mode": "file", "tls_cert_path": cert, "tls_key_path": key,
                "http_tls_listen": "0.0.0.0:443", "imap_tls_listen": "0.0.0.0:993",
                "smtp_listen": "0.0.0.0:25", "submission_tls_listen": "0.0.0.0:465",
            }.items()) + "\n"
        self.write("")
        self.cli("admin-web", "path", "/database-panel")

    def write(self, extra):
        docker("run", "--rm", "-i", *self.mounts(), "--entrypoint", "sh",
               self.image, "-c", 'cat > "$1"', "sh", self.path,
               input=self.base_config + "\n" + extra + "\n")

    def cli(self, *args, env=None, check=True):
        flags = [item for k, v in (env or {}).items() for item in ("-e", k + "=" + v)]
        return docker("run", "--rm", *flags, *self.mounts(), "--entrypoint",
                      "/bin/madmail", self.image, "--config", self.path,
                      *args, check=check)

    def stop(self):
        docker("rm", "-fv", self.name, check=False)

    def start(self, api="/api/admin", env=None):
        self.stop()
        flags = [item for k, v in (env or {}).items() for item in ("-e", k + "=" + v)]
        docker("run", "-d", "--name", self.name, "-p", "127.0.0.1::443",
               *flags, *self.mounts(), "--entrypoint", "/bin/madmail", self.image,
               "--config", self.path, "run", "--libexec", "/var/lib/madmail")
        self.refresh_address()
        self.api = api
        self.wait()
        self.token = docker("exec", self.name, "madmail", "--config", self.path,
                            "admin-token", "--raw").stdout.strip()

    def refresh_address(self):
        port = docker("port", self.name, "443/tcp").stdout.strip().split(":")[-1]
        self.url = "https://127.0.0.1:" + port

    def request(self, path, payload=None):
        req = urllib.request.Request(self.url + path,
            data=None if payload is None else json.dumps(payload).encode(),
            headers={"Content-Type": "application/json"})
        try:
            with urllib.request.urlopen(req, context=self.context, timeout=3) as response:
                return response.status, response.read()
        except urllib.error.HTTPError as error:
            return error.code, error.read()

    def wait(self):
        for _ in range(40):
            try:
                if self.request("/")[0] == 200:
                    return
            except (OSError, urllib.error.URLError):
                pass
            time.sleep(0.5)
        raise AssertionError(f"{self.suffix}: server did not become ready")

    def rpc(self, resource, body=None, auth=True):
        payload = {"method": "GET" if body is None else "POST", "resource": resource}
        if body is not None:
            payload["body"] = body
        if auth:
            payload["headers"] = {"Authorization": "Bearer " + self.token}
        return json.loads(self.request(self.api, payload)[1])

    def settings(self):
        result = self.rpc("/admin/settings")
        assert result["status"] == 200, result
        return result["body"]

    def check(self, web, retention, minimum, maximum, source, enabled=True):
        settings = self.settings()
        for key, expected in {"admin_web_path": web, "message_retention": retention,
                              "turn_relay_port_min": str(minimum),
                              "turn_relay_port_max": str(maximum)}.items():
            assert str(settings[key]["value"]) == expected, (key, settings[key], expected)
            meta = settings["configuration_sources"][key]
            assert meta["source"].startswith(source), (key, meta, source)
            assert meta["locked"] == (source != "database"), (key, meta)
        assert settings["admin_web_enabled"] == ("enabled" if enabled else "disabled")
        assert self.request(web + "/")[0] == (200 if enabled else 404)
        assert self.rpc("/admin/status", auth=False)["status"] == 401
        if source != "database":
            for key, value in [("admin_web_path", "/bad"), ("message_retention", "2d"),
                               ("turn_relay_port_min", str(minimum + 1))]:
                for action in ("set", "reset"):
                    result = self.rpc("/admin/settings/" + key, {"action": action, "value": value})
                    assert result["status"] == 409 and result["error_code"] == "configuration_locked", result
            result = self.cli("admin-web", "path", "/bad", check=False)
            assert result.returncode and "configuration_locked" in result.stdout + result.stderr

    def cleanup(self):
        self.stop()
        for volume in (self.state, self.config):
            docker("volume", "rm", volume, check=False)


def matrix(image, suffix, example):
    relay = Relay(image, suffix)
    try:
        relay.install()
        relay.start()
        for key, value in [("message_retention", "3d"), ("turn_relay_port_min", "52000"),
                           ("turn_relay_port_max", "52100")]:
            result = relay.rpc("/admin/settings/" + key, {"action": "set", "value": value})
            assert result["status"] == 200, result
        relay.check("/database-panel", "3d", 52000, 52100, "database")
        print(f"PASS {suffix}: omitted settings use editable database values", flush=True)
        relay.stop()
        relay.write(example)
        relay.start("/private-api")
        relay.check("/private-admin", "14d", 50000, 50100, "file:")
        print(f"PASS {suffix}: exact documentation example overrides DB; CLI/API writes and resets rejected", flush=True)
        relay.stop()
        relay.write(example.replace("true", "false"))
        relay.start("/private-api")
        relay.check("/private-admin", "14d", 50000, 50100, "file:", enabled=False)
        assert relay.settings()["message_retention_enabled"] == "disabled"
        relay.stop()
        relay.write(example)
        print(f"PASS {suffix}: explicit file false disables features instead of falling back to DB", flush=True)
        env = {"CHATMAIL_ADMIN_WEB_PATH": "/environment-panel", "CHATMAIL_ADMIN_PATH": "/environment-api",
               "CHATMAIL_MESSAGE_RETENTION": "21d", "CHATMAIL_TURN_RELAY_PORT_MIN": "51000",
               "CHATMAIL_TURN_RELAY_PORT_MAX": "51100", "CHATMAIL_ADMIN_WEB_ENABLED": "false",
               "CHATMAIL_MESSAGE_RETENTION_ENABLED": "false"}
        relay.start("/environment-api", env)
        relay.check("/environment-panel", "21d", 51000, 51100, "environment:", enabled=False)
        assert relay.settings()["message_retention_enabled"] == "disabled"
        print(f"PASS {suffix}: ENV overrides file and DB; explicit false disables web and retention", flush=True)
        relay.stop()
        relay.write("")
        env["CHATMAIL_ADMIN_WEB_ENABLED"] = "true"
        relay.start("/environment-api", env)
        relay.check("/environment-panel", "21d", 51000, 51100, "environment:")
        docker("restart", relay.name)
        relay.refresh_address()
        relay.wait()
        relay.check("/environment-panel", "21d", 51000, 51100, "environment:")
        print(f"PASS {suffix}: ENV-only settings and service snapshot protect CLI without matching ENV, including restart", flush=True)
        relay.start()
        relay.check("/database-panel", "3d", 52000, 52100, "database")
        assert relay.cli("admin-web", "path", "/editable-again").returncode == 0
        print(f"PASS {suffix}: removing file/ENV protections restores original DB values and CLI editing", flush=True)
        relay.stop()
        result = relay.cli("status", env={"CHATMAIL_ADMIN_WEB_ENABLED": "not-a-bool"}, check=False)
        assert result.returncode, "invalid ENV accepted"
        invalid = ('admin_web_enabled = "not-a-bool"\n' if suffix == "toml"
                   else 'settings {\n admin_web_enabled not-a-bool\n}\n')
        relay.write(invalid)
        assert relay.cli("status", check=False).returncode, "invalid file boolean accepted"
        invalid = ('unknown_operational_key = true\n' if suffix == "toml"
                   else 'settings {\n unknown_operational_key true\n}\n')
        relay.write(invalid)
        assert relay.cli("status", check=False).returncode, "unknown setting accepted"
        invalid = ('turn_relay_port_min = 60000\nturn_relay_port_max = 50000\n' if suffix == "toml"
                   else 'settings {\n turn_relay_port_min 60000\n turn_relay_port_max 50000\n}\n')
        relay.write(invalid)
        assert relay.cli("status", check=False).returncode, "invalid relay range accepted"
        print(f"PASS {suffix}: invalid ENV, unknown settings and reversed port ranges rejected", flush=True)
    finally:
        relay.cleanup()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--image", default="madmail-local:test-docker")
    args = parser.parse_args()
    docs = (Path(__file__).resolve().parents[1] / "docs/guide/configuration.md").read_text()
    for suffix in ("toml", "conf"):
        snippet = re.search(r"```" + suffix + r"\n(.*?)```", docs, re.S).group(1)
        matrix(args.image, suffix, snippet)
    print("PASS: TOML, .conf, ENV and database precedence matrix")
