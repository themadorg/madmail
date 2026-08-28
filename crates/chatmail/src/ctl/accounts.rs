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

//! `chatmail accounts` — direct DB account management (Madmail ctl parity).

use std::fs;
use std::path::Path;

use chatmail_auth::{hash_password, is_importable_hash, normalize_username, verify_password};
use chatmail_config::cli::AccountsCommand;
use chatmail_config::{build_dclogin_link, Args, DcloginMailSettings};
use chatmail_db::{
    account_info, blocklist, delete_quota_row, get_bool_setting, load_mail_port_overrides,
    passwords, settings_keys, DbPool, BULK_DELETE_REASON, CLI_BAN_REASON, CLI_DELETE_REASON,
};
use chatmail_storage::MailboxStore;
use chatmail_types::{ChatmailError, Result};
use getrandom::fill;
use serde::{Deserialize, Serialize};

use super::account_ops::{delete_account_full, is_internal_settings_key, provision_account};
use super::blocklist_cmd::print_ban_list;
use super::context::CtlContext;
use super::output::CtlOut;
use super::util::{confirm, read_password_stdin};

const ADMIN_USERNAME_LEN: usize = 12;
const ADMIN_PASSWORD_LEN: usize = 24;

#[derive(Serialize)]
struct CreateUserResult {
    dclogin: String,
}

#[derive(Serialize, Deserialize)]
struct ExportUser {
    username: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    password: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    hash: String,
}

pub async fn accounts(args: &Args, cmd: &AccountsCommand) -> Result<()> {
    let ctx = CtlContext::from_args(args)?;
    let pool = ctx.open_pool().await?;
    let mailbox = MailboxStore::new(&ctx.state_dir);
    let domain = registration_domain(&ctx);

    match cmd {
        AccountsCommand::Status => accounts_status(args, &ctx, &pool, &mailbox).await,
        AccountsCommand::Info { username } => {
            let u = ensure_email(username, &domain)?;
            accounts_info(args, &ctx, &pool, &mailbox, &u).await
        }
        AccountsCommand::Create { username, password } => {
            let u = ensure_email(username, &domain)?;
            let pw = match password {
                Some(p) => p.clone(),
                None => read_password_stdin()?,
            };
            accounts_create(args, &ctx, &pool, &mailbox, &u, &pw).await
        }
        AccountsCommand::Dclogin { username, password } => {
            let u = ensure_email(username, &domain)?;
            let pw = match password {
                Some(p) => p.clone(),
                None => read_password_stdin()?,
            };
            accounts_dclogin(args, &ctx, &pool, &u, &pw).await
        }
        AccountsCommand::CreateRandom { json_only } => {
            create_random_account(args, &ctx, &pool, &mailbox, *json_only).await
        }
        AccountsCommand::Delete { username, yes } => {
            let u = ensure_email(username, &domain)?;
            accounts_delete(args, &pool, &mailbox, &u, *yes, CLI_DELETE_REASON).await
        }
        AccountsCommand::Ban {
            username,
            reason,
            yes,
        } => {
            let u = ensure_email(username, &domain)?;
            let r = reason.as_deref().unwrap_or(CLI_BAN_REASON);
            accounts_delete(args, &pool, &mailbox, &u, *yes, r).await
        }
        AccountsCommand::Unban { username, yes } => {
            let u = ensure_email(username, &domain)?;
            accounts_unban(args, &pool, &u, *yes).await
        }
        AccountsCommand::BanList => {
            let out = CtlOut::from_args(args, "accounts ban-list");
            print_ban_list(&pool, &out).await
        }
        AccountsCommand::Export { output } => accounts_export(args, &pool, output.as_deref()).await,
        AccountsCommand::Import { file } => {
            accounts_import(args, &pool, &mailbox, &domain, file).await
        }
        AccountsCommand::DeleteAll { yes } => {
            accounts_delete_all(args, &pool, &mailbox, *yes).await
        }
    }
}

