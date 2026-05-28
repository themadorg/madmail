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

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::install_cli::{CertificateCommand, InstallArgs};

/// Global CLI (Madmail-compatible subcommands; see `docs/TDD/14-cli-tools.md`).
#[derive(Debug, Parser, Clone)]
#[command(
    name = "chatmail",
    about = "Chatmail mail server (Madmail-compatible CLI)",
    long_about = "Composable Chatmail server. Use `run` to start SMTP/IMAP/HTTP.\n\
                  Operator tools mirror `madmail` / `maddy` (accounts, federation, admin-token, …).\n\
                  See context/madmail/docs/chatmail/commands.md for full reference."
)]
#[command(subcommand_required = false)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    #[command(flatten)]
    pub args: Args,
}

#[derive(Debug, Subcommand, Clone)]
pub enum Command {
    /// Start the mail server (default).
    Run,
    /// Replace this executable from a signed local file or URL.
    Upgrade {
        /// Path to signed binary, or `http://` / `https://` URL to download one.
        #[arg(value_name = "PATH_OR_URL")]
        path_or_url: String,
    },
    /// Download and install update from URL (alias for `upgrade`).
    Update {
        /// Path to signed binary, or `http://` / `https://` URL (same as `upgrade`).
        #[arg(value_name = "URL")]
        path_or_url: String,
    },
    /// Display the admin API credentials.
    #[command(name = "admin-token")]
    AdminToken {
        /// Print only the raw token (for `TOKEN=$(chatmail admin-token --raw)`).
        #[arg(long)]
        raw: bool,
        /// Do not print the login QR code.
        #[arg(long)]
        no_qr: bool,
    },
    /// Serve the embedded admin-web SPA.
    #[command(name = "admin-web")]
    AdminWeb {
        #[command(subcommand)]
        cmd: AdminWebCommand,
    },
    /// Print version and exit.
    Version,
    /// Account management (status, create, ban, …).
    #[command(subcommand)]
    Accounts(AccountsCommand),
    /// List blocklisted usernames (`accounts ban-list`).
    #[command(name = "ban-list")]
    BanList,
    /// Manage blocked users (prevent re-registration).
    #[command(subcommand)]
    Blocklist(BlocklistCommand),
    /// Create a random account; print JSON (`accounts create-random`).
    #[command(name = "create-user")]
    CreateUser {
        /// Print only JSON (for scripts).
        #[arg(long)]
        json_only: bool,
    },
    /// Fully delete a user account (credentials + mail + blocklist).
    Delete {
        /// Account email / username.
        username: String,
        /// Skip confirmation prompt.
        #[arg(short, long)]
        yes: bool,
        /// Blocklist reason stored in DB.
        #[arg(long, default_value = "account deleted via CLI")]
        reason: String,
    },
    /// Endpoint override cache management.
    #[command(name = "endpoint-cache", visible_aliases = ["dns-cache"], subcommand)]
    EndpointCache(EndpointCacheCommand),
    /// Pull-based email relay (exchanger) management.
    Exchanger,
    /// Federation policy and rules management.
    #[command(subcommand)]
    Federation(FederationCommand),
    /// Generate password hashes for pass_table.
    Hash,
    /// Export default HTML files to a directory.
    #[command(name = "html-export")]
    HtmlExport {
        #[arg(value_name = "DEST_DIR")]
        dest: PathBuf,
    },
    /// Serve HTML from an external directory (`embedded` to revert).
    #[command(name = "html-serve")]
    HtmlServe {
        #[arg(value_name = "WWW_DIR")]
        www_dir: String,
    },
    /// IMAP mailboxes (folders) management.
    #[command(name = "imap-mboxes")]
    ImapMboxes,
    /// IMAP messages management.
    #[command(name = "imap-msgs")]
    ImapMsgs,
    /// IMAP storage accounts management.
    #[command(name = "imap-acct")]
    ImapAcct,
    /// Install and configure the mail server.
    Install(InstallArgs),
    /// TLS certificates (Let's Encrypt via lers).
    Certificate {
        #[command(subcommand)]
        cmd: CertificateCommand,
    },
    /// View or change the website language.
    #[command(subcommand_required = false)]
    Language {
        #[command(subcommand)]
        command: Option<LanguageCommand>,
    },
    /// Open / close public account registration (`__REGISTRATION_OPEN__`).
    #[command(subcommand)]
    Registration(RegistrationCommand),
    /// Migrate submission PGP policy in config.
    #[command(name = "migrate-pgp-config")]
    MigratePgpConfig,
    /// Show server status (connections, users, uptime).
    Status {
        /// Per-port breakdown.
        #[arg(long, short = 'd')]
        details: bool,
    },
    /// Manage admin-panel ports.
    #[command(subcommand)]
    Port(PortCommand),
    /// View or set max message size (`appendlimit` / `max_message_size`).
    #[command(name = "message-size")]
    MessageSize {
        #[command(subcommand)]
        cmd: Option<MessageSizeCommand>,
    },
    /// Delivery queue management.
    Queue,
    /// Scheduled maintenance jobs (retention, unused accounts, purge).
    #[command(subcommand)]
    Tasks(TasksCommand),
    /// Manage registration tokens.
    #[command(
        name = "registration-tokens",
        visible_aliases = ["reg-tokens", "tokens"],
        subcommand
    )]
    RegistrationTokens(RegistrationTokensCommand),
    /// Apply DB config overrides and restart the service.
    Reload {
        /// Override admin API base URL (default: from config + settings DB).
        #[arg(long)]
        url: Option<String>,
        /// Skip TLS certificate verification (self-signed dev servers).
        #[arg(long)]
        insecure: bool,
    },
    /// DeltaChat contact sharing management.
    #[command(subcommand)]
    Sharing(SharingCommand),
    /// Manage SMTP submission access scope.
    #[command(name = "submission-access")]
    SubmissionAccess,
    /// Uninstall the mail server.
    Uninstall(UninstallArgs),
    /// Local credentials management.
    Creds,
    /// Enable, disable, or inspect WebIMAP HTTP API.
    #[command(subcommand)]
    Webimap(ServiceToggleCommand),
    /// Enable, disable, or inspect WebSMTP HTTP send API.
    #[command(subcommand)]
    Websmtp(ServiceToggleCommand),
}

