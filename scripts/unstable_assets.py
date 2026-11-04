#!/usr/bin/env python3
"""Unsigned prerelease asset contract.

Future testing prereleases upload 16 assets: six server binaries, six
archives, the Windows tray, an unsigned testing installer, ``SOURCE.txt``,
and ``SHA256SUMS``. The checksum file covers every other uploaded payload
and ``SOURCE.txt``. It does not list itself.

Linux ``.tar.gz`` archives contain one regular file named ``madmail`` with
mode ``0755``. The Windows ``.zip`` contains one regular file named
``madmail.exe`` with the same mode. Those layouts match the public v2.30.5
release assets.

``madmail-linux-amd64-legacy`` is the public static-pie build:
``cargo rustc -p chatmail --release --locked --bin madmail -- -C target-feature=+crt-static``.
The root manifest is virtual, so the server package has to be selected.
The ``+crt-static`` flag stays after ``--``.
``scripts/build-release-static.sh`` is not in this repository. The checker
requires no ELF interpreter, no ``DT_NEEDED`` entries, and a GNU ABI note.
A GLIBC symbol ceiling is not that static contract. ``zig cc`` is not used
for this file.

Every server links the SQLite amalgamation bundled in the locked
``libsqlite3-sys`` crate. The workspace ``sqlx`` feature ``sqlite`` enables
``sqlx-sqlite/bundled``. There is no separate SQLite download. The default
amd64 build does not link the runner's system SQLite.

The unsigned Windows installer is compiled with Inno Setup 6. That compiler
emits an x86 setup launcher (``IMAGE_FILE_MACHINE_I386``, ``0x14c``) even
when the packaged server and tray are amd64. ``ArchitecturesInstallIn64BitMode``
selects the install mode, not the launcher. The server and tray stay amd64.

Iroh relays are the public ``n0-computer/iroh`` v0.35.0 release assets.
Archive and member hashes below were computed from those official files.
GitHub did not publish a checksum file for that release. Architecture was
read from ELF and PE headers. The relay binaries were not executed.
"""

import argparse
import gzip
import hashlib
import io
import re
import shutil
import stat
import subprocess
import sys
import tarfile
import tomllib
import urllib.request
import zipfile
from dataclasses import dataclass
from pathlib import Path

IROH_VERSION = "v0.35.0"
# Official n0-computer/iroh v0.35.0 assets. Hashes are sha256 of the archive
# bytes and of the single extracted relay member.
IROH_RELAYS = {
    "x86_64-unknown-linux-musl": {
        "url": (
            "https://github.com/n0-computer/iroh/releases/download/"
            "v0.35.0/iroh-relay-v0.35.0-x86_64-unknown-linux-musl.tar.gz"
        ),
        "archive_sha256": "4103e64c5cd1fb94e8b11df0b9c572a38d44c6fd3cc174b921c806e7a183aadf",
        "archive_size": 4599527,
        "package": "tar",
        "member": "iroh-relay",
        "binary_sha256": "45c81199dbd70f8c4c30fef7f3b9727ca6e3cea8f2831333eeaf8aa71bf0fac1",
        "family": "elf",
        "machine": 62,
    },
    "aarch64-unknown-linux-musl": {
        "url": (
            "https://github.com/n0-computer/iroh/releases/download/"
            "v0.35.0/iroh-relay-v0.35.0-aarch64-unknown-linux-musl.tar.gz"
        ),
        "archive_sha256": "9e4bea5072f91844c4f09ea1f796a16081c28124538476373661954ff53bfbb5",
        "archive_size": 4241662,
        "package": "tar",
        "member": "iroh-relay",
        "binary_sha256": "f8ef27631fac213b3ef668d02acd5b3e215292746a3fc71d90c63115446008b1",
        "family": "elf",
        "machine": 183,
    },
    "x86_64-pc-windows-msvc": {
        "url": (
            "https://github.com/n0-computer/iroh/releases/download/"
            "v0.35.0/iroh-relay-v0.35.0-x86_64-pc-windows-msvc.zip"
        ),
        "archive_sha256": "8454a3d3445cd9ed3b48b54c4916370e0d53cf7dafcc09f3152709663c75f107",
        "archive_size": 3880297,
        "package": "zip",
        "member": "iroh-relay.exe",
        "binary_sha256": "7f2adfb3db14880aeef013d0334f7aa4114db2ebcdbf196fb9f1d142e91c60e2",
        "family": "pe",
        "machine": 0x8664,
    },
}

