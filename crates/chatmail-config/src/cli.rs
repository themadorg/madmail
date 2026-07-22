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
    name = "madmail",
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
        /// Path to signed binary, or `http://` / `https://` URL to a raw binary or `.tar.gz` / `.tgz` archive.
        #[arg(value_name = "PATH_OR_URL")]
        path_or_url: String,
        /// Allow HTTPS downloads with self-signed or otherwise untrusted TLS certificates.
        ///
        /// Does **not** skip Ed25519 signature verification — unsigned or bad-signed binaries
        /// are always rejected. Without this flag, certificate verification is enforced; on
        /// failure an interactive TTY may prompt `[y/N]`.
        #[arg(long = "accept-unsafe-https")]
        accept_unsafe_https: bool,
    },
    /// Replace this executable from a signed local file or URL (alias for `upgrade`).
    Update {
        /// Path to signed binary, or `http://` / `https://` URL to a raw binary or `.tar.gz` / `.tgz` archive.
        #[arg(value_name = "PATH_OR_URL")]
        path_or_url: String,
        /// Allow HTTPS downloads with self-signed or otherwise untrusted TLS certificates.
        ///
        /// Does **not** skip Ed25519 signature verification — unsigned or bad-signed binaries
        /// are always rejected. Without this flag, certificate verification is enforced; on
        /// failure an interactive TTY may prompt `[y/N]`.
        #[arg(long = "accept-unsafe-https")]
        accept_unsafe_https: bool,
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
    /// Convert custom `www_dir` Go templates to Minijinja (interactive).
    #[command(name = "html-migrate")]
    HtmlMigrate {
        /// Apply migration without prompting (default: ask when Go-style HTML is found).
        #[arg(long, short = 'y')]
        yes: bool,
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
    Install(Box<InstallArgs>),
    /// TLS certificates (Let's Encrypt / file / self-signed).
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
    /// Windows service management (`install`, `start`, `stop`, `status`, `uninstall`).
    ///
    /// On non-Windows platforms these subcommands return a clear error.
    #[command(subcommand)]
    Service(ServiceCommand),
    /// Windows Firewall rules for Madmail listeners (`apply`, `remove`).
    ///
    /// On non-Windows platforms these subcommands return a clear error.
    #[command(subcommand)]
    Firewall(FirewallCommand),
    /// Local credentials management.
    Creds,
    /// Enable, disable, or inspect WebIMAP HTTP API.
    #[command(subcommand)]
    Webimap(ServiceToggleCommand),
    /// Enable, disable, or inspect WebSMTP HTTP send API.
    #[command(subcommand)]
    Websmtp(ServiceToggleCommand),
    /// Browser CORS origins for WebIMAP, WebSMTP, and `/new` (`__WEBMAIL_CORS_ORIGINS__`).
    #[command(
        name = "webmail-cors",
        visible_aliases = ["webmail-dev"],
        subcommand_required = false
    )]
    WebmailCors {
        #[command(subcommand)]
        cmd: Option<WebmailCorsCommand>,
    },
    /// Delta Chat push notifications (`auto` / `on` / `off`).
    #[command(subcommand)]
    Push(PushCommand),
    /// Manage Shadowsocks circumvention proxy (enable, cipher, password, client URL).
    #[command(subcommand_required = false, visible_aliases = ["pr"])]
    Proxy {
        #[command(subcommand)]
        cmd: Option<ProxyCommand>,
    },
    /// Print shell tab-completion scripts (`bash`, `zsh`, `fish`).
    #[command(subcommand)]
    Completion(CompletionShell),
    /// Emit roff man page for the CLI (Madmail hidden helper).
    #[command(name = "generate-man", hide = true)]
    GenerateMan,
    /// Emit fish completion script (Madmail hidden helper).
    #[command(name = "generate-fish-completion", hide = true)]
    GenerateFishCompletion,
}

/// `madmail completion` — shell tab completion (clap_complete).
#[derive(Debug, Subcommand, Clone)]
pub enum CompletionShell {
    /// Bash completion script for `/usr/share/bash-completion/completions/<binary>`.
    Bash,
    /// Zsh completion script for `/usr/share/zsh/site-functions/_<binary>`.
    Zsh,
    /// Fish completion script for `/usr/share/fish/vendor_completions.d/<binary>.fish`.
    Fish,
}