/// `chatmail webimap` / `websmtp` — `__WEBIMAP_ENABLED__` / `__WEBSMTP_ENABLED__`.
#[derive(Debug, Subcommand, Clone)]
pub enum ServiceToggleCommand {
    /// Show whether the API is enabled.
    Status,
    /// Enable the API.
    Enable,
    /// Disable the API (HTTP 404).
    Disable,
}

/// `chatmail message-size` — `__APPENDLIMIT__` / `__MAX_MESSAGE_SIZE__`.
#[derive(Debug, Subcommand, Clone)]
pub enum MessageSizeCommand {
    /// Show effective limit and DB overrides.
    Status,
    /// Set both limits (e.g. `100M`, `1G`).
    Set {
        #[arg(value_name = "SIZE")]
        size: String,
    },
    /// Clear DB overrides (revert to config file / default).
    Reset,
}

/// `chatmail language` — `__LANGUAGE__` (en, fa, ru, es).
#[derive(Debug, Subcommand, Clone)]
pub enum LanguageCommand {
    /// Show current language (default subcommand).
    Status,
    /// Set language code.
    Set {
        #[arg(value_name = "LANG")]
        lang: String,
    },
    /// Remove DB override (use config default).
    Reset,
}

/// `chatmail tasks` — maintenance jobs (Madmail `imapsql` cleanup + queue purge).
#[derive(Debug, Subcommand, Clone)]
pub enum TasksCommand {
    /// List available maintenance jobs and config-driven schedule.
    List,
    /// Run one job now (`prune-old-messages`, `prune-unused-accounts`, …).
    Run {
        #[arg(value_name = "TASK")]
        task: String,
        /// Override retention (`24h`, `7d`, `720h`); required for `prune-unread-older` without config.
        #[arg(long)]
        retention: Option<String>,
    },
    /// Run all jobs enabled by `storage.imapsql` retention settings in config.
    RunAll,
}

/// `chatmail endpoint-cache` — outbound delivery DNS overrides.
#[derive(Debug, Subcommand, Clone)]
pub enum EndpointCacheCommand {
    /// List all endpoint override entries.
    List,
    /// Create or update an entry (`LOOKUP_KEY TARGET_HOST [COMMENT]`).
    Set {
        #[arg(value_name = "LOOKUP_KEY")]
        lookup_key: String,
        #[arg(value_name = "TARGET_HOST")]
        target_host: String,
        #[arg(value_name = "COMMENT")]
        comment: Option<String>,
    },
    /// Show one entry.
    Get {
        #[arg(value_name = "LOOKUP_KEY")]
        lookup_key: String,
    },
    /// Remove an entry.
    #[command(alias = "delete")]
    Remove {
        #[arg(value_name = "LOOKUP_KEY")]
        lookup_key: String,
    },
}

