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

#![allow(clippy::field_reassign_with_default)]

use std::sync::Arc;

use chatmail_config::AppConfig;
use chatmail_config::{DEFAULT_MAX_FEDERATION_BYTES, DEFAULT_MAX_MESSAGE_BYTES};
use chatmail_db::{
    get_bool_setting, init_memory_db, message_stats_snapshot, record_smtp_accepted,
    seed_install_defaults, settings_keys,
};
use chatmail_push::{push_stats_snapshot, record_successful_delivery};
use chatmail_state::{AppState, ReloadRequest, ReloadScope};
use serde_json::json;
use tempfile::TempDir;
use tokio::sync::mpsc;

use crate::resources;
use crate::AdminState;

async fn test_state(token: &str, file_config: AppConfig) -> (AdminState, TempDir) {
    let pool = init_memory_db().await.unwrap();
    seed_install_defaults(&pool).await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let app = Arc::new(AppState::with_quota_and_message_limit(
        dir.path(),
        chatmail_config::DEFAULT_QUOTA_BYTES,
        &file_config,
        pool.clone(),
    ));
    app.hydrate(&pool, &file_config).await.unwrap();
    let st = AdminState::new(
        pool,
        app,
        file_config,
        dir.path().to_path_buf(),
        "example.org".into(),
        token.to_string(),
        None,
    );
    (st, dir)
}

#[tokio::test]
async fn p9_shadowsocks_not_configured() {
    let (st, _dir) = test_state(
        "secret-token-01234567890123456789012345678901",
        AppConfig::default(),
    )
    .await;
    let (_, body) = resources::dispatch(&st, "GET", "/admin/services/shadowsocks", &json!({}))
        .await
        .unwrap();
    assert_eq!(
        body.unwrap().get("status").and_then(|v| v.as_str()),
        Some("disabled")
    );
    let err = resources::dispatch(
        &st,
        "POST",
        "/admin/services/shadowsocks",
        &json!({ "action": "enable" }),
    )
    .await
    .unwrap_err();
    assert_eq!(err.0, 400);
    assert!(err.1.contains("not configured"));
}

#[tokio::test]
async fn p9_shadowsocks_configured_toggle() {
    let mut cfg = AppConfig::default();
    cfg.ss_addr = Some("0.0.0.0:8388".into());
    cfg.ss_password = Some("test-pass".into());
    cfg.ss_cipher = Some("aes-128-gcm".into());
    let (st, _dir) = test_state("secret-token-01234567890123456789012345678901", cfg).await;

    let (_, body) = resources::dispatch(&st, "GET", "/admin/services/shadowsocks", &json!({}))
        .await
        .unwrap();
    assert_eq!(
        body.unwrap().get("status").and_then(|v| v.as_str()),
        Some("enabled")
    );

    let (_, body) = resources::dispatch(
        &st,
        "POST",
        "/admin/services/shadowsocks",
        &json!({ "action": "disable" }),
    )
    .await
    .unwrap();
    assert_eq!(
        body.unwrap().get("status").and_then(|v| v.as_str()),
        Some("disabled")
    );
}

#[tokio::test]
async fn p9_push_service_toggle() {
    let (st, _dir) = test_state(
        "secret-token-01234567890123456789012345678901",
        AppConfig::default(),
    )
    .await;

    let (_, body) = resources::dispatch(&st, "GET", "/admin/services/push", &json!({}))
        .await
        .unwrap();
    let body = body.unwrap();
    assert_eq!(
        body.get("status").and_then(|v| v.as_str()),
        Some("disabled")
    );
    assert_eq!(body.get("mode").and_then(|v| v.as_str()), Some("off"));
    assert!(body
        .get("successful_notifications")
        .and_then(|v| v.as_i64())
        .is_some());

    let (_, body) = resources::dispatch(
        &st,
        "POST",
        "/admin/services/push",
        &json!({ "action": "auto" }),
    )
    .await
    .unwrap();
    let body = body.unwrap();
    assert_eq!(body.get("status").and_then(|v| v.as_str()), Some("enabled"));
    assert_eq!(body.get("mode").and_then(|v| v.as_str()), Some("auto"));

    let (_, body) = resources::dispatch(
        &st,
        "POST",
        "/admin/services/push",
        &json!({ "action": "disable" }),
    )
    .await
    .unwrap();
    let body = body.unwrap();
    assert_eq!(
        body.get("status").and_then(|v| v.as_str()),
        Some("disabled")
    );
    assert_eq!(body.get("mode").and_then(|v| v.as_str()), Some("off"));
    assert_eq!(
        body.get("restart_required").and_then(|v| v.as_bool()),
        Some(true)
    );

    let (_, body) = resources::dispatch(
        &st,
        "POST",
        "/admin/services/push",
        &json!({ "action": "auto" }),
    )
    .await
    .unwrap();
    let body = body.unwrap();
    assert_eq!(body.get("status").and_then(|v| v.as_str()), Some("enabled"));
    assert_eq!(body.get("mode").and_then(|v| v.as_str()), Some("auto"));
}