/// `madmail proxy` — Shadowsocks (`__SS_*__`).
#[derive(Debug, Subcommand, Clone)]
pub enum ProxyCommand {
    /// Show Shadowsocks configuration and client URL.
    Status,
    /// Enable Shadowsocks listener (requires `ss_addr` + `ss_password` in config).
    Enable,
    /// Disable Shadowsocks listener.
    Disable,
    /// View or change cipher (`__SS_CIPHER__`).
    #[command(subcommand_required = false)]
    Cipher {
        #[command(subcommand)]
        cmd: Option<ProxySettingCommand>,
    },
    /// View or change password (`__SS_PASSWORD__`).
    #[command(subcommand_required = false)]
    Password {
        #[command(subcommand)]
        cmd: Option<ProxySettingCommand>,
    },
}

/// `madmail proxy cipher` / `proxy password` — DB override set/reset.
#[derive(Debug, Subcommand, Clone)]
pub enum ProxySettingCommand {
    /// Show effective value and DB override.
    Status,
    /// Set DB override.
    Set {
        #[arg(value_name = "VALUE")]
        value: String,
    },
    /// Clear DB override (revert to config file).
    Reset,
}

/// `madmail push` — `__PUSH_MODE__` (`auto` disables after repeated proxy failures).
#[derive(Debug, Subcommand, Clone)]
pub enum PushCommand {
    /// Show mode, runtime status, and failure counters.
    Status,
    /// Auto mode (default): enabled until 5 consecutive notification failures.
    Auto,
    /// Force push on (no auto-disable).
    On,
    /// Force push off.
    Off,
}

/// `madmail webmail-cors` — `__WEBMAIL_CORS_ORIGINS__` (+ dev enable workflow).
#[derive(Debug, Subcommand, Clone)]
pub enum WebmailCorsCommand {
    /// Show CORS origins and WebIMAP/WebSMTP status (default).
    Status,
    /// Replace the full origins list (comma or newline separated; `*` = any).
    Set {
        #[arg(value_name = "ORIGINS")]
        value: String,
    },
    /// Append one allowed browser origin.
    Add {
        #[arg(value_name = "ORIGIN")]
        origin: String,
    },
    /// Remove one origin from the list.
    Remove {
        #[arg(value_name = "ORIGIN")]
        origin: String,
    },
    /// Clear all CORS origins.
    Reset,
    /// Enable browser access (WebIMAP + WebSMTP). ORIGIN is optional on v2.11+ (request Origin is reflected).
    Enable {
        #[arg(value_name = "ORIGIN")]
        origin: Option<String>,
    },
    /// Disable browser access (turn off WebIMAP + WebSMTP).
    Disable,
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
    /// Start the listener (HTTP/HTTPS only).
    Enable,
    /// Stop the listener (HTTP/HTTPS only).
    Disable,
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

/// Default Windows service name registered by `madmail service install`.
pub const DEFAULT_WINDOWS_SERVICE_NAME: &str = "Madmail";

/// `madmail service` — Windows SCM install / lifecycle (no-op error on Unix).
#[derive(Debug, Subcommand, Clone)]
pub enum ServiceCommand {
    /// Register the Windows service (binPath uses this process + `--config` / `--state-dir`).
    Install {
        /// SCM service name.
        #[arg(long, default_value = DEFAULT_WINDOWS_SERVICE_NAME)]
        name: String,
        /// Start the service immediately after install.
        #[arg(long)]
        start: bool,
    },
    /// Remove the Windows service registration (stops it first when running).
    Uninstall {
        #[arg(long, default_value = DEFAULT_WINDOWS_SERVICE_NAME)]
        name: String,
    },
    /// Start the Windows service.
    Start {
        #[arg(long, default_value = DEFAULT_WINDOWS_SERVICE_NAME)]
        name: String,
    },
    /// Stop the Windows service.
    Stop {
        #[arg(long, default_value = DEFAULT_WINDOWS_SERVICE_NAME)]
        name: String,
    },
    /// Print service state (Running / Stopped / …).
    Status {
        #[arg(long, default_value = DEFAULT_WINDOWS_SERVICE_NAME)]
        name: String,
    },
}

/// Prefix for Madmail Windows Firewall rule display names (`Madmail (SMTP)`, …).
pub const FIREWALL_RULE_PREFIX: &str = "Madmail";

/// `madmail firewall` — Windows Firewall inbound rules (error on Unix).
#[derive(Debug, Subcommand, Clone)]
pub enum FirewallCommand {
    /// Create named inbound allow rules for mail/HTTP ports.
    Apply {
        /// Also open TURN (UDP/TCP 3478).
        #[arg(long)]
        turn: bool,
        /// Also open Shadowsocks (TCP 8388).
        #[arg(long)]
        ss: bool,
        /// Also open Iroh relay (TCP 3340).
        #[arg(long)]
        iroh: bool,
    },
    /// Delete all firewall rules whose names start with `Madmail`.
    Remove,
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