/// `chatmail port` — admin-panel listener ports and local/public mode.
#[derive(Debug, Subcommand, Clone)]
pub enum PortCommand {
    /// Show mode and value for all admin-panel ports.
    Status,
    #[command(subcommand, name = "smtp")]
    Smtp(PortServiceCommand),
    #[command(subcommand, name = "submission")]
    Submission(PortServiceCommand),
    #[command(subcommand, name = "submission-tls", alias = "submission_tls")]
    SubmissionTls(PortServiceCommand),
    #[command(subcommand, name = "imap")]
    Imap(PortServiceCommand),
    #[command(subcommand, name = "imap-tls", alias = "imap_tls")]
    ImapTls(PortServiceCommand),
    #[command(subcommand, name = "turn")]
    Turn(PortServiceCommand),
    #[command(subcommand, name = "sasl")]
    Sasl(PortServiceCommand),
    #[command(subcommand, name = "iroh")]
    Iroh(PortServiceCommand),
    #[command(subcommand, name = "shadowsocks", alias = "ss")]
    Shadowsocks(PortServiceCommand),
    #[command(subcommand, name = "http")]
    Http(PortServiceCommand),
    #[command(subcommand, name = "https")]
    Https(PortServiceCommand),
}

/// Per-service `port <name> …` subcommands.
#[derive(Debug, Subcommand, Clone)]
pub enum PortServiceCommand {
    /// Show current mode and value.
    Status,
    /// Set port number (`1-65535`).
    Set {
        #[arg(value_name = "PORT")]
        port: String,
    },
    /// Reset port to config/default.
    Reset,
    /// Listen on localhost only.
    Local,
    /// Listen on all interfaces.
    Public,
}

/// `chatmail federation` — policy and domain rules.
#[derive(Debug, Subcommand, Clone)]
pub enum FederationCommand {
    /// Set global federation posture (`accept` or `reject`).
    Policy {
        #[arg(value_name = "accept|reject")]
        policy: String,
    },
    /// Add domain to rules (blocklist when policy is ACCEPT).
    Block {
        #[arg(value_name = "DOMAIN")]
        domain: String,
    },
    /// Add domain to rules (allowlist when policy is REJECT).
    Allow {
        #[arg(value_name = "DOMAIN")]
        domain: String,
    },
    /// Remove a domain from the rules table.
    Remove {
        #[arg(value_name = "DOMAIN")]
        domain: String,
    },
    /// Remove all domain exceptions.
    Flush,
    /// Show current policy and all active rules.
    List,
    /// Show live federation traffic diagnostics from DB.
    Status,
    /// Add domain to silent-dismiss list (accept mail, do not deliver).
    Dismiss {
        #[arg(value_name = "DOMAIN")]
        domain: String,
    },
    /// Remove domain from silent-dismiss list.
    #[command(name = "undismiss")]
    Undismiss {
        #[arg(value_name = "DOMAIN")]
        domain: String,
    },
    /// List all silent-dismiss domains.
    #[command(name = "dismiss-list")]
    DismissList,
    /// Remove all silent-dismiss domains.
    #[command(name = "dismiss-flush")]
    DismissFlush,
}

/// `chatmail registration-tokens` — invite tokens for `/new`.
#[derive(Debug, Subcommand, Clone)]
pub enum RegistrationTokensCommand {
    /// Create a new registration token.
    Create {
        #[arg(long)]
        token: Option<String>,
        #[arg(long = "max-uses", default_value_t = 1)]
        max_uses: i32,
        #[arg(long, default_value = "")]
        comment: String,
        /// Expiration duration (e.g. `72h`, `168h`).
        #[arg(long)]
        expires: Option<String>,
    },
    /// List all registration tokens.
    List,
    /// Show detailed status for a specific token.
    Status {
        #[arg(value_name = "TOKEN")]
        token: String,
    },
    /// Delete a registration token.
    Delete {
        #[arg(value_name = "TOKEN")]
        token: String,
    },
}

/// `chatmail sharing` — Delta Chat contact share links (`sharing.db`).
#[derive(Debug, Subcommand, Clone)]
pub enum SharingCommand {
    /// List all contact share links.
    List,
    /// Create a new share link (`SLUG URL [NAME]`).
    Create {
        #[arg(value_name = "SLUG")]
        slug: String,
        #[arg(value_name = "URL")]
        url: String,
        #[arg(value_name = "NAME")]
        name: Option<String>,
    },
    /// Reserve a slug (link points to `reserved`).
    Reserve {
        #[arg(value_name = "SLUG")]
        slug: String,
    },
    /// Remove a share link.
    #[command(alias = "delete")]
    Remove {
        #[arg(value_name = "SLUG")]
        slug: String,
    },
    /// Edit an existing share link (`SLUG NEW_URL [NEW_NAME]`).
    Edit {
        #[arg(value_name = "SLUG")]
        slug: String,
        #[arg(value_name = "NEW_URL")]
        url: String,
        #[arg(value_name = "NEW_NAME")]
        name: Option<String>,
    },
}

