"""Regressions for the unsigned prerelease asset contract. No network."""

import hashlib
import io
import struct
import tarfile
import unittest
import zipfile
from pathlib import Path

import subprocess

import unstable_assets as assets


GENERATION = "a" * 40
VERSION = "1.2.4"
RELAYS = {
    "x86_64-unknown-linux-musl": b"RELAY-BYTES-X64-PINNED",
    "aarch64-unknown-linux-musl": b"RELAY-BYTES-ARM-PINNED",
    "x86_64-pc-windows-msvc": b"RELAY-BYTES-WIN-PINNED",
}


def elf(machine, glibc, extra=b""):
    if not glibc:
        header = bytearray(64)
        header[0:4] = b"\x7fELF"
        header[4] = 2
        header[5] = 1
        struct.pack_into("<H", header, 18, machine)
        return bytes(header) + extra
    strings = b"\0libc.so.6\0"
    offsets = []
    for major, minor in glibc:
        offsets.append(len(strings))
        strings += f"GLIBC_{major}.{minor}\0".encode()
    verneed = bytearray(struct.pack("<HHIII", 1, len(offsets), 1, 16, 0))
    for index, offset in enumerate(offsets):
        nxt = 16 if index + 1 < len(offsets) else 0
        verneed += struct.pack("<IHHII", 0, 0, 1, offset, nxt)
    shstr = b"\0.dynstr\0.gnu.version_r\0.shstrtab\0"
    shoff = 64
    shnum = 4
    content_at = shoff + shnum * 64
    dynstr_at = content_at
    ver_at = dynstr_at + len(strings)
    shstr_at = ver_at + len(verneed)
    parts = bytearray(content_at)
    parts[0:4] = b"\x7fELF"
    parts[4] = 2
    parts[5] = 1
    struct.pack_into("<H", parts, 18, machine)
    struct.pack_into("<Q", parts, 40, shoff)
    struct.pack_into("<H", parts, 58, 64)
    struct.pack_into("<H", parts, 60, shnum)
    struct.pack_into("<H", parts, 62, 3)

    def shdr(name, kind, offset, size, link=0):
        header = bytearray(64)
        struct.pack_into("<I", header, 0, name)
        struct.pack_into("<I", header, 4, kind)
        struct.pack_into("<Q", header, 24, offset)
        struct.pack_into("<Q", header, 32, size)
        struct.pack_into("<I", header, 40, link)
        return header

    parts[shoff : shoff + 64] = shdr(0, 0, 0, 0)
    parts[shoff + 64 : shoff + 128] = shdr(1, 3, dynstr_at, len(strings))
    parts[shoff + 128 : shoff + 192] = shdr(9, assets.SHT_GNU_VERNEED, ver_at, len(verneed), 1)
    parts[shoff + 192 : shoff + 256] = shdr(24, 3, shstr_at, len(shstr))
    return bytes(parts) + strings + bytes(verneed) + shstr + extra


def pe(machine, extra=b""):
    data = bytearray(96)
    data[0:2] = b"MZ"
    struct.pack_into("<I", data, 0x3C, 64)
    data[64:68] = b"PE\0\0"
    struct.pack_into("<H", data, 68, machine)
    return bytes(data) + extra


