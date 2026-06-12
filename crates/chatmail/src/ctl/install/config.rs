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

//! Generated `maddy.conf` (Madmail-compatible; madmail-v2 uses `tls file` + `madmail certificate`).

use std::path::PathBuf;

pub struct InstallConfig {
    pub binary_name: String,
    pub binary_path: PathBuf,
    pub maddy_user: String,
    pub maddy_group: String,
    pub hostname: String,
    pub primary_domain: String,
    pub local_domains: String,
    pub state_dir: PathBuf,
    pub runtime_dir: String,
    pub public_ip: String,
    pub tls_mode: String,
    pub cert_path: PathBuf,
    pub key_path: PathBuf,
    pub acme_email: String,
    pub generate_certs: bool,
    pub turn_off_tls: bool,
    pub enable_chatmail: bool,
    pub enable_contact_sharing: bool,
    pub enable_ss: bool,
    pub enable_turn: bool,
    pub turn_port: String,
    pub turn_secret: String,
    pub turn_ttl: u32,
    pub ss_addr: String,
    pub ss_password: String,
    pub ss_cipher: String,
    pub language: String,
    pub config_dir: PathBuf,
    pub config_path: PathBuf,
    /// User passed `--config-dir` and/or `--state-dir` (systemd uses explicit paths).
    pub paths_explicit: bool,
    /// Use `StateDirectory` / `ConfigurationDirectory` in the systemd unit (default FHS layout).
    pub use_default_systemd_paths: bool,
    pub system_install: bool,
    pub skip_user: bool,
    pub skip_systemd: bool,
    pub generated: String,
}

pub fn render_maddy_conf(c: &InstallConfig) -> String {
    let log_block = "# Disable logging as requested\nlog off\n";
    let require_tls_smtp = if c.turn_off_tls {
        ""
    } else {
        "        require_tls\n"
    };
    let require_tls_sub = if c.turn_off_tls {
        ""
    } else {
        "            require_tls"
    };
    let mx_auth = if c.turn_off_tls {
        r#"    mx_auth {
        local_policy {
            min_tls_level none
            min_mx_level none
        }
    }"#
    } else {
        r#"    mx_auth {
        dane
        mtasts {
            cache fs
            fs_dir mtasts_cache/
        }
        local_policy {
            min_tls_level encrypted
            min_mx_level none
        }
    }"#
    };

    let imap_turn = if c.enable_turn {
        format!(
            r#"
    turn_enable yes
    turn_server {}
    turn_port {}
    turn_secret {}
    turn_ttl {}"#,
            c.public_ip, c.turn_port, c.turn_secret, c.turn_ttl
        )
    } else {
        String::new()
    };

    let turn_block = if c.enable_turn {
        format!(
            r#"
turn udp://0.0.0.0:{port} tcp://0.0.0.0:{port} {{
    realm {ip}
    secret {secret}
    relay_ip {ip}
    debug no
}}
"#,
            port = c.turn_port,
            ip = c.public_ip,
            secret = c.turn_secret,
        )
    } else {
        String::new()
    };

    let ss_block = if c.enable_ss {
        format!(
            r#"
    ss_addr "{}"
    ss_password "{}"
    ss_cipher "{}"
"#,
            c.ss_addr, c.ss_password, c.ss_cipher
        )
    } else {
        String::new()
    };

    let turn_off = if c.turn_off_tls { "yes" } else { "no" };
    let contact = if c.enable_contact_sharing {
        "yes"
    } else {
        "no"
    };
    let lang = c.language.as_str();

    let chatmail_http = if c.enable_chatmail {
        format!(
            r#"
chatmail tcp://0.0.0.0:80 {{
    debug false
    mail_domain $(primary_domain)
    mx_domain $(primary_domain)
    web_domain $(primary_domain)
    auth_db local_authdb
    storage local_mailboxes
    password_length 16
    public_ip $(public_ip)
    turn_off_tls {turn_off}
    enable_contact_sharing {contact}
    language {lang}
{ss_block}}}