ZIG_URL = "https://ziglang.org/download/0.14.1/zig-x86_64-linux-0.14.1.tar.xz"
ZIG_SHA256 = "24aeeec8af16c381934a6cd7d95c807a8cb2cf7df9fa40d359aa884195c4716c"
ZIG_SIZE = 49_086_504
ZIG_ROOT = "zig-x86_64-linux-0.14.1"
ZIG_VERSION = "0.14.1"

CARGO_ZIGBUILD_URL = (
    "https://github.com/rust-cross/cargo-zigbuild/releases/download/"
    "v0.23.4/cargo-zigbuild-x86_64-unknown-linux-gnu.tar.xz"
)
CARGO_ZIGBUILD_SHA256 = "6b69963040818ca1ef573d8135a95aa902e17cf7f873c32c6426642dadfe7f59"
CARGO_ZIGBUILD_SIZE = 1_160_688
CARGO_ZIGBUILD_MEMBER = "cargo-zigbuild-x86_64-unknown-linux-gnu/cargo-zigbuild"

TESTING_MARK = "TESTING ONLY"
VERSION_PREFIX = "madmail-build-version:"
ADMIN_PREFIX = "madmail-admin-web-generation:"
SOURCE_NAME = "SOURCE.txt"
SUMS_NAME = "SHA256SUMS"
TRAY_NAME = "madmail-tray-windows-amd64.exe"
SETUP_NAME = "madmail-windows-amd64-setup.exe"
ELF_MACHINE = {"amd64": 62, "arm64": 183}
PE_AMD64 = 0x8664
# Inno Setup 6's setup launcher. The iss file does not select another one.
PE_I386 = 0x14C
INSTALLER_MACHINE = PE_I386
SHT_GNU_VERNEED = 0x6FFFFFFE
PT_DYNAMIC = 2
PT_INTERP = 3
PT_NOTE = 4
DT_NULL = 0
DT_NEEDED = 1
NT_GNU_ABI_TAG = 1
SHA = re.compile(r"[0-9a-f]{40}\Z")


class AssetError(Exception):
    """A packaging failure safe to print in CI."""


@dataclass(frozen=True)
class Server:
    name: str
    archive: str
    member: str
    package: str
    family: str
    machine: int
    link: str
    relay: str


SERVERS = (
    Server(
        "madmail-linux-amd64",
        "madmail-linux-amd64.tar.gz",
        "madmail",
        "tar",
        "elf",
        ELF_MACHINE["amd64"],
        "glibc",
        "x86_64-unknown-linux-musl",
    ),
    Server(
        "madmail-linux-amd64-legacy",
        "madmail-linux-amd64-legacy.tar.gz",
        "madmail",
        "tar",
        "elf",
        ELF_MACHINE["amd64"],
        "legacy",
        "x86_64-unknown-linux-musl",
    ),
    Server(
        "madmail-linux-amd64-musl",
        "madmail-linux-amd64-musl.tar.gz",
        "madmail",
        "tar",
        "elf",
        ELF_MACHINE["amd64"],
        "musl",
        "x86_64-unknown-linux-musl",
    ),
    Server(
        "madmail-linux-arm64",
        "madmail-linux-arm64.tar.gz",
        "madmail",
        "tar",
        "elf",
        ELF_MACHINE["arm64"],
        "glibc",
        "aarch64-unknown-linux-musl",
    ),
    Server(
        "madmail-linux-arm64-musl",
        "madmail-linux-arm64-musl.tar.gz",
        "madmail",
        "tar",
        "elf",
        ELF_MACHINE["arm64"],
        "musl",
        "aarch64-unknown-linux-musl",
    ),
    Server(
        "madmail-windows-amd64.exe",
        "madmail-windows-amd64.zip",
        "madmail.exe",
        "zip",
        "pe",
        PE_AMD64,
        "windows",
        "x86_64-pc-windows-msvc",
    ),
)
SERVER_BY_NAME = {server.name: server for server in SERVERS}
BINARY_NAMES = tuple(server.name for server in SERVERS) + (TRAY_NAME, SETUP_NAME)
ARCHIVE_NAMES = tuple(server.archive for server in SERVERS)
PAYLOAD_NAMES = BINARY_NAMES + ARCHIVE_NAMES
# Uploaded set, including metadata. SHA256SUMS is last and is not a checksum line.
UPLOADED_NAMES = PAYLOAD_NAMES + (SOURCE_NAME, SUMS_NAME)


