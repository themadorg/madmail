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

use std::sync::Arc;

use chatmail_auth::{normalize_username, AuthContext};
use chatmail_config::CredentialPolicy;
use chatmail_db::DbPool;
use chatmail_iroh::IrohDiscovery;
use chatmail_pgp::{enforce_encryption, EnforceOptions};
use chatmail_state::{AppState, NewMessageEvent};
use chatmail_storage::{
    commit_mailbox_blob_from_tmp, copy_message, expunge_deleted, list_mailbox_messages,
    mailbox_exists, move_message, read_blob, read_blob_known, read_blob_range_known,
    storage_policy::FsyncMode, store_add_flags, stream_append_direct_final_no_hash,
    stream_append_to_tmp, write_blob_mailbox, StoredMessage,
};
use chatmail_turn::TurnDiscovery;
use chatmail_types::{ChatmailError, Result};
use rustls::ServerConfig;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::broadcast;
use tokio::sync::broadcast::error::RecvError;
use tokio_rustls::server::TlsStream;
use tokio_rustls::TlsAcceptor;
use tracing::{debug, warn};

/// Max time a single IDLE notification may spend writing unsolicited updates to the client socket.
/// Per-subscriber egress isolation (Stalwart push-manager pattern): a half-open / wedged TCP
/// connection is dropped instead of pinning its IDLE task (and its broadcast receiver) forever.
/// Generous so it only ever trips on a genuinely dead socket — Delta Chat's EXISTS/RECENT writes
/// are a few bytes.
const IDLE_EMIT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

#[derive(Clone)]
pub struct ImapSessionConfig {
    pub hostname: String,
    pub primary_domain: String,
    pub jit_domain: Option<String>,
    pub credential_policy: CredentialPolicy,
    /// When set, advertise `METADATA` and serve `/shared/vendor/deltachat/turn`.
    pub turn: Option<TurnDiscovery>,
    /// When set, serve `/shared/vendor/deltachat/irohrelay` (WebXDC realtime).
    pub iroh: Option<IrohDiscovery>,
    /// Delta Chat push (`XDELTAPUSH` + `SETMETADATA /private/devicetoken`).
    pub push_enabled: bool,
    /// TLS upgrade on cleartext port 143 (not used on implicit-TLS :993 listeners).
    pub starttls_config: Option<Arc<ServerConfig>>,
}

impl ImapSessionConfig {
    pub fn advertise_metadata(&self) -> bool {
        self.turn.as_ref().is_some_and(TurnDiscovery::enabled)
            || self.iroh.as_ref().is_some_and(IrohDiscovery::enabled)
            || self.push_enabled
    }
}

pub struct ImapSession {
    ctx: Arc<AppState>,
    pool: DbPool,
    cfg: ImapSessionConfig,
    _tag: u32,
    authenticated_user: Option<String>,
    selected_mailbox: Option<String>,
    selected_folder_needs_expunge: bool,
    messages: Vec<MailMessage>,
    /// EXISTS count last *announced* to the client for the selected mailbox. The IDLE catch-up
    /// must compare against this, NOT `messages.len()`: FETCH reloads `messages` from disk
    /// (line in `handle_fetch`), so a message that lands during the EXISTS→FETCH window gets
    /// silently absorbed into `messages` and the next IDLE would see "no growth" and never push
    /// an EXISTS for it — the client then only discovers it on Delta Chat's ~75s periodic
    /// refresh. Tracking the announced count decouples "what the client knows" from the
    /// connection's scratch view of the maildir.
    announced_exists: usize,
    /// Memoized INBOX listing keyed by `EventBus::inbox_version`. FETCH/STORE/IDLE re-check the
    /// listing many times per client cycle; without this each call did a full `read_dir` + per
    /// file `stat`. Under a 60-recipient burst that per-command directory walk — not delivery —
    /// was the throughput wall. The cache is invalidated whenever the version counter advances
    /// (any local or remote mutation bumps it), so it never serves stale state.
    cached_inbox_version: u64,
    cached_inbox_messages: Option<Vec<MailMessage>>,
    /// Per-user delivery subscription kept alive for the whole connection so that messages
    /// arriving while the client is *between* IDLE commands (i.e. mid FETCH/STORE/re-IDLE) stay
    /// buffered in the broadcast channel instead of being dropped. Re-subscribing on every IDLE
    /// (the old behaviour) lost every notification fired in that window, so under a 60-recipient
    /// burst most receivers missed the live EXISTS push and only discovered the mail on Delta
    /// Chat's ~75s periodic IDLE refresh — the cause of the heavy tail latency and message loss.
    events_rx: Option<broadcast::Receiver<NewMessageEvent>>,
}

#[derive(Clone)]
struct MailMessage {
    uid: u32,
    id: String,
    /// Exact maildir filename discovered by the listing scan (`new/`/`cur/`, incl. `:2,` suffix).
    /// Lets a body FETCH open the file directly instead of re-scanning the directory.
    filename: String,
    size: u64,
    internal_date: String,
    flags: chatmail_storage::MaildirFlags,
}

impl ImapSession {
    pub fn new(ctx: Arc<AppState>, pool: DbPool, cfg: ImapSessionConfig) -> Self {
        Self {
            ctx,
            pool,
            cfg,
            _tag: 0,
            authenticated_user: None,
            selected_mailbox: None,
            selected_folder_needs_expunge: false,
            messages: Vec::new(),
            announced_exists: 0,
            cached_inbox_version: 0,
            cached_inbox_messages: None,
            events_rx: None,
        }
    }

    pub async fn handle_connection(&mut self, stream: TcpStream) -> Result<()> {
        if self.cfg.starttls_config.is_some() {
            self.serve_with_starttls_upgrade(stream).await
        } else {
            self.serve_loop(stream, false, false).await
        }
    }

    pub async fn handle_tls_connection(&mut self, stream: TlsStream<TcpStream>) -> Result<()> {
        // RFC 8314: implicit TLS (:993) must emit `* OK`; after STARTTLS the client already
        // saw the cleartext greeting and must not get a duplicate.
        let greeted = self.cfg.starttls_config.is_some();
        self.serve_loop(stream, true, greeted).await
    }

    async fn serve_with_starttls_upgrade(&mut self, stream: TcpStream) -> Result<()> {
        let (reader, mut writer) = tokio::io::split(stream);
        let mut lines = BufReader::new(reader);

        writer
            .write_all(format!("* OK {} IMAP4rev1 ready\r\n", self.cfg.hostname).as_bytes())
            .await?;

        loop {
            let mut line = String::new();
            if lines.read_line(&mut line).await? == 0 {
                break;
            }
            let line = line.trim_end().to_string();
            if line.is_empty() {
                continue;
            }
            let (tag, cmd, args) = parse_command(&line);
            let cmd_upper = cmd.to_ascii_uppercase();

            if cmd_upper == "STARTTLS" {
                let t = tag.unwrap_or("*");
                let Some(cfg) = self.cfg.starttls_config.clone() else {
                    writer
                        .write_all(format!("{t} BAD STARTTLS not available\r\n").as_bytes())
                        .await?;
                    continue;
                };
                writer
                    .write_all(format!("{t} OK Begin TLS negotiation now\r\n").as_bytes())
                    .await?;
                writer.flush().await?;
                let reader = lines.into_inner();
                let stream = reader.unsplit(writer);
                let acceptor = TlsAcceptor::from(cfg);
                let tls = acceptor
                    .accept(stream)
                    .await
                    .map_err(|e| ChatmailError::protocol(format!("STARTTLS failed: {e}")))?;
                return self.handle_tls_connection(tls).await;
            }

            if cmd_upper == "LOGIN" || cmd_upper == "AUTHENTICATE" {
                let t = tag.unwrap_or("*");
                writer
                    .write_all(
                        format!("{t} NO [PRIVACYREQUIRED] TLS required for authentication\r\n")
                            .as_bytes(),
                    )
                    .await?;
                continue;
            }

            let resp = self
                .dispatch(&mut lines, tag, &cmd, &args, &mut writer, false)
                .await?;
            if let Some(r) = resp {
                writer.write_all(r.as_bytes()).await?;
            }
            if cmd_upper == "LOGOUT" {
                writer
                    .write_all(b"* BYE madmail-v2 logging out\r\n")
                    .await?;
                if let Some(t) = tag {
                    writer
                        .write_all(format!("{t} OK LOGOUT completed\r\n").as_bytes())
                        .await?;
                }
                break;
            }
        }
        Ok(())
    }

    async fn serve_loop<S>(&mut self, stream: S, tls_active: bool, greeted: bool) -> Result<()>
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        let (reader, mut writer) = tokio::io::split(stream);
        let mut lines = BufReader::new(reader);

        if !greeted {
            writer
                .write_all(format!("* OK {} IMAP4rev1 ready\r\n", self.cfg.hostname).as_bytes())
                .await?;
        }

