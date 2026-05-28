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

use std::io::{self, Write};

use chatmail_types::Result;

pub fn confirm(prompt: &str, skip: bool) -> Result<bool> {
    if skip {
        return Ok(true);
    }
    eprint!("{prompt} [y/N] ");
    io::stdout().flush().ok();
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    Ok(matches!(
        line.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}

pub fn read_password_stdin() -> Result<String> {
    eprint!("Password: ");
    io::stdout().flush().ok();
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    let s = line.trim().to_string();
    if s.is_empty() {
        return Err(chatmail_types::ChatmailError::config(
            "password required (use --password or enter at prompt)",
        ));
    }
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confirm_skips_prompt_when_yes_flag() {
        assert!(confirm("Delete everything?", true).unwrap());
    }
}