pub async fn ban_list(args: &Args) -> Result<()> {
    let ctx = CtlContext::from_args(args)?;
    let pool = ctx.open_pool().await?;
    let out = CtlOut::from_args(args, "ban-list");
    print_ban_list(&pool, &out).await
}

pub async fn create_user(args: &Args, json_only: bool) -> Result<()> {
    let ctx = CtlContext::from_args(args)?;
    let pool = ctx.open_pool().await?;
    let mailbox = MailboxStore::new(&ctx.state_dir);
    create_random_account(args, &ctx, &pool, &mailbox, json_only).await
}

fn registration_domain(ctx: &CtlContext) -> String {
    let host = ctx.config.hostname.as_deref().unwrap_or("127.0.0.1");
    ctx.config.effective_registration_domain(Some(host))
}

fn ensure_email(raw: &str, domain: &str) -> Result<String> {
    let t = raw.trim();
    if t.is_empty() {
        return Err(ChatmailError::config("username is required"));
    }
    if t.contains('@') {
        normalize_username(t)
    } else {
        normalize_username(&format!("{t}@{domain}"))
    }
}

async fn accounts_status(
    args: &Args,
    ctx: &CtlContext,
    pool: &DbPool,
    mailbox: &MailboxStore,
) -> Result<()> {
    let out = CtlOut::from_args(args, "accounts status");
    let users = passwords::list_users(pool).await?;
    let login_count = users
        .iter()
        .filter(|u| !is_internal_settings_key(u))
        .count();

    let reg_open = get_bool_setting(pool, settings_keys::REGISTRATION_OPEN, false).await?;
    let token_required =
        get_bool_setting(pool, settings_keys::REGISTRATION_TOKEN_REQUIRED, false).await?;
    let jit = get_bool_setting(pool, settings_keys::JIT_REGISTRATION_ENABLED, true).await?;

    let blocked = blocklist::list_blocked_users(pool).await?.len();
    let mail_root = ctx.state_dir.join("mail");
    let mail_count = if mail_root.is_dir() {
        fs::read_dir(&mail_root)?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .count()
    } else {
        0
    };

    if out.is_json() {
        return out.emit(serde_json::json!({
            "login_count": login_count,
            "registration_open": reg_open,
            "token_required": token_required,
            "jit_enabled": jit,
            "blocklisted": blocked,
            "mail_directories": mail_count,
            "state_dir": ctx.state_dir.display().to_string(),
            "database": ctx.db_path.display().to_string(),
        }));
    }

    out.line(format!("Login accounts: {login_count}"));
    out.line(format!(
        "Registration: {}",
        if reg_open { "open" } else { "closed" }
    ));
    out.line(format!(
        "Registration token required: {}",
        if token_required { "yes" } else { "no" }
    ));
    out.line(format!(
        "JIT registration: {}",
        if jit { "enabled" } else { "disabled" }
    ));
    out.line(format!("Blocklisted: {blocked}"));
    out.line(format!("Mail directories: {mail_count}"));
    out.line(format!("State directory: {}", ctx.state_dir.display()));
    out.line(format!("Database: {}", ctx.db_path.display()));
    let _ = mailbox;
    Ok(())
}