        loop {
            let mut line = String::new();
            if lines.read_line(&mut line).await? == 0 {
                break;
            }
            let line = line.trim_end().to_string();
            if line.is_empty() {
                continue;
            }
            let (tag, cmd, args) = parse_command(&line);
            let cmd_upper = cmd.to_ascii_uppercase();

            if cmd_upper == "STARTTLS" {
                let t = tag.unwrap_or("*");
                let msg = if tls_active {
                    format!("{t} BAD STARTTLS already active\r\n")
                } else {
                    format!("{t} BAD STARTTLS not available\r\n")
                };
                writer.write_all(msg.as_bytes()).await?;
                continue;
            }

            if (cmd_upper == "LOGIN" || cmd_upper == "AUTHENTICATE")
                && self.cfg.starttls_config.is_some()
                && !tls_active
            {
                let t = tag.unwrap_or("*");
                writer
                    .write_all(
                        format!("{t} NO [PRIVACYREQUIRED] TLS required for authentication\r\n")
                            .as_bytes(),
                    )
                    .await?;
                continue;
            }

            let resp = self
                .dispatch(&mut lines, tag, &cmd, &args, &mut writer, tls_active)
                .await?;
            if let Some(r) = resp {
                writer.write_all(r.as_bytes()).await?;
            }
            if cmd_upper == "LOGOUT" {
                writer
                    .write_all(b"* BYE madmail-v2 logging out\r\n")
                    .await?;
                if let Some(t) = tag {
                    writer
                        .write_all(format!("{t} OK LOGOUT completed\r\n").as_bytes())
                        .await?;
                }
                break;
            }
        }
        Ok(())
    }

    async fn dispatch<R, W>(
        &mut self,
        lines: &mut BufReader<R>,
        tag: Option<&str>,
        cmd: &str,
        args: &str,
        writer: &mut W,
        tls_active: bool,
    ) -> Result<Option<String>>
    where
        R: tokio::io::AsyncRead + Unpin,
        W: AsyncWriteExt + Unpin,
    {
        let t = tag.unwrap_or("*");
        match cmd.to_ascii_uppercase().as_str() {
            "CAPABILITY" => Ok(Some(format!(
                "* CAPABILITY {}\r\n{t} OK CAPABILITY completed\r\n",
                capability_string(
                    self.cfg.advertise_metadata(),
                    self.cfg.push_enabled,
                    self.cfg.starttls_config.is_some() && !tls_active,
                )
            ))),
            "NOOP" => Ok(Some(format!("{t} OK NOOP completed\r\n"))),
            "LOGIN" => {
                let (user, pass) = parse_login_args(args)?;
                let auth = AuthContext {
                    pool: self.pool.clone(),
                    state: Arc::clone(&self.ctx),
                    primary_domain: self.cfg.primary_domain.clone(),
                    jit_domain: self.cfg.jit_domain.clone(),
                    credential_policy: self.cfg.credential_policy,
                };
                chatmail_auth::authenticate(&auth, &user, &pass).await?;
                let user = normalize_username(&user)?;
                self.ctx.mailbox_store.init_user_dir(&user).await?;
                let _ = self
                    .ctx
                    .mailbox_store
                    .init_mailbox_dir(&user, "DeltaChat")
                    .await;
                self.authenticated_user = Some(user);
                Ok(Some(format!("{t} OK LOGIN completed\r\n")))
            }
            "LIST" | "LSUB" => {
                let _user = self.require_user()?;
                let resp = self.format_list_response(t).await?;
                Ok(Some(resp))
            }
            "CREATE" => {
                let user = self.require_user()?;
                let mailbox = parse_mailbox_name(args)
                    .ok_or_else(|| ChatmailError::protocol("CREATE missing mailbox"))?;
                self.ctx
                    .mailbox_store
                    .init_mailbox_dir(&user, &mailbox)
                    .await?;
                Ok(Some(format!("{t} OK CREATE completed\r\n")))
            }
            "SELECT" | "EXAMINE" => {
                let user = self.require_user()?;
                let resp = self.select_mailbox(t, args, &user, cmd).await?;
                Ok(Some(resp))
            }
            "CLOSE" => {
                if self.selected_mailbox.is_some() {
                    let user = self.require_user()?;
                    if let Some(folder) = self.selected_mailbox.clone() {
                        if self.selected_folder_needs_expunge {
                            let _ = expunge_deleted(&self.ctx.mailbox_store, &user, &folder).await;
                            self.selected_folder_needs_expunge = false;
                            // Files removed on disk → invalidate any cached listing.
                            self.bump_inbox(&user, &folder);
                        }
                    }
                }
                if self.selected_mailbox.is_none() {
                    return Ok(Some(format!("{t} BAD No mailbox selected\r\n")));
                }
                self.selected_mailbox = None;
                self.messages.clear();
                self.announced_exists = 0;
                Ok(Some(format!("{t} OK CLOSE completed\r\n")))
            }
            "STATUS" => {
                let user = self.require_user()?;
                let mailbox = parse_mailbox_name(args).unwrap_or_else(|| "INBOX".into());
                if !mailbox_exists(&self.ctx.mailbox_store, &user, &mailbox).await {
                    return Ok(Some(format!("{t} NO Mailbox does not exist\r\n")));
                }
                let msgs = list_messages(&self.ctx, &user, &mailbox).await?;
                let exists = msgs.len();
                let uid_next = msgs.last().map(|m| m.uid + 1).unwrap_or(1);
                Ok(Some(format!(
                    "* STATUS \"{mailbox}\" (MESSAGES {exists} UIDNEXT {uid_next} UIDVALIDITY 1 UNSEEN {exists})\r\n{t} OK STATUS completed\r\n"
                )))
            }
            "FETCH" => {
                let user = self.require_user()?;
                self.handle_fetch(t, args, &user, false, writer).await?;
                Ok(None)
            }
            "UID" if args.to_ascii_uppercase().starts_with("FETCH") => {
                let user = self.require_user()?;
                let rest = args.split_once(' ').map(|(_, r)| r).unwrap_or("");
                self.handle_fetch(t, rest, &user, true, writer).await?;
                Ok(None)
            }
            "UID" if args.to_ascii_uppercase().starts_with("STORE") => {
                let user = self.require_user()?;
                let rest = args.split_once(' ').map(|(_, r)| r).unwrap_or("");
                let resp = self.handle_store(t, rest, &user, true).await?;
                Ok(Some(resp))
            }
            "UID" if args.to_ascii_uppercase().starts_with("MOVE") => {
                let user = self.require_user()?;
                let rest = args.split_once(' ').map(|(_, r)| r).unwrap_or("");
                let resp = self.handle_move(t, rest, &user).await?;
                Ok(Some(resp))
            }
            "UID" if args.to_ascii_uppercase().starts_with("COPY") => {
                let user = self.require_user()?;
                let rest = args.split_once(' ').map(|(_, r)| r).unwrap_or("");
                let resp = self.handle_copy(t, rest, &user).await?;
                Ok(Some(resp))
            }
            "STORE" => {
                let user = self.require_user()?;
                let resp = self.handle_store(t, args, &user, false).await?;
                Ok(Some(resp))
            }
            "APPEND" => {
                let user = self.require_user()?;
                // Synchronizing literal `{N}` (no +): client waits for continuation (RFC 3501).
                if let Some((_, _, non_sync)) = parse_literal_spec(args) {
                    if !non_sync {
                        writer.write_all(b"+ Ready\r\n").await?;
                        writer.flush().await?;
                    }
                }
                match self.handle_append(lines, t, args, &user).await {
                    Ok(resp) => Ok(Some(resp)),
                    Err(ChatmailError::EncryptionNeeded(_)) => {
                        Ok(Some(format!("{t} NO [ENCRYPTED] APPEND rejected\r\n")))
                    }
                    Err(ChatmailError::QuotaExceeded { .. }) => {
                        Ok(Some(format!("{t} NO [OVERQUOTA] APPEND rejected\r\n")))
                    }
                    Err(ChatmailError::MessageTooLarge) => Ok(Some(format!(
                        "{t} NO [TOOBIG] {}\r\n",
                        chatmail_types::MESSAGE_FILE_TOO_BIG
                    ))),
                    Err(e) => Err(e),
                }
            }
            "IDLE" => {
                self.handle_idle(lines, t, writer).await?;
                Ok(None)
            }
            "GETQUOTAROOT" => {
                let user = self.require_user()?;
                Ok(Some(format_quota_quotaroot(t, args, &user, &self.ctx)))
            }
            "GETQUOTA" => {
                let user = self.require_user()?;
                Ok(Some(format_quota_getquota(t, args, &user, &self.ctx)))
            }
            "GETMETADATA" => {
                if self.authenticated_user.is_none() {
                    return Ok(Some(format!(
                        "{t} NO [AUTHENTICATIONREQUIRED] login first\r\n"
                    )));
                }
                let user = self.require_user()?;
                Ok(Some(self.handle_getmetadata(t, args, &user).await?))
            }
            "SETMETADATA" => {
                if self.authenticated_user.is_none() {
                    return Ok(Some(format!(
                        "{t} NO [AUTHENTICATIONREQUIRED] login first\r\n"
                    )));
                }
                if !self.cfg.push_enabled {
                    return Ok(Some(format!(
                        "{t} NO [CANNOT] push notifications disabled\r\n"
                    )));
                }
                if let Some((_, _, non_sync)) = parse_literal_spec(args) {
                    if !non_sync {
                        writer.write_all(b"+ Ready\r\n").await?;
                        writer.flush().await?;
                    }
                }
                let user = self.require_user()?;
                Ok(Some(self.handle_setmetadata(lines, t, args, &user).await?))
            }
            "LOGOUT" => Ok(None),
            _ => Ok(Some(format!("{t} BAD command not supported\r\n"))),
        }
    }

    fn require_user(&self) -> Result<String> {
        self.authenticated_user
            .clone()
            .ok_or(ChatmailError::AuthFailed)
    }

    /// Load a mailbox listing, serving the INBOX from an in-session cache keyed by the per-user
    /// version counter so repeated re-checks within one client cycle don't re-scan the maildir.
    async fn load_messages(&mut self, user: &str, mailbox: &str) -> Result<Vec<MailMessage>> {
        if !mailbox.eq_ignore_ascii_case("INBOX") {
            return list_messages(&self.ctx, user, mailbox).await;
        }
        let version = self.ctx.events.inbox_version(user);
        if version == self.cached_inbox_version {
            if let Some(cached) = &self.cached_inbox_messages {
                return Ok(cached.clone());
            }
        }
        let msgs = list_messages(&self.ctx, user, mailbox).await?;
        self.cached_inbox_version = version;
        self.cached_inbox_messages = Some(msgs.clone());
        Ok(msgs)
    }

    /// Mark the INBOX listing changed after a local mutation so the cache (and any IDLE re-check)
    /// reloads on next access.
    fn bump_inbox(&self, user: &str, mailbox: &str) {
        if mailbox.eq_ignore_ascii_case("INBOX") {
            self.ctx.events.bump_inbox_version(user);
        }
    }

    /// Read a message body, preferring the filename the listing already discovered (a direct file
    /// open) and falling back to the scanning `read_blob` only if the entry moved (e.g. a flag
    /// change relocated it between `new/` and `cur/`) since the cached listing was built.
    async fn read_message_body(
        &self,
        user: &str,
        mailbox: &str,
        m: &MailMessage,
    ) -> Result<Vec<u8>> {
        if !m.filename.is_empty() {
            if let Some(bytes) =
                read_blob_known(&self.ctx.mailbox_store, user, mailbox, &m.filename).await?
            {
                return Ok(bytes);
            }
        }
        read_blob(&self.ctx.mailbox_store, user, mailbox, &m.id).await
    }

    /// Read a byte range of a message body (for `BODY[]<offset.count>` partial fetches), preferring
    /// the known filename and falling back to slicing a full read if the entry moved.
    async fn read_message_body_range(
        &self,
        user: &str,
        mailbox: &str,
        m: &MailMessage,
        offset: u64,
        count: Option<u64>,
    ) -> Result<Vec<u8>> {
        if !m.filename.is_empty() {
            if let Some(bytes) = read_blob_range_known(
                &self.ctx.mailbox_store,
                user,
                mailbox,
                &m.filename,
                offset,
                count,
            )
            .await?
            {
                return Ok(bytes);
            }
        }
        let full = read_blob(&self.ctx.mailbox_store, user, mailbox, &m.id).await?;
        let start = (offset as usize).min(full.len());
        let end = match count {
            Some(c) => (start + c as usize).min(full.len()),
            None => full.len(),
        };
        Ok(full[start..end].to_vec())
    }

    /// Serve a FETCH response, writing the result directly to the connection writer.
    ///
    /// The response is assembled into a single `Vec<u8>` and written with one `write_all` +
    /// `flush` (response-level batching). Message bodies and header sections are appended as raw
    /// bytes — never routed through `str::from_utf8` — so PGP-encrypted / MIME binary payloads
    /// (images, videos, voice) reach the client byte-for-byte. RFC 3501 §6.4.5 literals are
    /// 8-bit clean, and the advertised `{N}` length must match exactly, so any UTF-8 lossy
    /// conversion here would corrupt media and desync the literal framing.
    async fn handle_fetch<W>(
        &mut self,
        tag: &str,
        args: &str,
        user: &str,
        by_uid: bool,
        writer: &mut W,
    ) -> Result<()>
    where
        W: AsyncWriteExt + Unpin,
    {
        let mode = fetch_response_mode(args);
        let partial = if mode == FetchResponseMode::FullBody {
            parse_body_partial(args)
        } else {
            None
        };
        // Reload INBOX so FETCH after SMTP delivery sees new messages on this connection.
        let mailbox = self
            .selected_mailbox
            .as_deref()
            .unwrap_or("INBOX")
            .to_string();
        self.messages = self.load_messages(user, &mailbox).await?;

        let selected: Vec<_> = select_fetch_messages(&self.messages, args, by_uid);

        let mut out: Vec<u8> = Vec::new();
        for m in selected {
            let seq = self
                .messages
                .iter()
                .position(|x| x.uid == m.uid)
                .map(|i| (i + 1) as u32)
                .unwrap_or(m.uid);
            if mode == FetchResponseMode::FullBody {
                let mailbox = self.selected_mailbox.as_deref().unwrap_or("INBOX");
                if let Some((offset, count)) = partial {
                    // Partial body fetch: stream only the requested window and echo the origin
                    // octet in the section spec (RFC 3501 `BODY[]<origin> {len}`).
                    let chunk = self
                        .read_message_body_range(user, mailbox, m, offset, count)
                        .await?;
                    out.extend_from_slice(
                        format!(
                            "* {seq} FETCH (UID {} RFC822.SIZE {} BODY[]<{}> {{{}}}\r\n",
                            m.uid,
                            m.size,
                            offset,
                            chunk.len()
                        )
                        .as_bytes(),
                    );
                    out.extend_from_slice(&chunk);
                    out.extend_from_slice(b")\r\n");
                } else {
                    let body = self.read_message_body(user, mailbox, m).await?;
                    out.extend_from_slice(
                        format!(
                            "* {seq} FETCH (UID {} RFC822.SIZE {} BODY[] {{{}}}\r\n",
                            m.uid,
                            m.size,
                            body.len()
                        )
                        .as_bytes(),
                    );
                    out.extend_from_slice(&body);
                    // Literal ends; close FETCH list (no CRLF between literal and ')' — go-imap compat).
                    out.extend_from_slice(b")\r\n");
                }
            } else if mode == FetchResponseMode::Headers {
                let mailbox = self.selected_mailbox.as_deref().unwrap_or("INBOX");
                let body = self.read_message_body(user, mailbox, m).await?;
                let headers = filter_header_fields(&body, header_field_names(args));
                let section = body_section_for_fetch(args);
                let mut attrs = format!("UID {} RFC822.SIZE {}", m.uid, m.size);
                if args.contains("INTERNALDATE") {
                    attrs.push_str(&format!(" INTERNALDATE \"{}\"", m.internal_date));
                }
                out.extend_from_slice(
                    format!("* {seq} FETCH ({attrs} {section} {{{}}}\r\n", headers.len())
                        .as_bytes(),
                );
                out.extend_from_slice(&headers);
                out.extend_from_slice(b")\r\n");
            } else {
                out.extend_from_slice(
                    format!(
                        "* {seq} FETCH ({}{})\r\n",
                        format_fetch_attrs(m),
                        format_fetch_flags(&m.flags),
                    )
                    .as_bytes(),
                );
            }
        }
        out.extend_from_slice(format!("{tag} OK FETCH completed\r\n").as_bytes());
        writer.write_all(&out).await?;
        writer.flush().await?;
        Ok(())
    }

    async fn handle_store(
        &mut self,
        tag: &str,
        args: &str,
        user: &str,
        by_uid: bool,
    ) -> Result<String> {
        let mailbox = self
            .selected_mailbox
            .clone()
            .ok_or_else(|| ChatmailError::protocol("No mailbox selected"))?;
        let mailbox = mailbox.as_str();
        let (uid_set, mode, flags) = parse_store_args(args)?;
        if mode != StoreMode::Add {
            return Ok(format!("{tag} BAD unsupported STORE mode\r\n"));
        }
        let uids = uid_set;
        if uids.is_empty() {
            return Ok(format!("{tag} OK STORE completed\r\n"));
        }

        let add_seen =
            mode == StoreMode::Add && flags.iter().any(|f| f.eq_ignore_ascii_case("\\Seen"));
        let add_deleted =
            mode == StoreMode::Add && flags.iter().any(|f| f.eq_ignore_ascii_case("\\Deleted"));
        if add_deleted {
            self.selected_folder_needs_expunge = true;
        }

        let mut out = String::new();
        let mut deleted_count = 0usize;
        for uid in uids {
            let Some(msg) = self.messages.iter().find(|m| m.uid == uid) else {
                continue;
            };
            let new_flags = store_add_flags(
                &self.ctx.mailbox_store,
                user,
                mailbox,
                &msg.id,
                add_seen,
                add_deleted,
            )
            .await?;
            if add_deleted {
                deleted_count += 1;
            }
            let seq = self
                .messages
                .iter()
                .position(|x| x.uid == uid)
                .map(|i| (i + 1) as u32)
                .unwrap_or(uid);
            out.push_str(&format!(
                "* {seq} FETCH (UID {uid}{})\r\n",
                format_fetch_flags(&new_flags),
            ));
        }

        // The client is removing `deleted_count` messages it already knew about; lower the
        // announced baseline by that many (NOT to the new total) so a message that arrived during
        // the FETCH→STORE window is still strictly greater than the baseline and gets its EXISTS.
        self.announced_exists = self.announced_exists.saturating_sub(deleted_count);
        // STORE changed flags / deletion state on disk → invalidate the cached listing.
        self.bump_inbox(user, mailbox);
        self.messages = self.load_messages(user, mailbox).await?;
        out.push_str(&format!(
            "{tag} OK {} completed\r\n",
            if by_uid { "UID STORE" } else { "STORE" }
        ));
        Ok(out)
    }

    async fn handle_move(&mut self, tag: &str, args: &str, user: &str) -> Result<String> {
        let from = self
            .selected_mailbox
            .clone()
            .ok_or_else(|| ChatmailError::protocol("No mailbox selected"))?;
        let from = from.as_str();
        let (uid_set, dest) = parse_uid_set_and_mailbox(args)?;
        let uids: Vec<u32> = if uid_set.is_empty() { vec![] } else { uid_set };
        let msgs: Vec<_> = uids
            .iter()
            .filter_map(|uid| self.messages.iter().find(|m| m.uid == *uid))
            .collect();
        if msgs.is_empty() {
            return Ok(format!("{tag} NO [TRYCREATE] No such messages\r\n"));
        }
        if !mailbox_exists(&self.ctx.mailbox_store, user, &dest).await {
            self.ctx.mailbox_store.init_mailbox_dir(user, &dest).await?;
        }
        let moved = msgs.len();
        for m in &msgs {
            move_message(&self.ctx.mailbox_store, user, from, &dest, &m.id).await?;
        }
        // Client removed `moved` known messages from the source mailbox; lower the baseline so a
        // concurrently-delivered message is still announced on the next IDLE.
        self.announced_exists = self.announced_exists.saturating_sub(moved);
        // Messages left the source mailbox → invalidate the cached listing.
        self.bump_inbox(user, from);
        self.messages = self.load_messages(user, from).await?;
        Ok(format!("{tag} OK UID MOVE completed\r\n"))
    }

    async fn handle_copy(&mut self, tag: &str, args: &str, user: &str) -> Result<String> {
        let from = self
            .selected_mailbox
            .as_deref()
            .ok_or_else(|| ChatmailError::protocol("No mailbox selected"))?;
        let (uid_set, dest) = parse_uid_set_and_mailbox(args)?;
        let msgs: Vec<_> = uid_set
            .iter()
            .filter_map(|uid| self.messages.iter().find(|m| m.uid == *uid))
            .collect();
        if msgs.is_empty() {
            return Ok(format!("{tag} NO [TRYCREATE] No such messages\r\n"));
        }
        if !mailbox_exists(&self.ctx.mailbox_store, user, &dest).await {
            self.ctx.mailbox_store.init_mailbox_dir(user, &dest).await?;
        }
        for m in &msgs {
            copy_message(&self.ctx.mailbox_store, user, from, &dest, &m.id).await?;
        }
        Ok(format!("{tag} OK UID COPY completed\r\n"))
    }

    /// RFC 2177 IDLE: wait for `DONE`, push unsolicited EXISTS/RECENT when mail arrives.
    async fn handle_idle<R, W>(
        &mut self,
        lines: &mut BufReader<R>,
        tag: &str,
        writer: &mut W,
    ) -> Result<()>
    where
        R: tokio::io::AsyncRead + Unpin,
        W: AsyncWriteExt + Unpin,
    {
        if self.selected_mailbox.is_none() {
            writer
                .write_all(format!("{tag} BAD No mailbox selected\r\n").as_bytes())
                .await?;
            return Ok(());
        }
        let user = self.require_user()?.clone();
        let mailbox = self.selected_mailbox.clone().unwrap_or_default();

        // Reuse the connection-scoped subscription if we already have one. The receiver keeps
        // buffering deliveries while the client is between IDLE commands (FETCH/STORE/re-IDLE),
        // so a message that arrives in that window is delivered on the *next* IDLE instead of
        // being lost until Delta Chat's periodic refresh. Only subscribe fresh on the first IDLE
        // of the connection. (Taken out of the Option so the loop below can borrow `self` for
        // emit_idle_updates; restored before returning.)
        let mut rx = self
            .events_rx
            .take()
            .unwrap_or_else(|| self.ctx.events.subscribe(&user));

        writer.write_all(b"+ idling\r\n").await?;
        writer.flush().await?;

        debug!(%user, %mailbox, exists = self.messages.len(), "IMAP IDLE started");

        // Catch up on anything delivered before we started waiting (or buffered while between
        // IDLEs). Idempotent: emit_idle_updates only pushes EXISTS/RECENT when the on-disk count
        // actually grew, so a duplicate buffered event in the loop below is a harmless no-op.
        self.emit_idle_updates(writer, &user).await?;

        let mut idle_line = String::new();
        loop {
            tokio::select! {
                // Client DONE must preempt waiting on mail (Delta Chat ends IDLE promptly).
                biased;
                n = lines.read_line(&mut idle_line) => {
                    match n? {
                        0 => break,
                        _ => {
                            if is_idle_done(&idle_line) {
                                debug!(%user, line = %idle_line.trim(), "IMAP IDLE client DONE");
                                break;
                            }
                            idle_line.clear();
                        }
                    }
                }
                ev = rx.recv() => {
                    match ev {
                        Ok(ev) => {
                            debug!(%user, msg_id = %ev.msg_id, "IMAP IDLE delivery event");
                            match tokio::time::timeout(
                                IDLE_EMIT_TIMEOUT,
                                self.emit_idle_updates(writer, &user),
                            )
                            .await
                            {
                                Ok(r) => r?,
                                Err(_) => {
                                    warn!(%user, "IMAP IDLE notification write timed out; dropping stuck connection");
                                    break;
                                }
                            }
                        }
                        Err(RecvError::Lagged(n)) => {
                            self.ctx.events.record_lag();
                            debug!(%user, skipped = n, "IMAP IDLE event bus lagged, resyncing");
                            match tokio::time::timeout(
                                IDLE_EMIT_TIMEOUT,
                                self.emit_idle_updates(writer, &user),
                            )
                            .await
                            {
                                Ok(r) => r?,
                                Err(_) => {
                                    warn!(%user, "IMAP IDLE resync write timed out; dropping stuck connection");
                                    break;
                                }
                            }
                        }
                        Err(RecvError::Closed) => break,
                    }
                }
            }
        }

        // Keep the subscription alive for the next IDLE so deliveries during FETCH/STORE/re-IDLE
        // are buffered rather than lost.
        self.events_rx = Some(rx);

        writer
            .write_all(format!("{tag} OK IDLE terminated\r\n").as_bytes())
            .await?;
        writer.flush().await?;
        Ok(())
    }

    /// Reload INBOX and send unsolicited EXISTS / RECENT if the message count grew.
    async fn emit_idle_updates<W>(&mut self, writer: &mut W, user: &str) -> Result<()>
    where
        W: AsyncWriteExt + Unpin,
    {
        let prev_exists = self.announced_exists;
        let mailbox = self
            .selected_mailbox
            .clone()
            .unwrap_or_else(|| "INBOX".to_string());
        self.messages = self.load_messages(user, &mailbox).await?;
        let new_exists = self.messages.len();
        if new_exists <= prev_exists {
            return Ok(());
        }
        self.announced_exists = new_exists;
        let recent = new_exists - prev_exists;
        writer
            .write_all(format!("* {new_exists} EXISTS\r\n").as_bytes())
            .await?;
        writer
            .write_all(format!("* {recent} RECENT\r\n").as_bytes())
            .await?;
        writer.flush().await?;
        debug!(
            %user,
            prev_exists,
            new_exists,
            recent,
            "IMAP IDLE unsolicited update"
        );
        Ok(())
    }

    async fn handle_getmetadata(&self, tag: &str, args: &str, user: &str) -> Result<String> {
        let (mailbox, keys) = parse_getmetadata_args(args);
        let mb = imap_quote_mailbox(&mailbox);
        let mut entries = Vec::new();
        for key in &keys {
            if key == chatmail_push::DEVICETOKEN_KEY {
                if !self.cfg.push_enabled {
                    continue;
                }
                let tokens = chatmail_push::list_device_tokens(&self.pool, user).await?;
                let joined = tokens.join(" ");
                entries.push(format_metadata_value(
                    key,
                    if joined.is_empty() {
                        None
                    } else {
                        Some(joined.as_str())
                    },
                ));
                continue;
            }
            if let Some(v) =
                shared_metadata_value(key, self.cfg.turn.as_ref(), self.cfg.iroh.as_ref())
            {
                entries.push(format_metadata_value(key, v.as_deref()));
            }
        }
        Ok(if entries.is_empty() {
            format!("{tag} OK GETMETADATA completed\r\n")
        } else {
            format!(
                "* METADATA {mb} ({entries})\r\n{tag} OK GETMETADATA completed\r\n",
                entries = entries.join(" "),
            )
        })
    }

    async fn handle_setmetadata<R>(
        &self,
        lines: &mut BufReader<R>,
        tag: &str,
        args: &str,
        user: &str,
    ) -> Result<String>
    where
        R: tokio::io::AsyncRead + Unpin,
    {
        let mailbox = parse_setmetadata_mailbox(args);
        if !mailbox.eq_ignore_ascii_case("INBOX") {
            return Ok(format!(
                "{tag} NO [CANNOT] SETMETADATA only supported on INBOX\r\n"
            ));
        }
        if !args.contains(chatmail_push::DEVICETOKEN_KEY) {
            return Ok(format!("{tag} BAD unsupported metadata key\r\n"));
        }

        let token = if let Some((_, size, _)) = parse_literal_spec(args) {
            if size > 8192 {
                return Ok(format!("{tag} NO [TOOBIG] device token too long\r\n"));
            }
            let mut buf = vec![0u8; size];
            lines.read_exact(&mut buf).await?;
            let mut extra = String::new();
            lines.read_line(&mut extra).await?;
            String::from_utf8(buf)
                .map_err(|_| ChatmailError::protocol("invalid device token encoding"))?
        } else if let Some(quoted) = parse_quoted_devicetoken_value(args) {
            quoted
        } else if args
            .split_whitespace()
            .any(|p| p.eq_ignore_ascii_case("NIL"))
        {
            return Ok(format!("{tag} OK SETMETADATA completed\r\n"));
        } else {
            return Ok(format!("{tag} BAD invalid SETMETADATA value\r\n"));
        };

        chatmail_push::upsert_device_token(&self.pool, user, &token).await?;
        Ok(format!("{tag} OK SETMETADATA completed\r\n"))
    }

    async fn handle_append<R>(
        &mut self,
        lines: &mut BufReader<R>,
        tag: &str,
        args: &str,
        user: &str,
    ) -> Result<String>
    where
        R: tokio::io::AsyncRead + Unpin,
    {
        let max_bytes = self.ctx.message_size.effective();
        let mailbox = self
            .selected_mailbox
            .clone()
            .unwrap_or_else(|| "INBOX".to_string());
        let stream_threshold = self.ctx.mailbox_store.policy().stream_threshold;

        let (literal, written_len) = if let Some((_, size)) = parse_literal_size(args) {
            if size as u64 > max_bytes {
                return Err(ChatmailError::message_too_large());
            }
            if size >= stream_threshold {
                let msg_id = uuid::Uuid::new_v4().to_string();

                let (tmp_path, header, hash, written) =
                    if self.ctx.mailbox_store.policy().fsync_mode == FsyncMode::Never {
                        // Ultra-fast Dovecot-like path for never + large distinct: direct to final
                        // location, no hashing, no CAS. File is already in new/ when we return.
                        let (written, header) = stream_append_direct_final_no_hash(
                            &self.ctx.mailbox_store,
                            user,
                            &mailbox,
                            &msg_id,
                            lines,
                            size as u64,
                        )
                        .await?;
                        // tmp_path here is actually the final path; dummy hash is ignored downstream
                        // because we short-circuit commit for Never.
                        (
                            self.ctx
                                .mailbox_store
                                .maildir_for_mailbox(user, &mailbox)
                                .new
                                .join(&msg_id),
                            header,
                            [0u8; 32],
                            written,
                        )
                    } else {
                        // Normal path (with hashing for CAS)
                        stream_append_to_tmp(
                            &self.ctx.mailbox_store,
                            user,
                            &mailbox,
                            &msg_id,
                            lines,
                            size as u64,
                        )
                        .await?
                    };
                let mut extra = String::new();
                lines.read_line(&mut extra).await?;
                // PGP / Secure-Join enforcement only needs the header region (the
                // application/pgp-encrypted marker lives in the first MIME part), so we validate
                // the captured prefix instead of re-reading the whole file.
                if let Err(e) = enforce_encryption(
                    &header,
                    &EnforceOptions {
                        mail_from: user.to_string(),
                        recipients: vec![user.to_string()],
                    },
                ) {
                    tokio::fs::remove_file(&tmp_path).await.ok();
                    return Err(e);
                }
                if let Err(e) = self.ctx.quota.check_quota(user, written) {
                    tokio::fs::remove_file(&tmp_path).await.ok();
                    return Err(e);
                }
                // Under mail_fsync=never + large distinct message we use the direct-to-final
                // no-hash path above. The file is already in its final location in new/.
                // Skip the entire commit/CAS machinery (another Dovecot "when never, do almost nothing" match).
                if self.ctx.mailbox_store.policy().fsync_mode
                    != chatmail_storage::storage_policy::FsyncMode::Never
                {
                    commit_mailbox_blob_from_tmp(
                        &self.ctx.mailbox_store,
                        user,
                        &mailbox,
                        &msg_id,
                        &tmp_path,
                        hash,
                        written,
                    )
                    .await?;
                }

                self.ctx.quota.record_write(user, written);
                self.ctx.events.notify_new_message(user, &msg_id);
                self.announced_exists = self.announced_exists.saturating_add(1);
                // Do not rescan the mailbox here: notify_new_message already bumped the INBOX
                // version, so the next command that needs the listing reloads lazily. Forcing a
                // readdir+stat after every APPEND is the dominant cost under concurrent large
                // uploads (Dovecot appends to its index incrementally instead of rescanning).
                return Ok(format!("{tag} OK APPEND completed\r\n"));
            }
            let mut literal = vec![0; size];
            lines.read_exact(&mut literal).await?;
            let mut extra = String::new();
            lines.read_line(&mut extra).await?;
            (literal, size)
        } else {
            let mut literal = Vec::new();
            let mut over_limit = false;
            loop {
                let mut dl = String::new();
                if lines.read_line(&mut dl).await? == 0 {
                    break;
                }
                let dl = dl.trim_end();
                if dl == "." {
                    break;
                }
                if !over_limit {
                    let unstuffed = dl.strip_prefix('.').unwrap_or(dl);
                    let add = unstuffed.len() as u64 + 2;
                    if literal.len() as u64 + add > max_bytes {
                        over_limit = true;
                    } else {
                        literal.extend_from_slice(unstuffed.as_bytes());
                        literal.extend_from_slice(b"\r\n");
                    }
                }
            }
            if over_limit {
                return Err(ChatmailError::message_too_large());
            }
            let written_len = literal.len();
            (literal, written_len)
        };

        enforce_encryption(
            &literal,
            &EnforceOptions {
                mail_from: user.to_string(),
                recipients: vec![user.to_string()],
            },
        )?;
        self.ctx.quota.check_quota(user, written_len as u64)?;
        let msg_id = uuid::Uuid::new_v4().to_string();
        write_blob_mailbox(&self.ctx.mailbox_store, user, &mailbox, &msg_id, &literal).await?;
        self.ctx.quota.record_write(user, written_len as u64);
        self.ctx.events.notify_new_message(user, &msg_id);
        self.announced_exists = self.announced_exists.saturating_add(1);
        // See the streaming path above: skip the post-APPEND rescan; the next command that needs
        // the listing reloads lazily after the version bump from notify_new_message.
        Ok(format!("{tag} OK APPEND completed\r\n"))
    }
}

