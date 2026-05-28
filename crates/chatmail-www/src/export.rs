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

//! Export embedded www assets (Madmail `html-export`).

use std::path::Path;

use chatmail_types::{ChatmailError, Result};

use crate::assets::WwwAssets;

/// Write all embedded `www/` files to `dest_dir` (preserves relative paths).
pub fn export_www_files(dest_dir: &Path) -> Result<usize> {
    std::fs::create_dir_all(dest_dir).map_err(ChatmailError::from)?;
    let mut count = 0usize;
    for rel in WwwAssets::iter() {
        let rel = rel.as_ref();
        let data = WwwAssets::get(rel)
            .ok_or_else(|| ChatmailError::config(format!("missing embedded asset: {rel}")))?;
        let out = dest_dir.join(rel);
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(ChatmailError::from)?;
        }
        std::fs::write(&out, data.data.as_ref()).map_err(ChatmailError::from)?;
        println!("Exported: {rel}");
        count += 1;
    }
    Ok(count)
}
