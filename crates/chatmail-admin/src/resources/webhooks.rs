// Copyright (C) 2026 themadorg
// SPDX-License-Identifier: AGPL-3.0-or-later

use chatmail_state::webhooks::WebhookPatch;
use chatmail_types::ChatmailError;
use serde_json::Value;

use crate::AdminState;

pub async fn service(st: &AdminState, method: &str, body: &Value) -> super::AdminResult {
    match method {
        "GET" => Ok((200, Some(st.app.webhooks.snapshot()))),
        "PUT" => {
            let patch: WebhookPatch = serde_json::from_value(body.clone())
                .map_err(|_| (400, "invalid webhook settings".into()))?;
            st.app
                .webhooks
                .update(patch)
                .await
                .map_err(|error| match error {
                    ChatmailError::Config(message) => (400, message),
                    _ => (500, "failed to persist webhook settings".into()),
                })?;
            Ok((200, Some(st.app.webhooks.snapshot())))
        }
        "POST" => {
            if body.get("action").and_then(Value::as_str) != Some("test") {
                return Err((400, "action must be test".into()));
            }
            st.app.webhooks.send_test().await.map_err(|_| {
                (
                    502,
                    "operator webhook test delivery failed (check configuration and receiver)"
                        .into(),
                )
            })?;
            Ok((200, Some(st.app.webhooks.snapshot())))
        }
        _ => Err((405, "use GET, PUT or POST".into())),
    }
}