async fn accounts_info(
    args: &Args,
    ctx: &CtlContext,
    pool: &DbPool,
    mailbox: &MailboxStore,
    username: &str,
) -> Result<()> {
    let out = CtlOut::from_args(args, "accounts info");
    let hash = passwords::get_user_hash(pool, username).await?;
    let blocked = blocklist::is_blocked(pool, username).await?;
    let block_reason = if blocked {
        blocklist::list_blocked_users(pool)
            .await?
            .into_iter()
            .find(|(u, _, _)| u == username)
            .map(|(_, r, _)| r)
    } else {
        None
    };

    let info = account_info::list_account_quota_info(pool)
        .await?
        .get(username)
        .copied()
        .unwrap_or_default();

    let maildir = mailbox.maildir_for_user(username);
    let mail_exists = maildir.root.exists();

    if out.is_json() {
        return out.emit(serde_json::json!({
            "username": username,
            "credentials": hash.is_some(),
            "blocklisted": blocked,
            "block_reason": block_reason,
            "created_at": if info.created_at > 0 { Some(info.created_at) } else { None },
            "first_login_at": if info.first_login_at > 0 { Some(info.first_login_at) } else { None },
            "last_login_at": if info.last_login_at > 0 { Some(info.last_login_at) } else { None },
            "maildir_present": mail_exists,
            "maildir_path": if mail_exists { Some(maildir.root.display().to_string()) } else { None },
        }));
    }

    out.line(format!("Username: {username}"));
    out.line(format!(
        "Credentials: {}",
        if hash.is_some() { "present" } else { "missing" }
    ));
    if blocked {
        out.line(format!(
            "Blocklisted: yes ({})",
            block_reason.as_deref().unwrap_or("unknown")
        ));
    } else {
        out.line("Blocklisted: no");
    }
    if info.created_at > 0 {
        out.line(format!("Created at (unix): {}", info.created_at));
    }
    if info.first_login_at > 0 {
        out.line(format!("First login at (unix): {}", info.first_login_at));
    }
    if info.last_login_at > 0 {
        out.line(format!("Last login at (unix): {}", info.last_login_at));
    }
    out.line(format!(
        "Maildir: {}",
        if mail_exists { "present" } else { "missing" }
    ));
    if mail_exists {
        out.line(format!("Maildir path: {}", maildir.root.display()));
    }
    let _ = ctx;
    Ok(())
}

async fn accounts_create(
    args: &Args,
    ctx: &CtlContext,
    pool: &DbPool,
    mailbox: &MailboxStore,
    username: &str,
    password: &str,
) -> Result<()> {
    let out = CtlOut::from_args(args, "accounts create");
    if passwords::user_exists(pool, username).await? {
        return Err(ChatmailError::config(format!(
            "account already exists: {username}"
        )));
    }
    if blocklist::is_blocked(pool, username).await? {
        return Err(ChatmailError::config(format!(
            "username is blocklisted: {username}"
        )));
    }
    let hash = hash_password(password)?;
    provision_account(pool, mailbox, username, &hash).await?;
    let dclogin = dclogin_uri(ctx, pool, username, password).await?;
    print_dclogin(
        &out,
        username,
        &dclogin,
        Some(format!("Created account: {username}")),
    )
}

async fn accounts_dclogin(
    args: &Args,
    ctx: &CtlContext,
    pool: &DbPool,
    username: &str,
    password: &str,
) -> Result<()> {
    let out = CtlOut::from_args(args, "accounts dclogin");
    let Some(hash) = passwords::get_user_hash(pool, username).await? else {
        return Err(ChatmailError::config(format!(
            "no such account: {username}"
        )));
    };
    if !verify_password(password, &hash)? {
        return Err(ChatmailError::config(
            "password does not match this account",
        ));
    }
    let dclogin = dclogin_uri(ctx, pool, username, password).await?;
    print_dclogin(&out, username, &dclogin, None)
}

async fn dclogin_uri(
    ctx: &CtlContext,
    pool: &DbPool,
    username: &str,
    password: &str,
) -> Result<String> {
    let db_ports = load_mail_port_overrides(pool).await?;
    let mail = DcloginMailSettings::from_config_with_db(&ctx.config, None, &db_ports);
    Ok(build_dclogin_link(username, password, &mail))
}

fn print_dclogin(
    out: &CtlOut,
    username: &str,
    dclogin: &str,
    created: Option<String>,
) -> Result<()> {
    let data = serde_json::json!({
        "username": username,
        "email": username,
        "dclogin": dclogin,
    });
    if out.is_json() {
        return match created {
            Some(msg) => out.done_msg("", data, msg),
            None => out.emit(data),
        };
    }
    if let Some(msg) = created {
        out.line(msg);
        out.blank();
    }
    out.line("Delta Chat login URI (IMAP + SMTP host/port/TLS). Paste this; do not hand-build from username and password alone:");
    out.blank();
    out.line(dclogin);
    out.blank();
    Ok(())
}

