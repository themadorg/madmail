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

use std::path::Path;

use chatmail_config::{AppConfig, DcloginMailSettings, RuntimeListeners};
use chatmail_db::{resolve_default_quota_bytes, DbPool};
use chatmail_types::Result;

use crate::context_cache::WwwContextCache;
use crate::www_facts::{format_retention_label, retention_info_line};
use minijinja::Environment;
use serde::Serialize;

use crate::assets::WwwAssets;
use crate::go_template::prepare_template;

/// Template context (Madmail `serveTemplate` fields).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
#[allow(non_snake_case)]
pub struct WwwContext {
    pub MailDomain: String,
    pub MXDomain: String,
    pub WebDomain: String,
    pub PublicIP: String,
    pub Version: String,
    pub RegistrationOpen: bool,
    pub JitRegistrationEnabled: bool,
    pub Language: String,
    /// HTTP Host clients used to open the registration page (for dclogin `ih`/`sh`).
    pub ClientHost: String,
    pub ImapPortTLS: String,
    pub ImapPortStartTLS: String,
    pub SmtpPortTLS: String,
    pub SmtpPortStartTLS: String,
    pub DcloginImapSecurity: String,
    pub DcloginSmtpSecurity: String,
    pub DefaultQuota: i64,
    /// `ss://…` for DeltaChat SOCKS5 (from config + DB Shadowsocks settings).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub SSURL: String,
    pub V2rayNGConfigWS: String,
    pub V2rayNGConfigGRPC: String,
    /// Set when `storage.imapsql retention` is configured; localized sentence for info page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub MessageRetentionLine: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub Custom: Option<CustomFields>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
#[allow(non_snake_case)]
pub struct CustomFields {
    pub Slug: String,
    pub URL: String,
    pub Name: String,
}

pub struct TemplateEngine {
    /// Compiled embedded templates (`www-src/` in the binary).
    embedded: Option<Environment<'static>>,
    /// `html-serve` / `html-export` tree — templates read from disk on every render.
    external_root: Option<std::path::PathBuf>,
}

impl Default for TemplateEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl TemplateEngine {
    /// Embedded templates only (tests / default).
    pub fn new() -> Self {
        Self {
            embedded: Some(Self::load_embedded_env()),
            external_root: None,
        }
    }

    /// Default: all HTML templates compiled into RAM. With `www_dir`: live disk reload.
    pub fn from_config(config: &AppConfig) -> Self {
        if let Some(ref dir) = config.www_dir {
            if dir.is_dir() {
                if walk_html_files(dir, dir, &mut |_, _| Ok(())).is_ok() {
                    tracing::info!(
                        path = %dir.display(),
                        "www: external www_dir (live disk reload)"
                    );
                    return Self {
                        embedded: None,
                        external_root: Some(dir.clone()),
                    };
                }
                tracing::warn!(
                    path = %dir.display(),
                    "www_dir set but contains no .html files; using embedded RAM default"
                );
            }
        }
        tracing::debug!("www: default HTML templates from embedded RAM");
        Self::new()
    }

    /// `html-serve` / exported directory (not the default site).
    pub fn is_external(&self) -> bool {
        self.external_root.is_some()
    }

    /// Default config: templates served from RAM (`rust_embed`), never disk.
    pub fn is_embedded(&self) -> bool {
        self.external_root.is_none()
    }

    fn load_embedded_env() -> Environment<'static> {
        let mut env = Environment::new();
        add_filters(&mut env);
        for path in WwwAssets::iter() {
            let path = path.as_ref();
            if path.ends_with(".html") {
                if let Some(data) = WwwAssets::get(path) {
                    let src = std::str::from_utf8(data.data.as_ref()).unwrap_or("");
                    let prepared = prepare_template(src);
                    env.add_template_owned(path.to_string(), prepared)
                        .expect("template");
                }
            }
        }
        env
    }

    pub fn render(&self, name: &str, ctx: &WwwContext) -> Result<String> {
        if let Some(root) = &self.external_root {
            return Self::render_external(root, name, ctx);
        }
        let env = self.embedded.as_ref().ok_or_else(|| {
            chatmail_types::ChatmailError::config("www template engine not initialized")
        })?;
        let tmpl = env
            .get_template(name)
            .map_err(|e| chatmail_types::ChatmailError::config(e.to_string()))?;
        tmpl.render(ctx)
            .map_err(|e| chatmail_types::ChatmailError::config(e.to_string()))
    }

