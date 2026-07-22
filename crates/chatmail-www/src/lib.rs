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
mod contact_sharing;
pub mod context_cache;
pub mod cors;
pub mod export;
pub mod gate;
mod go_template;
pub mod handlers;
pub mod response;
pub mod router;
pub mod template;
pub mod webimap;
pub mod webimap_ws;
mod www_facts;
pub mod www_migrate;

pub use export::export_www_files;
pub use go_template::{looks_like_go_template, prepare_template};
pub use router::{www_router, WwwState};
pub use www_migrate::{
    format_www_template_error, migrate_www_dir, migrate_www_html_file, rewrite_legacy_qr_js,
    scan_literal_brace_warnings, scan_www_dir_for_go_templates, transform_html_source,
    FileMigrateOutcome, MigrateReport,
};

#[cfg(test)]
mod tests;
