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

//! Embed `iroh-relay` v0.35.0 (matches Delta Chat core and cmdeploy).
//! Binary is fetched by `make init`, not at compile time.

use std::env;
use std::fs;
use std::path::PathBuf;

const VERSION: &str = "v0.35.0";

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let assets = manifest_dir.join("assets");
    let binary = assets.join("iroh-relay");
    let version_file = assets.join("VERSION");

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/VERSION");
    println!("cargo:rerun-if-changed=assets/iroh-relay");

    if binary.exists() {
        let dest = out_dir.join("iroh-relay");
        fs::copy(&binary, &dest).expect("copy iroh-relay to OUT_DIR");
        println!("cargo:rustc-env=CHATMAIL_IROH_RELAY_PATH={}", dest.display());
        println!(
            "cargo:rustc-env=CHATMAIL_IROH_RELAY_VERSION={}",
            fs::read_to_string(&version_file)
                .unwrap_or_else(|_| VERSION.to_string())
                .trim()
        );
    } else {
        println!(
            "cargo:warning=iroh-relay binary missing in assets/; run `make init` to download v0.35.0"
        );
    }
}
