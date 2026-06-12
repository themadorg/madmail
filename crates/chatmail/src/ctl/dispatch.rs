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

use chatmail_config::{Cli, Command};
use chatmail_types::{ChatmailError, Result};

use chatmail_db::settings_keys;

use super::{
    accounts, admin_token, admin_web, blocklist_cmd, certificate, delete_cmd, docs, endpoint_cache,
    federation, html, install, language, message_size, port, push, registration,
    registration_tokens, reload, service_toggle, sharing, status_cmd, tasks, uninstall, version,
};

pub async fn dispatch(cli: &Cli) -> Result<()> {
    match &cli.command {
        None | Some(Command::Run) => Err(ChatmailError::config(
            "internal error: dispatch called for server run",
        )),
        Some(Command::Upgrade { path_or_url }) | Some(Command::Update { path_or_url }) => {
            crate::upgrade::upgrade_command(path_or_url, cli.args.json)
        }
        Some(Command::AdminToken { raw, no_qr }) => {
            admin_token::admin_token(&cli.args, *raw, *no_qr).await
        }
        Some(Command::AdminWeb { cmd }) => admin_web::admin_web(&cli.args, cmd).await,
        Some(Command::Version) => version::print_version(&cli.args),
        Some(Command::Install(args)) => install::install(&cli.args, args).await,
        Some(Command::Certificate { cmd }) => certificate::certificate(&cli.args, cmd).await,
        Some(Command::Accounts(cmd)) => accounts::accounts(&cli.args, cmd).await,
        Some(Command::BanList) => accounts::ban_list(&cli.args).await,
        Some(Command::Blocklist(cmd)) => blocklist_cmd::blocklist(&cli.args, cmd).await,
        Some(Command::CreateUser { json_only }) => {
            accounts::create_user(&cli.args, *json_only).await
        }
        Some(Command::Delete {
            username,
            yes,
            reason,
        }) => delete_cmd::delete(&cli.args, username, *yes, reason).await,
        Some(Command::HtmlExport { dest }) => html::html_export(&cli.args, dest).await,
        Some(Command::HtmlServe { www_dir }) => html::html_serve(&cli.args, www_dir).await,
        Some(Command::Language { command }) => {
            language::language(&cli.args, command.as_ref()).await
        }
        Some(Command::MessageSize { cmd }) => {
            message_size::message_size(&cli.args, cmd.as_ref()).await
        }
        Some(Command::Registration(cmd)) => registration::registration(&cli.args, cmd).await,
        Some(Command::Webimap(cmd)) => {
            service_toggle::run(
                &cli.args,
                settings_keys::WEBIMAP_ENABLED,
                "WebIMAP HTTP API",
                cmd,
            )
            .await
        }
        Some(Command::Websmtp(cmd)) => {
            service_toggle::run(
                &cli.args,
                settings_keys::WEBSMTP_ENABLED,
                "WebSMTP HTTP send API",
                cmd,
            )
            .await
        }
        Some(Command::Push(cmd)) => push::push(&cli.args, cmd).await,
        Some(Command::Federation(cmd)) => federation::federation(&cli.args, cmd).await,
        Some(Command::RegistrationTokens(cmd)) => {
            registration_tokens::registration_tokens(&cli.args, cmd).await
        }
        Some(Command::Sharing(cmd)) => sharing::sharing(&cli.args, cmd).await,
        Some(Command::Status { details }) => status_cmd::status(&cli.args, *details).await,
        Some(Command::Uninstall(flags)) => uninstall::uninstall(&cli.args, flags).await,
        Some(Command::EndpointCache(cmd)) => endpoint_cache::endpoint_cache(&cli.args, cmd).await,
        Some(Command::Port(cmd)) => port::port(&cli.args, cmd).await,
        Some(Command::Reload { url, insecure }) => {
            reload::reload(&cli.args, url.as_deref(), *insecure).await
        }
        Some(Command::Tasks(cmd)) => tasks::tasks(&cli.args, cmd).await,
        Some(Command::Completion(shell)) => docs::print_completion(shell),
        Some(Command::GenerateMan) => docs::print_generate_man(&cli.args),
        Some(Command::GenerateFishCompletion) => docs::print_generate_fish_completion(&cli.args),
        Some(cmd) => not_implemented(cmd),
    }
}

fn not_implemented(cmd: &Command) -> Result<()> {
    let name = command_name(cmd);
    Err(ChatmailError::config(format!(
        "'madmail {name}' is not implemented in madmail-v2 yet.\n\
         See docs/TDD/14-cli-tools.md and context/madmail/docs/chatmail/commands.md.\n\
         Implemented: run, upgrade, update, version, admin-token, admin-web, install, certificate, \
         accounts, ban-list, blocklist, create-user, delete, registration, language, \
         html-export, html-serve, webimap, websmtp, push, federation, registration-tokens, sharing, \
         status, uninstall, endpoint-cache, port, reload, message-size, tasks, completion"
    )))
}

fn command_name(cmd: &Command) -> &'static str {
    match cmd {
        Command::Run => "run",
        Command::Upgrade { .. } => "upgrade",
        Command::Update { .. } => "update",
        Command::AdminToken { .. } => "admin-token",
        Command::AdminWeb { .. } => "admin-web",
        Command::Version => "version",
        Command::Accounts { .. } => "accounts",
        Command::BanList => "ban-list",
        Command::Blocklist { .. } => "blocklist",
        Command::CreateUser { .. } => "create-user",
        Command::Delete { .. } => "delete",
        Command::EndpointCache(_) => "endpoint-cache",
        Command::Exchanger => "exchanger",
        Command::Federation { .. } => "federation",
        Command::Hash => "hash",
        Command::HtmlExport { .. } => "html-export",
        Command::HtmlServe { .. } => "html-serve",
        Command::ImapMboxes => "imap-mboxes",
        Command::ImapMsgs => "imap-msgs",
        Command::ImapAcct => "imap-acct",
        Command::Install { .. } => "install",
        Command::Certificate { cmd: _ } => "certificate",
        Command::Language { .. } => "language",
        Command::Registration { .. } => "registration",
        Command::MigratePgpConfig => "migrate-pgp-config",
        Command::Status { .. } => "status",
        Command::Port(_) => "port",
        Command::Queue => "queue",
        Command::RegistrationTokens { .. } => "registration-tokens",
        Command::Reload { .. } => "reload",
        Command::Sharing { .. } => "sharing",
        Command::SubmissionAccess => "submission-access",
        Command::Uninstall { .. } => "uninstall",
        Command::Creds => "creds",
        Command::Webimap { .. } => "webimap",
        Command::Websmtp { .. } => "websmtp",
        Command::Push { .. } => "push",
        Command::MessageSize { .. } => "message-size",
        Command::Tasks { .. } => "tasks",
        Command::Completion { .. } => "completion",
        Command::GenerateMan => "generate-man",
        Command::GenerateFishCompletion => "generate-fish-completion",
    }
}
