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

mod accounts;
mod blocklist;
mod dns;
mod exchangers;
mod federation;
mod federation_size;
mod message_size;
mod notice;
mod proxy;
mod push;
mod queue;
mod quota;
mod settings;
mod status_storage;
mod toggles;
mod tokens;
mod webmail_dev;

use serde_json::Value;

use crate::AdminState;

pub type AdminResult = Result<(u16, Option<Value>), (u16, String)>;

pub async fn dispatch(st: &AdminState, method: &str, resource: &str, body: &Value) -> AdminResult {
    match resource {
        "/admin/status" => status_storage::status(st, method).await,
        "/admin/overview" => status_storage::overview(st, method).await,
        "/admin/storage" => status_storage::storage(st, method).await,
        "/admin/restart" => status_storage::restart(method),
        "/admin/reload" => status_storage::reload(st, method, body).await,
        "/admin/registration" => toggles::registration(st, method, body).await,
        "/admin/registration/jit" => toggles::jit(st, method, body).await,
        "/admin/services/turn" => {
            toggles::service_bool(st, method, body, chatmail_db::settings_keys::TURN_ENABLED).await
        }
        "/admin/services/iroh" => {
            toggles::service_bool(st, method, body, chatmail_db::settings_keys::IROH_ENABLED).await
        }
        "/admin/services/push" => push::service(st, method, body).await,
        "/admin/services/admin_web" => {
            toggles::service_bool(
                st,
                method,
                body,
                chatmail_db::settings_keys::ADMIN_WEB_ENABLED,
            )
            .await
        }
        "/admin/services/auto_purge_seen" => {
            toggles::service_bool(
                st,
                method,
                body,
                chatmail_db::settings_keys::AUTO_PURGE_SEEN,
            )
            .await
        }
        "/admin/services/message_retention" => {
            toggles::service_bool(
                st,
                method,
                body,
                chatmail_db::settings_keys::MESSAGE_RETENTION_ENABLED,
            )
            .await
        }
        "/admin/services/webimap" => {
            toggles::service_bool(
                st,
                method,
                body,
                chatmail_db::settings_keys::WEBIMAP_ENABLED,
            )
            .await
        }
        "/admin/services/websmtp" => {
            toggles::service_bool(
                st,
                method,
                body,
                chatmail_db::settings_keys::WEBSMTP_ENABLED,
            )
            .await
        }
        "/admin/services/webmail_dev" => webmail_dev::webmail_dev(st, method, body).await,
        "/admin/services/shadowsocks" => {
            proxy::proxy_service(st, method, body, chatmail_db::settings_keys::SS_ENABLED).await
        }
        "/admin/services/ss_ws" => proxy::proxy_transport_disabled(method, body).await,
        "/admin/services/ss_grpc" => proxy::proxy_transport_disabled(method, body).await,
        "/admin/services/http_proxy" => proxy::http_proxy_service(st, method, body).await,
        "/admin/settings/federation" => federation::policy(st, method, body).await,
        "/admin/federation/rules" => federation::rules(st, method, body).await,
        "/admin/federation/silent-dismiss" => federation::silent_dismiss(st, method, body).await,
        "/admin/federation/servers" => federation::servers(st, method).await,
        "/admin/accounts" => accounts::accounts(st, method, body).await,
        "/admin/blocklist" => blocklist::blocklist(st, method, body).await,
        "/admin/quota" => quota::quota(st, method, body).await,
        "/admin/message-size" => message_size::message_size(st, method, body).await,
        "/admin/federation-size" => federation_size::federation_size(st, method, body).await,
        "/admin/dns" => dns::dns(st, method, body).await,
        "/admin/exchangers" => exchangers::exchangers(st, method, body).await,
        "/admin/registration-token" => tokens::registration_token(st, method, body).await,
        "/admin/notice" => notice::notice(st, method, body).await,
        "/admin/queue" => queue::queue(st, method, body).await,
        "/admin/settings" => settings::all_settings(st, method).await,
        r if r.starts_with("/admin/settings/") => {
            let name = r.strip_prefix("/admin/settings/").unwrap_or("");
            if proxy::PROXY_SETTING_NAMES.contains(&name)
                || matches!(
                    name,
                    "ss_port" | "ss_ws_port" | "ss_grpc_port" | "ss_cipher" | "ss_password"
                )
            {
                proxy::proxy_setting(st, method, body, name).await
            } else {
                settings::named_setting(st, method, r, body).await
            }
        }
        _ => Err((404, format!("unknown resource: {resource}"))),
    }
}