#[tokio::test]
async fn p9_status_push_stats() {
    let (st, _dir) = test_state(
        "secret-token-01234567890123456789012345678901",
        AppConfig::default(),
    )
    .await;
    let before = push_stats_snapshot();
    record_successful_delivery();
    record_successful_delivery();
    record_successful_delivery();

    let (_, body) = resources::dispatch(&st, "GET", "/admin/status", &json!({}))
        .await
        .unwrap();
    let body = body.unwrap();
    let push = body.get("push").expect("push block");
    assert_eq!(push.get("enabled").and_then(|v| v.as_bool()), Some(false));
    assert_eq!(
        push.get("successful_notifications")
            .and_then(|v| v.as_i64()),
        Some(before + 3)
    );
    assert_eq!(push_stats_snapshot(), before + 3);
}

#[tokio::test]
async fn p9_status_message_counters() {
    let (st, _dir) = test_state(
        "secret-token-01234567890123456789012345678901",
        AppConfig::default(),
    )
    .await;
    record_smtp_accepted(false);
    record_smtp_accepted(true);

    let (_, body) = resources::dispatch(&st, "GET", "/admin/status", &json!({}))
        .await
        .unwrap();
    let body = body.unwrap();
    assert_eq!(body.get("sent_messages").and_then(|v| v.as_i64()), Some(2));
    assert_eq!(
        body.get("received_messages").and_then(|v| v.as_i64()),
        Some(1)
    );
    assert_eq!(message_stats_snapshot().0, 2);
}

#[tokio::test]
async fn p9_admin_status_get() {
    let (st, _dir) = test_state(
        "secret-token-01234567890123456789012345678901",
        AppConfig::default(),
    )
    .await;
    let (status, body) = resources::dispatch(&st, "GET", "/admin/status", &json!({}))
        .await
        .unwrap();
    assert_eq!(status, 200);
    assert!(body.unwrap().get("version").is_some());
}

#[tokio::test]
async fn p9_admin_overview_get() {
    let (st, _dir) = test_state(
        "secret-token-01234567890123456789012345678901",
        AppConfig::default(),
    )
    .await;
    let (status, body) = resources::dispatch(&st, "GET", "/admin/overview", &json!({}))
        .await
        .unwrap();
    assert_eq!(status, 200);
    let body = body.unwrap();
    assert!(body.get("version").is_some());
    assert!(body.get("users").is_some());
    assert!(body.get("uptime").is_some());
    assert!(body.get("disk").is_some());
    assert!(body.get("tokens").is_some());
    assert!(body.get("sent_messages").is_some());
    assert!(body.get("imap").is_some());
    assert!(body.get("turn").is_some());
    assert!(body.get("shadowsocks").is_some());
    assert_eq!(
        body.get("tokens")
            .and_then(|v| v.get("total"))
            .and_then(|v| v.as_i64()),
        Some(0)
    );
    assert!(body.get("settings").is_some());
}

