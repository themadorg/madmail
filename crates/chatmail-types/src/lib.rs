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

pub mod domains;
pub mod error;

pub use domains::{
    address_domain, address_is_local, build_local_domains, domain_forms, is_ipv4_literal,
    validate_login_domain, wrap_ip_domain,
};
pub use error::{ChatmailError, Result, MESSAGE_FILE_TOO_BIG};