impl ImapSession {
    async fn format_list_response(&self, tag: &str) -> Result<String> {
        let user = self.require_user()?;
        let mut out = String::new();
        for name in ["INBOX", "DeltaChat"] {
            if mailbox_exists(&self.ctx.mailbox_store, &user, name).await {
                out.push_str(&format!("* LIST (\\HasNoChildren) \"/\" \"{name}\"\r\n"));
            }
        }
        out.push_str(&format!("{tag} OK LIST completed\r\n"));
        Ok(out)
    }

    async fn select_mailbox(
        &mut self,
        tag: &str,
        args: &str,
        user: &str,
        cmd: &str,
    ) -> Result<String> {
        let mailbox = parse_mailbox_name(args).unwrap_or_else(|| "INBOX".into());
        if !mailbox_exists(&self.ctx.mailbox_store, user, &mailbox).await {
            return Ok(format!("{tag} NO [NONEXISTENT] Mailbox does not exist\r\n"));
        }
        self.selected_mailbox = Some(mailbox.clone());
        self.messages = self.load_messages(user, &mailbox).await?;
        let exists = self.messages.len();
        self.announced_exists = exists;
        let uid_next = self.messages.last().map(|m| m.uid + 1).unwrap_or(1);
        Ok(format!(
            "* {exists} EXISTS\r\n* 0 RECENT\r\n* OK [UIDVALIDITY 1] UIDs valid\r\n* OK [UIDNEXT {uid_next}] Predicted next UID\r\n{tag} OK [{cmd}] completed\r\n"
        ))
    }
}

async fn list_messages(ctx: &AppState, user: &str, mailbox: &str) -> Result<Vec<MailMessage>> {
    Ok(list_mailbox_messages(&ctx.mailbox_store, user, mailbox)
        .await?
        .into_iter()
        .map(stored_to_mail_message)
        .collect())
}

/// Carry the persistent uidlist UID through to the IMAP layer instead of renumbering by position,
/// so UIDs stay stable for the life of the mailbox (IMAP UIDVALIDITY contract).
fn stored_to_mail_message(m: StoredMessage) -> MailMessage {
    MailMessage {
        uid: m.uid,
        id: m.base_id,
        filename: m.filename,
        size: m.size,
        internal_date: format_internal_date(m.internal_date),
        flags: m.flags,
    }
}

fn format_fetch_attrs(m: &MailMessage) -> String {
    format!("UID {} RFC822.SIZE {}", m.uid, m.size)
}