def iroh_marker(triple):
    return f"madmail-iroh-relay:{IROH_VERSION}:{triple}"


def version_marker(version):
    return f"{VERSION_PREFIX}{version}"


def admin_marker(generation):
    return f"{ADMIN_PREFIX}{generation}"


def _u16(data, offset):
    return int.from_bytes(data[offset : offset + 2], "little")


def _u32(data, offset):
    return int.from_bytes(data[offset : offset + 4], "little")


def _u64(data, offset):
    return int.from_bytes(data[offset : offset + 8], "little")


def elf_machine(data):
    if len(data) < 64 or data[:4] != b"\x7fELF" or data[4] != 2 or data[5] != 1:
        raise AssetError("not a little-endian ELF64 file")
    return _u16(data, 18)


def glibc_versions(data):
    """GLIBC symbol versions from SHT_GNU_verneed. Empty means no glibc requirement."""
    if len(data) < 64 or data[:4] != b"\x7fELF" or data[4] != 2 or data[5] != 1:
        raise AssetError("not a little-endian ELF64 file")
    e_shoff = _u64(data, 40)
    e_shentsize = _u16(data, 58)
    e_shnum = _u16(data, 60)
    if e_shnum == 0:
        return set()
    if e_shentsize < 64 or e_shoff + e_shnum * e_shentsize > len(data):
        raise AssetError("truncated ELF section headers")
    found = set()
    for index in range(e_shnum):
        off = e_shoff + index * e_shentsize
        if _u32(data, off + 4) != SHT_GNU_VERNEED:
            continue
        sec_off = _u64(data, off + 24)
        sec_size = _u64(data, off + 32)
        link = _u32(data, off + 40)
        str_off, str_size = _section_bounds(data, e_shoff, e_shentsize, link)
        found |= _verneed_glibc(data, sec_off, sec_size, str_off, str_size)
    return found


def _section_bounds(data, e_shoff, e_shentsize, index):
    off = e_shoff + index * e_shentsize
    if off + 64 > len(data):
        raise AssetError("truncated ELF string table")
    start = _u64(data, off + 24)
    size = _u64(data, off + 32)
    if start + size > len(data):
        raise AssetError("truncated ELF string table")
    return start, size


def _cstring(blob, offset):
    end = blob.find(b"\0", offset)
    if end < 0:
        raise AssetError("unterminated ELF version string")
    return blob[offset:end].decode("ascii", "replace")


def _verneed_glibc(data, sec_off, sec_size, str_off, str_size):
    if sec_off + sec_size > len(data):
        raise AssetError("truncated GNU verneed")
    strings = data[str_off : str_off + str_size]
    found = set()
    cursor = 0
    while cursor + 16 <= sec_size:
        base = sec_off + cursor
        count = _u16(data, base + 2)
        aux = _u32(data, base + 8)
        nxt = _u32(data, base + 12)
        aux_at = cursor + aux
        for _ in range(count):
            if aux_at + 16 > sec_size:
                raise AssetError("truncated GNU vernaux")
            name_at = _u32(data, sec_off + aux_at + 8)
            if name_at >= len(strings):
                raise AssetError("GNU version name is outside the string table")
            name = _cstring(strings, name_at)
            match = re.fullmatch(r"GLIBC_(\d+)\.(\d+)", name)
            if match:
                found.add((int(match.group(1)), int(match.group(2))))
            step = _u32(data, sec_off + aux_at + 12)
            if step == 0:
                break
            aux_at += step
        if nxt == 0:
            break
        cursor += nxt
    return found


def pe_machine(data):
    if len(data) < 0x40 or data[:2] != b"MZ":
        raise AssetError("not a PE file")
    pe = _u32(data, 0x3C)
    if pe + 6 > len(data) or data[pe : pe + 4] != b"PE\0\0":
        raise AssetError("not a PE file")
    return _u16(data, pe + 4)