    /// Emit machine-readable JSON on stdout (no decorative text or QR codes).
    #[arg(long, global = true)]
    pub json: bool,
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
        clear_install_path_flag_bleed(&mut cli);
        cli
    }
}

/// Global `--state-dir` carries a production default; clap also fills install's optional
/// `state_dir` with that value. Clear it unless the user passed `--state-dir` / `--libexec`.
fn clear_install_path_flag_bleed(cli: &mut Cli) {
    if !crate::paths::argv_has_state_dir_flag() {
        if let Some(Command::Install(ref mut args)) = cli.command {
            args.state_dir = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn upgrade_and_update_accept_url_or_path() {
        let cli = Cli::try_parse_from(["madmail", "upgrade", "https://relay.example/bin/madmail"])
            .unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Upgrade {
                path_or_url,
                accept_unsafe_https: false,
            }) if path_or_url == "https://relay.example/bin/madmail"
        ));

        let cli = Cli::try_parse_from(["madmail", "update", "/tmp/madmail-signed"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Update {
                path_or_url,
                accept_unsafe_https: false,
            }) if path_or_url == "/tmp/madmail-signed"
        ));
    }

    #[test]
    fn upgrade_and_update_accept_unsafe_https_flag() {
        let cli = Cli::try_parse_from([
            "madmail",
            "upgrade",
            "--accept-unsafe-https",
            "https://relay.example/bin/madmail",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Upgrade {
                accept_unsafe_https: true,
                ..
            })
        ));

        let cli = Cli::try_parse_from([
            "madmail",
            "update",
            "https://relay.example/a.tar.gz",
            "--accept-unsafe-https",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Update {
                accept_unsafe_https: true,
                path_or_url,
            }) if path_or_url == "https://relay.example/a.tar.gz"
        ));
    }

    #[test]
    fn html_migrate_accepts_yes_flag() {
        let cli = Cli::try_parse_from(["madmail", "html-migrate"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::HtmlMigrate { yes: false })
        ));
        let cli = Cli::try_parse_from(["madmail", "html-migrate", "-y"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::HtmlMigrate { yes: true })
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

        let cli = Cli::try_parse_from(["chatmail", "--state-dir", "/tmp/custom-state"])
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
    fn service_subcommands_parse() {
        use super::ServiceCommand;

        let cli = Cli::try_parse_from(["chatmail", "service", "status"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Service(ServiceCommand::Status { name })) if name == DEFAULT_WINDOWS_SERVICE_NAME
        ));

        let cli = Cli::try_parse_from([
            "chatmail",
            "service",
            "install",
            "--name",
            "MadmailTest",
            "--start",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Service(ServiceCommand::Install {
                name,
                start: true,
            })) if name == "MadmailTest"
        ));
    }

    #[test]
    fn firewall_subcommands_parse() {
        use super::FirewallCommand;

        let cli = Cli::try_parse_from(["chatmail", "firewall", "remove"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Firewall(FirewallCommand::Remove))
        ));

        let cli =
            Cli::try_parse_from(["chatmail", "firewall", "apply", "--turn", "--ss", "--iroh"])
                .unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Firewall(FirewallCommand::Apply {
                turn: true,
                ss: true,
                iroh: true,
            }))
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

        let cli =
            Cli::try_parse_from(["chatmail", "blocklist", "add", "bad@x.org", "spam"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Blocklist(BlocklistCommand::Add {
                username,
                reason: Some(r),
            })) if username == "bad@x.org" && r == "spam"
        ));
    }

    #[test]
    fn default_install_subcommand_flags_are_unset() {
        let mut cli =
            Cli::try_parse_from(["madmail", "install", "--simple", "--ip", "1.2.3.4"]).unwrap();
        clear_install_path_flag_bleed(&mut cli);
        let Some(Command::Install(args)) = cli.command else {
            panic!("expected install subcommand");
        };
        assert!(
            args.config_dir.is_none(),
            "config_dir should be unset for default install, got {:?}",
            args.config_dir
        );
        assert!(
            args.state_dir.is_none(),
            "state_dir should be unset for default install, got {:?}",
            args.state_dir
        );
    }
}
