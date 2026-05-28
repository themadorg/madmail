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

pub mod assets;
pub mod context_cache;
mod www_facts;
pub mod export;
pub mod gate;
pub mod handlers;
pub mod response;
pub mod router;
pub mod template;
pub mod webimap;
pub mod webimap_ws;

pub use export::export_www_files;
pub use router::{www_router, WwwState};

#[cfg(test)]
mod tests;
