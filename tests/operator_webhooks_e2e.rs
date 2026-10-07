// Copyright (C) 2026 themadorg
// SPDX-License-Identifier: AGPL-3.0-or-later

mod support;

use std::sync::Arc;
use std::time::Duration;

use chatmail_admin::{admin_router, AdminState};
use chatmail_config::AppConfig;
use chatmail_db::{set_setting, settings_keys};
use serde_json::{json, Value};
use support::{pgp_mime_for_user, spawn_mail_servers, ImapClient};
use tokio::net::TcpListener;
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

const ADMIN_TOKEN: &str = "operator-test-token-012345678901234567890123456789";

async fn rpc(
    client: &reqwest::Client,
    url: &str,
    token: &str,
    method: &str,
    resource: &str,
    body: Value,
) -> Value {
    let response = client
        .post(url)
        .header("Content-Type", "application/json")
        .body(
            json!({"method": method, "resource": resource,
            "headers": {"Authorization": format!("Bearer {token}")}, "body": body})
            .to_string(),
        )
        .send()
        .await
        .unwrap();
    serde_json::from_slice(&response.bytes().await.unwrap()).unwrap()
}

async fn received(server: &MockServer, count: usize) -> Vec<Value> {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let requests = server.received_requests().await.unwrap();
            if requests.len() >= count {
                return requests
                    .iter()
                    .map(|r| serde_json::from_slice(&r.body).unwrap())
                    .collect();
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn admin_web_jit_registration_and_imap_smtp_quota_rejections() {
    let dir = tempfile::tempdir().unwrap();
    let servers = spawn_mail_servers(dir.path()).await;
    let receiver = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&receiver)
        .await;
    let config = AppConfig {
        hostname: Some("test".into()),
        primary_domain: Some("test".into()),
        ..AppConfig::default()
    };
    let admin = AdminState::new(
        servers.pool.clone(),
        Arc::clone(&servers.ctx),
        config.clone(),
        dir.path().into(),
        "test".into(),
        ADMIN_TOKEN.into(),
        None,
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let admin_url = format!("http://{}/", listener.local_addr().unwrap());
    let handle = tokio::spawn(async move {
        axum::serve(listener, admin_router(admin)).await.unwrap();
    });
    let client = reqwest::Client::new();
    let resource = "/admin/services/webhooks";

    let baseline = rpc(&client, &admin_url, ADMIN_TOKEN, "GET", resource, json!({})).await;
    assert_eq!(baseline["body"]["enabled"], false);
    assert_eq!(baseline["body"]["url"], "");
    assert_eq!(baseline["body"]["secret_configured"], false);
    let unauthorized = rpc(
        &client,
        &admin_url,
        "wrong",
        "PUT",
        resource,
        json!({"enabled": true}),
    )
    .await;
    assert_eq!(unauthorized["status"], 401);
    assert_eq!(
        rpc(&client, &admin_url, "wrong", "GET", resource, json!({})).await["status"],
        401
    );
    assert_eq!(
        rpc(
            &client,
            &admin_url,
            "wrong",
            "POST",
            resource,
            json!({"action": "test"})
        )
        .await["status"],
        401
    );

    let configured = rpc(&client, &admin_url, ADMIN_TOKEN, "PUT", resource, json!({
        "enabled": true, "url": receiver.uri(), "secret": "not-returned-secret", "retry_attempts": 0,
        "event_user_registered": true, "event_quota_exceeded": true
    })).await;
    assert_eq!(configured["status"], 200);
    assert_eq!(configured["body"]["secret_configured"], true);
    assert!(!configured.to_string().contains("not-returned-secret"));
    let generic = rpc(
        &client,
        &admin_url,
        ADMIN_TOKEN,
        "GET",
        "/admin/settings",
        json!({}),
    )
    .await;
    assert_eq!(generic["status"], 200);
    assert!(!generic.to_string().contains("not-returned-secret"));
    assert!(!generic.to_string().contains(&receiver.uri()));
    let invalid = rpc(
        &client,
        &admin_url,
        ADMIN_TOKEN,
        "PUT",
        resource,
        json!({"url": "http://remote.example"}),
    )
    .await;
    assert_eq!(invalid["status"], 400);
    assert!(invalid["error"].as_str().unwrap().contains("https://"));

    let account = rpc(
        &client,
        &admin_url,
        ADMIN_TOKEN,
        "POST",
        "/admin/accounts",
        json!({}),
    )
    .await;
    assert_eq!(account["status"], 201);
    set_setting(&servers.pool, settings_keys::REGISTRATION_OPEN, "true")
        .await
        .unwrap();
    set_setting(
        &servers.pool,
        settings_keys::JIT_REGISTRATION_ENABLED,
        "true",
    )
    .await
    .unwrap();
    servers.ctx.auth.hydrate(&servers.pool).await.unwrap();
    let www_url = format!("http://{}", servers.http_addr);
    let web_response = client
        .post(format!("{www_url}/new"))
        .header("Content-Type", "application/json")
        .body("{}")
        .send()
        .await
        .unwrap();
    assert!(web_response.status().is_success());
    let web: Value = serde_json::from_slice(&web_response.bytes().await.unwrap()).unwrap();
    let mut imap = ImapClient::connect(servers.imap_addr).await;
    let login = imap.command("a1 LOGIN jituser01@test longpassword1").await;
    assert!(login.contains("a1 OK"), "{login}");
    let login_again = imap.command("a2 LOGIN jituser01@test longpassword1").await;
    assert!(login_again.contains("a2 OK"), "{login_again}");

    let events = received(&receiver, 3).await;
    assert_eq!(events.len(), 3);
    for source in ["admin", "web", "jit"] {
        let event = events.iter().find(|e| e["source"] == source).unwrap();
        assert_eq!(event["event"], "user.registered");
        assert_eq!(event["registration_token_used"], false);
        assert!(event["timestamp"].as_u64().unwrap() > 0);
        assert!(event["username"].as_str().unwrap().ends_with("@test"));
        assert!(event.get("password").is_none());
        assert!(event.get("body").is_none());
        assert!(event.get("token").is_none());
    }
    servers.ctx.quota.set_max_bytes("jituser01@test", 1);
    let message = pgp_mime_for_user("jituser01@test");
    let append = imap
        .append_literal(&format!("a3 APPEND INBOX {{{}}}", message.len()), &message)
        .await;
    assert!(append.contains("a3 NO"), "{append}");
    let again = imap
        .append_literal(&format!("a4 APPEND INBOX {{{}}}", message.len()), &message)
        .await;
    assert!(again.contains("a4 NO"), "{again}");
    let events = received(&receiver, 4).await;
    let quota = &events[3];
    assert_eq!(quota["event"], "user.quota_exceeded");
    assert_eq!(quota["username"], "jituser01@test");
    assert_eq!(quota["path"], "imap.append");
    assert_eq!(quota["max_bytes"], 1);
    assert_eq!(quota["used_bytes"], 0);
    assert_eq!(quota["incoming_bytes"], message.len());

    // A separate account/path is not coalesced by the first user's dedup window.
    let sender = web["email"].as_str().unwrap();
    let password = web["password"].as_str().unwrap();
    servers.ctx.quota.set_max_bytes(sender, 1);
    let submitted = pgp_mime_for_user(sender);
    let rejected = support::smtp_submit(
        servers.smtp_addr,
        sender,
        sender,
        sender,
        password,
        std::str::from_utf8(&submitted).unwrap(),
    )
    .await;
    assert!(rejected.contains("552"), "{rejected}");
    let events = received(&receiver, 5).await;
    assert_eq!(events[4]["path"], "smtp.submission");
    assert_eq!(events[4]["username"], sender);

    let disabled = rpc(
        &client,
        &admin_url,
        ADMIN_TOKEN,
        "PUT",
        resource,
        json!({"event_user_registered": false}),
    )
    .await;
    assert_eq!(disabled["status"], 200);
    assert_eq!(
        rpc(
            &client,
            &admin_url,
            ADMIN_TOKEN,
            "POST",
            "/admin/accounts",
            json!({})
        )
        .await["status"],
        201
    );
    // Soft hydration retains settings and counters, and an explicit test bypasses event gates.
    servers.ctx.hydrate(&servers.pool, &config).await.unwrap();
    assert_eq!(
        rpc(
            &client,
            &admin_url,
            ADMIN_TOKEN,
            "POST",
            resource,
            json!({"action": "test"})
        )
        .await["status"],
        200
    );
    let events = received(&receiver, 6).await;
    assert_eq!(events.len(), 6);
    assert_eq!(events[5]["event"], "webhook.test");
    let snapshot = rpc(&client, &admin_url, ADMIN_TOKEN, "GET", resource, json!({})).await;
    assert_eq!(snapshot["body"]["successful_deliveries"], 6);
    assert_eq!(snapshot["body"]["consecutive_failures"], 0);
    assert!(!snapshot.to_string().contains("not-returned-secret"));
    // Public templates never receive operator configuration.
    let public = client
        .get(format!("{www_url}/"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(!public.contains(&receiver.uri()));
    assert!(!public.contains("not-returned-secret"));
    // Failed deliveries expose sanitized RPC diagnostics and recover their streak.
    receiver.reset().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&receiver)
        .await;
    let failed = rpc(
        &client,
        &admin_url,
        ADMIN_TOKEN,
        "POST",
        resource,
        json!({"action": "test"}),
    )
    .await;
    assert_eq!(failed["status"], 502);
    assert!(!failed.to_string().contains("not-returned-secret"));
    assert!(!failed.to_string().contains(&receiver.uri()));
    assert_eq!(
        rpc(&client, &admin_url, ADMIN_TOKEN, "GET", resource, json!({})).await["body"]
            ["consecutive_failures"],
        1
    );
    receiver.reset().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&receiver)
        .await;

    // Turning the quota event off changes notification policy, not quota rejection.
    assert_eq!(
        rpc(
            &client,
            &admin_url,
            ADMIN_TOKEN,
            "PUT",
            resource,
            json!({"event_quota_exceeded": false})
        )
        .await["status"],
        200
    );
    let user = account["body"]["email"].as_str().unwrap();
    let pass = account["body"]["password"].as_str().unwrap();
    let mut disabled_imap = ImapClient::connect(servers.imap_addr).await;
    assert!(disabled_imap
        .command(&format!("b1 LOGIN {user} {pass}"))
        .await
        .contains("b1 OK"));
    servers.ctx.quota.set_max_bytes(user, 1);
    let body = pgp_mime_for_user(user);
    assert!(disabled_imap
        .append_literal(&format!("b2 APPEND INBOX {{{}}}", body.len()), &body)
        .await
        .contains("b2 NO"));
    assert!(receiver.received_requests().await.unwrap().is_empty());
    assert_eq!(
        rpc(
            &client,
            &admin_url,
            ADMIN_TOKEN,
            "POST",
            resource,
            json!({"action": "test"})
        )
        .await["status"],
        200
    );
    assert_eq!(
        rpc(&client, &admin_url, ADMIN_TOKEN, "GET", resource, json!({})).await["body"]
            ["consecutive_failures"],
        0
    );
    assert_eq!(
        rpc(
            &client,
            &admin_url,
            ADMIN_TOKEN,
            "PUT",
            resource,
            json!({"enabled": false})
        )
        .await["status"],
        200
    );
    assert_eq!(
        rpc(
            &client,
            &admin_url,
            ADMIN_TOKEN,
            "POST",
            resource,
            json!({"action": "test"})
        )
        .await["status"],
        502
    );
    assert_eq!(receiver.received_requests().await.unwrap().len(), 1);
    handle.abort();
}

#[tokio::test]
async fn invite_metadata_and_concurrent_jit_emit_no_credentials_or_duplicate_events() {
    let dir = tempfile::tempdir().unwrap();
    let servers = spawn_mail_servers(dir.path()).await;
    let receiver = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&receiver)
        .await;
    servers
        .ctx
        .webhooks
        .update(
            serde_json::from_value(json!({
                "enabled": true, "url": receiver.uri(), "retry_attempts": 0
            }))
            .unwrap(),
        )
        .await
        .unwrap();
    set_setting(&servers.pool, settings_keys::REGISTRATION_OPEN, "true")
        .await
        .unwrap();
    set_setting(
        &servers.pool,
        settings_keys::JIT_REGISTRATION_ENABLED,
        "true",
    )
    .await
    .unwrap();
    servers.ctx.auth.hydrate(&servers.pool).await.unwrap();
    let token = "private-test-invitation";
    let admin = AdminState::new(
        servers.pool.clone(),
        Arc::clone(&servers.ctx),
        AppConfig::default(),
        dir.path().into(),
        "test".into(),
        ADMIN_TOKEN.into(),
        None,
    );
    let created = chatmail_admin::resources::dispatch(
        &admin,
        "POST",
        "/admin/registration-token",
        &json!({"token": token, "max_uses": 1}),
    )
    .await
    .unwrap();
    assert!((200..300).contains(&created.0));
    let url = format!("http://{}/new", servers.http_addr);
    let client = reqwest::Client::new();
    let invalid = client
        .post(&url)
        .header("Content-Type", "application/json")
        .body(json!({"token": "missing-invite"}).to_string())
        .send()
        .await
        .unwrap();
    assert!(!invalid.status().is_success());
    assert!(receiver.received_requests().await.unwrap().is_empty());
    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .body(json!({"token": token}).to_string())
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success());
    let credentials: Value = serde_json::from_slice(&response.bytes().await.unwrap()).unwrap();
    let context = chatmail_auth::AuthContext {
        pool: servers.pool.clone(),
        state: Arc::clone(&servers.ctx),
        primary_domain: "test".into(),
        jit_domain: None,
        credential_policy: chatmail_config::CredentialPolicy::default(),
    };
    let (first, second, third) = tokio::join!(
        chatmail_auth::authenticate(&context, "concurrentjit@test", "private-jit-password"),
        chatmail_auth::authenticate(&context, "concurrentjit@test", "private-jit-password"),
        chatmail_auth::authenticate(&context, "concurrentjit@test", "private-jit-password"),
    );
    first.unwrap();
    second.unwrap();
    third.unwrap();
    let events = received(&receiver, 2).await;
    assert_eq!(events.len(), 2);
    assert_eq!(events[0]["source"], "web");
    assert_eq!(events[0]["registration_token_used"], true);
    assert_eq!(events[0]["username"], credentials["email"]);
    assert_eq!(events[1]["source"], "jit");
    assert_eq!(events[1]["username"], "concurrentjit@test");
    for event in events {
        assert!(!event.to_string().contains(token));
        assert!(!event
            .to_string()
            .contains(credentials["password"].as_str().unwrap()));
        assert!(!event.to_string().contains("private-jit-password"));
        let object = event.as_object().unwrap();
        assert_eq!(object.len(), 5);
        assert!(object.keys().all(|key| [
            "event",
            "timestamp",
            "username",
            "source",
            "registration_token_used"
        ]
        .contains(&key.as_str())));
    }
}