def elf_program_headers(data):
    """ELF64 program headers. An empty table is not an error."""
    if len(data) < 64 or data[:4] != b"\x7fELF" or data[4] != 2 or data[5] != 1:
        raise AssetError("not a little-endian ELF64 file")
    e_phoff = _u64(data, 32)
    e_phentsize = _u16(data, 54)
    e_phnum = _u16(data, 56)
    if e_phnum == 0:
        return []
    if e_phentsize < 56 or e_phoff + e_phnum * e_phentsize > len(data):
        raise AssetError("truncated ELF program headers")
    headers = []
    for index in range(e_phnum):
        off = e_phoff + index * e_phentsize
        headers.append(
            {
                "type": _u32(data, off),
                "flags": _u32(data, off + 4),
                "offset": _u64(data, off + 8),
                "filesz": _u64(data, off + 32),
                "align": _u64(data, off + 48),
            }
        )
    return headers


def _segment(data, header, label):
    start = header["offset"]
    size = header["filesz"]
    if start + size > len(data):
        raise AssetError(f"truncated ELF {label}")
    return data[start : start + size]


def elf_interpreter(data):
    """Interpreter path from PT_INTERP, or None when the segment is absent.

    A static PIE may still have PT_DYNAMIC. That is not an interpreter.
    """
    found = None
    for header in elf_program_headers(data):
        if header["type"] != PT_INTERP:
            continue
        blob = _segment(data, header, "interpreter")
        found = blob.split(b"\0", 1)[0]
    return found


def elf_needed_libraries(data):
    """DT_NEEDED string-table offsets. Empty when nothing is dynamically linked.

    PT_DYNAMIC with only DT_NULL, or no PT_DYNAMIC at all, returns an empty list.
    """
    needed = []
    for header in elf_program_headers(data):
        if header["type"] != PT_DYNAMIC:
            continue
        blob = _segment(data, header, "dynamic section")
        cursor = 0
        while cursor + 16 <= len(blob):
            tag = int.from_bytes(blob[cursor : cursor + 8], "little", signed=True)
            if tag == DT_NULL:
                break
            if tag == DT_NEEDED:
                needed.append(int.from_bytes(blob[cursor + 8 : cursor + 16], "little"))
            cursor += 16
    return needed


def _note_align(header):
    align = header["align"]
    if align in (4, 8):
        return align
    return 4


def _iter_notes(blob, align):
    cursor = 0
    while cursor + 12 <= len(blob):
        namesz = int.from_bytes(blob[cursor : cursor + 4], "little")
        descsz = int.from_bytes(blob[cursor + 4 : cursor + 8], "little")
        ntype = int.from_bytes(blob[cursor + 8 : cursor + 12], "little")
        name_at = cursor + 12
        name_end = name_at + namesz
        padded_name = (name_end + align - 1) & ~(align - 1)
        desc_end = padded_name + descsz
        padded_desc = (desc_end + align - 1) & ~(align - 1)
        if name_end > len(blob) or desc_end > len(blob):
            break
        name = blob[name_at:name_end].rstrip(b"\0")
        yield name, ntype
        if padded_desc <= cursor:
            break
        cursor = padded_desc


def elf_has_gnu_abi_tag(data):
    """True when a PT_NOTE carries NT_GNU_ABI_TAG (owner GNU, type 1).

    A build-id note is not this tag. Static PIE glibc binaries have the ABI tag
    and no DT_NEEDED entries.
    """
    for header in elf_program_headers(data):
        if header["type"] != PT_NOTE:
            continue
        for name, ntype in _iter_notes(_segment(data, header, "note"), _note_align(header)):
            if name == b"GNU" and ntype == NT_GNU_ABI_TAG:
                return True
    return False


def _require(data, needle, label):
    if needle.encode("utf-8") not in data and needle.encode("utf-16le") not in data:
        raise AssetError(f"missing {label}")


