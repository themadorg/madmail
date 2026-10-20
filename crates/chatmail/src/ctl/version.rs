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

use chatmail_config::Args;
use chatmail_types::Result;

use super::output::CtlOut;

/// Product name printed by `madmail version` (Madmail parity: `madmail-v2 2.4.0`).
pub const VERSION_PRODUCT: &str = "madmail-v2";

/// Contiguous package identity retained in the server binary. Not part of CLI text.
pub const BUILD_VERSION_MARKER: &str = concat!("madmail-build-version:", env!("CARGO_PKG_VERSION"));

/// Keep [`BUILD_VERSION_MARKER`] reachable so the linker retains those bytes.
pub fn retain_build_version_marker() {
    let marker = BUILD_VERSION_MARKER;
    if std::hint::black_box(marker.as_ptr()).is_null() {
        std::process::abort();
    }
}

/// Print package version (Madmail `maddy version`).
pub fn print_version(args: &Args) -> Result<()> {
    retain_build_version_marker();
    // Old `maddy update` (through 2.20.0) chmods the live path to 0700 then
    // runs `version` as root. Restore 0755 so systemd User= can exec (#147).
    crate::upgrade::heal_current_exe_permissions();
    let out = CtlOut::from_args(args, "version");
    let version = env!("CARGO_PKG_VERSION");
    if out.is_json() {
        out.emit(serde_json::json!({
            "name": VERSION_PRODUCT,
            "version": version,
        }))?;
    } else {
        println!("{VERSION_PRODUCT} {version}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_text_matches_madmail_v2_format() {
        let version = env!("CARGO_PKG_VERSION");
        assert_eq!(
            format!("{VERSION_PRODUCT} {version}"),
            format!("madmail-v2 {version}")
        );
    }

    #[test]
    fn build_version_marker_names_the_package_version() {
        assert_eq!(
            BUILD_VERSION_MARKER,
            concat!("madmail-build-version:", env!("CARGO_PKG_VERSION"))
        );
    }
}