#[tokio::test]
async fn p9_ss_ws_and_grpc_transports_disabled() {
    let mut cfg = AppConfig::default();
    cfg.ss_addr = Some("0.0.0.0:8388".into());
    cfg.ss_password = Some("pw".into());
    let (st, _dir) = test_state("secret-token-01234567890123456789012345678901", cfg).await;

    for path in ["/admin/services/ss_ws", "/admin/services/ss_grpc"] {
        let (_, body) = resources::dispatch(&st, "GET", path, &json!({}))
            .await
            .unwrap();
        assert_eq!(
            body.unwrap().get("status").and_then(|v| v.as_str()),
            Some("disabled")
        );
        let err = resources::dispatch(&st, "POST", path, &json!({ "action": "enable" }))
            .await
            .unwrap_err();
        assert_eq!(err.0, 400);
    }

    let (_, body) = resources::dispatch(&st, "GET", "/admin/settings", &json!({}))
        .await
        .unwrap();
    let body = body.unwrap();
    assert_eq!(
        body.get("ss_ws_enabled").and_then(|v| v.as_str()),
        Some("disabled")
    );
    assert_eq!(
        body.get("ss_grpc_enabled").and_then(|v| v.as_str()),
        Some("disabled")
    );
}

#[tokio::test]
async fn p9_all_settings_includes_shadowsocks_url_field() {
    let mut cfg = AppConfig::default();
    cfg.ss_addr = Some("0.0.0.0:8388".into());
    cfg.ss_password = Some("pw".into());
    let (st, _dir) = test_state("secret-token-01234567890123456789012345678901", cfg).await;
    let (_, body) = resources::dispatch(&st, "GET", "/admin/settings", &json!({}))
        .await
        .unwrap();
    let body = body.unwrap();
    assert_eq!(
        body.get("ss_enabled").and_then(|v| v.as_str()),
        Some("enabled")
    );
    assert!(body
        .get("shadowsocks_url")
        .and_then(|v| v.as_str())
        .is_some_and(|s| s.starts_with("ss://")));
}

#[tokio::test]
async fn admin_message_size_get_put_delete() {
    let (st, _dir) = test_state(
        "secret-token-01234567890123456789012345678901",
        AppConfig::default(),
    )
    .await;

    let (_, body) = resources::dispatch(&st, "GET", "/admin/message-size", &json!({}))
        .await
        .unwrap();
    let body = body.unwrap();
    assert_eq!(
        body.get("effective_bytes").and_then(|v| v.as_u64()),
        Some(100 * 1024 * 1024)
    );
    assert_eq!(body.get("effective").and_then(|v| v.as_str()), Some("100M"));

    let (_, body) =
        resources::dispatch(&st, "PUT", "/admin/message-size", &json!({ "size": "64M" }))
            .await
            .unwrap();
    assert_eq!(
        body.unwrap()
            .get("effective_bytes")
            .and_then(|v| v.as_u64()),
        Some(64 * 1024 * 1024)
    );
    assert_eq!(st.app.message_size.effective(), 64 * 1024 * 1024);

    let (_, body) = resources::dispatch(&st, "DELETE", "/admin/message-size", &json!({}))
        .await
        .unwrap();
    assert_eq!(
        body.unwrap()
            .get("effective_bytes")
            .and_then(|v| v.as_u64()),
        Some(DEFAULT_MAX_MESSAGE_BYTES)
    );
}

#[tokio::test]
async fn admin_message_size_put_rejects_invalid() {
    let (st, _dir) = test_state(
        "secret-token-01234567890123456789012345678901",
        AppConfig::default(),
    )
    .await;
    let err = resources::dispatch(
        &st,
        "PUT",
        "/admin/message-size",
        &json!({ "size": "not-a-size" }),
    )
    .await
    .unwrap_err();
    assert_eq!(err.0, 400);
}