/// `chatmail uninstall` flags (Madmail `ctl/uninstall.go`).
#[derive(Debug, Parser, Clone)]
pub struct UninstallArgs {
    /// Skip confirmation prompts.
    #[arg(long)]
    pub force: bool,
    /// Keep mail data, databases, and state directory.
    #[arg(long)]
    pub keep_data: bool,
    /// Keep service user and group accounts.
    #[arg(long)]
    pub keep_user: bool,
    /// Keep configuration files.
    #[arg(long)]
    pub keep_config: bool,
    /// Keep server binary.
    #[arg(long)]
    pub keep_binary: bool,
    /// Show what would be removed without removing anything.
    #[arg(long)]
    pub dry_run: bool,
    /// Uninstallation log file.
    #[arg(long, default_value = "/var/log/madmail-uninstall.log")]
    pub log_file: PathBuf,
}

/// `chatmail registration` — `__REGISTRATION_OPEN__`.
#[derive(Debug, Subcommand, Clone)]
pub enum RegistrationCommand {
    /// Allow `/new` registration when tokens/policy permit.
    Open,
    /// Block new registrations.
    Close,
    /// Show open/closed.
    Status,
}

/// `chatmail accounts` / `madmail accounts` (direct DB).
#[derive(Debug, Subcommand, Clone)]
pub enum AccountsCommand {
    /// Credentials and storage summary.
    Status,
    /// One account (credentials, quota, blocklist).
    Info {
        /// Email address.
        username: String,
    },
    /// Create login + maildir + quota row.
    Create {
        username: String,
        /// Password (prompted on stdin if omitted).
        #[arg(short, long)]
        password: Option<String>,
    },
    /// Random account; prints JSON credentials.
    #[command(name = "create-random")]
    CreateRandom {
        #[arg(long)]
        json_only: bool,
    },
    /// Remove credentials, mail, and blocklist the address.
    Delete {
        username: String,
        #[arg(short, long)]
        yes: bool,
    },
    /// Same as delete with moderation reason.
    Ban {
        username: String,
        /// Blocklist reason (default: banned via CLI).
        reason: Option<String>,
        #[arg(short, long)]
        yes: bool,
    },
    /// Remove blocklist entry only (does not restore mail/creds).
    Unban {
        username: String,
        #[arg(short, long)]
        yes: bool,
    },
    /// List blocklisted usernames.
    #[command(name = "ban-list")]
    BanList,
    /// Export usernames (and hashes) as JSON.
    Export {
        /// Write to file instead of stdout.
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Import accounts from JSON file.
    Import {
        /// Path to JSON array of `{username, password?, hash?}`.
        file: PathBuf,
    },
    /// Delete all user accounts (destructive).
    #[command(name = "delete-all")]
    DeleteAll {
        #[arg(short, long)]
        yes: bool,
    },
}

/// `chatmail blocklist` / `madmail blocklist`.
#[derive(Debug, Subcommand, Clone)]
pub enum BlocklistCommand {
    /// List all blocked users.
    List,
    /// Block a username from re-registration.
    Add {
        username: String,
        /// Optional reason (default: manually blocked via CLI).
        reason: Option<String>,
    },
    /// Unblock a username.
    Remove {
        username: String,
        #[arg(short, long)]
        yes: bool,
    },
}

#[derive(Debug, Subcommand, Clone)]
pub enum AdminWebCommand {
    /// Show admin web dashboard status.
    Status,
    /// Enable the admin web dashboard.
    Enable,
    /// Disable the admin web dashboard.
    Disable,
    /// Set or reset the admin web path.
    Path {
        #[arg(value_name = "PATH")]
        path: Option<String>,
        #[arg(long)]
        reset: bool,
    },
}

/// Server flags (`--config`, `--state-dir`). Logging: `log` / `debug` in config only.
#[derive(Debug, Parser, Clone)]
pub struct Args {
    /// Configuration file path (`CHATMAIL_CONFIG`). Defaults to `./data/chatmail.toml` when present.
    #[arg(
        long,
        env = "CHATMAIL_CONFIG",
        default_value = "/etc/madmail/madmail.conf",
        global = true
    )]
    pub config: PathBuf,

    /// Persistent state directory (`--libexec` is a Madmail/maddy alias for the same path).
    #[arg(
        long,
        alias = "libexec",
        env = "CHATMAIL_STATE_DIR",
        default_value = "/var/lib/madmail",
        global = true
    )]
    pub state_dir: PathBuf,

    /// Initialize and exit (used by integration tests; skips signal wait).
    #[arg(long, default_value_t = false, hide = true, global = true)]
    pub boot_once: bool,
}

