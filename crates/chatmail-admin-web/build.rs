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

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let embed_dst = manifest.join("embed");

    println!("cargo:rerun-if-changed=build.rs");

    if let Ok(src) = std::env::var("CHATMAIL_ADMIN_WEB_BUILD") {
        let src = PathBuf::from(&src);
        if src.join("index.html").is_file() {
            println!("cargo:rerun-if-changed={}", src.display());
            if embed_dst.exists() {
                fs::remove_dir_all(&embed_dst).ok();
            }
            copy_dir_all(&src, &embed_dst).expect("copy admin-web build into embed/");
            return;
        }
        println!(
            "cargo:warning=CHATMAIL_ADMIN_WEB_BUILD={src:?} has no index.html; falling back to embed/ or placeholder"
        );
    }

    if embed_dst.join("index.html").is_file() {
        println!("cargo:rerun-if-changed={}", embed_dst.join("index.html").display());
        return;
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