def static_pie(machine, extra=b"", *, needed=False, interp=None, abi=True, dynamic=True):
    """ELF64 with real program headers. Static PIE may include PT_DYNAMIC without DT_NEEDED."""
    kinds = []
    if abi:
        kinds.append("note")
    if interp is not None:
        kinds.append("interp")
    if dynamic:
        kinds.append("dynamic")
    phoff = 64
    phentsize = 56
    blob = bytearray(phoff + len(kinds) * phentsize)
    blob[0:4] = b"\x7fELF"
    blob[4] = 2
    blob[5] = 1
    struct.pack_into("<H", blob, 18, machine)
    struct.pack_into("<Q", blob, 32, phoff)
    struct.pack_into("<H", blob, 54, phentsize)
    struct.pack_into("<H", blob, 56, len(kinds))
    cursor = len(blob)
    placed = []
    for kind in kinds:
        if kind == "note":
            raw = struct.pack("<III", 4, 16, assets.NT_GNU_ABI_TAG)
            raw += b"GNU\0" + struct.pack("<IIII", 0, 3, 2, 0)
        elif kind == "interp":
            raw = interp if isinstance(interp, bytes) else interp.encode()
            if not raw.endswith(b"\0"):
                raw += b"\0"
        else:
            raw = b""
            if needed:
                raw += struct.pack("<qQ", assets.DT_NEEDED, 1)
            raw += struct.pack("<qQ", assets.DT_NULL, 0)
        placed.append((kind, cursor, raw))
        blob.extend(raw)
        cursor += len(raw)
    type_by_kind = {"note": assets.PT_NOTE, "interp": assets.PT_INTERP, "dynamic": assets.PT_DYNAMIC}
    for index, (kind, offset, raw) in enumerate(placed):
        struct.pack_into(
            "<IIQQQQQQ",
            blob,
            phoff + index * phentsize,
            type_by_kind[kind],
            4,
            offset,
            0,
            0,
            len(raw),
            len(raw),
            4,
        )
    blob.extend(extra)
    return bytes(blob)


def binary_for(server, version=VERSION, generation=GENERATION, relays=None, glibc=None):
    relays = RELAYS if relays is None else relays
    marker = (
        assets.version_marker(version)
        + assets.admin_marker(generation)
        + assets.iroh_marker(server.relay)
        + server.name
    ).encode() + relays[server.relay]
    if server.link == "legacy":
        return static_pie(server.machine, marker)
    if server.link == "glibc":
        versions = [(2, 34), (2, 39)] if glibc is None else glibc
    elif server.link == "musl":
        versions = [] if glibc is None else glibc
    else:
        versions = None
    if server.family == "elf":
        return elf(server.machine, versions or [], marker)
    return pe(server.machine, marker)


def _git(root, *args):
    result = subprocess.run(
        ["git", *args], cwd=root, text=True, capture_output=True, check=False
    )
    if result.returncode:
        raise AssertionError(result.stderr or result.stdout)
    return result.stdout.strip()


def _git_commit(root, files, message):
    root.mkdir(parents=True, exist_ok=True)
    if not (root / ".git").exists():
        _git(root, "init", "-b", "main")
        _git(root, "config", "user.email", "test@example.invalid")
        _git(root, "config", "user.name", "Test")
    for name, text in files.items():
        path = root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")
    _git(root, "add", ".")
    _git(root, "commit", "-m", message)


def write_inputs(directory, **overrides):
    directory.mkdir(parents=True)
    blobs = {}
    for server in assets.SERVERS:
        blobs[server.name] = binary_for(server)
    blobs[assets.TRAY_NAME] = pe(
        assets.PE_AMD64, assets.version_marker(VERSION).encode() + b"tray-only"
    )
    blobs[assets.SETUP_NAME] = pe(
        assets.INSTALLER_MACHINE,
        f"{assets.TESTING_MARK}\nversion:{VERSION}\n".encode() + b"setup",
    )
    blobs.update(overrides)
    for name, blob in blobs.items():
        (directory / name).write_bytes(blob)
    return blobs