def check_link(server, data):
    if server.family == "elf":
        if elf_machine(data) != server.machine:
            raise AssetError(f"{server.name} has the wrong ELF architecture")
        versions = glibc_versions(data)
        if server.link == "musl":
            if versions:
                raise AssetError(f"{server.name} requires glibc")
            if elf_interpreter(data) is not None:
                raise AssetError(f"{server.name} requires an ELF interpreter")
            if elf_needed_libraries(data):
                raise AssetError(f"{server.name} requires shared libraries")
        elif server.link == "legacy":
            # Fully static glibc has no verneed. A 2.35 ceiling is not this contract.
            if versions or elf_interpreter(data) is not None or elf_needed_libraries(data):
                raise AssetError(f"{server.name} is not a fully static binary")
            if not elf_has_gnu_abi_tag(data):
                raise AssetError(f"{server.name} is not a glibc static-pie binary")
        elif server.link == "glibc":
            if not versions:
                raise AssetError(f"{server.name} is not a glibc build")
        else:
            raise AssetError("unknown server link")
        return
    if pe_machine(data) != server.machine:
        raise AssetError(f"{server.name} has the wrong PE architecture")


def check_server(server, data, version, generation, relays):
    if not data:
        raise AssetError(f"empty binary: {server.name}")
    check_link(server, data)
    _require(data, version_marker(version), f"version marker in {server.name}")
    _require(data, admin_marker(generation), f"admin-web generation in {server.name}")
    marker = iroh_marker(server.relay)
    _require(data, marker, f"iroh marker in {server.name}")
    relay = relays.get(server.relay)
    if not relay or relay not in data:
        raise AssetError(f"{server.name} does not contain its pinned iroh relay")


def check_tray(data, version):
    if not data:
        raise AssetError("empty tray binary")
    if pe_machine(data) != PE_AMD64:
        raise AssetError("tray has the wrong PE architecture")
    _require(data, version_marker(version), "version marker in the tray")


def check_installer(data, version):
    if not data:
        raise AssetError("empty installer")
    # The setup launcher is not the amd64 server or tray. Inno Setup 6 emits 0x14c.
    machine = pe_machine(data)
    if machine != INSTALLER_MACHINE:
        raise AssetError(
            f"installer launcher has the wrong architecture: 0x{machine:x}, "
            f"expected 0x{INSTALLER_MACHINE:x}"
        )
    _require(data, TESTING_MARK, "testing marker in the installer")
    _require(data, version, "numeric version in the installer")
    if b"Ed25519" in data and b"production signing key" in data:
        raise AssetError("installer claims a production signing key")


def load_binaries(directory):
    directory = Path(directory)
    if not directory.is_dir() or directory.is_symlink():
        raise AssetError("assets directory is unsafe")
    root = directory.resolve()
    names = []
    for path in directory.iterdir():
        if path.is_symlink() or not path.is_file():
            raise AssetError("assets directory has an unexpected entry")
        if root not in path.resolve().parents:
            raise AssetError("unsafe asset path")
        names.append(path.name)
    if set(names) != set(BINARY_NAMES) or len(names) != len(BINARY_NAMES):
        raise AssetError("assets directory is not the eight required binaries")
    return {name: (directory / name).read_bytes() for name in BINARY_NAMES}


def _write_tar(path, member, data):
    info = tarfile.TarInfo(member)
    info.size = len(data)
    info.mode = 0o755
    info.type = tarfile.REGTYPE
    info.mtime = 0
    info.uid = 0
    info.gid = 0
    info.uname = ""
    info.gname = ""
    # Gzip mtime stays 0 so a retry of the same bytes is byte-identical.
    raw = io.BytesIO()
    with gzip_open(raw) as compressed:
        with tarfile.open(fileobj=compressed, mode="w", format=tarfile.USTAR_FORMAT) as archive:
            archive.addfile(info, io.BytesIO(data))
    path.write_bytes(raw.getvalue())


def gzip_open(raw):
    return gzip.GzipFile(fileobj=raw, mode="wb", filename="", mtime=0, compresslevel=9)


def _write_zip(path, member, data):
    info = zipfile.ZipInfo(member, date_time=(1980, 1, 1, 0, 0, 0))
    info.compress_type = zipfile.ZIP_DEFLATED
    info.external_attr = 0o100755 << 16
    info.create_system = 3
    with zipfile.ZipFile(path, "w") as archive:
        archive.writestr(info, data)