#[tokio::test]
async fn admin_federation_size_get_put_delete() {
    let (st, _dir) = test_state(
        "secret-token-01234567890123456789012345678901",
        AppConfig::default(),
    )
    .await;

    let (_, body) = resources::dispatch(&st, "GET", "/admin/federation-size", &json!({}))
        .await
        .unwrap();
    let body = body.unwrap();
    assert_eq!(
        body.get("effective_bytes").and_then(|v| v.as_u64()),
        Some(DEFAULT_MAX_FEDERATION_BYTES)
    );
    assert_eq!(body.get("effective").and_then(|v| v.as_str()), Some("70M"));

    let (_, body) = resources::dispatch(
        &st,
        "PUT",
        "/admin/federation-size",
        &json!({ "size": "64M" }),
    )
    .await
    .unwrap();
    assert_eq!(
        body.unwrap()
            .get("effective_bytes")
            .and_then(|v| v.as_u64()),
        Some(64 * 1024 * 1024)
    );
    assert_eq!(st.app.federation_size.effective(), 64 * 1024 * 1024);

    let (_, body) = resources::dispatch(&st, "DELETE", "/admin/federation-size", &json!({}))
        .await
        .unwrap();
    assert_eq!(
        body.unwrap()
            .get("effective_bytes")
            .and_then(|v| v.as_u64()),
        Some(DEFAULT_MAX_FEDERATION_BYTES)
    );
}

#[tokio::test]
async fn admin_federation_settings_includes_size() {
    let (st, _dir) = test_state(
        "secret-token-01234567890123456789012345678901",
        AppConfig::default(),
    )
    .await;
    let (_, body) = resources::dispatch(&st, "GET", "/admin/settings/federation", &json!({}))
        .await
        .unwrap();
    let body = body.unwrap();
    assert_eq!(
        body.get("federation_size_effective_bytes")
            .and_then(|v| v.as_u64()),
        Some(DEFAULT_MAX_FEDERATION_BYTES)
    );
    assert_eq!(
        body.get("max_federation_size").and_then(|v| v.as_str()),
        Some("70M")
    );
}

#[tokio::test]
async fn admin_settings_max_federation_size_updates_effective() {
    let (st, _dir) = test_state(
        "secret-token-01234567890123456789012345678901",
        AppConfig::default(),
    )
    .await;
    resources::dispatch(
        &st,
        "POST",
        "/admin/settings/max_federation_size",
        &json!({ "action": "set", "value": "55M" }),
    )
    .await
    .unwrap();
    assert_eq!(st.app.federation_size.effective(), 55 * 1024 * 1024);
}

#[tokio::test]
async fn admin_settings_appendlimit_updates_effective() {
    let (st, _dir) = test_state(
        "secret-token-01234567890123456789012345678901",
        AppConfig::default(),
    )
    .await;
    resources::dispatch(
        &st,
        "POST",
        "/admin/settings/appendlimit",
        &json!({ "action": "set", "value": "10M" }),
    )
    .await
    .unwrap();
    assert_eq!(st.app.message_size.effective(), 10 * 1024 * 1024);
}

#[tokio::test]
async fn p9_federation_silent_dismiss_crud() {
    let (st, _dir) = test_state(
        "secret-token-01234567890123456789012345678901",
        AppConfig::default(),
    )
    .await;

    let (_, body) = resources::dispatch(&st, "GET", "/admin/federation/silent-dismiss", &json!({}))
        .await
        .unwrap();
    assert_eq!(body.unwrap().get("total").and_then(|v| v.as_u64()), Some(0));

    let (_, body) = resources::dispatch(
        &st,
        "POST",
        "/admin/federation/silent-dismiss",
        &json!({ "domain": "a.com" }),
    )
    .await
    .unwrap();
    assert_eq!(body.unwrap().get("total").and_then(|v| v.as_u64()), Some(1));

    assert!(st
        .app
        .federation_silent_dismiss
        .is_dismissed("user@a.com", &["local.test".into()]));

    let (_, body) = resources::dispatch(
        &st,
        "DELETE",
        "/admin/federation/silent-dismiss",
        &json!({ "domain": "a.com" }),
    )
    .await
    .unwrap();
    assert_eq!(
        body.unwrap().get("remaining").and_then(|v| v.as_u64()),
        Some(0)
    );
}