impl Cli {
    pub fn is_server_mode(&self) -> bool {
        matches!(self.command, None | Some(Command::Run))
    }

    pub fn server_args(&self) -> &Args {
        &self.args
    }

    pub fn parse_normalized() -> Self {
        let mut cli = Self::parse();
        crate::paths::apply_cli_defaults(&mut cli.args);
        cli
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn upgrade_and_update_accept_url_or_path() {
        let cli = Cli::try_parse_from([
            "madmail",
            "upgrade",
            "https://relay.example/bin/madmail",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Upgrade { path_or_url }) if path_or_url == "https://relay.example/bin/madmail"
        ));

        let cli = Cli::try_parse_from(["madmail", "update", "/tmp/madmail-signed"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Update { path_or_url }) if path_or_url == "/tmp/madmail-signed"
        ));
    }

    #[test]
    fn madmail_systemd_argv_accepts_libexec_after_run() {
        let cli = Cli::try_parse_from([
            "madmail",
            "--config",
            "/etc/madmail/madmail.conf",
            "run",
            "--libexec",
            "/var/lib/madmail",
        ])
        .expect("madmail systemd ExecStart argv");
        assert_eq!(cli.args.config, PathBuf::from("/etc/madmail/madmail.conf"));
        assert_eq!(cli.args.state_dir, PathBuf::from("/var/lib/madmail"));
    }

    #[test]
    fn p1_ut01_cli_defaults_and_overrides() {
        let mut cli = Cli::try_parse_from(["chatmail"]).expect("parse defaults");
        crate::paths::apply_cli_defaults(&mut cli.args);
        assert_eq!(cli.args.config, crate::paths::detect_default_config_path());
        assert_eq!(cli.args.state_dir, crate::paths::detect_default_state_dir());
        assert!(cli.is_server_mode());

        let cli = Cli::try_parse_from([
            "chatmail",
            "--state-dir",
            "/tmp/custom-state",
        ])
        .expect("parse overrides");
        assert_eq!(cli.args.state_dir, PathBuf::from("/tmp/custom-state"));

        let cli = Cli::try_parse_from(["chatmail", "run"]).expect("run subcommand");
        assert!(matches!(cli.command, Some(Command::Run)));
    }

    #[test]
    fn admin_web_subcommands_parse() {
        let cli = Cli::try_parse_from(["chatmail", "admin-web", "status"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::AdminWeb {
                cmd: AdminWebCommand::Status
            })
        ));
    }

    #[test]
    fn accounts_subcommands_parse() {
        use super::AccountsCommand;

        let cli = Cli::try_parse_from(["chatmail", "accounts", "status"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Accounts(AccountsCommand::Status))
        ));

        let cli = Cli::try_parse_from([
            "chatmail",
            "accounts",
            "create",
            "u@example.org",
            "--password",
            "secret",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Accounts(AccountsCommand::Create {
                username,
                password: Some(pw),
            })) if username == "u@example.org" && pw == "secret"
        ));

        let cli = Cli::try_parse_from(["chatmail", "ban-list"]).unwrap();
        assert!(matches!(cli.command, Some(Command::BanList)));

        let cli = Cli::try_parse_from([
            "chatmail",
            "delete",
            "gone@example.org",
            "-y",
            "--reason",
            "cli",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Delete {
                username,
                yes: true,
                reason,
            }) if username == "gone@example.org" && reason == "cli"
        ));
    }

    #[test]
    fn blocklist_subcommands_parse() {
        use super::BlocklistCommand;

        let cli = Cli::try_parse_from(["chatmail", "blocklist", "list"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Blocklist(BlocklistCommand::List))
        ));

        let cli = Cli::try_parse_from([
            "chatmail",
            "blocklist",
            "add",
            "bad@x.org",
            "spam",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Blocklist(BlocklistCommand::Add {
                username,
                reason: Some(r),
            })) if username == "bad@x.org" && r == "spam"
        ));
    }
}