def archive_members(package, data):
    """Return regular-file members as (name, mode, bytes)."""
    members = []
    if package == "tar":
        try:
            archive = tarfile.open(fileobj=io.BytesIO(data), mode="r:gz")
        except tarfile.TarError as exc:
            raise AssetError("invalid tar.gz") from exc
        with archive:
            for item in archive.getmembers():
                if item.isdir():
                    continue
                if not item.isreg():
                    raise AssetError("archive contains a non-regular member")
                extracted = archive.extractfile(item)
                if extracted is None:
                    raise AssetError("archive member is missing")
                members.append((item.name, item.mode & 0o777, extracted.read()))
        return members
    try:
        archive = zipfile.ZipFile(io.BytesIO(data))
    except zipfile.BadZipFile as exc:
        raise AssetError("invalid zip") from exc
    with archive:
        for info in archive.infolist():
            if info.is_dir():
                continue
            mode = (info.external_attr >> 16) & 0o777
            members.append((info.filename, mode, archive.read(info)))
    return members


def assert_archive(package, data, member, binary):
    members = archive_members(package, data)
    files = [(name, mode, blob) for name, mode, blob in members if Path(name).name == member]
    if len(members) != 1 or len(files) != 1:
        raise AssetError(f"archive must contain only {member}")
    name, mode, blob = files[0]
    if Path(name).name != member or mode != 0o755 or blob != binary:
        raise AssetError(f"archive member {member} does not match the binary")


def prepare(directory, dest, version, generation, relays):
    """Validate the eight binaries and write them plus their six archives."""
    if not SHA.fullmatch(generation):
        raise AssetError("admin-web generation is not the recorded gitlink")
    if not re.fullmatch(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)", version):
        raise AssetError("version is not numeric X.Y.Z")
    binaries = load_binaries(directory)
    digests = {name: hashlib.sha256(blob).hexdigest() for name, blob in binaries.items()}
    if len(set(digests.values())) != len(digests):
        raise AssetError("release binaries are not distinct builds")
    for server in SERVERS:
        check_server(server, binaries[server.name], version, generation, relays)
    check_tray(binaries[TRAY_NAME], version)
    check_installer(binaries[SETUP_NAME], version)
    dest = Path(dest)
    dest.mkdir(parents=True, exist_ok=True)
    written = []
    for name, blob in binaries.items():
        target = dest / name
        target.write_bytes(blob)
        written.append(target)
    for server in SERVERS:
        target = dest / server.archive
        blob = binaries[server.name]
        if server.package == "tar":
            _write_tar(target, server.member, blob)
        else:
            _write_zip(target, server.member, blob)
        assert_archive(server.package, target.read_bytes(), server.member, blob)
        written.append(target)
    return sorted(written, key=lambda item: item.name)


def checksum_text(paths):
    """Checksum every payload and SOURCE.txt. Never include SHA256SUMS."""
    lines = []
    names = [path.name for path in paths]
    if SUMS_NAME in names:
        raise AssetError("checksum input includes SHA256SUMS")
    if set(names) != set(PAYLOAD_NAMES + (SOURCE_NAME,)):
        raise AssetError("checksum input is not the distributed payload set")
    for path in sorted(paths, key=lambda item: item.name):
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        lines.append(f"{digest}  {path.name}")
    return "\n".join(lines) + "\n"


def _git(root, *args):
    result = subprocess.run(
        ["git", *args],
        cwd=root,
        text=True,
        capture_output=True,
        check=False,
    )
    if result.returncode:
        raise AssetError("git command failed")
    return result.stdout.strip()


def manifest_version(root):
    """Read the numeric version from this checkout's manifests."""
    root = Path(root)
    version = (root / ".version").read_text(encoding="utf-8").strip()
    if not re.fullmatch(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)", version):
        raise AssetError("manifest version is not numeric")
    try:
        cargo_version = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))[
            "workspace"
        ]["package"]["version"]
    except (OSError, tomllib.TOMLDecodeError, KeyError, TypeError) as exc:
        raise AssetError("Cargo.toml version does not match the manifest") from exc
    if cargo_version != version:
        raise AssetError("Cargo.toml version does not match the manifest")
    return version


def check_build_only(root, sha):
    """Validate one selected commit. This does not require or create a release tag."""
    root = Path(root)
    if not SHA.fullmatch(sha) or _git(root, "rev-parse", "HEAD") != sha:
        raise AssetError("checkout is not the selected commit")
    version = manifest_version(root)
    generation = check_source(root, version, sha)
    return version, generation