#[tokio::test]
async fn p9_admin_web_path_set_enables_and_reloads() {
    let pool = init_memory_db().await.unwrap();
    seed_install_defaults(&pool).await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let app = Arc::new(AppState::with_quota_and_message_limit(
        dir.path(),
        chatmail_config::DEFAULT_QUOTA_BYTES,
        &AppConfig::default(),
        pool.clone(),
    ));
    app.hydrate(&pool, &AppConfig::default()).await.unwrap();
    let (reload_tx, mut reload_rx) = mpsc::channel::<ReloadRequest>(1);
    tokio::spawn(async move {
        if let Some(req) = reload_rx.recv().await {
            assert_eq!(req.scope, ReloadScope::HttpRoutes);
            if let Some(done) = req.done {
                let _ = done.send(Ok(()));
            }
        }
    });
    let st = AdminState::new(
        pool.clone(),
        app,
        AppConfig::default(),
        dir.path().to_path_buf(),
        "example.org".into(),
        "secret-token-01234567890123456789012345678901".into(),
        Some(reload_tx),
    );

    let (_, body) = resources::dispatch(
        &st,
        "POST",
        "/admin/settings/admin_web_path",
        &json!({ "action": "set", "value": "/xxx" }),
    )
    .await
    .unwrap();
    let body = body.unwrap();
    assert_eq!(body.get("value").and_then(|v| v.as_str()), Some("/xxx"));
    assert_eq!(
        body.get("restart_required").and_then(|v| v.as_bool()),
        Some(true)
    );
    assert!(
        get_bool_setting(&pool, settings_keys::ADMIN_WEB_ENABLED, false)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn turn_relay_port_settings_in_all_settings_snapshot() {
    let (st, _dir) = test_state(
        "secret-token-01234567890123456789012345678901",
        AppConfig::default(),
    )
    .await;
    let (_, body) = resources::dispatch(&st, "GET", "/admin/settings", &json!({}))
        .await
        .unwrap();
    let body = body.unwrap();
    assert_eq!(
        body.get("turn_relay_port_min")
            .and_then(|v| v.get("value"))
            .and_then(|v| v.as_str()),
        Some("49152")
    );
    assert_eq!(
        body.get("turn_relay_port_max")
            .and_then(|v| v.get("value"))
            .and_then(|v| v.as_str()),
        Some("65535")
    );
}

#[tokio::test]
async fn turn_relay_port_settings_set_get_reset() {
    let (st, _dir) = test_state(
        "secret-token-01234567890123456789012345678901",
        AppConfig::default(),
    )
    .await;

    let (_, body) = resources::dispatch(
        &st,
        "POST",
        "/admin/settings/turn_relay_port_min",
        &json!({ "action": "set", "value": "55000" }),
    )
    .await
    .unwrap();
    let body = body.unwrap();
    assert_eq!(body.get("value").and_then(|v| v.as_str()), Some("55000"));
    assert_eq!(
        body.get("restart_required").and_then(|v| v.as_bool()),
        Some(true)
    );

    let (_, body) = resources::dispatch(
        &st,
        "POST",
        "/admin/settings/turn_relay_port_max",
        &json!({ "action": "set", "value": "55010" }),
    )
    .await
    .unwrap();
    assert_eq!(
        body.unwrap().get("value").and_then(|v| v.as_str()),
        Some("55010")
    );

    let (_, body) = resources::dispatch(
        &st,
        "GET",
        "/admin/settings/turn_relay_port_min",
        &json!({}),
    )
    .await
    .unwrap();
    assert_eq!(
        body.unwrap().get("value").and_then(|v| v.as_str()),
        Some("55000")
    );

    resources::dispatch(
        &st,
        "POST",
        "/admin/settings/turn_relay_port_min",
        &json!({ "action": "reset" }),
    )
    .await
    .unwrap();
    let (_, body) = resources::dispatch(
        &st,
        "GET",
        "/admin/settings/turn_relay_port_min",
        &json!({}),
    )
    .await
    .unwrap();
    assert_eq!(
        body.unwrap().get("is_set").and_then(|v| v.as_bool()),
        Some(false)
    );
}

#[tokio::test]
async fn turn_relay_port_settings_reject_min_above_max() {
    let (st, _dir) = test_state(
        "secret-token-01234567890123456789012345678901",
        AppConfig::default(),
    )
    .await;
    resources::dispatch(
        &st,
        "POST",
        "/admin/settings/turn_relay_port_max",
        &json!({ "action": "set", "value": "50000" }),
    )
    .await
    .unwrap();
    let err = resources::dispatch(
        &st,
        "POST",
        "/admin/settings/turn_relay_port_min",
        &json!({ "action": "set", "value": "60000" }),
    )
    .await
    .unwrap_err();
    assert_eq!(err.0, 400);
    assert!(err.1.contains("must be <="));
}

#[tokio::test]
async fn turn_relay_port_settings_reject_control_port_in_range() {
    let (st, _dir) = test_state(
        "secret-token-01234567890123456789012345678901",
        AppConfig::default(),
    )
    .await;
    let err = resources::dispatch(
        &st,
        "POST",
        "/admin/settings/turn_relay_port_min",
        &json!({ "action": "set", "value": "3000" }),
    )
    .await
    .unwrap_err();
    assert_eq!(err.0, 400);
    assert!(err.1.contains("3478"));
}

#[tokio::test]
async fn webmail_dev_enable_without_origin_sets_services_only() {
    let (st, _dir) = test_state(
        "secret-token-01234567890123456789012345678901",
        AppConfig::default(),
    )
    .await;
    let (_, body) = resources::dispatch(
        &st,
        "POST",
        "/admin/services/webmail_dev",
        &json!({ "action": "enable" }),
    )
    .await
    .unwrap();
    let body = body.unwrap();
    assert_eq!(body.get("status").and_then(|v| v.as_str()), Some("enabled"));
    assert_eq!(
        body.get("browser_access_enabled").and_then(|v| v.as_bool()),
        Some(true)
    );
}

#[tokio::test]
async fn webmail_dev_disable_turns_off_services() {
    let (st, _dir) = test_state(
        "secret-token-01234567890123456789012345678901",
        AppConfig::default(),
    )
    .await;
    resources::dispatch(
        &st,
        "POST",
        "/admin/services/webmail_dev",
        &json!({ "action": "enable" }),
    )
    .await
    .unwrap();
    let (_, body) = resources::dispatch(
        &st,
        "POST",
        "/admin/services/webmail_dev",
        &json!({ "action": "disable" }),
    )
    .await
    .unwrap();
    let body = body.unwrap();
    assert_eq!(
        body.get("status").and_then(|v| v.as_str()),
        Some("disabled")
    );
    assert_eq!(
        body.get("browser_access_enabled").and_then(|v| v.as_bool()),
        Some(false)
    );
}

#[tokio::test]
async fn webmail_dev_enable_sets_services_and_cors() {
    let (st, _dir) = test_state(
        "secret-token-01234567890123456789012345678901",
        AppConfig::default(),
    )
    .await;
    let (_, body) = resources::dispatch(
        &st,
        "POST",
        "/admin/services/webmail_dev",
        &json!({
            "action": "enable",
            "origin": "http://127.0.0.1:5173"
        }),
    )
    .await
    .unwrap();
    let body = body.unwrap();
    assert_eq!(body.get("status").and_then(|v| v.as_str()), Some("enabled"));
    assert_eq!(
        body.get("webimap_enabled").and_then(|v| v.as_str()),
        Some("enabled")
    );
    assert_eq!(
        body.get("websmtp_enabled").and_then(|v| v.as_str()),
        Some("enabled")
    );
    assert_eq!(
        body.get("cors_origins").and_then(|v| v.as_str()),
        Some("http://127.0.0.1:5173")
    );
}

#[tokio::test]
async fn p9_auth_gate_bearer() {
    use std::collections::HashMap;
    let gate = crate::auth::AuthGate::new("secret-token-01234567890123456789012345678901".into());
    let mut ok = HashMap::new();
    ok.insert(
        "Authorization".into(),
        "Bearer secret-token-01234567890123456789012345678901".into(),
    );
    assert!(gate.authenticate(&ok, "127.0.0.1"));
    let mut bad = HashMap::new();
    bad.insert("Authorization".into(), "Bearer wrong".into());
    assert!(!gate.authenticate(&bad, "127.0.0.1"));
}