    /// Read and render one HTML file from `www_dir` (picks up edits without restart).
    fn render_external(root: &Path, name: &str, ctx: &WwwContext) -> Result<String> {
        let path = root.join(name);
        let src = std::fs::read_to_string(&path).map_err(|e| {
            chatmail_types::ChatmailError::config(format!("www template {}: {e}", path.display()))
        })?;
        let mut env = Environment::new();
        add_filters(&mut env);
        env.add_template_owned(name.to_string(), prepare_template(&src))
            .map_err(|e| chatmail_types::ChatmailError::config(e.to_string()))?;
        env.get_template(name)
            .map_err(|e| chatmail_types::ChatmailError::config(e.to_string()))?
            .render(ctx)
            .map_err(|e| chatmail_types::ChatmailError::config(e.to_string()))
    }
}

fn add_filters(env: &mut Environment<'_>) {
    env.add_filter("clean_domain", clean_domain);
    env.add_filter("format_bytes", format_bytes);
    env.add_filter("safe_html", safe_html);
    env.add_filter("upper", |s: String| -> String { s.to_uppercase() });
}

fn walk_html_files(
    root: &Path,
    dir: &Path,
    f: &mut dyn FnMut(&str, &Path) -> std::result::Result<(), String>,
) -> std::result::Result<(), String> {
    for entry in std::fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.is_dir() {
            walk_html_files(root, &path, f)?;
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("html"))
        {
            let rel = path
                .strip_prefix(root)
                .map_err(|e| e.to_string())?
                .to_string_lossy();
            f(rel.trim_start_matches('/'), &path)?;
        }
    }
    Ok(())
}

fn clean_domain(v: String) -> String {
    v.trim_matches(['[', ']']).to_string()
}

fn format_bytes(b: i64) -> String {
    const UNIT: i64 = 1024;
    if b < UNIT {
        return format!("{b} B");
    }
    let mut n = b;
    let mut exp = 0i64;
    while n >= UNIT {
        n /= UNIT;
        exp += 1;
    }
    let div = UNIT.pow(exp as u32);
    format!(
        "{:.1} {}B",
        b as f64 / div as f64,
        b"KMGTPE"[exp as usize - 1] as char
    )
}

fn safe_html(s: String) -> minijinja::Value {
    minijinja::Value::from_safe_string(s)
}

pub async fn build_context(
    pool: &DbPool,
    config: &AppConfig,
    custom: Option<CustomFields>,
    http_host: Option<&str>,
    runtime: Option<&RuntimeListeners>,
    state_dir: &Path,
    cache: &WwwContextCache,
) -> Result<WwwContext> {
    cache.ensure_fresh(pool, config, state_dir).await?;
    let cached = cache
        .snapshot()
        .await
        .ok_or_else(|| chatmail_types::ChatmailError::config("www context cache empty"))?;

    let mail_domain = config.effective_registration_domain(http_host);
    let mx_domain = config
        .mx_domain
        .clone()
        .unwrap_or_else(|| mail_domain.clone());
    let web_domain = config
        .hostname
        .clone()
        .unwrap_or_else(|| mail_domain.clone());
    let public_ip = config.public_ip.clone().unwrap_or_default();

    let mail = DcloginMailSettings::from_config_with_db_and_runtime(
        config,
        http_host,
        &cached.db_ports,
        runtime,
    );

    let host_hint = http_host.unwrap_or(web_domain.as_str());
    let ss_urls = cached
        .ss_runtime
        .as_ref()
        .map(|rt| rt.urls(host_hint))
        .unwrap_or_default();

    let default_quota = resolve_default_quota_bytes(pool, config).await? as i64;
    let message_retention_line =
        format_retention_label(config).map(|label| retention_info_line(&cached.language, &label));

    Ok(WwwContext {
        MailDomain: mail_domain,
        MXDomain: mx_domain,
        WebDomain: web_domain,
        PublicIP: public_ip,
        Version: env!("CARGO_PKG_VERSION").to_string(),
        RegistrationOpen: cached.registration_open,
        JitRegistrationEnabled: cached.jit_registration_enabled,
        Language: cached.language,
        ClientHost: mail.client_host,
        ImapPortTLS: mail.imap_port_tls,
        ImapPortStartTLS: mail.imap_port_starttls,
        SmtpPortTLS: mail.smtp_port_tls,
        SmtpPortStartTLS: mail.smtp_port_starttls,
        DcloginImapSecurity: mail.dclogin_imap_security,
        DcloginSmtpSecurity: mail.dclogin_smtp_security,
        DefaultQuota: default_quota,
        SSURL: ss_urls.shadowsocks_url,
        V2rayNGConfigWS: ss_urls.v2ray_ng_ws,
        V2rayNGConfigGRPC: ss_urls.v2ray_ng_grpc,
        MessageRetentionLine: message_retention_line,
        Custom: custom,
    })
}
