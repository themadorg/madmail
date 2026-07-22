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

use chatmail::boot;
use chatmail::ctl::{self, print_error_json};
use chatmail_config::{Cli, Command};

fn main() {
    #[cfg(windows)]
    {
        if chatmail::ctl::argv_has_service_flag() {
            if let Err(e) = chatmail::service_host::run_service_dispatcher() {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
            return;
        }
    }

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    rt.block_on(async_main());
}

async fn async_main() {
    let cli = Cli::parse_normalized();
    let json = cli.args.json;
    let result = match cli.command {
        None | Some(Command::Run) => boot::run(cli.args).await,
        _ => ctl::dispatch(&cli).await,
    };
    if let Err(e) = result {
        if json {
            print_error_json(&e.to_string());
        } else {
            eprintln!("Error: {e}");
        }
        std::process::exit(1);
    }
}
