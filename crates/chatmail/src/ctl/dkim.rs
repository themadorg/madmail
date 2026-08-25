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

//! `madmail dkim` — print or check the outbound federation DKIM record.

use chatmail_config::cli::DkimCommand;
use chatmail_config::Args;
use chatmail_delivery::dkim::{check_dns, publish_info, status_info};
use chatmail_types::{ChatmailError, Result};

use super::context::CtlContext;
use super::output::CtlOut;

pub async fn dkim(args: &Args, cmd: Option<&DkimCommand>) -> Result<()> {
    match cmd {
        None | Some(DkimCommand::Show) => show(args),
        Some(DkimCommand::Check) => check(args).await,
        Some(DkimCommand::Status) => status(args).await,
    }
}

fn show(args: &Args) -> Result<()> {
    let ctx = CtlContext::from_args(args)?;
    let out = CtlOut::from_args(args, "dkim show");

    let registration = ctx.config.effective_registration_domain(None);
    let data = publish_info(&ctx.state_dir, &registration).map_err(ChatmailError::config)?;

    if out.is_json() {
        return out.emit(data);
    }

    let selector = data["selector"].as_str().unwrap_or("default");
    let domain = data["domain"].as_str().unwrap_or(&registration);
    let publishable = data["publishable"].as_bool().unwrap_or(false);
    let generated = data["generated"].as_bool().unwrap_or(false);
    let private_path = data["private_key_path"].as_str().unwrap_or("-");
    let txt_file = data["txt_path"].as_str().unwrap_or("-");

    out.blank();
    out.line("  DKIM (outbound federation)");
    out.blank();
    out.line(format!("  Selector:        {selector}"));
    if publishable {
        out.line(format!("  Signing domain:  {domain}  (d=)"));
        if let Some(fqdn) = data["dns_fqdn"].as_str() {
            out.line(format!("  DNS name:        {fqdn}"));
        }
    } else {
        out.line(format!(
            "  Signing domain:  {domain}  (not a DNS name — signing skipped)"
        ));
    }
    out.line(format!("  Private key:     {private_path}"));
    out.line(format!("  Public TXT file: {txt_file}"));
    out.blank();
    if publishable {
        if let Some(txt) = data["txt"].as_str() {
            out.line("  Publish this single-line TXT record:");
            out.blank();
            out.line(txt);
            out.blank();
        }
        out.line("  cmdeploy filtermail still rejects signed mail until this record is live.");
        out.line("  After publishing: madmail dkim check");
        if generated {
            out.line("  Key was created by this command (same as first outbound send).");
        }
    } else if let Some(reason) = data["reason"].as_str() {
        out.line(format!("  {reason}"));
    }
    out.blank();
    Ok(())
}

async fn check(args: &Args) -> Result<()> {
    let ctx = CtlContext::from_args(args)?;
    let out = CtlOut::from_args(args, "dkim check");
    let registration = ctx.config.effective_registration_domain(None);
    let data = check_dns(&ctx.state_dir, &registration)
        .await
        .map_err(ChatmailError::config)?;

    let fqdn = data["dns_fqdn"].as_str().unwrap_or("-");
    let matched = data["matched"].as_bool().unwrap_or(false);
    let checked = data["checked"].as_bool().unwrap_or(false);
    let lookup_failed = data.get("lookup_error").is_some();
    let json_fail = checked && (!matched || lookup_failed);

    if out.is_json() {
        out.emit(&data)?;
        if json_fail {
            // Payload already on stdout; skip the ok:false stderr envelope.
            std::process::exit(1);
        }
        return Ok(());
    }

    out.blank();
    out.line("  DKIM DNS check");
    out.blank();
    out.line(format!(
        "  Selector:        {}",
        data["selector"].as_str().unwrap_or("default")
    ));
    out.line(format!(
        "  Signing domain:  {}",
        data["domain"].as_str().unwrap_or(&registration)
    ));
    out.line(format!("  DNS name:        {fqdn}"));
    out.blank();
    if !checked {
        if let Some(reason) = data["reason"].as_str() {
            out.line(format!("  Skipped: {reason}"));
        } else {
            out.line("  Skipped: mail domain is not a DNS name.");
        }
        out.blank();
        return Ok(());
    }
    if let Some(err) = data["lookup_error"].as_str() {
        out.line(format!("  Lookup failed: {err}"));
        out.blank();
        return Err(ChatmailError::config(format!(
            "DKIM DNS lookup failed: {err}"
        )));
    }
    let found = data["dns_txt"].as_array().map(|a| a.len()).unwrap_or(0);
    if matched {
        out.line(format!(
            "  Result:          OK — published TXT matches ({found} record(s))"
        ));
        out.blank();
        return Ok(());
    }
    out.line("  Result:          FAIL — DNS TXT does not match the local key");
    if found == 0 {
        out.line("  No TXT at this name. Publish the value from: madmail dkim show");
    } else {
        out.line("  Found TXT:");
        if let Some(arr) = data["dns_txt"].as_array() {
            for v in arr {
                if let Some(s) = v.as_str() {
                    out.line(format!("    {s}"));
                }
            }
        }
        out.line("  Expected (madmail dkim show):");
        if let Some(exp) = data["expected_txt"].as_str() {
            out.line(format!("    {exp}"));
        }
    }
    out.blank();
    Err(ChatmailError::config(format!(
        "DKIM TXT at {fqdn} does not match the local key"
    )))
}

async fn status(args: &Args) -> Result<()> {
    let ctx = CtlContext::from_args(args)?;
    let out = CtlOut::from_args(args, "dkim status");
    let registration = ctx.config.effective_registration_domain(None);
    let data = status_info(&ctx.state_dir, &registration)
        .await
        .map_err(ChatmailError::config)?;

    if out.is_json() {
        return out.emit(data);
    }

    let domain = data["domain"].as_str().unwrap_or(&registration);
    let key = data["key_present"].as_bool().unwrap_or(false);
    let publishable = data["publishable"].as_bool().unwrap_or(false);
    let dns_checked = data["dns_checked"].as_bool().unwrap_or(false);
    let dns_matched = data["dns_matched"].as_bool().unwrap_or(false);

    out.blank();
    out.line("  DKIM status (outbound federation)");
    out.blank();
    out.line(format!(
        "  Selector:        {}",
        data["selector"].as_str().unwrap_or("default")
    ));
    out.line(format!("  Signing domain:  {domain}"));
    if let Some(fqdn) = data["dns_fqdn"].as_str() {
        out.line(format!("  DNS name:        {fqdn}"));
    }
    out.line(format!(
        "  Local key:       {}",
        if key { "present" } else { "missing" }
    ));
    let dns_line = if !dns_checked {
        "skipped"
    } else if dns_matched {
        "OK (TXT matches)"
    } else if data.get("lookup_error").is_some() {
        "lookup failed"
    } else {
        "not published / mismatch"
    };
    out.line(format!("  DNS:             {dns_line}"));
    out.blank();
    if !publishable {
        if let Some(reason) = data["reason"].as_str() {
            out.line(format!("  {reason}"));
        }
        out.blank();
    } else if !dns_matched {
        out.line("  Publish with madmail dkim show, then madmail dkim check.");
        out.blank();
    }
    Ok(())
}
