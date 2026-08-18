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

//! `madmail dkim` — print the outbound federation DKIM record to publish.

use chatmail_config::cli::DkimCommand;
use chatmail_config::Args;
use chatmail_delivery::dkim::{
    private_key_path, public_txt_path, signing_domain, DkimSigner, DKIM_SELECTOR,
};
use chatmail_types::{ChatmailError, Result};
use serde_json::json;

use super::context::CtlContext;
use super::output::CtlOut;

const IP_REASON: &str =
    "DKIM d= cannot be an IP literal; use a DNS mail domain, then publish default._domainkey";

pub async fn dkim(args: &Args, cmd: Option<&DkimCommand>) -> Result<()> {
    match cmd {
        None | Some(DkimCommand::Show) => show(args),
    }
}

fn show(args: &Args) -> Result<()> {
    let ctx = CtlContext::from_args(args)?;
    let out = CtlOut::from_args(args, "dkim show");

    let registration = ctx.config.effective_registration_domain(None);
    let selector = DKIM_SELECTOR;
    let private_path = private_key_path(&ctx.state_dir, selector);
    let txt_file = public_txt_path(&ctx.state_dir, selector);
    let dns_name = format!("{selector}._domainkey");

    let Some(domain) = signing_domain(&registration) else {
        let data = json!({
            "selector": selector,
            "domain": registration,
            "dns_name": dns_name,
            "dns_fqdn": serde_json::Value::Null,
            "private_key_path": private_path.display().to_string(),
            "txt_path": txt_file.display().to_string(),
            "txt": serde_json::Value::Null,
            "key_present": private_path.is_file(),
            "generated": false,
            "publishable": false,
            "reason": IP_REASON,
        });
        if out.is_json() {
            return out.emit(data);
        }
        out.blank();
        out.line("  DKIM (outbound federation)");
        out.blank();
        out.line(format!("  Selector:        {selector}"));
        out.line(format!(
            "  Signing domain:  {registration}  (not a DNS name — signing skipped)"
        ));
        out.line(format!("  Private key:     {}", private_path.display()));
        out.line(format!("  Public TXT file: {}", txt_file.display()));
        out.blank();
        out.line(format!("  {IP_REASON}"));
        out.blank();
        return Ok(());
    };

    let existed = private_path.is_file();
    let signer = DkimSigner::load_or_create(&ctx.state_dir, selector, &domain)
        .map_err(ChatmailError::config)?;
    let txt = signer
        .public_txt(&ctx.state_dir)
        .map_err(ChatmailError::config)?;
    let dns_fqdn = format!("{dns_name}.{}", signer.domain);

    let data = json!({
        "selector": selector,
        "domain": signer.domain,
        "dns_name": dns_name,
        "dns_fqdn": dns_fqdn,
        "private_key_path": private_path.display().to_string(),
        "txt_path": txt_file.display().to_string(),
        "txt": txt,
        "key_present": true,
        "generated": !existed,
        "publishable": true,
    });

    if out.is_json() {
        return out.emit(data);
    }

    out.blank();
    out.line("  DKIM (outbound federation)");
    out.blank();
    out.line(format!("  Selector:        {selector}"));
    out.line(format!("  Signing domain:  {}  (d=)", signer.domain));
    out.line(format!("  DNS name:        {dns_fqdn}"));
    out.line(format!("  Private key:     {}", private_path.display()));
    out.line(format!("  Public TXT file: {}", txt_file.display()));
    out.blank();
    out.line("  Publish this single-line TXT record:");
    out.blank();
    out.line(txt);
    out.blank();
    out.line("  cmdeploy filtermail still rejects signed mail until this record is live.");
    if !existed {
        out.line("  Key was created by this command (same as first outbound send).");
    }
    out.blank();
    Ok(())
}
