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

//! Operator CLI (Madmail `internal/cli/ctl` parity).

mod account_ops;
mod accounts;
mod admin_login_qr;
mod admin_token;
mod admin_url;
mod admin_web;
mod blocklist_cmd;
mod certificate;
mod context;
mod delete_cmd;
mod dispatch;
mod endpoint_cache;
mod federation;
mod html;
mod install;
mod language;
mod message_size;
mod port;
mod registration;
mod reload;
mod request_reload;
mod registration_tokens;
mod service_toggle;
mod sharing;
mod status_cmd;
mod tasks;
mod uninstall;
mod util;
mod version;

#[cfg(test)]
mod dispatch_tests;
#[cfg(test)]
mod ops_tests;
#[cfg(test)]
mod test_harness;

pub use admin_token::admin_token;
pub use dispatch::dispatch;
pub use version::print_version;