def check_source(root, version, sha):
    """Return the admin-web gitlink when manifests, lock, and checkout agree."""
    root = Path(root)
    if _git(root, "rev-parse", "HEAD") != sha or not SHA.fullmatch(sha):
        raise AssetError("checkout is not the unstable tag commit")
    if (root / ".version").read_text(encoding="utf-8").strip() != version:
        raise AssetError(".version does not match the unstable tag")
    try:
        cargo_version = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))[
            "workspace"
        ]["package"]["version"]
    except (OSError, tomllib.TOMLDecodeError, KeyError, TypeError) as exc:
        raise AssetError("Cargo.toml version does not match the unstable tag") from exc
    if cargo_version != version:
        raise AssetError("Cargo.toml version does not match the unstable tag")
    recorded = _git(root, "rev-parse", "HEAD:external/madmail-admin-web")
    if not SHA.fullmatch(recorded):
        raise AssetError("admin-web gitlink is not a commit")
    checked = _git(root, "-C", "external/madmail-admin-web", "rev-parse", "HEAD")
    if checked != recorded:
        raise AssetError("admin-web checkout does not match the recorded gitlink")
    if _git(root, "rev-parse", "HEAD:Cargo.lock") != _git(root, "hash-object", "Cargo.lock"):
        raise AssetError("Cargo.lock does not match the unstable tag commit")
    return recorded


def _download(url, *, limit=None, exact=None, opener=None):
    if opener is None:

        def opener(target):
            request = urllib.request.Request(
                target, headers={"User-Agent": "madmail-unstable-build"}
            )
            return urllib.request.urlopen(request, timeout=120)

    if exact is None and limit is None:
        raise AssetError("download has no size cap")
    cap = exact if exact is not None else limit
    try:
        with opener(url) as response:
            chunks = []
            total = 0
            while True:
                block = response.read(1024 * 1024)
                if not block:
                    break
                total += len(block)
                if total > cap:
                    raise AssetError("download exceeded the pinned size")
                chunks.append(block)
    except AssetError:
        raise
    except Exception as exc:
        raise AssetError("download failed") from exc
    data = b"".join(chunks)
    if exact is not None and len(data) != exact:
        raise AssetError("download size does not match the pin")
    return data


def _sha256(data):
    return hashlib.sha256(data).hexdigest()


def verify_relay_archive(triple, data, pins=None):
    pins = IROH_RELAYS if pins is None else pins
    pin = pins[triple]
    if len(data) != pin["archive_size"] or _sha256(data) != pin["archive_sha256"]:
        raise AssetError("iroh relay archive does not match the pinned hash")
    members = archive_members(pin["package"], data)
    files = [item for item in members if Path(item[0]).name == pin["member"]]
    if len(files) != 1:
        raise AssetError("iroh relay archive member is missing")
    blob = files[0][2]
    if _sha256(blob) != pin["binary_sha256"]:
        raise AssetError("iroh relay binary does not match the pinned hash")
    if pin["family"] == "elf":
        if elf_machine(blob) != pin["machine"]:
            raise AssetError("iroh relay ELF architecture does not match the pin")
    elif pe_machine(blob) != pin["machine"]:
        raise AssetError("iroh relay PE architecture does not match the pin")
    return blob


def fetch_relays(opener=None, pins=None):
    pins = IROH_RELAYS if pins is None else pins
    found = {}
    for triple, pin in pins.items():
        data = _download(pin["url"], exact=pin["archive_size"], opener=opener)
        found[triple] = verify_relay_archive(triple, data, pins)
    return found


def _safe_extract(archive, dest):
    root = dest.resolve()
    for member in archive.getmembers():
        if member.issym() or member.islnk():
            raise AssetError("tool archive contains a link")
        target = (root / member.name).resolve()
        if target != root and root not in target.parents:
            raise AssetError("tool archive escapes its destination")
    try:
        archive.extractall(dest, filter="data")
    except TypeError:
        archive.extractall(dest)


