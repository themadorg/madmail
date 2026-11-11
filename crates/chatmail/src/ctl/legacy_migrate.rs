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

//! `madmail legacy-migrate` — one-time switch off a legacy `maddy` executable.

use chatmail_config::Args;
use chatmail_types::Result;
use serde_json::json;

use super::output::CtlOut;

pub fn legacy_migrate(args: &Args) -> Result<()> {
    let out = CtlOut::from_args(args, "legacy-migrate");
    let report = crate::legacy_install::bootstrap()?;
    let running: Vec<_> = report
        .running
        .iter()
        .map(|(service, exe)| json!({ "service": service, "exe": exe }))
        .collect();
    let data = json!({
        "changed": report.changed,
        "already_canonical": report.already_canonical,
        "stable": report.stable,
        "services": report.services,
        "retargeted": report.retargeted,
        "running": running,
        "backups": report.backups,
    });
    let human = if report.changed {
        let mut text = format!("Legacy maddy paths now use {}.", report.stable);
        if !report.services.is_empty() {
            text.push_str(&format!("\nServices: {}.", report.services.join(", ")));
        }
        if !report.retargeted.is_empty() {
            text.push_str(&format!(
                "\nCommands repointed: {}.",
                report.retargeted.join(", ")
            ));
        }
        for (service, exe) in &report.running {
            text.push_str(&format!("\n{service} is running {exe}."));
        }
        if !report.backups.is_empty() {
            text.push_str(&format!(
                "\nPrevious executable saved at {}.",
                report.backups.join(", ")
            ));
        }
        text
    } else if report.already_canonical {
        let mut text = format!(
            "The maddy command and service already use {}.",
            report.stable
        );
        for (service, exe) in &report.running {
            text.push_str(&format!("\n{service} is running {exe}."));
        }
        if report.services.is_empty() {
            text.push_str("\nNo madmail systemd unit was found.");
        }
        text
    } else {
        format!(
            "No legacy maddy layout needed a change at {}.",
            report.stable
        )
    };
    let message = if report.changed {
        "Legacy maddy service now runs the active madmail binary"
    } else {
        "Legacy maddy layout already uses the active madmail binary"
    };
    out.done_msg(human, data, message)
}
