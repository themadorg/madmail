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

use axum::{
    http::{header, StatusCode},
    response::IntoResponse,
    routing::get,
    Router,
};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::metrics::{gather_bytes, init_metrics};

async fn metrics_handler() -> impl IntoResponse {
    match gather_bytes() {
        Ok(body) => {
            let text = String::from_utf8(body).unwrap_or_else(|e| {
                String::from_utf8_lossy(&e.into_bytes()).into_owned()
            });
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "text/plain; version=0.0.4; charset=utf-8")],
                text,
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("metrics encode failed: {e}"),
        )
            .into_response(),
    }
}

/// Serve `GET /metrics` until `cancel` is triggered (Madmail `openmetrics` module).
pub async fn run_openmetrics_listener(
    addr: &str,
    cancel: CancellationToken,
) -> chatmail_types::Result<()> {
    init_metrics();
    let listener = TcpListener::bind(addr).await.map_err(|e| {
        chatmail_types::ChatmailError::config(format!("openmetrics bind {addr}: {e}"))
    })?;
    let app = Router::new().route("/metrics", get(metrics_handler));
    info!(%addr, "openmetrics listening");

    let serve = axum::serve(listener, app).with_graceful_shutdown(async move {
        cancel.cancelled().await;
    });

    serve.await.map_err(|e| {
        chatmail_types::ChatmailError::config(format!("openmetrics serve {addr}: {e}"))
    })?;
    info!(%addr, "openmetrics listener stopped");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{to_bytes, Body};
    use axum::http::Request;
    use tower::ServiceExt;

    #[tokio::test]
    async fn metrics_handler_returns_prometheus_text() {
        init_metrics();
        crate::metrics::record_smtp_started("handler_test");
        let app = Router::new().route("/metrics", get(metrics_handler));
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/metrics")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let text = String::from_utf8_lossy(&body);
        assert!(
            text.contains("maddy_smtp_started_transactions"),
            "body: {text}"
        );
    }
}
