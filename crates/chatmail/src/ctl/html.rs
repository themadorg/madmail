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

//! `chatmail html-export` / `html-serve` (Madmail `ctl/html.go`).

use std::path::Path;

use chatmail_config::update_config_www_dir;
use chatmail_config::Args;
use chatmail_types::{ChatmailError, Result};
use chatmail_www::export_www_files;

use super::context::CtlContext;

pub async fn html_export(args: &Args, dest: &Path) -> Result<()> {
    let _ = CtlContext::from_args(args)?;
    let n = export_www_files(dest)?;
    println!("Successfully exported {n} files to {}", dest.display());
    Ok(())
}

pub async fn html_serve(args: &Args, www_dir: &str) -> Result<()> {
    let _ctx = CtlContext::from_args(args)?;

    let embedded = matches!(
        www_dir.trim().to_ascii_lowercase().as_str(),
        "embedded" | "embed" | "internal"
    );

    let www_path = if embedded {
        None
    } else {
        let p = Path::new(www_dir);
        if !p.is_dir() {
            return Err(ChatmailError::config(format!(
                "directory not found: {}",
                p.display()
            )));
        }
        Some(p.canonicalize().unwrap_or_else(|_| p.to_path_buf()))
    };

    if !args.config.is_file() {
        return Err(ChatmailError::config(format!(
            "config file not found: {} — pass --config",
            args.config.display()
        )));
    }

    update_config_www_dir(&args.config, www_path.as_deref())?;

    if embedded {
        println!(
            "Successfully updated {} to use embedded HTML files.",
            args.config.display()
        );
    } else {
        let p = www_path.as_ref().unwrap();
        println!(
            "Successfully updated {} to serve HTML from {}",
            args.config.display(),
            p.display()
        );
        println!();
        println!("Ensure the chatmail service user can read this directory.");
        println!("Example: sudo chown -R chatmail:chatmail {}", p.display());
    }
    println!();
    println!("Restart chatmail to apply: sudo systemctl restart madmail");
    Ok(())
}