async fn create_random_account(
    args: &Args,
    ctx: &CtlContext,
    pool: &DbPool,
    mailbox: &MailboxStore,
    json_only: bool,
) -> Result<()> {
    let out = CtlOut::from_args(args, "create-user");
    let domain = registration_domain(ctx);
    let db_ports = load_mail_port_overrides(pool).await?;
    let mail = DcloginMailSettings::from_config_with_db(&ctx.config, None, &db_ports);

    const MAX_ATTEMPTS: u32 = 5;
    for _ in 0..MAX_ATTEMPTS {
        let localpart = random_alnum(ADMIN_USERNAME_LEN)?;
        let password = random_password(ADMIN_PASSWORD_LEN)?;
        let email = format!("{localpart}@{domain}");

        if blocklist::is_blocked(pool, &email).await? {
            continue;
        }
        if passwords::user_exists(pool, &email).await? {
            continue;
        }

        let hash = hash_password(&password)?;
        match provision_account(pool, mailbox, &email, &hash).await {
            Ok(()) => {
                let dclogin = build_dclogin_link(&email, &password, &mail);
                if json_only && !args.json {
                    let body = serde_json::to_string_pretty(&CreateUserResult { dclogin })
                        .map_err(|e| ChatmailError::config(format!("JSON: {e}")))?;
                    println!("{body}");
                    return Ok(());
                }
                if args.json {
                    return out.emit(serde_json::json!({
                        "username": localpart,
                        "password": password,
                        "email": email,
                        "dclogin": dclogin,
                    }));
                }
                let body = serde_json::to_string_pretty(&CreateUserResult { dclogin })
                    .map_err(|e| ChatmailError::config(format!("JSON: {e}")))?;
                println!("{body}");
                return Ok(());
            }
            Err(_) => continue,
        }
    }
    Err(ChatmailError::config(
        "failed to create random account after max retries",
    ))
}

async fn accounts_delete(
    args: &Args,
    pool: &DbPool,
    mailbox: &MailboxStore,
    username: &str,
    yes: bool,
    reason: &str,
) -> Result<()> {
    let out = CtlOut::from_args(args, "accounts delete");
    if !confirm(
        &format!("Delete account {username} (credentials, mail, blocklist)?"),
        yes,
    )? {
        return out.aborted();
    }
    delete_account_full(pool, mailbox, username, reason).await?;
    if out.is_json() {
        out.done_msg(
            "",
            serde_json::json!({ "username": username, "reason": reason }),
            format!("Deleted and blocklisted: {username}"),
        )
    } else {
        out.line(format!("Deleted and blocklisted: {username}"));
        out.line(format!("Reason: {reason}"));
        Ok(())
    }
}

async fn accounts_unban(args: &Args, pool: &DbPool, username: &str, yes: bool) -> Result<()> {
    let out = CtlOut::from_args(args, "accounts unban");
    if !confirm(&format!("Unblock {username}?"), yes)? {
        return out.aborted();
    }
    blocklist::unblock_user(pool, username).await?;
    out.done_msg(
        format!("Unblocked: {username}"),
        serde_json::json!({ "username": username }),
        format!("Unblocked: {username}"),
    )
}

async fn accounts_export(args: &Args, pool: &DbPool, output: Option<&Path>) -> Result<()> {
    let out = CtlOut::from_args(args, "accounts export");
    let users = passwords::list_users(pool).await?;
    let mut entries = Vec::new();
    for u in users {
        if is_internal_settings_key(&u) {
            continue;
        }
        let hash = passwords::get_user_hash(pool, &u).await?;
        entries.push(ExportUser {
            username: u,
            password: String::new(),
            hash: hash.unwrap_or_default(),
        });
    }
    let body = serde_json::to_string_pretty(&entries)
        .map_err(|e| ChatmailError::config(format!("export JSON: {e}")))?;
    if let Some(path) = output {
        fs::write(path, &body)?;
        out.done_msg(
            format!("Exported {} accounts to {}", entries.len(), path.display()),
            serde_json::json!({ "count": entries.len(), "output": path.display().to_string() }),
            format!("Exported {} accounts", entries.len()),
        )
    } else if out.is_json() {
        out.emit(serde_json::json!({ "accounts": entries }))
    } else {
        println!("{body}");
        Ok(())
    }
}