fn format_fetch_flags(flags: &chatmail_storage::MaildirFlags) -> String {
    let imap = flags.imap_flags();
    if imap.is_empty() {
        String::new()
    } else {
        format!(
            " FLAGS ({})",
            imap.iter()
                .map(|f| f.to_string())
                .collect::<Vec<_>>()
                .join(" ")
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StoreMode {
    Add,
    Remove,
    Replace,
}

fn parse_store_args(args: &str) -> Result<(Vec<u32>, StoreMode, Vec<String>)> {
    let args = args.trim();
    let (uid_part, rest) = args
        .split_once('+')
        .or_else(|| args.split_once('-'))
        .or_else(|| args.split_once("FLAGS"))
        .ok_or_else(|| ChatmailError::protocol("invalid STORE args"))?;
    let uid_part = uid_part.trim();
    let uids = parse_num_set(&format!("{uid_part} ()")).unwrap_or_default();

    let mode = if rest.starts_with("FLAGS") || args.contains("-FLAGS") {
        if args.contains("+FLAGS") {
            StoreMode::Add
        } else if args.contains("-FLAGS") {
            StoreMode::Remove
        } else {
            StoreMode::Replace
        }
    } else if args.contains('+') {
        StoreMode::Add
    } else if args.contains('-') {
        StoreMode::Remove
    } else {
        StoreMode::Replace
    };

    let flags = parse_flag_list(args);
    Ok((uids, mode, flags))
}

fn parse_flag_list(args: &str) -> Vec<String> {
    let Some(start) = args.find('(') else {
        return Vec::new();
    };
    let Some(end) = args[start..].find(')') else {
        return Vec::new();
    };
    args[start + 1..start + end]
        .split_whitespace()
        .map(|s| s.trim_matches('"').to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn parse_uid_set_and_mailbox(args: &str) -> Result<(Vec<u32>, String)> {
    let args = args.trim();
    let mailbox = if args.contains('"') {
        parse_mailbox_name(args)
            .ok_or_else(|| ChatmailError::protocol("MOVE/COPY missing mailbox"))?
    } else {
        args.split_whitespace()
            .last()
            .map(|s| s.to_string())
            .ok_or_else(|| ChatmailError::protocol("MOVE/COPY missing mailbox"))?
    };
    let uid_part = if args.contains('"') {
        args.split('"').next().unwrap_or("").trim().to_string()
    } else {
        let parts: Vec<&str> = args.split_whitespace().collect();
        if parts.len() < 2 {
            return Err(ChatmailError::protocol("MOVE/COPY missing UID set"));
        }
        parts[..parts.len() - 1].join(" ")
    };
    let uids = parse_num_set(&format!("{uid_part} ()")).unwrap_or_default();
    Ok((uids, mailbox))
}

/// Advertised IMAP capabilities (TDD `03-imap-server.md`: XCHATMAIL, XDELTAPUSH, IDLE, QUOTA, METADATA).
pub fn capability_string(
    advertise_metadata: bool,
    advertise_push: bool,
    advertise_starttls: bool,
) -> String {
    let mut caps = vec![
        "IMAP4rev1",
        "IDLE",
        "QUOTA",
        "MOVE",
        "UIDPLUS",
        "AUTH=PLAIN",
        "LITERAL+",
        "XCHATMAIL",
    ];
    if advertise_push {
        caps.push("XDELTAPUSH");
    }
    if advertise_starttls {
        caps.push("STARTTLS");
    }
    if advertise_metadata {
        caps.push("METADATA");
    }
    caps.join(" ")
}

/// Parse mailbox name from `SELECT INBOX`, `SELECT "DeltaChat"`, or `MOVE 1 DeltaChat`.
fn parse_mailbox_name(args: &str) -> Option<String> {
    let args = args.trim();
    if let Some(i) = args.find('"') {
        let rest = &args[i + 1..];
        let end = rest.find('"')?;
        return Some(rest[..end].to_string());
    }
    let token = args.split_whitespace().next()?.trim_matches('"');
    if token.is_empty() {
        None
    } else {
        Some(token.to_string())
    }
}

/// Parse `GETMETADATA "" (/shared/comment ...)` (Delta Chat core).
fn parse_getmetadata_args(args: &str) -> (String, Vec<String>) {
    let mut mailbox = String::new();
    let mut keys = Vec::new();
    let mut rest = args.trim();
    if rest.starts_with('"') {
        if let Some(end) = rest[1..].find('"') {
            mailbox = rest[1..end + 1].to_string();
            rest = rest[end + 2..].trim();
        }
    }
    if let Some(start) = rest.find('(') {
        let inner = &rest[start + 1..];
        if let Some(end) = inner.rfind(')') {
            for part in inner[..end].split_whitespace() {
                let k = part.trim().trim_matches('"');
                if !k.is_empty() {
                    keys.push(k.to_string());
                }
            }
        }
    } else if !rest.is_empty() {
        for part in rest.split_whitespace() {
            let k = part.trim().trim_matches('"');
            if !k.is_empty() {
                keys.push(k.to_string());
            }
        }
    }
    (mailbox, keys)
}

fn imap_quote_mailbox(name: &str) -> String {
    if name.is_empty() {
        "\"\"".into()
    } else {
        format!("\"{}\"", name.replace('\\', "\\\\").replace('"', "\\\""))
    }
}

/// Mailbox name for SETMETADATA (first token only; do not scan value literals).
fn parse_setmetadata_mailbox(args: &str) -> String {
    let args = args.trim();
    if args.starts_with('"') {
        return parse_mailbox_name(args).unwrap_or_else(|| "INBOX".into());
    }
    args.split_whitespace()
        .next()
        .unwrap_or("INBOX")
        .trim_matches('"')
        .to_string()
}

fn parse_quoted_devicetoken_value(args: &str) -> Option<String> {
    let idx = args.find(chatmail_push::DEVICETOKEN_KEY)?;
    let rest = args[idx + chatmail_push::DEVICETOKEN_KEY.len()..].trim();
    if !rest.starts_with('"') {
        return None;
    }
    let inner = &rest[1..];
    let end = inner.find('"')?;
    Some(inner[..end].to_string())
}

fn shared_metadata_value(
    key: &str,
    turn: Option<&TurnDiscovery>,
    iroh: Option<&IrohDiscovery>,
) -> Option<Option<String>> {
    // None = omit key; Some(None) = explicit NIL; Some(Some(s)) = value
    match key {
        "/shared/comment" | "/shared/admin" => Some(None),
        "/shared/vendor/deltachat/irohrelay" => {
            let iroh = iroh.filter(|i| i.enabled())?;
            Some(Some(iroh.relay_url.clone()))
        }
        "/shared/vendor/deltachat/turn" | "/shared/vendor/deltachat/turns" => {
            let turn = turn.filter(|t| t.enabled())?;
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            let line = turn.metadata_line(now).ok()?;
            Some(Some(line))
        }
        "/shared/vendor/deltachat/turn-test-relay-only" => {
            turn.filter(|t| t.enabled() && t.turn_test_relay_only)?;
            Some(Some("1".to_string()))
        }
        _ => None,
    }
}

fn format_metadata_value(key: &str, value: Option<&str>) -> String {
    match value {
        None => format!("{key} NIL"),
        Some(v) => format!("{key} \"{v}\""),
    }
}

/// Default GETMETADATA lines for tests (Delta Chat TURN key).
pub fn turn_metadata_response(turn: &TurnDiscovery) -> String {
    shared_getmetadata_response("t0", "(/shared/vendor/deltachat/turn)", Some(turn), None)
}

/// Default GETMETADATA lines for tests (Delta Chat Iroh key).
pub fn iroh_metadata_response(iroh: &IrohDiscovery) -> String {
    shared_getmetadata_response(
        "t0",
        "(/shared/vendor/deltachat/irohrelay)",
        None,
        Some(iroh),
    )
}

/// RFC 5464 solicited response for shared server keys (TURN/Iroh tests).
fn shared_getmetadata_response(
    tag: &str,
    args: &str,
    turn: Option<&TurnDiscovery>,
    iroh: Option<&IrohDiscovery>,
) -> String {
    let (mailbox, keys) = parse_getmetadata_args(args);
    let mb = imap_quote_mailbox(&mailbox);
    let entries: Vec<String> = keys
        .iter()
        .filter_map(|key| {
            shared_metadata_value(key, turn, iroh).map(|v| format_metadata_value(key, v.as_deref()))
        })
        .collect();
    if entries.is_empty() {
        return format!("{tag} OK GETMETADATA completed\r\n");
    }
    format!(
        "* METADATA {mb} ({entries})\r\n{tag} OK GETMETADATA completed\r\n",
        entries = entries.join(" "),
    )
}

/// RFC 2087: STORAGE quota is reported in kilobytes.
fn format_quota_quotaroot(tag: &str, args: &str, user: &str, ctx: &AppState) -> String {
    let mailbox = parse_mailbox_name(args).unwrap_or_else(|| "INBOX".into());
    let used_kb = ctx.quota.used_bytes(user) / 1024;
    let max_kb = ctx.quota.max_bytes(user) / 1024;
    format!(
        "* QUOTAROOT {mb} \"ROOT\"\r\n* QUOTA \"ROOT\" (STORAGE {used_kb} {max_kb})\r\n{tag} OK GETQUOTAROOT completed\r\n",
        mb = imap_quote_mailbox(&mailbox),
    )
}

fn format_quota_getquota(tag: &str, args: &str, user: &str, ctx: &AppState) -> String {
    let root = parse_quota_root(args).unwrap_or_else(|| "ROOT".into());
    let used_kb = ctx.quota.used_bytes(user) / 1024;
    let max_kb = ctx.quota.max_bytes(user) / 1024;
    format!(
        "* QUOTA {root} (STORAGE {used_kb} {max_kb})\r\n{tag} OK GETQUOTA completed\r\n",
        root = imap_quote_mailbox(&root),
    )
}

fn parse_quota_root(args: &str) -> Option<String> {
    let args = args.trim();
    if args.starts_with('"') {
        parse_mailbox_name(args)
    } else {
        let root = args.split_whitespace().next()?.trim_matches('"');
        if root.is_empty() {
            None
        } else {
            Some(root.to_string())
        }
    }
}

/// Select messages for FETCH (sequence numbers) or UID FETCH (UIDs).
fn select_fetch_messages<'a>(
    messages: &'a [MailMessage],
    args: &str,
    by_uid: bool,
) -> Vec<&'a MailMessage> {
    let Some(nums) = parse_num_set(args) else {
        return messages.iter().collect();
    };
    if by_uid {
        messages.iter().filter(|m| nums.contains(&m.uid)).collect()
    } else {
        nums.iter()
            .filter_map(|seq| messages.get(seq.saturating_sub(1) as usize))
            .collect()
    }
}

/// Parse `1`, `2,3`, or `1:*` from the sequence/UID prefix before `(` in FETCH args.
/// True for `DONE` or tagged `a042 DONE` (RFC 2177 continuation).
fn is_idle_done(line: &str) -> bool {
    let upper = line.trim().to_ascii_uppercase();
    if upper.is_empty() {
        return false;
    }
    if upper == "DONE" {
        return true;
    }
    let mut parts = upper.split_whitespace();
    matches!(parts.next(), Some(_tag)) && parts.any(|p| p == "DONE")
}

fn parse_num_set(args: &str) -> Option<Vec<u32>> {
    let prefix = args.split('(').next()?.trim();
    if prefix.is_empty() {
        return None;
    }
    let mut uids = Vec::new();
    for part in prefix.split(',') {
        let part = part.trim();
        if let Some((start, end)) = part.split_once(':') {
            let start: u32 = start.parse().ok()?;
            let end: u32 = if end == "*" {
                u32::MAX
            } else {
                end.parse().ok()?
            };
            let cap = start.saturating_add(256).min(end);
            uids.extend(start..=cap);
        } else if let Ok(uid) = part.parse::<u32>() {
            uids.push(uid);
        }
    }
    if uids.is_empty() {
        None
    } else {
        Some(uids)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FetchResponseMode {
    FullBody,
    Headers,
    Metadata,
}

/// Classify FETCH data items (avoid treating RFC822.SIZE as full RFC822).
fn fetch_response_mode(args: &str) -> FetchResponseMode {
    if args.contains("BODY.PEEK[]") || args.contains("BODY[]") {
        return FetchResponseMode::FullBody;
    }
    if args.contains("HEADER.FIELDS")
        || args.contains("BODY.PEEK[HEADER")
        || args.contains("BODY[HEADER")
    {
        return FetchResponseMode::Headers;
    }
    if args.split_whitespace().any(|w| w == "RFC822") {
        return FetchResponseMode::FullBody;
    }
    FetchResponseMode::Metadata
}

/// Parse an RFC 3501 partial-fetch suffix `BODY[]<offset[.count]>` (or `BODY.PEEK[]<...>`).
/// Returns `(offset, Some(count))` for `<o.c>`, `(offset, None)` for `<o>`, or `None` when the
/// client requested the whole body. Only the empty-section form `BODY[]` is supported for partials
/// here (matching what this server serves); other sections fall through to their normal handling.
fn parse_body_partial(args: &str) -> Option<(u64, Option<u64>)> {
    let marker_pos = ["BODY.PEEK[]", "BODY[]"]
        .iter()
        .find_map(|m| args.find(m).map(|p| p + m.len()))?;
    let rest = &args[marker_pos..];
    let inner = rest.strip_prefix('<')?;
    let end = inner.find('>')?;
    let spec = &inner[..end];
    match spec.split_once('.') {
        Some((o, c)) => Some((o.trim().parse().ok()?, Some(c.trim().parse().ok()?))),
        None => Some((spec.trim().parse().ok()?, None)),
    }
}

fn extract_headers(body: &[u8]) -> &[u8] {
    body.windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|i| &body[..i + 4])
        .unwrap_or(body)
}

/// Section name for FETCH responses. Clients may request `BODY.PEEK[...]` but imap-proto only
/// parses solicited `BODY[HEADER.FIELDS (...)]` in responses (Delta Chat / async-imap).
fn body_section_for_fetch(args: &str) -> String {
    if let Some(start) = args.find("HEADER.FIELDS") {
        let rest = &args[start..];
        if let Some(end) = rest.find(')') {
            return format!("BODY[{}]", &rest[..=end]);
        }
    }
    "BODY[HEADER.FIELDS]".into()
}

fn header_field_names(args: &str) -> Vec<String> {
    let Some(start) = args.find("HEADER.FIELDS") else {
        return Vec::new();
    };
    let rest = &args[start + "HEADER.FIELDS".len()..];
    let rest = rest.trim_start();
    let Some(inner) = rest.strip_prefix('(') else {
        return Vec::new();
    };
    let end = inner.find(')').unwrap_or(inner.len());
    inner[..end]
        .split_whitespace()
        .map(|s| s.trim_matches('"').to_ascii_uppercase())
        .collect()
}

fn filter_header_fields(body: &[u8], names: Vec<String>) -> Vec<u8> {
    let raw = extract_headers(body);
    if names.is_empty() {
        return raw.to_vec();
    }
    let text = std::str::from_utf8(raw).unwrap_or("");
    let mut out = Vec::new();
    let mut keep = false;
    for line in text.split("\r\n") {
        if line.is_empty() {
            out.extend_from_slice(b"\r\n");
            continue;
        }
        if line.starts_with(|c: char| c.is_ascii_whitespace()) {
            if keep {
                out.extend_from_slice(line.as_bytes());
                out.extend_from_slice(b"\r\n");
            }
            continue;
        }
        let field = line
            .split(':')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_uppercase();
        keep = names.iter().any(|n| n == &field);
        if keep {
            out.extend_from_slice(line.as_bytes());
            out.extend_from_slice(b"\r\n");
        }
    }
    if !out.ends_with(b"\r\n\r\n") {
        if !out.ends_with(b"\r\n") {
            out.extend_from_slice(b"\r\n");
        }
        out.extend_from_slice(b"\r\n");
    }
    out
}

/// RFC 3501 `date-time` in UTC for INTERNALDATE.
fn format_internal_date(st: std::time::SystemTime) -> String {
    let secs = st
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let dt = time::OffsetDateTime::from_unix_timestamp(secs)
        .unwrap_or_else(|_| time::OffsetDateTime::now_utc());
    dt.format(
        &time::format_description::parse(
            "[day padding:zero]-[month repr:short]-[year] [hour]:[minute]:[second] +0000",
        )
        .expect("valid format"),
    )
    .unwrap_or_else(|_| "01-Jan-1970 00:00:00 +0000".into())
}

fn parse_command(line: &str) -> (Option<&str>, String, String) {
    let mut parts = line.splitn(3, ' ');
    let first = parts.next().unwrap_or("");
    if first.chars().all(|c| c.is_ascii_alphanumeric()) && first.len() <= 8 {
        let tag = Some(first);
        let cmd = parts.next().unwrap_or("").to_string();
        let args = parts.next().unwrap_or("").to_string();
        (tag, cmd, args)
    } else {
        (None, first.to_string(), parts.collect::<Vec<_>>().join(" "))
    }
}

fn parse_literal_size(args: &str) -> Option<(usize, usize)> {
    parse_literal_spec(args).map(|(start, n, _)| (start, n))
}

/// Literal in APPEND args: `{123}` (sync) or `{123+}` (non-sync / LITERAL+).
fn parse_literal_spec(args: &str) -> Option<(usize, usize, bool)> {
    let start = args.find('{')?;
    let end = args.find('}')?;
    let inner = args[start + 1..end].trim();
    let non_sync = inner.ends_with('+');
    let n_str = inner.trim_end_matches('+').trim();
    let n: usize = n_str.parse().ok()?;
    Some((start, n, non_sync))
}

fn parse_login_args(args: &str) -> Result<(String, String)> {
    let mut it = args.split_whitespace();
    let user = it
        .next()
        .ok_or_else(|| ChatmailError::protocol("LOGIN missing user"))?
        .trim_matches('"');
    let pass = it
        .next()
        .ok_or_else(|| ChatmailError::protocol("LOGIN missing pass"))?
        .trim_matches('"');
    Ok((user.to_string(), pass.to_string()))
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;
    use chatmail_storage::{write_blob, MailboxStore};
    use std::sync::Arc;
    use std::time::Duration;
    // write_blob delivers to INBOX

    /// P5-UT01: CAPABILITY includes Chatmail extensions (TDD + cmdeploy `test_capabilities`).
    #[test]
    fn p5_ut01_test_capability_includes_chatmail_extensions() {
        let caps = capability_string(false, false, false);
        assert!(caps.contains("IMAP4rev1"));
        assert!(caps.contains("IDLE"));
        assert!(caps.contains("QUOTA"));
        assert!(caps.contains("MOVE"));
        assert!(caps.contains("XCHATMAIL"));
        assert!(
            !caps.contains("XDELTAPUSH"),
            "XDELTAPUSH is advertised only when push is enabled"
        );
        assert!(
            !caps.contains("METADATA"),
            "METADATA is advertised only when TURN/Iroh/push discovery is enabled"
        );

        let with_push = capability_string(false, true, false);
        assert!(with_push.contains("XDELTAPUSH"));

        let with_starttls = capability_string(false, false, true);
        assert!(with_starttls.contains("STARTTLS"));

        let with_metadata = capability_string(true, false, false);
        assert!(with_metadata.contains("METADATA"));
    }

    #[test]
    fn test_is_idle_done() {
        assert!(is_idle_done("DONE"));
        assert!(is_idle_done("done\r\n"));
        assert!(is_idle_done("a042 DONE"));
        assert!(!is_idle_done("a042 NOOP"));
    }

    #[test]
    fn test_fetch_sequence_uses_index_not_uid() {
        let msgs = vec![
            MailMessage {
                uid: 100,
                id: "first".into(),
                filename: "first".into(),
                size: 1,
                internal_date: "01-Jan-2020 00:00:00 +0000".into(),
                flags: Default::default(),
            },
            MailMessage {
                uid: 200,
                id: "second".into(),
                filename: "second".into(),
                size: 2,
                internal_date: "02-Jan-2020 00:00:00 +0000".into(),
                flags: Default::default(),
            },
        ];
        let by_seq = select_fetch_messages(&msgs, "2 (BODY.PEEK[])", false);
        assert_eq!(by_seq.len(), 1);
        assert_eq!(by_seq[0].id, "second");
        let by_uid = select_fetch_messages(&msgs, "2 (BODY.PEEK[])", true);
        assert!(by_uid.is_empty());
        let by_uid2 = select_fetch_messages(&msgs, "200 (BODY.PEEK[])", true);
        assert_eq!(by_uid2.len(), 1);
        assert_eq!(by_uid2[0].id, "second");
    }

    #[test]
    fn p6_fetch_body_peek_header_fields_section() {
        let args = "(UID INTERNALDATE RFC822.SIZE BODY.PEEK[HEADER.FIELDS (MESSAGE-ID FROM)])";
        assert_eq!(
            body_section_for_fetch(args),
            "BODY[HEADER.FIELDS (MESSAGE-ID FROM)]"
        );
    }

    #[test]
    fn p6_fetch_prefetch_response_parses_with_imap_proto() {
        // imap-proto does not parse `BODY.PEEK[...]` in FETCH responses (only `BODY[...]`).
        let peek = b"* 1 FETCH (UID 1 BODY.PEEK[HEADER.FIELDS (MESSAGE-ID)] {19}\r\n\
                      Message-ID: <a@b>\r\n\r\n)\r\n";
        assert!(imap_proto::parser::parse_response(peek).is_err());

        let wire =
            b"* 1 FETCH (UID 1 RFC822.SIZE 99 BODY[HEADER.FIELDS (MESSAGE-ID FROM)] {34}\r\n\
                      Message-ID: <a@b>\r\nFrom: <a@b>\r\n\r\n)\r\n";
        match imap_proto::parser::parse_response(wire) {
            Ok((_, imap_proto::Response::Fetch(_, _))) => {}
            other => panic!("imap-proto should parse prefetch FETCH: {other:?}"),
        }
        let with_date = b"* 1 FETCH (UID 1 INTERNALDATE \"15-May-2026 13:27:29 +0000\" \
                          BODY[HEADER.FIELDS (CHAT-VERSION)] {21}\r\nChat-Version: 1.0\r\n\r\n)\r\n";
        match imap_proto::parser::parse_response(with_date) {
            Ok((_, imap_proto::Response::Fetch(_, _))) => {}
            other => panic!("INTERNALDATE in FETCH should parse: {other:?}"),
        }
    }

    #[test]
    fn test_fetch_uid_rfc822_size_is_metadata_only() {
        assert_eq!(
            fetch_response_mode("1 (UID RFC822.SIZE)"),
            FetchResponseMode::Metadata
        );
        assert_eq!(
            fetch_response_mode("1 (UID RFC822.SIZE BODY.PEEK[])"),
            FetchResponseMode::FullBody
        );
    }

    #[test]
    fn test_parse_store_args() {
        let (uids, mode, flags) = parse_store_args("1 +FLAGS (\\Seen)").unwrap();
        assert_eq!(uids, vec![1]);
        assert_eq!(mode, StoreMode::Add);
        assert_eq!(flags, vec!["\\Seen".to_string()]);

        let (uids, _, flags) = parse_store_args("2,3 +FLAGS (\\Deleted)").unwrap();
        assert_eq!(uids, vec![2, 3]);
        assert_eq!(flags, vec!["\\Deleted".to_string()]);
    }

    #[test]
    fn test_parse_uid_set_and_mailbox() {
        let (uids, mb) = parse_uid_set_and_mailbox("1 DeltaChat").unwrap();
        assert_eq!(uids, vec![1]);
        assert_eq!(mb, "DeltaChat");
    }

    #[tokio::test]
    async fn p5_store_seen_moves_to_cur() {
        use chatmail_storage::{list_mailbox_messages, store_add_flags, write_blob};

        let tmp = tempfile::tempdir().unwrap();
        let store = MailboxStore::new(tmp.path());
        store.init_user_dir("u@test").await.unwrap();
        let body = b"From: a@b.test\r\nTo: u@test\r\nContent-Type: multipart/encrypted; boundary=b\r\n\r\n--b\r\nContent-Type: application/pgp-encrypted\r\n\r\nv\r\n--b--\r\n";
        write_blob(&store, "u@test", "m1", body).await.unwrap();
        store_add_flags(&store, "u@test", "INBOX", "m1", true, false)
            .await
            .unwrap();
        let msgs = list_mailbox_messages(&store, "u@test", "INBOX")
            .await
            .unwrap();
        assert_eq!(msgs.len(), 1);
        assert!(msgs[0].flags.seen);
    }

    #[tokio::test]
    async fn p5_move_between_mailboxes() {
        use chatmail_storage::{list_mailbox_messages, move_message, write_blob};

        let tmp = tempfile::tempdir().unwrap();
        let store = MailboxStore::new(tmp.path());
        store.init_user_dir("u@test").await.unwrap();
        store.init_mailbox_dir("u@test", "DeltaChat").await.unwrap();
        let body = b"From: a@b.test\r\nTo: u@test\r\nContent-Type: multipart/encrypted; boundary=b\r\n\r\n--b\r\nContent-Type: application/pgp-encrypted\r\n\r\nv\r\n--b--\r\n";
        write_blob(&store, "u@test", "m1", body).await.unwrap();
        move_message(&store, "u@test", "INBOX", "DeltaChat", "m1")
            .await
            .unwrap();
        assert!(list_mailbox_messages(&store, "u@test", "INBOX")
            .await
            .unwrap()
            .is_empty());
        assert_eq!(
            list_mailbox_messages(&store, "u@test", "DeltaChat")
                .await
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn parse_quoted_devicetoken_extracts_value() {
        let v = parse_quoted_devicetoken_value(r#"INBOX (/private/devicetoken "openpgp:abc123" )"#)
            .expect("quoted");
        assert_eq!(v, "openpgp:abc123");
    }

    #[tokio::test]
    async fn setmetadata_and_getmetadata_devicetoken_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let pool = chatmail_db::init_memory_db().await.unwrap();
        let ctx = Arc::new(AppState::new(dir.path(), pool.clone()));
        let mut session = ImapSession::new(
            ctx,
            pool.clone(),
            ImapSessionConfig {
                hostname: "imap.test".into(),
                primary_domain: "test".into(),
                jit_domain: None,
                credential_policy: CredentialPolicy::default(),
                turn: None,
                iroh: None,
                push_enabled: true,
                starttls_config: None,
            },
        );
        session.authenticated_user = Some("u@test".into());

        let wire = br#"INBOX (/private/devicetoken "tok-one" )"#.as_slice();
        let mut reader = BufReader::new(wire);
        let resp = session
            .handle_setmetadata(
                &mut reader,
                "s1",
                r#"INBOX (/private/devicetoken "tok-one" )"#,
                "u@test",
            )
            .await
            .unwrap();
        assert!(resp.contains("OK SETMETADATA"), "{resp}");

        let get = session
            .handle_getmetadata("g1", "INBOX /private/devicetoken", "u@test")
            .await
            .unwrap();
        assert!(get.contains("tok-one"), "{get}");
    }

    #[test]
    fn test_parse_command_tag() {
        let (tag, cmd, _) = parse_command("a001 CAPABILITY");
        assert_eq!(tag, Some("a001"));
        assert_eq!(cmd, "CAPABILITY");
    }

    /// P5-UT02: maildir listing after delivery exposes messages for FETCH.
    #[tokio::test]
    async fn p5_ut02_test_list_messages_after_write() {
        let dir = tempfile::tempdir().unwrap();
        let pool = chatmail_db::init_memory_db().await.unwrap();
        let store = MailboxStore::new(dir.path());
        let ctx = AppState::new(dir.path(), pool);
        let body = b"From: a@b.test\r\nTo: u@example.org\r\nContent-Type: multipart/encrypted; boundary=b\r\n\r\n--b\r\nContent-Type: application/pgp-encrypted\r\n\r\nv\r\n--b--\r\n";
        write_blob(&store, "u@example.org", "m1", body)
            .await
            .unwrap();
        let msgs = list_messages(&ctx, "u@example.org", "INBOX").await.unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].size, body.len() as u64);
    }

    /// APPEND `{N}` larger than message cap is rejected before reading the literal.
    #[tokio::test]
    async fn append_literal_over_message_limit_returns_toobig() {
        use chatmail_config::AppConfig;
        use std::sync::Arc;

        let dir = tempfile::tempdir().unwrap();
        let pool = chatmail_db::init_memory_db().await.unwrap();
        let mut cfg = AppConfig::default();
        cfg.appendlimit = Some("2K".into());
        cfg.max_message_size = Some("2K".into());
        let ctx = Arc::new(AppState::with_quota_and_message_limit(
            dir.path(),
            chatmail_config::DEFAULT_QUOTA_BYTES,
            &cfg,
            pool.clone(),
        ));
        ctx.hydrate(&pool, &cfg).await.unwrap();
        assert_eq!(ctx.message_size.effective(), 2048);

        let mut session = ImapSession::new(
            ctx,
            pool,
            ImapSessionConfig {
                hostname: "imap.test".into(),
                primary_domain: "test".into(),
                jit_domain: None,
                credential_policy: CredentialPolicy::default(),
                turn: None,
                iroh: None,
                push_enabled: true,
                starttls_config: None,
            },
        );

        let literal_len = 3000usize;
        let args = format!("INBOX {{{literal_len}}}");
        let mut wire = vec![0u8; literal_len];
        wire.extend_from_slice(b"\r\n");
        let mut reader = BufReader::new(wire.as_slice());

        let err = session
            .handle_append(&mut reader, "a001", &args, "u@test")
            .await
            .unwrap_err();
        assert!(matches!(err, ChatmailError::MessageTooLarge));
        assert!(
            wire[..literal_len].iter().all(|&b| b == 0),
            "literal must not be read when over limit"
        );
    }

    /// P5-UT03: APPEND path uses PGP enforcement (rejects plaintext).
    #[test]
    fn p5_ut03_test_append_rejects_plaintext() {
        let plain = b"From: u@example.org\r\nSubject: x\r\nContent-Type: text/plain\r\n\r\nno";
        let err = enforce_encryption(
            plain,
            &EnforceOptions {
                mail_from: "u@example.org".into(),
                recipients: vec!["u@example.org".into()],
            },
        )
        .unwrap_err();
        assert!(matches!(err, ChatmailError::EncryptionNeeded(_)));
    }

    /// P6-S07: METADATA exposes Iroh relay URL (Delta Chat core).
    #[test]
    fn p6_s07_test_iroh_metadata_response() {
        let iroh = IrohDiscovery {
            relay_url: "http://203.0.113.50:3340".into(),
        };
        let resp = iroh_metadata_response(&iroh);
        assert!(resp.contains("/shared/vendor/deltachat/irohrelay \"http://203.0.113.50:3340\""));
        assert!(resp.contains("OK GETMETADATA"));
    }

    /// P6-UT02: METADATA exposes TURN relay key.
    #[test]
    fn p9_ut04_test_turn_metadata_response() {
        let turn = TurnDiscovery {
            server: "127.0.0.1".into(),
            port: 3478,
            secret: "test-secret".into(),
            ttl_secs: 86400,
            turn_test_relay_only: false,
        };
        let resp = shared_getmetadata_response(
            "t1",
            "(/shared/comment /shared/vendor/deltachat/turn)",
            Some(&turn),
            None,
        );
        assert!(resp.contains("/shared/vendor/deltachat/turn"));
        assert!(resp.contains("OK GETMETADATA"));
        assert!(
            resp.contains("* METADATA \"\" (/shared/comment NIL /shared/vendor/deltachat/turn"),
            "RFC 5464 entry-list: {resp}"
        );
        assert!(!resp.contains("/shared/comment \"\""));
    }

    #[test]
    fn p9_ut04_test_metadata_full_admin_comment_turn() {
        let turn = TurnDiscovery {
            server: "turn.test".into(),
            port: 3478,
            secret: "s".into(),
            ttl_secs: 60,
            turn_test_relay_only: false,
        };
        let resp = shared_getmetadata_response(
            "t1",
            "(/shared/comment /shared/admin /shared/vendor/deltachat/turn)",
            Some(&turn),
            None,
        );
        assert!(
            resp.contains(
                "* METADATA \"\" (/shared/comment NIL /shared/admin NIL /shared/vendor/deltachat/turn"
            ),
            "{resp}"
        );
    }

    #[tokio::test]
    async fn p6_ut02_test_quota_quotaroot_format() {
        let dir = tempfile::tempdir().unwrap();
        let pool = chatmail_db::init_memory_db().await.unwrap();
        let ctx = AppState::new(dir.path(), pool);
        let resp = format_quota_quotaroot("t1", "INBOX", "u@test", &ctx);
        assert!(resp.contains("QUOTAROOT"));
        assert!(resp.contains("QUOTA \"ROOT\" (STORAGE"));
        assert!(resp.contains("OK GETQUOTAROOT"));
    }

    /// P6-UT02: EXISTS/RECENT only when mailbox count increases.
    #[tokio::test]
    async fn p6_ut02_test_emit_idle_updates_format() {
        let dir = tempfile::tempdir().unwrap();
        let pool = chatmail_db::init_memory_db().await.unwrap();
        let ctx = Arc::new(AppState::new(dir.path(), pool.clone()));
        let body = b"From: a@b.test\r\nTo: u@example.org\r\nContent-Type: multipart/encrypted; boundary=b\r\n\r\n--b\r\nContent-Type: application/pgp-encrypted\r\n\r\nv\r\n--b--\r\n";
        write_blob(&ctx.mailbox_store, "u@example.org", "m1", body)
            .await
            .unwrap();

        let mut session = ImapSession::new(
            Arc::clone(&ctx),
            chatmail_db::init_memory_db().await.unwrap(),
            ImapSessionConfig {
                hostname: "imap.test".into(),
                primary_domain: "test".into(),
                jit_domain: None,
                credential_policy: CredentialPolicy::default(),
                turn: None,
                iroh: None,
                push_enabled: true,
                starttls_config: None,
            },
        );
        session.authenticated_user = Some("u@example.org".into());
        session.selected_mailbox = Some("INBOX".into());
        session.messages = list_messages(&ctx, "u@example.org", "INBOX").await.unwrap();
        assert_eq!(session.messages.len(), 1);
        // Simulate SELECT having announced the 1 pre-existing message to the client.
        session.announced_exists = 1;

        let mut buf = Vec::new();
        write_blob(&ctx.mailbox_store, "u@example.org", "m2", body)
            .await
            .unwrap();
        session
            .emit_idle_updates(&mut buf, "u@example.org")
            .await
            .unwrap();
        let out = String::from_utf8(buf).unwrap();
        assert!(out.contains("* 2 EXISTS"));
        assert!(out.contains("* 1 RECENT"));

        let mut no_dup = Vec::new();
        session
            .emit_idle_updates(&mut no_dup, "u@example.org")
            .await
            .unwrap();
        assert!(
            no_dup.is_empty(),
            "no duplicate EXISTS when count unchanged"
        );
    }

    /// P6-UT01: IDLE path uses EventBus (TDD `03-imap-server.md` — EXISTS on delivery).
    #[tokio::test]
    async fn p6_ut01_test_idle_receives_delivery_event() {
        let dir = tempfile::tempdir().unwrap();
        let pool = chatmail_db::init_memory_db().await.unwrap();
        let ctx = Arc::new(AppState::new(dir.path(), pool.clone()));
        let mut rx = ctx.events.subscribe("u@example.org");
        let ctx_bg = Arc::clone(&ctx);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(30)).await;
            ctx_bg.events.notify_new_message("u@example.org", "mid-1");
        });
        let ev = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("timeout")
            .expect("recv");
        assert_eq!(ev.username, "u@example.org");
        assert_eq!(ev.msg_id, "mid-1");
    }
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod integration_tests {
    use super::*;
    use chatmail_auth::hash_password;
    use chatmail_storage::write_blob;
    use std::net::TcpListener as StdListener;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    async fn imap_dialog(pool: DbPool, ctx: Arc<AppState>, script: &[&str]) -> String {
        imap_dialog_with_discovery(pool, ctx, None, None, script).await
    }

    /// Spawn an IMAP server bound to an ephemeral port and return its address + handle.
    /// Unlike `imap_dialog`, the caller drives the socket with raw bytes so binary FETCH
    /// literals can be inspected byte-for-byte (no lossy UTF-8 conversion).
    async fn spawn_imap_server(pool: DbPool, ctx: Arc<AppState>) -> std::net::SocketAddr {
        ctx.auth.hydrate(&pool).await.unwrap();
        let std_listener = StdListener::bind("127.0.0.1:0").unwrap();
        std_listener.set_nonblocking(true).unwrap();
        let addr = std_listener.local_addr().unwrap();
        tokio::spawn(async move {
            let listener = tokio::net::TcpListener::from_std(std_listener).unwrap();
            let (stream, _) = listener.accept().await.unwrap();
            let mut session = ImapSession::new(
                ctx,
                pool,
                ImapSessionConfig {
                    hostname: "imap.test".into(),
                    primary_domain: "test".into(),
                    jit_domain: None,
                    credential_policy: CredentialPolicy::default(),
                    turn: None,
                    iroh: None,
                    push_enabled: true,
                    starttls_config: None,
                },
            );
            let _ = session.handle_connection(stream).await;
        });
        tokio::time::sleep(Duration::from_millis(20)).await;
        addr
    }

    /// Read from `stream` until `needle` appears in the accumulated raw bytes (or timeout).
    async fn read_until(stream: &mut TcpStream, needle: &[u8]) -> Vec<u8> {
        let mut acc: Vec<u8> = Vec::new();
        let mut buf = [0u8; 8192];
        let _ = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let n = stream.read(&mut buf).await.unwrap_or(0);
                if n == 0 {
                    break;
                }
                acc.extend_from_slice(&buf[..n]);
                if acc.windows(needle.len()).any(|w| w == needle) {
                    break;
                }
            }
        })
        .await;
        acc
    }

    /// P10-UT05: FETCH BODY[] returns binary (non-UTF-8) bodies byte-for-byte.
    ///
    /// Simulates a PGP-encrypted / MIME binary payload (raw 0x00..0xFF bytes including invalid
    /// UTF-8 sequences). The pre-fix code routed the body through `str::from_utf8(..).unwrap_or("")`
    /// which silently truncated/garbled such payloads — the root cause of "images and videos do
    /// not load correctly". The literal length and the body bytes must both round-trip exactly.
    #[tokio::test]
    async fn p10_ut05_fetch_binary_body_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let pool = chatmail_db::init_memory_db().await.unwrap();
        let hash = hash_password("pw").unwrap();
        chatmail_db::passwords::create_user(&pool, "u@test", &hash)
            .await
            .unwrap();
        let ctx = Arc::new(AppState::new(dir.path(), pool.clone()));

        // A header block (ASCII) followed by a binary body with every byte value, including
        // sequences that are invalid UTF-8 (0xFF, 0xFE, lone continuation bytes, NUL).
        let mut body: Vec<u8> = b"From: u@test\r\nTo: u@test\r\nSubject: media\r\n\r\n".to_vec();
        body.extend((0u16..=255).map(|b| b as u8));
        body.extend_from_slice(&[0xFF, 0xFE, 0x80, 0x00, 0xC0, 0xAF]);
        write_blob(&ctx.mailbox_store, "u@test", "m1", &body)
            .await
            .unwrap();

        let addr = spawn_imap_server(pool, ctx).await;
        let mut stream = TcpStream::connect(addr).await.unwrap();
        let _ = read_until(&mut stream, b"IMAP4rev1 ready").await;

        stream.write_all(b"a001 LOGIN u@test pw\r\n").await.unwrap();
        let _ = read_until(&mut stream, b"a001 OK").await;
        stream.write_all(b"a002 SELECT INBOX\r\n").await.unwrap();
        let _ = read_until(&mut stream, b"a002 OK").await;
        stream.write_all(b"a003 FETCH 1 BODY[]\r\n").await.unwrap();
        let resp = read_until(&mut stream, b"a003 OK FETCH completed\r\n").await;

        // Parse the literal: `... BODY[] {<len>}\r\n<bytes>)\r\n`.
        let marker = b"BODY[] {";
        let mpos = resp
            .windows(marker.len())
            .position(|w| w == marker)
            .expect("BODY[] literal marker present");
        let after = &resp[mpos + marker.len()..];
        let brace = after.iter().position(|&b| b == b'}').unwrap();
        let len: usize = std::str::from_utf8(&after[..brace])
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(len, body.len(), "advertised literal length matches body");
        // Skip past `}\r\n` to the literal bytes.
        let literal_start = mpos + marker.len() + brace + 1 + 2;
        let literal = &resp[literal_start..literal_start + len];
        assert_eq!(literal, &body[..], "binary body round-trips byte-for-byte");
    }

    /// P10-UT07: `BODY[]<offset.count>` partial fetch returns only the requested window with the
    /// origin octet echoed, and the bytes match the corresponding slice of the full body.
    #[tokio::test]
    async fn p10_ut07_partial_body_fetch_returns_window() {
        let dir = tempfile::tempdir().unwrap();
        let pool = chatmail_db::init_memory_db().await.unwrap();
        let hash = hash_password("pw").unwrap();
        chatmail_db::passwords::create_user(&pool, "u@test", &hash)
            .await
            .unwrap();
        let ctx = Arc::new(AppState::new(dir.path(), pool.clone()));

        let mut body: Vec<u8> = b"From: u@test\r\n\r\n".to_vec();
        body.extend((0u16..=255).map(|b| b as u8));
        write_blob(&ctx.mailbox_store, "u@test", "m1", &body)
            .await
            .unwrap();

        let addr = spawn_imap_server(pool, ctx).await;
        let mut stream = TcpStream::connect(addr).await.unwrap();
        let _ = read_until(&mut stream, b"IMAP4rev1 ready").await;
        stream.write_all(b"a001 LOGIN u@test pw\r\n").await.unwrap();
        let _ = read_until(&mut stream, b"a001 OK").await;
        stream.write_all(b"a002 SELECT INBOX\r\n").await.unwrap();
        let _ = read_until(&mut stream, b"a002 OK").await;
        // Request 32 bytes starting at offset 16.
        stream
            .write_all(b"a003 FETCH 1 BODY[]<16.32>\r\n")
            .await
            .unwrap();
        let resp = read_until(&mut stream, b"a003 OK FETCH completed\r\n").await;

        let marker = b"BODY[]<16> {";
        let mpos = resp
            .windows(marker.len())
            .position(|w| w == marker)
            .expect("partial section spec echoes origin octet");
        let after = &resp[mpos + marker.len()..];
        let brace = after.iter().position(|&b| b == b'}').unwrap();
        let len: usize = std::str::from_utf8(&after[..brace])
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(len, 32, "partial literal length matches requested count");
        let literal_start = mpos + marker.len() + brace + 1 + 2;
        let literal = &resp[literal_start..literal_start + len];
        assert_eq!(literal, &body[16..48], "partial bytes match the body slice");
    }

    async fn imap_dialog_with_discovery(
        pool: DbPool,
        ctx: Arc<AppState>,
        turn: Option<TurnDiscovery>,
        iroh: Option<IrohDiscovery>,
        script: &[&str],
    ) -> String {
        ctx.auth.hydrate(&pool).await.unwrap();

        let std_listener = StdListener::bind("127.0.0.1:0").unwrap();
        std_listener.set_nonblocking(true).unwrap();
        let addr = std_listener.local_addr().unwrap();

        let pool_bg = pool.clone();
        let ctx_bg = Arc::clone(&ctx);
        tokio::spawn(async move {
            let listener = tokio::net::TcpListener::from_std(std_listener).unwrap();
            let (stream, _) = listener.accept().await.unwrap();
            let mut session = ImapSession::new(
                ctx_bg,
                pool_bg,
                ImapSessionConfig {
                    hostname: "imap.test".into(),
                    primary_domain: "test".into(),
                    jit_domain: None,
                    credential_policy: CredentialPolicy::default(),
                    turn,
                    iroh,
                    push_enabled: true,
                    starttls_config: None,
                },
            );
            let _ = session.handle_connection(stream).await;
        });

        tokio::time::sleep(Duration::from_millis(20)).await;
        let mut stream = TcpStream::connect(addr).await.unwrap();
        let mut transcript = String::new();
        let mut buf = [0u8; 8192];

        for line in script {
            if let Some(payload) = line.strip_prefix("LITERAL:") {
                stream.write_all(payload.as_bytes()).await.unwrap();
                stream.write_all(b"\r\n").await.unwrap();
            } else {
                stream.write_all(line.as_bytes()).await.unwrap();
                stream.write_all(b"\r\n").await.unwrap();
            }
            tokio::time::sleep(Duration::from_millis(40)).await;
            let chunk = tokio::time::timeout(Duration::from_secs(2), async {
                let mut acc = String::new();
                loop {
                    let n = stream.read(&mut buf).await.unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    acc.push_str(&String::from_utf8_lossy(&buf[..n]));
                    if acc.contains("completed")
                        || acc.contains("NO [ENCRYPTED]")
                        || acc.contains("NO [TOOBIG]")
                        || acc.contains("BAD ")
                    {
                        break;
                    }
                }
                acc
            })
            .await
            .unwrap_or_default();
            transcript.push_str(&chunk);
        }
        transcript
    }

    /// TDD `03-imap-server.md`: CAPABILITY → LOGIN → SELECT → FETCH.
    #[tokio::test]
    async fn p5_imap_login_select_fetch_flow() {
        let dir = tempfile::tempdir().unwrap();
        let pool = chatmail_db::init_memory_db().await.unwrap();
        let hash = hash_password("pw").unwrap();
        chatmail_db::passwords::create_user(&pool, "u@test", &hash)
            .await
            .unwrap();

        let ctx = Arc::new(AppState::new(dir.path(), pool.clone()));
        let body = b"From: u@test\r\nTo: u@test\r\nContent-Type: multipart/encrypted; boundary=b\r\n\r\n--b\r\nContent-Type: application/pgp-encrypted\r\n\r\nv\r\n--b--\r\n";
        write_blob(&ctx.mailbox_store, "u@test", "m1", body)
            .await
            .unwrap();

        let t = imap_dialog(
            pool,
            ctx,
            &[
                "a001 CAPABILITY",
                "a002 LOGIN u@test pw",
                "a003 SELECT INBOX",
                "a003b STATUS INBOX (UIDNEXT MESSAGES)",
                "a004 UID FETCH 1 (UID INTERNALDATE RFC822.SIZE BODY.PEEK[HEADER.FIELDS (MESSAGE-ID FROM)])",
                "a005 LOGOUT",
            ],
        )
        .await;
        assert!(t.contains("XCHATMAIL"), "caps: {t}");
        assert!(t.contains("a002 OK LOGIN"), "login: {t}");
        assert!(t.contains("UIDNEXT"), "select: {t}");
        assert!(t.contains("EXISTS"), "select: {t}");
        assert!(t.contains("STATUS"), "status: {t}");
        assert!(t.contains("RFC822.SIZE"), "fetch: {t}");
        assert!(t.contains("MESSAGE-ID"), "header fetch: {t}");
    }

    /// P5-UT03 over TCP: APPEND plaintext → NO [ENCRYPTED] (TDD `03-imap-server.md`).
    #[tokio::test]
    async fn p5_imap_append_plaintext_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let pool = chatmail_db::init_memory_db().await.unwrap();
        let hash = hash_password("pw").unwrap();
        chatmail_db::passwords::create_user(&pool, "u@test", &hash)
            .await
            .unwrap();
        let ctx = Arc::new(AppState::new(dir.path(), pool.clone()));
        let plain = b"From: u@test\r\nSubject: x\r\nContent-Type: text/plain\r\n\r\nn";
        let t = imap_dialog(
            pool,
            ctx,
            &[
                "b001 LOGIN u@test pw",
                &format!("b002 APPEND INBOX {{{}}}", plain.len()),
                &format!("LITERAL:{}", String::from_utf8_lossy(plain)),
            ],
        )
        .await;
        assert!(t.contains("NO [ENCRYPTED]"), "append reject: {t}");
    }

    #[tokio::test]
    async fn p5_imap_append_literal_toobig() {
        use chatmail_config::AppConfig;
        use chatmail_types::MESSAGE_FILE_TOO_BIG;

        let dir = tempfile::tempdir().unwrap();
        let pool = chatmail_db::init_memory_db().await.unwrap();
        let hash = hash_password("pw").unwrap();
        chatmail_db::passwords::create_user(&pool, "u@test", &hash)
            .await
            .unwrap();
        let mut cfg = AppConfig::default();
        cfg.appendlimit = Some("2K".into());
        cfg.max_message_size = Some("2K".into());
        let ctx = Arc::new(AppState::with_quota_and_message_limit(
            dir.path(),
            chatmail_config::DEFAULT_QUOTA_BYTES,
            &cfg,
            pool.clone(),
        ));
        ctx.hydrate(&pool, &cfg).await.unwrap();

        let payload = vec![b'x'; 3000];
        let t = imap_dialog(
            pool,
            ctx,
            &[
                "c001 LOGIN u@test pw",
                &format!("c002 APPEND INBOX {{{}}}", payload.len()),
                &format!("LITERAL:{}", String::from_utf8_lossy(&payload)),
            ],
        )
        .await;
        assert!(t.contains("NO [TOOBIG]"), "append toobig: {t}");
        assert!(t.contains(MESSAGE_FILE_TOO_BIG), "append toobig: {t}");
    }

    fn large_pgp_mime_body(min_bytes: usize) -> Vec<u8> {
        let header = b"From: u@test\r\nTo: u@test\r\nContent-Type: multipart/encrypted; boundary=b\r\n\r\n--b\r\nContent-Type: application/pgp-encrypted\r\n\r\nv\r\n--b--\r\n";
        let mut body = header.to_vec();
        if body.len() < min_bytes {
            body.extend(std::iter::repeat_n(b'X', min_bytes - body.len()));
        }
        body
    }

    /// P11-UT16: APPEND literals ≥ stream_threshold use the streaming tmp path end-to-end.
    #[tokio::test]
    async fn p11_imap_large_append_streaming_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let pool = chatmail_db::init_memory_db().await.unwrap();
        let hash = hash_password("pw").unwrap();
        chatmail_db::passwords::create_user(&pool, "u@test", &hash)
            .await
            .unwrap();
        let ctx = Arc::new(AppState::new(dir.path(), pool.clone()));
        let body = large_pgp_mime_body(70_000);
        assert!(
            body.len() >= ctx.mailbox_store.policy().stream_threshold,
            "test body must exceed streaming threshold"
        );

        let t = imap_dialog(
            pool,
            ctx.clone(),
            &[
                "d001 LOGIN u@test pw",
                &format!("d002 APPEND INBOX {{{}}}", body.len()),
                &format!("LITERAL:{}", String::from_utf8_lossy(&body)),
                "d003 SELECT INBOX",
                "d004 UID FETCH 1 (UID RFC822.SIZE)",
            ],
        )
        .await;
        assert!(t.contains("OK APPEND"), "large append: {t}");
        assert!(t.contains("* 1 EXISTS"), "select after large append: {t}");
        assert!(
            t.contains(&format!("RFC822.SIZE {}", body.len())),
            "size: {t}"
        );

        let new_dir = ctx.mailbox_store.maildir_for_user("u@test").new;
        let entries: Vec<_> = std::fs::read_dir(&new_dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        assert_eq!(entries.len(), 1, "exactly one maildir entry");
        assert_eq!(std::fs::read(&entries[0]).unwrap(), body);

        // First distinct write under CAS defers canonical blob population (Dovecot-style
        // fast path); the message lives only in maildir until a dedup hit ingests CAS.
        assert!(
            !dir.path().join("blobs").exists(),
            "first distinct streaming append must not populate CAS canonical yet"
        );
    }

    /// P11-UT17: streaming APPEND rejects plaintext and leaves no `new/` or `tmp/` artifacts.
    #[tokio::test]
    async fn p11_streaming_append_rejects_plaintext_without_artifacts() {
        use chatmail_storage::StoragePolicy;
        use std::io::Cursor;
        use tokio::io::BufReader;

        let dir = tempfile::tempdir().unwrap();
        let pool = chatmail_db::init_memory_db().await.unwrap();
        let policy = StoragePolicy {
            stream_threshold: 1024,
            ..StoragePolicy::default()
        };
        let ctx = Arc::new(AppState::with_quota_and_message_limit(
            dir.path(),
            chatmail_config::DEFAULT_QUOTA_BYTES,
            &chatmail_config::AppConfig::default(),
            pool.clone(),
        ));
        // Replace store with lower threshold for this test.
        let store = chatmail_storage::MailboxStore::with_policy(dir.path(), policy);
        let ctx = Arc::new(chatmail_state::AppState {
            mailbox_store: Arc::new(store),
            ..(*ctx).clone()
        });

        let plain_header = b"From: u@test\r\nSubject: x\r\nContent-Type: text/plain\r\n\r\n";
        let mut plain = plain_header.to_vec();
        plain.extend(std::iter::repeat_n(b'n', 2048 - plain.len()));

        let wire = format!("APPEND INBOX {{{}}}\r\n", plain.len());
        let mut payload = wire.into_bytes();
        payload.extend_from_slice(&plain);
        payload.extend_from_slice(b"\r\n");

        let mut session = ImapSession::new(
            ctx.clone(),
            pool,
            ImapSessionConfig {
                hostname: "imap.test".into(),
                primary_domain: "test".into(),
                jit_domain: None,
                credential_policy: CredentialPolicy::default(),
                turn: None,
                iroh: None,
                push_enabled: true,
                starttls_config: None,
            },
        );
        session.selected_mailbox = Some("INBOX".into());
        let mut reader = BufReader::new(Cursor::new(payload));
        let err = session
            .handle_append(
                &mut reader,
                "z001",
                &format!("INBOX {{{}}}", plain.len()),
                "u@test",
            )
            .await
            .unwrap_err();
        assert!(matches!(err, ChatmailError::EncryptionNeeded(_)));

        let paths = ctx.mailbox_store.maildir_for_user("u@test");
        assert!(
            std::fs::read_dir(&paths.new)
                .map(|mut d| d.next())
                .unwrap()
                .is_none(),
            "rejected append must not create new/ entry"
        );
        assert!(
            std::fs::read_dir(&paths.tmp)
                .map(|d| d.count())
                .unwrap_or(0)
                == 0,
            "rejected append must clean tmp/"
        );
    }

    /// P6 integration: IDLE receives unsolicited EXISTS/RECENT when mail is delivered.
    #[tokio::test]
    async fn p6_imap_idle_unsolicited_exists() {
        let dir = tempfile::tempdir().unwrap();
        let pool = chatmail_db::init_memory_db().await.unwrap();
        let hash = hash_password("pw").unwrap();
        chatmail_db::passwords::create_user(&pool, "u@test", &hash)
            .await
            .unwrap();

        let ctx = Arc::new(AppState::new(dir.path(), pool.clone()));
        ctx.auth.hydrate(&pool).await.unwrap();
        let body = b"From: u@test\r\nTo: u@test\r\nContent-Type: multipart/encrypted; boundary=b\r\n\r\n--b\r\nContent-Type: application/pgp-encrypted\r\n\r\nv\r\n--b--\r\n";
        write_blob(&ctx.mailbox_store, "u@test", "m1", body)
            .await
            .unwrap();

        let std_listener = StdListener::bind("127.0.0.1:0").unwrap();
        std_listener.set_nonblocking(true).unwrap();
        let addr = std_listener.local_addr().unwrap();

        let pool_bg = pool.clone();
        let ctx_bg = Arc::clone(&ctx);
        tokio::spawn(async move {
            let listener = tokio::net::TcpListener::from_std(std_listener).unwrap();
            let (stream, _) = listener.accept().await.unwrap();
            let mut session = ImapSession::new(
                ctx_bg,
                pool_bg,
                ImapSessionConfig {
                    hostname: "imap.test".into(),
                    primary_domain: "test".into(),
                    jit_domain: None,
                    credential_policy: CredentialPolicy::default(),
                    turn: None,
                    iroh: None,
                    push_enabled: true,
                    starttls_config: None,
                },
            );
            let _ = session.handle_connection(stream).await;
        });

        tokio::time::sleep(Duration::from_millis(20)).await;
        let mut stream = TcpStream::connect(addr).await.unwrap();
        let mut buf = [0u8; 8192];

        async fn read_until(sub: &str, stream: &mut TcpStream, buf: &mut [u8]) -> String {
            let mut acc = String::new();
            for _ in 0..50 {
                let n = stream.read(buf).await.unwrap_or(0);
                if n > 0 {
                    acc.push_str(&String::from_utf8_lossy(&buf[..n]));
                    if acc.contains(sub) {
                        return acc;
                    }
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            acc
        }

        async fn sendln(stream: &mut TcpStream, line: &str) {
            stream.write_all(line.as_bytes()).await.unwrap();
            stream.write_all(b"\r\n").await.unwrap();
        }

        read_until("IMAP4rev1 ready", &mut stream, &mut buf).await;
        sendln(&mut stream, "c001 LOGIN u@test pw").await;
        read_until("c001 OK", &mut stream, &mut buf).await;
        sendln(&mut stream, "c002 SELECT INBOX").await;
        read_until("c002 OK", &mut stream, &mut buf).await;
        sendln(&mut stream, "c003 IDLE").await;
        read_until("+ idling", &mut stream, &mut buf).await;

        write_blob(&ctx.mailbox_store, "u@test", "m2", body)
            .await
            .unwrap();
        ctx.events.notify_new_message("u@test", "m2");

        let idle_push = read_until("EXISTS", &mut stream, &mut buf).await;
        assert!(
            idle_push.contains("* 2 EXISTS"),
            "expected EXISTS after delivery, got: {idle_push}"
        );
        assert!(idle_push.contains("RECENT"), "expected RECENT: {idle_push}");

        sendln(&mut stream, "DONE").await;
        let end = read_until("c003 OK", &mut stream, &mut buf).await;
        assert!(end.contains("IDLE terminated"), "idle end: {end}");
    }

    /// Regression: a message delivered in the SELECT → IDLE gap (event fired with no subscriber)
    /// must surface on IDLE entry via catch-up, without waiting for a *second* delivery.
    ///
    /// This is the lost-wakeup that made 60-person group bursts drop trailing messages and
    /// inflate tail latency: receivers cycle IDLE→FETCH→STORE→re-IDLE, and the next burst
    /// message's notify is lost because the session is briefly unsubscribed.
    #[tokio::test]
    async fn p6_imap_idle_catches_up_on_entry_after_missed_notify() {
        let dir = tempfile::tempdir().unwrap();
        let pool = chatmail_db::init_memory_db().await.unwrap();
        let hash = hash_password("pw").unwrap();
        chatmail_db::passwords::create_user(&pool, "u@test", &hash)
            .await
            .unwrap();

        let ctx = Arc::new(AppState::new(dir.path(), pool.clone()));
        ctx.auth.hydrate(&pool).await.unwrap();
        let body = b"From: u@test\r\nTo: u@test\r\nContent-Type: multipart/encrypted; boundary=b\r\n\r\n--b\r\nContent-Type: application/pgp-encrypted\r\n\r\nv\r\n--b--\r\n";
        write_blob(&ctx.mailbox_store, "u@test", "m1", body)
            .await
            .unwrap();

        let std_listener = StdListener::bind("127.0.0.1:0").unwrap();
        std_listener.set_nonblocking(true).unwrap();
        let addr = std_listener.local_addr().unwrap();

        let pool_bg = pool.clone();
        let ctx_bg = Arc::clone(&ctx);
        tokio::spawn(async move {
            let listener = tokio::net::TcpListener::from_std(std_listener).unwrap();
            let (stream, _) = listener.accept().await.unwrap();
            let mut session = ImapSession::new(
                ctx_bg,
                pool_bg,
                ImapSessionConfig {
                    hostname: "imap.test".into(),
                    primary_domain: "test".into(),
                    jit_domain: None,
                    credential_policy: CredentialPolicy::default(),
                    turn: None,
                    iroh: None,
                    push_enabled: true,
                    starttls_config: None,
                },
            );
            let _ = session.handle_connection(stream).await;
        });

        tokio::time::sleep(Duration::from_millis(20)).await;
        let mut stream = TcpStream::connect(addr).await.unwrap();
        let mut buf = [0u8; 8192];

        async fn read_until(sub: &str, stream: &mut TcpStream, buf: &mut [u8]) -> String {
            let mut acc = String::new();
            for _ in 0..50 {
                let n = stream.read(buf).await.unwrap_or(0);
                if n > 0 {
                    acc.push_str(&String::from_utf8_lossy(&buf[..n]));
                    if acc.contains(sub) {
                        return acc;
                    }
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            acc
        }

        async fn sendln(stream: &mut TcpStream, line: &str) {
            stream.write_all(line.as_bytes()).await.unwrap();
            stream.write_all(b"\r\n").await.unwrap();
        }

        read_until("IMAP4rev1 ready", &mut stream, &mut buf).await;
        sendln(&mut stream, "c001 LOGIN u@test pw").await;
        read_until("c001 OK", &mut stream, &mut buf).await;
        sendln(&mut stream, "c002 SELECT INBOX").await;
        read_until("c002 OK", &mut stream, &mut buf).await;

        // Delivery + notify happen here, while the session is NOT in IDLE (no subscriber).
        // The event is therefore lost; only the on-disk state grows.
        write_blob(&ctx.mailbox_store, "u@test", "m2", body)
            .await
            .unwrap();
        ctx.events.notify_new_message("u@test", "m2");
        tokio::time::sleep(Duration::from_millis(30)).await;

        // No further notify is sent. IDLE must still report the new message via catch-up.
        sendln(&mut stream, "c003 IDLE").await;
        let idle_push = read_until("EXISTS", &mut stream, &mut buf).await;
        assert!(
            idle_push.contains("* 2 EXISTS"),
            "IDLE must catch up on the missed delivery, got: {idle_push}"
        );

        sendln(&mut stream, "DONE").await;
        let end = read_until("c003 OK", &mut stream, &mut buf).await;
        assert!(end.contains("IDLE terminated"), "idle end: {end}");
    }

    /// Regression for the 60-recipient group tail-latency/loss: a message that arrives during the
    /// EXISTS → FETCH → STORE(delete) window must still get an unsolicited EXISTS on the *next*
    /// IDLE. The old code used `messages.len()` as the IDLE baseline, but `handle_fetch` reloads
    /// `messages` from disk, so the freshly-arrived message was absorbed into the baseline and the
    /// next IDLE saw "no growth" — the client only learned about it on Delta Chat's ~75s refresh.
    #[tokio::test]
    async fn p6_imap_idle_announces_message_arriving_during_fetch_window() {
        let dir = tempfile::tempdir().unwrap();
        let pool = chatmail_db::init_memory_db().await.unwrap();
        let hash = hash_password("pw").unwrap();
        chatmail_db::passwords::create_user(&pool, "u@test", &hash)
            .await
            .unwrap();
        let ctx = Arc::new(AppState::new(dir.path(), pool.clone()));
        ctx.auth.hydrate(&pool).await.unwrap();
        let body = b"From: u@test\r\nTo: u@test\r\nContent-Type: multipart/encrypted; boundary=b\r\n\r\n--b\r\nContent-Type: application/pgp-encrypted\r\n\r\nv\r\n--b--\r\n";
        // First message already in the mailbox.
        write_blob(&ctx.mailbox_store, "u@test", "m1", body)
            .await
            .unwrap();

        let std_listener = StdListener::bind("127.0.0.1:0").unwrap();
        std_listener.set_nonblocking(true).unwrap();
        let addr = std_listener.local_addr().unwrap();

        let pool_bg = pool.clone();
        let ctx_bg = Arc::clone(&ctx);
        tokio::spawn(async move {
            let listener = tokio::net::TcpListener::from_std(std_listener).unwrap();
            let (stream, _) = listener.accept().await.unwrap();
            let mut session = ImapSession::new(
                ctx_bg,
                pool_bg,
                ImapSessionConfig {
                    hostname: "imap.test".into(),
                    primary_domain: "test".into(),
                    jit_domain: None,
                    credential_policy: CredentialPolicy::default(),
                    turn: None,
                    iroh: None,
                    push_enabled: true,
                    starttls_config: None,
                },
            );
            let _ = session.handle_connection(stream).await;
        });

        tokio::time::sleep(Duration::from_millis(20)).await;
        let mut stream = TcpStream::connect(addr).await.unwrap();
        let mut buf = [0u8; 8192];

        async fn read_until(sub: &str, stream: &mut TcpStream, buf: &mut [u8]) -> String {
            let mut acc = String::new();
            for _ in 0..50 {
                let n = stream.read(buf).await.unwrap_or(0);
                if n > 0 {
                    acc.push_str(&String::from_utf8_lossy(&buf[..n]));
                    if acc.contains(sub) {
                        return acc;
                    }
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            acc
        }
        async fn sendln(stream: &mut TcpStream, line: &str) {
            stream.write_all(line.as_bytes()).await.unwrap();
            stream.write_all(b"\r\n").await.unwrap();
        }

        read_until("IMAP4rev1 ready", &mut stream, &mut buf).await;
        sendln(&mut stream, "c001 LOGIN u@test pw").await;
        read_until("c001 OK", &mut stream, &mut buf).await;
        sendln(&mut stream, "c002 SELECT INBOX").await;
        read_until("c002 OK", &mut stream, &mut buf).await;

        // FETCH m1 (uid 1) — this reloads `messages` from disk.
        sendln(&mut stream, "c003 UID FETCH 1 (BODY[])").await;
        read_until("c003 OK", &mut stream, &mut buf).await;

        // m2 arrives *now*, during the FETCH→STORE window (notify is lost: no subscriber).
        write_blob(&ctx.mailbox_store, "u@test", "m2", body)
            .await
            .unwrap();
        ctx.events.notify_new_message("u@test", "m2");

        // Client deletes m1, which it already knew about.
        sendln(&mut stream, "c004 UID STORE 1 +FLAGS (\\Deleted)").await;
        read_until("c004 OK", &mut stream, &mut buf).await;

        // Next IDLE must announce m2 immediately via catch-up — no second delivery needed.
        sendln(&mut stream, "c005 IDLE").await;
        let idle_push = read_until("EXISTS", &mut stream, &mut buf).await;
        assert!(
            idle_push.contains("EXISTS"),
            "IDLE must announce the message that arrived during the FETCH/STORE window, got: {idle_push}"
        );

        sendln(&mut stream, "DONE").await;
        let end = read_until("c005 OK", &mut stream, &mut buf).await;
        assert!(end.contains("IDLE terminated"), "idle end: {end}");
    }

    /// Delta Chat `configure_mvbox`: EXAMINE → CLOSE → SELECT must not return BAD.
    #[tokio::test]
    async fn p6_imap_configure_mvbox_examine_close_select() {
        let dir = tempfile::tempdir().unwrap();
        let pool = chatmail_db::init_memory_db().await.unwrap();
        let hash = hash_password("pw").unwrap();
        chatmail_db::passwords::create_user(&pool, "u@test", &hash)
            .await
            .unwrap();
        let ctx = Arc::new(AppState::new(dir.path(), pool.clone()));

        let turn = TurnDiscovery {
            server: "turn.test".into(),
            port: 3478,
            secret: "s".into(),
            ttl_secs: 60,
            turn_test_relay_only: false,
        };
        let t = imap_dialog_with_discovery(
            pool,
            ctx,
            Some(turn),
            None,
            &[
                "d001 LOGIN u@test pw",
                "d002 EXAMINE DeltaChat",
                "d003 CLOSE",
                "d004 SELECT DeltaChat",
                "d005 GETMETADATA \"\" (/shared/comment /shared/vendor/deltachat/turn)",
                "d006 LOGOUT",
            ],
        )
        .await;
        assert!(t.contains("d002 OK"), "EXAMINE DeltaChat: {t}");
        assert!(t.contains("d003 OK CLOSE"), "CLOSE: {t}");
        assert!(t.contains("d004 OK [SELECT]"), "SELECT DeltaChat: {t}");
        assert!(t.contains("/shared/vendor/deltachat/turn"), "metadata: {t}");
    }

    fn loopback_tls_configs() -> (Arc<ServerConfig>, Arc<rustls::ClientConfig>) {
        use rcgen::generate_simple_self_signed;
        use rustls::pki_types::{CertificateDer, PrivateKeyDer};
        use rustls::{ClientConfig, RootCertStore};

        let rc = generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let cert = CertificateDer::from(rc.cert.der().to_vec());
        let key = PrivateKeyDer::Pkcs8(rc.key_pair.serialize_der().into());
        let server = Arc::new(
            ServerConfig::builder()
                .with_no_client_auth()
                .with_single_cert(vec![cert.clone()], key)
                .unwrap(),
        );
        let mut roots = RootCertStore::empty();
        roots.add(cert).unwrap();
        let client = Arc::new(
            ClientConfig::builder()
                .with_root_certificates(roots)
                .with_no_client_auth(),
        );
        (server, client)
    }

    async fn imap_dialog_starttls(script: &[&str]) -> String {
        let dir = tempfile::tempdir().unwrap();
        let pool = chatmail_db::init_memory_db().await.unwrap();
        let ctx = Arc::new(AppState::new(dir.path(), pool.clone()));
        ctx.auth.hydrate(&pool).await.unwrap();

        let (tls_server, _) = loopback_tls_configs();
        let std_listener = StdListener::bind("127.0.0.1:0").unwrap();
        std_listener.set_nonblocking(true).unwrap();
        let addr = std_listener.local_addr().unwrap();

        let pool_bg = pool.clone();
        let ctx_bg = Arc::clone(&ctx);
        tokio::spawn(async move {
            let listener = tokio::net::TcpListener::from_std(std_listener).unwrap();
            let (stream, _) = listener.accept().await.unwrap();
            let mut session = ImapSession::new(
                ctx_bg,
                pool_bg,
                ImapSessionConfig {
                    hostname: "imap.test".into(),
                    primary_domain: "test".into(),
                    jit_domain: None,
                    credential_policy: CredentialPolicy::default(),
                    turn: None,
                    iroh: None,
                    push_enabled: true,
                    starttls_config: Some(tls_server),
                },
            );
            let _ = session.handle_connection(stream).await;
        });

        tokio::time::sleep(Duration::from_millis(20)).await;
        let mut stream = TcpStream::connect(addr).await.unwrap();
        let mut transcript = String::new();
        let mut buf = [0u8; 8192];

        for line in script {
            stream.write_all(line.as_bytes()).await.unwrap();
            stream.write_all(b"\r\n").await.unwrap();
            tokio::time::sleep(Duration::from_millis(40)).await;
            let chunk = tokio::time::timeout(Duration::from_secs(2), async {
                let mut acc = String::new();
                loop {
                    let n = stream.read(&mut buf).await.unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    acc.push_str(&String::from_utf8_lossy(&buf[..n]));
                    if acc.contains("completed")
                        || acc.contains("PRIVACYREQUIRED")
                        || acc.contains("Begin TLS")
                        || acc.contains("BAD ")
                    {
                        break;
                    }
                }
                acc
            })
            .await
            .unwrap_or_default();
            transcript.push_str(&chunk);
        }
        transcript
    }

    /// RFC 2595: CAPABILITY advertises STARTTLS; LOGIN rejected until TLS upgrade.
    #[tokio::test]
    async fn imap_starttls_capability_and_login_gate() {
        let t = imap_dialog_starttls(&[
            "a001 CAPABILITY",
            "a002 LOGIN u@test secret",
            "a003 STARTTLS",
        ])
        .await;
        assert!(t.contains("STARTTLS"), "CAPABILITY: {t}");
        assert!(t.contains("NO [PRIVACYREQUIRED]"), "LOGIN before TLS: {t}");
        assert!(t.contains("OK Begin TLS negotiation"), "STARTTLS: {t}");
    }

    /// RFC 2595: full STARTTLS upgrade then LOGIN succeeds on encrypted stream.
    #[tokio::test]
    async fn imap_starttls_upgrade_then_login() {
        use rustls::pki_types::ServerName;
        use tokio_rustls::TlsConnector;

        let dir = tempfile::tempdir().unwrap();
        let pool = chatmail_db::init_memory_db().await.unwrap();
        let hash = hash_password("secret").unwrap();
        chatmail_db::passwords::create_user(&pool, "u@test", &hash)
            .await
            .unwrap();
        let ctx = Arc::new(AppState::new(dir.path(), pool.clone()));
        ctx.auth.hydrate(&pool).await.unwrap();

        let (tls_server, tls_client) = loopback_tls_configs();
        let std_listener = StdListener::bind("127.0.0.1:0").unwrap();
        std_listener.set_nonblocking(true).unwrap();
        let addr = std_listener.local_addr().unwrap();

        let pool_bg = pool.clone();
        let ctx_bg = Arc::clone(&ctx);
        tokio::spawn(async move {
            let listener = tokio::net::TcpListener::from_std(std_listener).unwrap();
            let (stream, _) = listener.accept().await.unwrap();
            let mut session = ImapSession::new(
                ctx_bg,
                pool_bg,
                ImapSessionConfig {
                    hostname: "imap.test".into(),
                    primary_domain: "test".into(),
                    jit_domain: None,
                    credential_policy: CredentialPolicy::default(),
                    turn: None,
                    iroh: None,
                    push_enabled: true,
                    starttls_config: Some(tls_server),
                },
            );
            let _ = session.handle_connection(stream).await;
        });

        tokio::time::sleep(Duration::from_millis(20)).await;
        let mut stream = TcpStream::connect(addr).await.unwrap();
        let mut buf = [0u8; 4096];

        let greeting = tokio::time::timeout(Duration::from_secs(3), async {
            let mut acc = String::new();
            loop {
                let n = stream.read(&mut buf).await.unwrap_or(0);
                if n == 0 {
                    break;
                }
                acc.push_str(&String::from_utf8_lossy(&buf[..n]));
                if acc.contains("IMAP4rev1 ready") {
                    break;
                }
            }
            acc
        })
        .await
        .unwrap_or_default();
        assert!(greeting.contains("IMAP4rev1 ready"), "greeting: {greeting}");

        stream.write_all(b"a001 STARTTLS\r\n").await.unwrap();
        let starttls = tokio::time::timeout(Duration::from_secs(3), async {
            let mut acc = String::new();
            loop {
                let n = stream.read(&mut buf).await.unwrap_or(0);
                if n == 0 {
                    break;
                }
                acc.push_str(&String::from_utf8_lossy(&buf[..n]));
                if acc.contains("Begin TLS negotiation") {
                    break;
                }
            }
            acc
        })
        .await
        .unwrap_or_default();
        assert!(
            starttls.contains("OK Begin TLS negotiation"),
            "STARTTLS: {starttls}"
        );

        let connector = TlsConnector::from(tls_client);
        let server_name = ServerName::try_from("localhost").unwrap();
        let mut tls = connector.connect(server_name, stream).await.unwrap();

        tls.write_all(b"a002 LOGIN u@test secret\r\n")
            .await
            .unwrap();
        let login = tokio::time::timeout(Duration::from_secs(3), async {
            let mut acc = String::new();
            loop {
                let n = tls.read(&mut buf).await.unwrap_or(0);
                if n == 0 {
                    break;
                }
                acc.push_str(&String::from_utf8_lossy(&buf[..n]));
                if acc.contains("LOGIN completed") || acc.contains("BAD ") {
                    break;
                }
            }
            acc
        })
        .await
        .unwrap_or_default();
        assert!(login.contains("a002 OK LOGIN completed"), "login: {login}");
    }

    /// RFC 8314: implicit TLS on :993 must emit untagged OK before client commands.
    #[tokio::test]
    async fn imap_implicit_tls_sends_greeting() {
        use rustls::pki_types::ServerName;
        use tokio_rustls::TlsAcceptor;
        use tokio_rustls::TlsConnector;

        let dir = tempfile::tempdir().unwrap();
        let pool = chatmail_db::init_memory_db().await.unwrap();
        let ctx = Arc::new(AppState::new(dir.path(), pool.clone()));
        ctx.auth.hydrate(&pool).await.unwrap();

        let (tls_server, tls_client) = loopback_tls_configs();
        let std_listener = StdListener::bind("127.0.0.1:0").unwrap();
        std_listener.set_nonblocking(true).unwrap();
        let addr = std_listener.local_addr().unwrap();
        let acceptor = TlsAcceptor::from(Arc::clone(&tls_server));

        let pool_bg = pool.clone();
        let ctx_bg = Arc::clone(&ctx);
        tokio::spawn(async move {
            let listener = tokio::net::TcpListener::from_std(std_listener).unwrap();
            let (stream, _) = listener.accept().await.unwrap();
            let tls = acceptor.accept(stream).await.unwrap();
            let mut session = ImapSession::new(
                ctx_bg,
                pool_bg,
                ImapSessionConfig {
                    hostname: "imap.test".into(),
                    primary_domain: "test".into(),
                    jit_domain: None,
                    credential_policy: CredentialPolicy::default(),
                    turn: None,
                    iroh: None,
                    push_enabled: true,
                    starttls_config: None,
                },
            );
            let _ = session.handle_tls_connection(tls).await;
        });

        tokio::time::sleep(Duration::from_millis(20)).await;
        let stream = TcpStream::connect(addr).await.unwrap();
        let connector = TlsConnector::from(tls_client);
        let server_name = ServerName::try_from("localhost").unwrap();
        let mut tls = connector.connect(server_name, stream).await.unwrap();
        let mut buf = [0u8; 4096];

        let greeting = tokio::time::timeout(Duration::from_secs(3), async {
            let mut acc = String::new();
            loop {
                let n = tls.read(&mut buf).await.unwrap_or(0);
                if n == 0 {
                    break;
                }
                acc.push_str(&String::from_utf8_lossy(&buf[..n]));
                if acc.contains("IMAP4rev1 ready") {
                    break;
                }
            }
            acc
        })
        .await
        .unwrap_or_default();
        assert!(
            greeting.contains("* OK imap.test IMAP4rev1 ready"),
            "greeting: {greeting}"
        );
    }
}