def extract_tool_tree(url, digest, size, dest, opener=None):
    """Extract a pinned tar.xz. Used for zig, which needs its lib directory."""
    data = _download(url, exact=size, opener=opener)
    if _sha256(data) != digest:
        raise AssetError("tool archive does not match the pinned hash")
    dest = Path(dest)
    if dest.exists():
        shutil.rmtree(dest)
    dest.mkdir(parents=True)
    try:
        with tarfile.open(fileobj=io.BytesIO(data), mode="r:xz") as archive:
            _safe_extract(archive, dest)
    except AssetError:
        raise
    except tarfile.TarError as exc:
        raise AssetError("tool archive is not tar.xz") from exc
    return dest


def install_archive(url, digest, size, dest, wanted_name, opener=None):
    data = _download(url, exact=size, opener=opener)
    if _sha256(data) != digest:
        raise AssetError("tool archive does not match the pinned hash")
    dest = Path(dest)
    dest.mkdir(parents=True, exist_ok=True)
    wanted = Path(wanted_name).name
    try:
        with tarfile.open(fileobj=io.BytesIO(data), mode="r:xz") as archive:
            matches = [
                member
                for member in archive.getmembers()
                if member.isfile() and Path(member.name).name == wanted
            ]
            if len(matches) != 1:
                raise AssetError("tool archive is missing its binary")
            extracted = archive.extractfile(matches[0])
            if extracted is None:
                raise AssetError("tool archive is missing its binary")
            target = dest / wanted
            target.write_bytes(extracted.read())
    except AssetError:
        raise
    except tarfile.TarError as exc:
        raise AssetError("tool archive is not tar.xz") from exc
    target.chmod(target.stat().st_mode | stat.S_IEXEC)
    return target


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)

    check = commands.add_parser("check-source")
    check.add_argument("--root", type=Path, default=Path.cwd())
    check.add_argument("--version", required=True)
    check.add_argument("--sha", required=True)

    fetch = commands.add_parser("fetch-relay")
    fetch.add_argument("--triple", required=True, choices=sorted(IROH_RELAYS))
    fetch.add_argument("--dest", type=Path, required=True)

    zig = commands.add_parser("install-zig")
    zig.add_argument("--dest", type=Path, required=True)

    czb = commands.add_parser("install-cargo-zigbuild")
    czb.add_argument("--dest", type=Path, required=True)

    verify = commands.add_parser("verify-server")
    verify.add_argument("--name", required=True, choices=sorted(SERVER_BY_NAME))
    verify.add_argument("--path", type=Path, required=True)
    verify.add_argument("--version", required=True)
    verify.add_argument("--generation", required=True)
    verify.add_argument("--relay", type=Path, required=True)

    tray = commands.add_parser("verify-tray")
    tray.add_argument("--path", type=Path, required=True)
    tray.add_argument("--version", required=True)

    installer = commands.add_parser("verify-installer")
    installer.add_argument("--path", type=Path, required=True)
    installer.add_argument("--version", required=True)

    args = parser.parse_args(argv)
    try:
        if args.command == "check-source":
            print(check_source(args.root, args.version, args.sha))
        elif args.command == "fetch-relay":
            relays = fetch_relays()
            args.dest.write_bytes(relays[args.triple])
        elif args.command == "install-zig":
            root = extract_tool_tree(ZIG_URL, ZIG_SHA256, ZIG_SIZE, args.dest)
            binary = root / ZIG_ROOT / "zig"
            if not binary.is_file():
                raise AssetError("zig archive is missing its binary")
            binary.chmod(binary.stat().st_mode | stat.S_IEXEC)
            print(binary)
        elif args.command == "install-cargo-zigbuild":
            binary = install_archive(
                CARGO_ZIGBUILD_URL,
                CARGO_ZIGBUILD_SHA256,
                CARGO_ZIGBUILD_SIZE,
                args.dest,
                CARGO_ZIGBUILD_MEMBER,
            )
            print(binary)
        elif args.command == "verify-server":
            server = SERVER_BY_NAME[args.name]
            relays = {server.relay: args.relay.read_bytes()}
            check_server(
                server,
                args.path.read_bytes(),
                args.version,
                args.generation,
                relays,
            )
        elif args.command == "verify-tray":
            check_tray(args.path.read_bytes(), args.version)
        elif args.command == "verify-installer":
            check_installer(args.path.read_bytes(), args.version)
        return 0
    except (AssetError, OSError, ValueError) as exc:
        message = str(exc) if isinstance(exc, AssetError) else type(exc).__name__
        print(f"unstable assets: {message}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