async fn accounts_import(
    args: &Args,
    pool: &DbPool,
    mailbox: &MailboxStore,
    domain: &str,
    file: &Path,
) -> Result<()> {
    let out = CtlOut::from_args(args, "accounts import");
    let raw = fs::read_to_string(file)?;
    let users: Vec<ExportUser> = serde_json::from_str(&raw)
        .map_err(|e| ChatmailError::config(format!("invalid import JSON: {e}")))?;

    let mut imported = 0i32;
    let mut skipped = 0i32;
    let mut errors = Vec::new();

    for u in users {
        if u.username.is_empty() {
            skipped += 1;
            continue;
        }
        if is_internal_settings_key(&u.username) {
            skipped += 1;
            continue;
        }

        let username = match ensure_email(&u.username, domain) {
            Ok(n) => n,
            Err(e) => {
                errors.push(format!("{}: {e}", u.username));
                continue;
            }
        };

        if passwords::user_exists(pool, &username).await? {
            skipped += 1;
            continue;
        }
        if blocklist::is_blocked(pool, &username).await? {
            skipped += 1;
            errors.push(format!("{username}: blocklisted"));
            continue;
        }

        let stored_hash = if !u.hash.is_empty() {
            if !is_importable_hash(&u.hash) {
                errors.push(format!("{username}: unsupported password hash format"));
                skipped += 1;
                continue;
            }
            u.hash
        } else {
            let password = if u.password.is_empty() {
                random_password(ADMIN_PASSWORD_LEN)?
            } else {
                u.password
            };
            hash_password(&password)?
        };

        if let Err(e) = provision_account(pool, mailbox, &username, &stored_hash).await {
            let _ = passwords::delete_user(pool, &username).await;
            let _ = delete_quota_row(pool, &username).await;
            errors.push(format!("{username}: {e}"));
            continue;
        }
        imported += 1;
    }

    if out.is_json() {
        out.emit(serde_json::json!({
            "imported": imported,
            "skipped": skipped,
            "errors": errors,
        }))
    } else {
        out.line(format!("Imported: {imported}, skipped: {skipped}"));
        for e in errors {
            eprintln!("{e}");
        }
        Ok(())
    }
}

async fn accounts_delete_all(
    args: &Args,
    pool: &DbPool,
    mailbox: &MailboxStore,
    yes: bool,
) -> Result<()> {
    let out = CtlOut::from_args(args, "accounts delete-all");
    let users = passwords::list_users(pool).await?;
    let count = users
        .iter()
        .filter(|u| !is_internal_settings_key(u))
        .count();
    if !confirm(
        &format!("Delete ALL {count} user accounts (destructive)?"),
        yes,
    )? {
        return out.aborted();
    }

    let mut deleted = 0i32;
    for u in users {
        if is_internal_settings_key(&u) {
            continue;
        }
        match delete_account_full(pool, mailbox, &u, BULK_DELETE_REASON).await {
            Ok(()) => deleted += 1,
            Err(e) => eprintln!("{u}: {e}"),
        }
    }
    out.done_msg(
        format!("Deleted {deleted} accounts (blocklisted with bulk reason)."),
        serde_json::json!({ "deleted": deleted }),
        format!("Deleted {deleted} accounts"),
    )
}

fn random_alnum(len: usize) -> Result<String> {
    const CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let mut b = vec![0u8; len];
    fill(&mut b).map_err(|e| ChatmailError::config(format!("random: {e}")))?;
    Ok(b.iter()
        .map(|x| CHARSET[(*x as usize) % CHARSET.len()] as char)
        .collect())
}

