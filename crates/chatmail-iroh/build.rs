// Copyright (C) 2026 themadorg
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Embed `iroh-relay` v0.35.0 for the Cargo target being compiled.
//!
//! `make init` downloads one host binary into `assets/iroh-relay`. Release
//! builds set `CHATMAIL_IROH_RELAY_BIN` to the pinned binary for the target
//! and `CHATMAIL_REQUIRE_IROH=1`. The ELF or PE machine field is checked in
//! process. A binary for another architecture is never embedded, and the
//! relay file is not executed here.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const VERSION: &str = "v0.35.0";
const ABSENT: &str = "madmail-iroh-relay:absent";

struct Expected {
    triple: &'static str,
    family: &'static str,
    machine: u16,
}

fn main() {
    println!("cargo::rustc-check-cfg=cfg(embed_iroh)");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/VERSION");
    println!("cargo:rerun-if-changed=assets/iroh-relay");
    println!("cargo:rerun-if-env-changed=CHATMAIL_IROH_RELAY_BIN");
    println!("cargo:rerun-if-env-changed=CHATMAIL_REQUIRE_IROH");

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let require = env::var("CHATMAIL_REQUIRE_IROH").ok().as_deref() == Some("1");
    let explicit = env::var_os("CHATMAIL_IROH_RELAY_BIN").map(PathBuf::from);
    let binary = match explicit.clone() {
        Some(path) => path,
        None => manifest_dir.join("assets").join("iroh-relay"),
    };
    if let Some(path) = explicit.as_ref() {
        println!("cargo:rerun-if-changed={}", path.display());
    }

    if !binary.is_file() {
        if require || explicit.is_some() {
            panic!(
                "iroh relay binary is missing at {}. Set CHATMAIL_IROH_RELAY_BIN to the pinned target relay.",
                binary.display()
            );
        }
        emit_marker(ABSENT);
        println!(
            "cargo:warning=iroh-relay binary missing in assets/; run `make init` to download v0.35.0"
        );
        return;
    }

    let Some(expected) = expected_relay() else {
        if require || explicit.is_some() {
            panic!(
                "no iroh relay mapping for {}-{}; refusing to embed",
                env::var("CARGO_CFG_TARGET_OS").unwrap_or_default(),
                env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default()
            );
        }
        emit_marker(ABSENT);
        return;
    };

    let bytes = fs::read(&binary).unwrap_or_else(|err| {
        panic!("read iroh relay {}: {err}", binary.display());
    });
    let (family, machine) = classify(&bytes).unwrap_or_else(|err| {
        panic!("iroh relay {}: {err}", binary.display());
    });
    if family != expected.family || machine != expected.machine {
        panic!(
            "iroh relay at {} is {family} machine {machine:#x}, not {} ({}/{:#x}). Refusing to embed a binary built for another platform.",
            binary.display(),
            expected.triple,
            expected.family,
            expected.machine
        );
    }
    let version = read_version(&manifest_dir.join("assets").join("VERSION"));
    if version != VERSION {
        panic!("iroh relay version {version} does not match {VERSION}");
    }

    let dest = out_dir.join("iroh-relay");
    fs::write(&dest, &bytes).expect("write iroh relay into OUT_DIR");
    fs::write(
        out_dir.join("iroh_embedded.rs"),
        format!(
            "pub const IROH_RELAY_BYTES: &[u8] = include_bytes!(r\"{dest}\");\n\
             pub const IROH_RELAY_VERSION: &str = \"{VERSION}\";\n",
            dest = dest.display(),
        ),
    )
    .expect("write iroh_embedded.rs");
    println!("cargo:rustc-cfg=embed_iroh");
    emit_marker(&format!("madmail-iroh-relay:{VERSION}:{}", expected.triple));
}

fn emit_marker(marker: &str) {
    println!("cargo:rustc-env=MADMAIL_IROH_RELAY_MARKER={marker}");
}

fn read_version(path: &Path) -> String {
    match fs::read_to_string(path) {
        Ok(text) if !text.trim().is_empty() => text.trim().to_string(),
        _ => VERSION.to_string(),
    }
}

fn expected_relay() -> Option<Expected> {
    let os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    match (os.as_str(), arch.as_str()) {
        ("linux", "x86_64") => Some(Expected {
            triple: "x86_64-unknown-linux-musl",
            family: "elf",
            machine: 62,
        }),
        ("linux", "aarch64") => Some(Expected {
            triple: "aarch64-unknown-linux-musl",
            family: "elf",
            machine: 183,
        }),
        ("windows", "x86_64") => Some(Expected {
            triple: "x86_64-pc-windows-msvc",
            family: "pe",
            machine: 0x8664,
        }),
        _ => None,
    }
}

fn classify(bytes: &[u8]) -> Result<(&'static str, u16), String> {
    if bytes.len() >= 64 && bytes.starts_with(b"\x7fELF") && bytes[4] == 2 && bytes[5] == 1 {
        let machine = u16::from_le_bytes([bytes[18], bytes[19]]);
        return Ok(("elf", machine));
    }
    if bytes.len() >= 0x40 && bytes.starts_with(b"MZ") {
        let pe = u32::from_le_bytes([bytes[0x3C], bytes[0x3D], bytes[0x3E], bytes[0x3F]]) as usize;
        if pe.saturating_add(6) <= bytes.len() && bytes.get(pe..pe + 4) == Some(b"PE\0\0") {
            let machine = u16::from_le_bytes([bytes[pe + 4], bytes[pe + 5]]);
            return Ok(("pe", machine));
        }
    }
    Err("not a little-endian ELF64 or PE file".to_string())
}
