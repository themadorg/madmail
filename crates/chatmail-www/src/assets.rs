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

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, RwLock};

use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "www-src/"]
pub struct WwwAssets;

/// Load every embedded static file into RAM at boot (default site — no `www_dir`).
pub fn preload_embedded_www(cache: &RwLock<HashMap<String, Arc<[u8]>>>) {
    let mut guard = match cache.write() {
        Ok(g) => g,
        Err(_) => return,
    };
    for path in WwwAssets::iter() {
        let path = path.as_ref();
        if path.ends_with(".html") {
            continue;
        }
        if guard.contains_key(path) {
            continue;
        }
        if let Some(file) = WwwAssets::get(path) {
            guard.insert(path.to_string(), Arc::from(file.data.into_owned()));
        }
    }
}

/// Default site: bytes from the binary only (never touches disk).
pub fn embedded_asset_bytes(path: &str) -> Option<Arc<[u8]>> {
    WwwAssets::get(path).map(|f| Arc::from(f.data.into_owned()))
}

/// Operator override (`html-serve`): read from the exported directory.
pub fn external_asset_bytes(path: &str, www_dir: &Path) -> Option<Vec<u8>> {
    let file = www_dir.join(path);
    if file.is_file() {
        std::fs::read(file).ok()
    } else {
        None
    }
}

pub fn read_asset(path: &str) -> Option<rust_embed::EmbeddedFile> {
    WwwAssets::get(path)
}

/// Whether an HTML template exists (external `www_dir` or embedded).
pub fn www_html_exists(path: &str, www_dir: Option<&Path>) -> bool {
    if let Some(dir) = www_dir {
        if dir.join(path).is_file() {
            return true;
        }
    }
    WwwAssets::get(path).is_some()
}