fn random_password(len: usize) -> Result<String> {
    const CHARSET: &[u8] =
        b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789!@#$%^&*()_+-=[]{}|;:,.<>?";
    let mut b = vec![0u8; len];
    fill(&mut b).map_err(|e| ChatmailError::config(format!("random: {e}")))?;
    Ok(b.iter()
        .map(|x| CHARSET[(*x as usize) % CHARSET.len()] as char)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatmail_auth::verify_password;
    use chatmail_config::Cli;
    use chatmail_db::init_db;
    use clap::Parser;
    use tempfile::TempDir;

    fn test_args() -> Args {
        Cli::try_parse_from(["chatmail"]).unwrap().args
    }

    #[tokio::test]
    async fn cli_accounts_create_delete_blocklist() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("credentials.db");
        let pool = init_db(&db_path).await.unwrap();
        let mailbox = MailboxStore::new(dir.path());

        let username = "cliuser@example.org";
        let hash = hash_password("testpass123").unwrap();
        provision_account(&pool, &mailbox, username, &hash)
            .await
            .unwrap();
        assert!(passwords::user_exists(&pool, username).await.unwrap());

        delete_account_full(&pool, &mailbox, username, CLI_DELETE_REASON)
            .await
            .unwrap();
        assert!(!passwords::user_exists(&pool, username).await.unwrap());
        assert!(blocklist::is_blocked(&pool, username).await.unwrap());
    }

    #[tokio::test]
    async fn cli_accounts_export_import_in_process() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("credentials.db");
        let pool = init_db(&db_path).await.unwrap();
        let mailbox = MailboxStore::new(dir.path());
        let email = "importer@example.org";
        let hash = hash_password("import-pass-99").unwrap();
        provision_account(&pool, &mailbox, email, &hash)
            .await
            .unwrap();

        let export_path = dir.path().join("out.json");
        accounts_export(&test_args(), &pool, Some(export_path.as_path()))
            .await
            .unwrap();

        delete_account_full(&pool, &mailbox, email, CLI_DELETE_REASON)
            .await
            .unwrap();
        blocklist::unblock_user(&pool, email).await.unwrap();

        accounts_import(
            &test_args(),
            &pool,
            &mailbox,
            "example.org",
            export_path.as_path(),
        )
        .await
        .unwrap();
        assert!(passwords::user_exists(&pool, email).await.unwrap());
    }

    #[tokio::test]
    async fn cli_accounts_import_sha512_crypt_hash() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("credentials.db");
        let pool = init_db(&db_path).await.unwrap();
        let mailbox = MailboxStore::new(dir.path());
        let email = "sha512import@example.org";
        let hash = "$6$testsalt$zcc0po6c786cz9LdMIli0E4Zox6uXK6Khb536rxCF/JO..UDVYHeg9zCKnpkm0FyMFumVno4DCKiS8pQLicRP.";
        let import_path = dir.path().join("sha512.json");
        fs::write(
            &import_path,
            format!(r#"[{{"username":"{email}","hash":"{hash}"}}]"#),
        )
        .unwrap();

        accounts_import(
            &test_args(),
            &pool,
            &mailbox,
            "example.org",
            import_path.as_path(),
        )
        .await
        .unwrap();

        let stored = passwords::get_user_hash(&pool, email)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored, hash);
        assert!(verify_password("testpass", &stored).unwrap());
    }

    #[tokio::test]
    async fn cli_accounts_status_runs() {
        let dir = TempDir::new().unwrap();
        let config = chatmail_config::AppConfig::default();
        let state_dir = dir.path().to_path_buf();
        let database = chatmail_config::effective_database_config(&state_dir, &config);
        let db_path = std::path::PathBuf::from(&database.dsn);
        let pool = init_db(&db_path).await.unwrap();
        blocklist::block_user(&pool, "gone@x.org", CLI_BAN_REASON)
            .await
            .unwrap();

        accounts_status(
            &test_args(),
            &CtlContext {
                config,
                state_dir,
                database,
                db_path,
            },
            &pool,
            &MailboxStore::new(dir.path()),
        )
        .await
        .unwrap();
    }
}
