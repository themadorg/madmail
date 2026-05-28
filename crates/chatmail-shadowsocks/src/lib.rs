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

//! Shadowsocks proxy (Madmail `chatmail` endpoint parity): raw TCP relay, URLs, optional Xray WS/gRPC.

mod allowed_ports;
mod cipher;
mod runtime;
mod server;
mod urls;
mod xray;

pub use allowed_ports::build_allowed_ports;
pub use runtime::{
    resolve_runtime, resolve_runtime_from_settings, ss_runtime_enabled, ShadowsocksRuntime,
};
pub use server::{spawn_shadowsocks_server, ShadowsocksHandle};
pub use urls::ShadowsocksUrls;
