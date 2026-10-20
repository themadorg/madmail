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

use std::fs;
use std::path::{Path, PathBuf};

const PLACEHOLDER_MARK: &str = "Admin Web UI Not Available";

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let embed_dst = manifest.join("embed");
    let require = std::env::var("CHATMAIL_REQUIRE_ADMIN_WEB").ok().as_deref() == Some("1");
    let generation = std::env::var("CHATMAIL_ADMIN_WEB_GENERATION").unwrap_or_default();

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=CHATMAIL_ADMIN_WEB_BUILD");
    println!("cargo:rerun-if-env-changed=CHATMAIL_ADMIN_WEB_GENERATION");
    println!("cargo:rerun-if-env-changed=CHATMAIL_REQUIRE_ADMIN_WEB");

    let mut embedded = false;
    if let Ok(src) = std::env::var("CHATMAIL_ADMIN_WEB_BUILD") {
        let src = PathBuf::from(&src);
        if src.join("index.html").is_file() {
            println!("cargo:rerun-if-changed={}", src.display());
            if embed_dst.exists() {
                fs::remove_dir_all(&embed_dst).ok();
            }
            copy_dir_all(&src, &embed_dst).expect("copy admin-web build into embed/");
            embedded = true;
        } else if require {
            panic!(
                "CHATMAIL_ADMIN_WEB_BUILD={} has no index.html",
                src.display()
            );
        } else {
            println!(
                "cargo:warning=CHATMAIL_ADMIN_WEB_BUILD={src:?} has no index.html; falling back to embed/ or placeholder"
            );
        }
    }

    if !embedded && embed_dst.join("index.html").is_file() {
        println!(
            "cargo:rerun-if-changed={}",
            embed_dst.join("index.html").display()
        );
        embedded = true;
    }

    if !embedded {
        if require {
            panic!(
                "admin web embed is missing; set CHATMAIL_ADMIN_WEB_BUILD to the recorded UI build"
            );
        }
        println!(
            "cargo:warning=admin-web embed/ missing; set CHATMAIL_ADMIN_WEB_BUILD to a SvelteKit build dir or populate embed/ — UI will return 503"
        );
        fs::create_dir_all(&embed_dst).ok();
        fs::write(
            embed_dst.join("index.html"),
            "<!doctype html><html><body><h1>Admin Web UI Not Available</h1>\
             <p>Populate <code>embed/</code> or set <code>CHATMAIL_ADMIN_WEB_BUILD</code> then rebuild chatmail.</p></body></html>",
        )
        .expect("placeholder index.html");
    }

    if require {
        let index = embed_dst.join("index.html");
        let text = fs::read_to_string(&index).unwrap_or_default();
        if text.contains(PLACEHOLDER_MARK) || text.trim().is_empty() {
            panic!("admin web index is missing or is the placeholder");
        }
    }
    emit_generation(&generation, require);
}

fn emit_generation(raw: &str, require: bool) {
    let generation = raw.trim();
    if require {
        let lowercase_hex = generation.len() == 40
            && generation
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase());
        if !lowercase_hex {
            panic!("CHATMAIL_ADMIN_WEB_GENERATION must be the lowercase 40-hex admin-web gitlink");
        }
    }
    let value = if generation.is_empty() {
        "none"
    } else {
        generation
    };
    if value.bytes().any(|byte| byte == b'\n' || byte == b'\r') {
        panic!("admin-web generation is not a single token");
    }
    println!("cargo:rustc-env=MADMAIL_ADMIN_WEB_GENERATION={value}");
}

fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &to)?;
        } else if ty.is_file() {
            fs::copy(entry.path(), to)?;
        }
    }
    Ok(())
}