chatmail tls://0.0.0.0:443 {{
    debug false
    mail_domain $(primary_domain)
    mx_domain $(primary_domain)
    web_domain $(primary_domain)
    auth_db local_authdb
    storage local_mailboxes
    password_length 16
    tls file {cert} {key}
    public_ip $(public_ip)
    turn_off_tls {turn_off}
    alpn_smtp submission
    alpn_imap imap
    enable_contact_sharing {contact}
    language {lang}
{ss_block}}}
"#,
            turn_off = turn_off,
            contact = contact,
            lang = lang,
            cert = c.cert_path.display(),
            key = c.key_path.display(),
            ss_block = ss_block,
        )
    } else {
        String::new()
    };

    let tls_mode_directives = match c.tls_mode.as_str() {
        "autocert" => format!("tls_mode autocert\nacme_email {}\n", c.acme_email),
        "file" => "tls_mode file\n".to_string(),
        "self_signed" => "tls_mode self_signed\n".to_string(),
        _ => String::new(),
    };

    format!(
        r##"## Maddy Mail Server - configuration file (generated by chatmail install)
# Generated on: {generated}
# TLS: madmail-v2 uses PEM files (`madmail certificate get` for Let's Encrypt autocert)

$(hostname) = {hostname}
$(primary_domain) = {primary_domain}
$(local_domains) = {local_domains}
$(public_ip) = {public_ip}
state_dir {state_dir}
runtime_dir {runtime_dir}

# TLS certificate paths (mode: {tls_mode})
{tls_mode_directives}tls file {cert} {key}

{log_block}

auth.pass_table local_authdb {{
    auto_create yes
    jit_domain $(primary_domain)
    table sql_table {{
        driver sqlite3
        dsn credentials.db
        table_name passwords
    }}
}}

storage.imapsql local_mailboxes {{
    auto_create yes
    driver sqlite3
    dsn imapsql.db
    retention 24h
    default_quota 1G
    appendlimit 100M
}}

hostname $(hostname)

table.chain local_rewrites {{
    optional_step regexp "(.+)\+(.+)@(.+)" "$1@$3"
    optional_step static {{
        entry postmaster postmaster@$(primary_domain)
    }}
}}

msgpipeline local_routing {{
    destination postmaster $(local_domains) {{
        modify {{
            replace_rcpt &local_rewrites
        }}
        deliver_to &local_mailboxes
    }}
    default_destination {{
        reject 550 5.1.1 "User doesn't exist"
    }}
}}

smtp tcp://0.0.0.0:25 {{
    limits {{
        all rate 20 1s
        all concurrency 200
    }}
    max_message_size 100M
    dmarc yes
    check {{
{require_tls_smtp}        require_mx_record
        dkim
        spf
    }}
    source $(local_domains) {{
        reject 501 5.1.8 "Use Submission for outgoing SMTP"
    }}
    default_source {{
        destination postmaster $(local_domains) {{
            deliver_to &local_routing
        }}
        default_destination {{
            reject 550 5.1.1 "User doesn't exist"
        }}
    }}
}}

submission tls://0.0.0.0:465 tcp://0.0.0.0:587 {{
    limits {{
        all rate 50 1s
    }}
    max_message_size 100M
    auth &local_authdb
    insecure_auth {insecure}
    source $(local_domains) {{
        check {{
{require_tls_sub}
            authorize_sender {{
                prepare_email &local_rewrites
                user_to_email identity
            }}
            pgp_encryption {{
                require_encryption yes
                allow_secure_join yes
            }}
        }}
        destination postmaster $(local_domains) {{
            deliver_to &local_routing
        }}
        default_destination {{
            modify {{
                dkim $(primary_domain) $(local_domains) default
            }}
            deliver_to &remote_queue
        }}
    }}
    default_source {{
        reject 501 5.1.8 "Non-local sender domain"
    }}
}}

target.remote outbound_delivery {{
    limits {{
        destination rate 20 1s
        destination concurrency 10
    }}
{mx_auth}
}}

target.queue remote_queue {{
    target &outbound_delivery
    autogenerated_msg_domain $(primary_domain)
    bounce {{
        destination postmaster $(local_domains) {{
            deliver_to &local_routing
        }}
        default_destination {{
            reject 550 5.0.0 "Refusing to send DSNs to non-local addresses"
        }}
    }}
}}

imap tls://0.0.0.0:993 tcp://0.0.0.0:143 {{
    auth &local_authdb
    storage &local_mailboxes
    insecure_auth {insecure}
{imap_turn}
    xchatmail yes
}}
{turn_block}
{chatmail_http}

# Prometheus / OpenMetrics (scrape http://127.0.0.1:9100/metrics)
# openmetrics tcp://127.0.0.1:9100 {{
#     debug no
# }}
"##,
        generated = c.generated,
        hostname = c.hostname,
        primary_domain = c.primary_domain,
        local_domains = c.local_domains,
        public_ip = c.public_ip,
        state_dir = c.state_dir.display(),
        runtime_dir = c.runtime_dir,
        tls_mode = c.tls_mode,
        tls_mode_directives = tls_mode_directives,
        cert = c.cert_path.display(),
        key = c.key_path.display(),
        log_block = log_block,
        require_tls_smtp = require_tls_smtp,
        require_tls_sub = require_tls_sub,
        mx_auth = mx_auth,
        insecure = turn_off,
        imap_turn = imap_turn,
        turn_block = turn_block,
        chatmail_http = chatmail_http,
    )
}

/// Build `$(local_domains)` for IP-based installs (Madmail `install.go` `--simple --ip`).
pub fn local_domains_for_ip(bare_ip: &str) -> String {
    format!("$(primary_domain) [{bare_ip}] {bare_ip}")
}
