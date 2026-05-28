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

pub mod connection_stats;
pub mod server;
pub mod session;

pub use connection_stats::{snapshot as imap_connection_snapshot, snapshot_peers as imap_connection_peers};
pub use server::run_imap_listener;
pub use session::{
    capability_string, iroh_metadata_response, turn_metadata_response, ImapSession,
    ImapSessionConfig,
};