class AssetContractTests(unittest.TestCase):
    def test_checksum_covers_payloads_and_source_but_not_itself(self):
        self.assertEqual(len(assets.UPLOADED_NAMES), 16)
        self.assertEqual(len(assets.PAYLOAD_NAMES), 14)
        self.assertNotIn(assets.SUMS_NAME, assets.PAYLOAD_NAMES)
        self.assertIn(assets.SOURCE_NAME, assets.UPLOADED_NAMES)
        directory = Path(self.enterContext(__import__("tempfile").TemporaryDirectory()))
        write_inputs(directory / "in")
        prepared = assets.prepare(directory / "in", directory / "out", VERSION, GENERATION, RELAYS)
        source = directory / "out" / assets.SOURCE_NAME
        source.write_text("b" * 40 + "\n", encoding="utf-8")
        text = assets.checksum_text(prepared + [source])
        names = [line.split("  ", 1)[1] for line in text.splitlines()]
        self.assertEqual(names, sorted(assets.PAYLOAD_NAMES + (assets.SOURCE_NAME,)))
        self.assertNotIn(assets.SUMS_NAME, names)
        with self.assertRaisesRegex(assets.AssetError, "SHA256SUMS"):
            assets.checksum_text(prepared + [source, directory / "out" / assets.SUMS_NAME])

    def test_archives_contain_the_exact_binary_bytes_and_mode(self):
        directory = Path(self.enterContext(__import__("tempfile").TemporaryDirectory()))
        blobs = write_inputs(directory / "in")
        assets.prepare(directory / "in", directory / "out", VERSION, GENERATION, RELAYS)
        again = directory / "again"
        assets.prepare(directory / "in", again, VERSION, GENERATION, RELAYS)
        for server in assets.SERVERS:
            packed = (directory / "out" / server.archive).read_bytes()
            assets.assert_archive(server.package, packed, server.member, blobs[server.name])
            self.assertEqual(packed, (again / server.archive).read_bytes())

    def test_missing_extra_and_identical_binaries_are_rejected(self):
        directory = Path(self.enterContext(__import__("tempfile").TemporaryDirectory()))
        write_inputs(directory / "ok")
        (directory / "ok" / assets.TRAY_NAME).unlink()
        with self.assertRaisesRegex(assets.AssetError, "eight required"):
            assets.prepare(directory / "ok", directory / "out1", VERSION, GENERATION, RELAYS)
        write_inputs(directory / "extra")
        (directory / "extra" / "NOTES.txt").write_bytes(b"no")
        with self.assertRaisesRegex(assets.AssetError, "eight required"):
            assets.prepare(directory / "extra", directory / "out2", VERSION, GENERATION, RELAYS)
        write_inputs(directory / "same")
        stolen = (directory / "same" / "madmail-linux-amd64").read_bytes()
        (directory / "same" / "madmail-linux-amd64-legacy").write_bytes(stolen)
        with self.assertRaisesRegex(assets.AssetError, "not distinct"):
            assets.prepare(directory / "same", directory / "out3", VERSION, GENERATION, RELAYS)

    def test_wrong_version_generation_relay_and_link_are_rejected(self):
        server = assets.SERVER_BY_NAME["madmail-linux-amd64-legacy"]
        good = binary_for(server)
        assets.check_server(server, good, VERSION, GENERATION, RELAYS)
        wrong_version = binary_for(server, version="9.9.9")
        with self.assertRaisesRegex(assets.AssetError, "version marker"):
            assets.check_server(server, wrong_version, VERSION, GENERATION, RELAYS)
        wrong_ui = binary_for(server, generation="b" * 40)
        with self.assertRaisesRegex(assets.AssetError, "admin-web"):
            assets.check_server(server, wrong_ui, VERSION, GENERATION, RELAYS)
        foreign = dict(RELAYS)
        foreign["x86_64-unknown-linux-musl"] = b"SOME-OTHER-RELAY"
        mismatched = binary_for(server, relays=foreign)
        with self.assertRaisesRegex(assets.AssetError, "pinned iroh relay"):
            assets.check_server(server, mismatched, VERSION, GENERATION, RELAYS)
        dynamic = elf(server.machine, [(2, 35)], good[64:])
        with self.assertRaisesRegex(assets.AssetError, "fully static"):
            assets.check_server(server, dynamic, VERSION, GENERATION, RELAYS)
        musl_bytes = binary_for(assets.SERVER_BY_NAME["madmail-linux-amd64-musl"])
        with self.assertRaisesRegex(assets.AssetError, "glibc static-pie"):
            assets.check_link(server, musl_bytes)
        musl = assets.SERVER_BY_NAME["madmail-linux-arm64-musl"]
        glibc_musl = binary_for(musl, glibc=[(2, 17)])
        with self.assertRaisesRegex(assets.AssetError, "requires glibc"):
            assets.check_server(musl, glibc_musl, VERSION, GENERATION, RELAYS)
        windows = assets.SERVER_BY_NAME["madmail-windows-amd64.exe"]
        arm_windows = pe(0xAA64, binary_for(windows)[96:])
        with self.assertRaisesRegex(assets.AssetError, "PE architecture"):
            assets.check_server(windows, arm_windows, VERSION, GENERATION, RELAYS)

    def test_installer_must_identify_testing_and_tray_is_not_a_server(self):
        tray = pe(assets.PE_AMD64, assets.version_marker(VERSION).encode())
        assets.check_tray(tray, VERSION)
        with self.assertRaisesRegex(assets.AssetError, "version marker"):
            assets.check_tray(pe(assets.PE_AMD64, b"tray"), VERSION)
        setup = pe(assets.INSTALLER_MACHINE, f"{assets.TESTING_MARK} {VERSION}".encode())
        assets.check_installer(setup, VERSION)
        utf16 = pe(
            assets.INSTALLER_MACHINE,
            assets.TESTING_MARK.encode("utf-16le") + VERSION.encode("utf-16le"),
        )
        assets.check_installer(utf16, VERSION)
        with self.assertRaisesRegex(assets.AssetError, "testing marker"):
            assets.check_installer(pe(assets.INSTALLER_MACHINE, VERSION.encode()), VERSION)
        with self.assertRaisesRegex(assets.AssetError, "launcher"):
            assets.check_installer(
                pe(assets.PE_AMD64, f"{assets.TESTING_MARK} {VERSION}".encode()),
                VERSION,
            )

    def test_pins_describe_three_public_relays_and_reject_a_bad_archive(self):
        self.assertEqual(set(assets.IROH_RELAYS), set(RELAYS))
        for triple, pin in assets.IROH_RELAYS.items():
            self.assertTrue(pin["url"].startswith("https://github.com/n0-computer/iroh/releases/download/v0.35.0/"))
            self.assertEqual(len(pin["archive_sha256"]), 64)
            self.assertEqual(len(pin["binary_sha256"]), 64)
            self.assertGreater(pin["archive_size"], 0)
            if triple.endswith("linux-musl"):
                self.assertEqual(pin["family"], "elf")
                self.assertEqual(pin["member"], "iroh-relay")
            else:
                self.assertEqual(pin["family"], "pe")
                self.assertEqual(pin["member"], "iroh-relay.exe")
        blob = elf(62, [])
        pin = {
            "demo": {
                "url": "https://example.invalid/relay.tar.gz",
                "archive_sha256": "",
                "archive_size": 0,
                "package": "tar",
                "member": "iroh-relay",
                "binary_sha256": hashlib.sha256(blob).hexdigest(),
                "family": "elf",
                "machine": 62,
            }
        }
        raw = io.BytesIO()
        with tarfile.open(fileobj=raw, mode="w:gz", format=tarfile.USTAR_FORMAT) as archive:
            info = tarfile.TarInfo("iroh-relay")
            info.size = len(blob)
            info.mode = 0o755
            archive.addfile(info, io.BytesIO(blob))
        data = raw.getvalue()
        pin["demo"]["archive_sha256"] = hashlib.sha256(data).hexdigest()
        pin["demo"]["archive_size"] = len(data)
        self.assertEqual(assets.verify_relay_archive("demo", data, pin), blob)
        pin["demo"]["machine"] = 183
        with self.assertRaisesRegex(assets.AssetError, "ELF architecture"):
            assets.verify_relay_archive("demo", data, pin)

    def test_fetch_uses_the_opener_and_checks_the_hash(self):
        blob = pe(assets.PE_AMD64, b"relay")
        raw = io.BytesIO()
        with zipfile.ZipFile(raw, "w") as archive:
            info = zipfile.ZipInfo("iroh-relay.exe", date_time=(1980, 1, 1, 0, 0, 0))
            info.external_attr = 0o100755 << 16
            info.create_system = 3
            archive.writestr(info, blob)
        data = raw.getvalue()
        pin = {
            "x86_64-pc-windows-msvc": {
                "url": "https://example.invalid/relay.zip",
                "archive_sha256": hashlib.sha256(data).hexdigest(),
                "archive_size": len(data),
                "package": "zip",
                "member": "iroh-relay.exe",
                "binary_sha256": hashlib.sha256(blob).hexdigest(),
                "family": "pe",
                "machine": assets.PE_AMD64,
            }
        }
        class Response:
            def __init__(self):
                self.offset = 0

            def __enter__(self):
                return self

            def __exit__(self, *_args):
                return False

            def read(self, size):
                chunk = data[self.offset : self.offset + size]
                self.offset += len(chunk)
                return chunk

        def opener(url):
            self.assertEqual(url, pin["x86_64-pc-windows-msvc"]["url"])
            return Response()

        fetched = assets.fetch_relays(opener=opener, pins=pin)
        self.assertEqual(fetched["x86_64-pc-windows-msvc"], blob)
        pin["x86_64-pc-windows-msvc"]["archive_sha256"] = "0" * 64
        with self.assertRaisesRegex(assets.AssetError, "pinned hash"):
            assets.fetch_relays(opener=opener, pins=pin)

    def test_tool_archive_rejects_links_and_path_escape(self):
        directory = Path(self.enterContext(__import__("tempfile").TemporaryDirectory()))
        archive_path = directory / "tools.tar"
        with tarfile.open(archive_path, "w") as archive:
            info = tarfile.TarInfo("../escape")
            info.size = 1
            archive.addfile(info, io.BytesIO(b"x"))
        with tarfile.open(archive_path) as archive:
            with self.assertRaisesRegex(assets.AssetError, "escapes"):
                assets._safe_extract(archive, directory / "dest")
        link_path = directory / "link.tar"
        with tarfile.open(link_path, "w") as archive:
            info = tarfile.TarInfo("zig")
            info.type = tarfile.SYMTYPE
            info.linkname = "other"
            archive.addfile(info)
        with tarfile.open(link_path) as archive:
            with self.assertRaisesRegex(assets.AssetError, "link"):
                assets._safe_extract(archive, directory / "dest2")

    def test_check_source_rejects_version_and_gitlink_mismatch(self):
        root = Path(self.enterContext(__import__("tempfile").TemporaryDirectory()))
        admin = root / "admin-origin"
        _git_commit(admin, {"README": "ui\n"}, message="admin")
        admin_sha = _git(admin, "rev-parse", "HEAD")
        checkout = root / "repo"
        checkout.mkdir()
        _git(checkout, "init", "-b", "main")
        _git(checkout, "config", "user.email", "test@example.invalid")
        _git(checkout, "config", "user.name", "Test")
        (checkout / "external").mkdir()
        clone = subprocess.run(
            ["git", "clone", "--quiet", str(admin), str(checkout / "external/madmail-admin-web")],
            check=False,
            capture_output=True,
            text=True,
        )
        if clone.returncode:
            raise AssertionError(clone.stderr or clone.stdout)
        (checkout / ".version").write_text(VERSION + "\n", encoding="utf-8")
        (checkout / "Cargo.toml").write_text(
            '[workspace.package]\nversion = "1.2.4"\n', encoding="utf-8"
        )
        (checkout / "Cargo.lock").write_text("# lock\n", encoding="utf-8")
        _git(checkout, "add", ".")
        _git(checkout, "commit", "-m", "seed")
        sha = _git(checkout, "rev-parse", "HEAD")
        self.assertEqual(assets.check_source(checkout, VERSION, sha), admin_sha)
        (checkout / ".version").write_text("9.9.9\n", encoding="utf-8")
        with self.assertRaisesRegex(assets.AssetError, r"\.version"):
            assets.check_source(checkout, VERSION, sha)
        (checkout / ".version").write_text(VERSION + "\n", encoding="utf-8")
        (checkout / "Cargo.toml").write_text(
            '[workspace.package]\nversion = "9.9.9"\n', encoding="utf-8"
        )
        with self.assertRaisesRegex(assets.AssetError, "Cargo.toml"):
            assets.check_source(checkout, VERSION, sha)
        (checkout / "Cargo.toml").write_text(
            '[workspace.package]\nversion = "1.2.4"\n', encoding="utf-8"
        )
        (checkout / "Cargo.lock").write_text("# changed\n", encoding="utf-8")
        with self.assertRaisesRegex(assets.AssetError, "Cargo.lock"):
            assets.check_source(checkout, VERSION, sha)
        (checkout / "Cargo.lock").write_text("# lock\n", encoding="utf-8")
        _git_commit(admin, {"README": "next\n"}, message="next")
        _git(checkout / "external/madmail-admin-web", "fetch", str(admin), "main")
        _git(checkout / "external/madmail-admin-web", "checkout", "--detach", "FETCH_HEAD")
        with self.assertRaisesRegex(assets.AssetError, "gitlink"):
            assets.check_source(checkout, VERSION, sha)

    def test_host_elf_parser_sees_glibc_without_executing_it(self):
        candidate = Path("/bin/ls")
        if not candidate.is_file() or candidate.read_bytes()[:4] != b"\x7fELF":
            self.skipTest("host /bin/ls is not an ELF")
        data = candidate.read_bytes()
        versions = assets.glibc_versions(data)
        self.assertTrue(versions)
        self.assertLessEqual(min(versions), (2, 35))
        self.assertTrue(assets.elf_interpreter(data) or assets.elf_needed_libraries(data))
        self.assertTrue(assets.elf_has_gnu_abi_tag(data))

    def test_musl_rejects_interpreter_and_needed_but_allows_static_pie(self):
        server = assets.SERVER_BY_NAME["madmail-linux-amd64-musl"]
        arm = assets.SERVER_BY_NAME["madmail-linux-arm64-musl"]
        for role in (server, arm):
            accepted = static_pie(role.machine, _marker(role), abi=False, dynamic=True)
            assets.check_server(role, accepted, VERSION, GENERATION, RELAYS)
            self.assertTrue(assets.elf_program_headers(accepted))
            self.assertFalse(assets.elf_needed_libraries(accepted))
            needed = static_pie(role.machine, _marker(role), abi=False, needed=True)
            with self.assertRaisesRegex(assets.AssetError, "shared libraries"):
                assets.check_server(role, needed, VERSION, GENERATION, RELAYS)
        # Review fixture: a previously accepted musl file plus one real PT_INTERP header.
        blob = bytearray(binary_for(server))
        interp = b"/lib/ld-musl-x86_64.so.1\0"
        off = len(blob)
        blob.extend(bytes(56))
        path_off = len(blob)
        blob.extend(interp)
        struct.pack_into("<Q", blob, 32, off)
        struct.pack_into("<H", blob, 54, 56)
        struct.pack_into("<H", blob, 56, 1)
        struct.pack_into("<IIQQQQQQ", blob, off, 3, 4, path_off, 0, 0, len(interp), len(interp), 1)
        with self.assertRaisesRegex(assets.AssetError, "ELF interpreter"):
            assets.check_server(server, bytes(blob), VERSION, GENERATION, RELAYS)
        host = Path("/bin/ls")
        if host.is_file() and host.read_bytes()[:4] == b"\x7fELF":
            data = host.read_bytes()
            if assets.elf_machine(data) == server.machine:
                with self.assertRaises(assets.AssetError):
                    assets.check_link(server, data)
                with self.assertRaisesRegex(assets.AssetError, "fully static"):
                    assets.check_link(assets.SERVER_BY_NAME["madmail-linux-amd64-legacy"], data)

    def test_compiled_crt_static_probe_matches_the_legacy_contract(self):
        rustc = shutil_which("rustc")
        if rustc is None:
            self.skipTest("rustc is not installed")
        if not Path("/usr/lib/x86_64-linux-gnu/libc.a").is_file():
            self.skipTest("glibc static archives are not installed")
        directory = Path(self.enterContext(__import__("tempfile").TemporaryDirectory()))
        source = directory / "probe.rs"
        source.write_text("fn main() {}\n", encoding="utf-8")
        binary = directory / "probe"
        compiled = subprocess.run(
            [rustc, "--edition", "2021", "-C", "target-feature=+crt-static", "-o", str(binary), str(source)],
            text=True,
            capture_output=True,
            check=False,
        )
        if compiled.returncode:
            self.fail(compiled.stderr or compiled.stdout)
        data = binary.read_bytes()
        self.assertIsNone(assets.elf_interpreter(data))
        self.assertEqual(assets.elf_needed_libraries(data), [])
        self.assertTrue(assets.elf_has_gnu_abi_tag(data))
        self.assertTrue(any(header["type"] == assets.PT_DYNAMIC for header in assets.elf_program_headers(data)))
        self.assertFalse(assets.glibc_versions(data))
        legacy = assets.SERVER_BY_NAME["madmail-linux-amd64-legacy"]
        assets.check_link(legacy, data)
        dynamic = subprocess.run(
            [rustc, "--edition", "2021", "-o", str(directory / "dynamic"), str(source)],
            text=True,
            capture_output=True,
            check=False,
        )
        if dynamic.returncode:
            self.fail(dynamic.stderr or dynamic.stdout)
        with self.assertRaisesRegex(assets.AssetError, "fully static"):
            assets.check_link(legacy, (directory / "dynamic").read_bytes())

    def test_locked_sqlite_is_bundled_and_workflows_do_not_prepare_another(self):
        root = Path(__file__).resolve().parents[1]
        cargo = (root / "Cargo.toml").read_text(encoding="utf-8")
        self.assertIn('"sqlite"', cargo)
        self.assertNotIn("sqlite-unbundled", cargo)
        lock = (root / "Cargo.lock").read_text(encoding="utf-8")
        self.assertIn('name = "sqlx"\nversion = "0.8.6"', lock)
        self.assertIn('name = "libsqlite3-sys"\nversion = "0.30.1"', lock)
        workflows = [
            root / ".github/workflows/ci.yml",
            root / ".github/workflows/unstable-matrix.yml",
        ]
        texts = [path.read_text(encoding="utf-8") for path in workflows]
        for forbidden in (
            "prepare-sqlite",
            "SQLITE3_LIB_DIR",
            "LIBSQLITE3_SYS_USE_PKG_CONFIG",
            "sqlite-autoconf",
            "x86_64-unknown-linux-gnu.2.35",
        ):
            for text in texts:
                self.assertNotIn(forbidden, text)
        self.assertIn("target-feature=+crt-static", texts[1])
        self.assertNotIn("target-feature=+crt-static", texts[0])

    def test_testing_installer_refuses_stable_resources_before_copying_files(self):
        iss = Path(__file__).resolve().parents[1] / "packaging/windows/Madmail.iss"
        text = iss.read_text(encoding="utf-8")
        stable_app = "AppId={{8F3C2A91-6B4E-4D2A-9C71-0A1B2C3D4E5F}"
        self.assertIn(stable_app, text)
        self.assertNotIn("SetupArchitecture=", text)
        flags = {
            "Arch": "amd64",
            "MyAppVersion": VERSION,
            "RequireTray": "1",
        }
        stable = _iss_lines(text, {**flags, "TestingBuild": "0"})
        testing = _iss_lines(text, {**flags, "TestingBuild": "1"})
        default = _iss_lines(text, dict(flags))
        stable_ids = [line for line in stable if line.startswith("AppId=")]
        testing_ids = [line for line in testing if line.startswith("AppId=")]
        self.assertEqual(stable_ids, [stable_app])
        self.assertEqual([line for line in default if line.startswith("AppId=")], [stable_app])
        self.assertEqual(len(testing_ids), 1)
        self.assertNotEqual(testing_ids, stable_ids)
        stable_text = "\n".join(stable)
        testing_text = "\n".join(testing)
        self.assertIn('#define MyAppName "Madmail"', stable_text)
        self.assertIn('#define MyServiceName "Madmail"', stable_text)
        self.assertIn('Name: "{commonappdata}\\Madmail"', stable_text)
        self.assertIn("DefaultDirName={autopf}\\{#MyAppName}", stable_text)
        self.assertNotIn("function InitializeSetup", stable_text)
        self.assertNotIn("TestingStableResourcesPresent", stable_text)
        self.assertNotIn("UsePreviousAppDir=no", stable_text)
        self.assertNotIn("MadmailTesting", stable_text)
        self.assertIn("function InitializeSetup", testing_text)
        self.assertIn("TestingStableResourcesPresent", testing_text)
        self.assertIn("Result := not TestingStableResourcesPresent", testing_text)
        self.assertIn("8F3C2A91-6B4E-4D2A-9C71-0A1B2C3D4E5F", testing_text)
        self.assertIn("_is1", testing_text)
        self.assertIn("SYSTEM\\CurrentControlSet\\Services\\Madmail", testing_text)
        self.assertIn("{commonappdata}\\Madmail", testing_text)
        self.assertIn("{autopf}\\Madmail", testing_text)
        self.assertIn("MadmailTray", testing_text)
        self.assertIn("UsePreviousAppDir=no", testing_text)
        self.assertIn("DefaultDirName={autopf}\\MadmailTesting", testing_text)
        self.assertIn(
            '#define MyTestingLabel MyAppVersion + "-unstable TESTING ONLY - NOT FOR PRODUCTION USE"',
            text,
        )
        self.assertNotIn("{#MyAppVersion}-unstable", text)
        self.assertIn('#define MyServiceName "Madmail"', testing_text)
        self.assertIn('Name: "{commonappdata}\\Madmail"', testing_text)
        gate = testing_text.split("function InitializeSetup", 1)[1].split("function ", 1)[0]
        for effect in ("FileCopy", "DeleteFile", "DelTree", "Exec(", "ShellExec"):
            self.assertNotIn(effect, gate)
        self.assertIn("SuppressibleMsgBox(", gate)
        self.assertFalse(any(line.strip().startswith("MsgBox(") for line in gate.splitlines()))


def _marker(server):
    return (
        assets.version_marker(VERSION)
        + assets.admin_marker(GENERATION)
        + assets.iroh_marker(server.relay)
        + server.name
    ).encode() + RELAYS[server.relay]


def _iss_lines(text, flags):
    """Resolve the iss preprocessor the same way a /D flag set does. No Inno run."""
    active = True
    stack = []
    found = []
    flags = dict(flags)
    for raw in text.splitlines():
        line = raw.strip()
        if line.startswith("#ifndef "):
            condition = line.split()[1] not in flags
            stack.append((active, condition))
            active = active and condition
        elif line.startswith("#if "):
            expression = line[4:].strip()
            name, value = expression.split(" == ", 1)
            condition = flags.get(name, "") == value.strip('"')
            stack.append((active, condition))
            active = active and condition
        elif line == "#else":
            parent, condition = stack[-1]
            active = parent and not condition
        elif line == "#endif":
            active = stack.pop()[0]
        elif active and line.startswith("#define "):
            _keyword, name, value = line.split(None, 2)
            flags[name] = value.strip('"')
            found.append(line)
        elif active:
            found.append(line)
    return found


def shutil_which(name):
    return __import__("shutil").which(name)


if __name__ == "__main__":
    unittest.main()
