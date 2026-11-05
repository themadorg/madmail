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

//! Binary WebSocket transport for the existing native mail sessions.
//!
//! The trusted TLS marker is installed by the HTTP TLS listener, never from headers.
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{
    rejection::WebSocketUpgradeRejection, Message, WebSocket, WebSocketUpgrade,
};
use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::Extension;
use chatmail_db::{get_bool_setting, settings_keys};
use chatmail_imap::{ImapSession, ImapSessionConfig};
use chatmail_smtp::{SmtpSession, SmtpSessionConfig};
use chatmail_state::SecureHttp;
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};
use tokio::sync::{watch, Mutex, Semaphore, SemaphorePermit};
use tokio::time::{timeout, timeout_at, Instant};
use tokio_util::sync::CancellationToken;

use crate::WwwState;

const CHUNK: usize = 16 * 1024;
const MAX_MESSAGE: usize = 64 * 1024;
const WRITE_TIMEOUT: Duration = Duration::from_secs(30);
const IDLE_TIMEOUT: Duration = Duration::from_secs(35 * 60);
const AUTH_TIMEOUT: Duration = Duration::from_secs(20);
static PRE_AUTH: Semaphore = Semaphore::const_new(32);
static CONNECTIONS: Semaphore = Semaphore::const_new(256);

/// Supplied by the supervisor so all transports use the same mail policy.
#[derive(Clone)]
pub struct BridgeSessions {
    pub imap: ImapSessionConfig,
    pub submission: SmtpSessionConfig,
}

#[derive(Clone, Copy)]
enum Protocol {
    Imap,
    Smtp,
}

/// Process lifetime token, independent of HTTP listener reloads.
#[derive(Clone)]
pub struct BridgeShutdown(pub CancellationToken);

type OptionalExtension<T> = Option<Extension<T>>;

pub async fn imap(
    State(st): State<WwwState>,
    secure: OptionalExtension<SecureHttp>,
    sessions: OptionalExtension<BridgeSessions>,
    cancel: OptionalExtension<BridgeShutdown>,
    headers: HeaderMap,
    uri: Uri,
    ws: Result<WebSocketUpgrade, WebSocketUpgradeRejection>,
) -> Response {
    upgrade(
        st,
        secure,
        sessions,
        cancel,
        headers,
        uri,
        ws,
        Protocol::Imap,
    )
    .await
}

pub async fn smtp(
    State(st): State<WwwState>,
    secure: OptionalExtension<SecureHttp>,
    sessions: OptionalExtension<BridgeSessions>,
    cancel: OptionalExtension<BridgeShutdown>,
    headers: HeaderMap,
    uri: Uri,
    ws: Result<WebSocketUpgrade, WebSocketUpgradeRejection>,
) -> Response {
    upgrade(
        st,
        secure,
        sessions,
        cancel,
        headers,
        uri,
        ws,
        Protocol::Smtp,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn upgrade(
    st: WwwState,
    secure: OptionalExtension<SecureHttp>,
    sessions: OptionalExtension<BridgeSessions>,
    cancel: OptionalExtension<BridgeShutdown>,
    headers: HeaderMap,
    uri: Uri,
    ws: Result<WebSocketUpgrade, WebSocketUpgradeRejection>,
    protocol: Protocol,
) -> Response {
    let enabled = get_bool_setting(
        &st.pool,
        settings_keys::WEBSOCKET_BRIDGE_ENABLED,
        st.config.websocket_bridge_enabled,
    )
    .await
    .unwrap_or(false);
    if !enabled {
        return StatusCode::NOT_FOUND.into_response();
    }
    if secure.is_none() {
        return StatusCode::NOT_FOUND.into_response();
    }
    if uri.query().is_some() {
        return (
            StatusCode::BAD_REQUEST,
            "query parameters are not supported",
        )
            .into_response();
    }
    if headers.contains_key(header::ORIGIN) {
        let allowed = crate::cors::load_origins(&st.pool).await;
        if !bridge_origin_allowed(&headers, &allowed) {
            return StatusCode::FORBIDDEN.into_response();
        }
    }
    let (Some(Extension(sessions)), Some(Extension(cancel))) = (sessions, cancel) else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    // Defense in depth: never create an inbound MX session through this route.
    if !sessions.submission.require_auth {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    let ws = match ws {
        Ok(ws) => ws,
        Err(rejection) => return rejection.into_response(),
    };
    let Ok(pre_auth) = PRE_AUTH.try_acquire() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let Ok(permit) = CONNECTIONS.try_acquire() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    ws.max_message_size(MAX_MESSAGE)
        .max_frame_size(MAX_MESSAGE)
        .on_upgrade(move |socket| async move {
            let _permit = permit;
            run(socket, st, sessions, protocol, cancel.0, pre_auth).await;
        })
        .into_response()
}

fn bridge_origin_allowed(headers: &HeaderMap, allowed: &[String]) -> bool {
    let mut values = headers.get_all(header::ORIGIN).iter();
    let Some(value) = values.next() else {
        return true;
    };
    if values.next().is_some() {
        return false;
    }
    value.to_str().is_ok_and(|origin| {
        crate::cors::is_valid_browser_origin(origin)
            && allowed.iter().any(|entry| entry != "*" && entry == origin)
    })
}

async fn run(
    socket: WebSocket,
    st: WwwState,
    sessions: BridgeSessions,
    protocol: Protocol,
    cancel: CancellationToken,
    pre_auth: SemaphorePermit<'static>,
) {
    let (client, server) = tokio::io::duplex(MAX_MESSAGE);
    let authenticated = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&authenticated);
    let mail = async move {
        match protocol {
            Protocol::Imap => {
                let _metrics = chatmail_metrics::conn_guard("imap");
                ImapSession::new(st.app, st.pool, sessions.imap)
                    .handle_secure_transport(server, flag)
                    .await
            }
            Protocol::Smtp => {
                let _metrics = chatmail_metrics::conn_guard("submission");
                SmtpSession::new(st.app, st.pool, sessions.submission)
                    .handle_secure_transport(server, flag)
                    .await
            }
        }
    };
    let (sink, source) = socket.split();
    let sink = Arc::new(Mutex::new(sink));
    let work = async {
        // Joining drains final protocol replies before the WebSocket closes.
        let _ = tokio::join!(mail, relay(Arc::clone(&sink), source, client));
    };
    let expired = tokio::select! {
        _ = work => false,
        _ = cancel.cancelled() => false,
        _ = authentication_deadline(authenticated, pre_auth) => true,
    };
    // The mail and relay futures have been dropped before writing the goodbye.
    if expired {
        let goodbye = match protocol {
            Protocol::Imap => b"* BYE Authentication timeout\r\n".as_slice(),
            Protocol::Smtp => b"421 Authentication timeout\r\n".as_slice(),
        };
        let _ = timeout(WRITE_TIMEOUT, async {
            let mut sink = sink.lock().await;
            sink.send(Message::Binary(goodbye.to_vec().into())).await?;
            sink.close().await
        })
        .await;
    }
}

async fn authentication_deadline(authenticated: Arc<AtomicBool>, pre_auth: SemaphorePermit<'_>) {
    let deadline = Instant::now() + AUTH_TIMEOUT;
    loop {
        if authenticated.load(Ordering::Acquire) {
            drop(pre_auth);
            std::future::pending::<()>().await;
            return;
        }
        if Instant::now() >= deadline {
            return;
        }
        tokio::time::sleep_until(deadline.min(Instant::now() + Duration::from_millis(50))).await;
    }
}

type SocketSink = Arc<Mutex<SplitSink<WebSocket, Message>>>;

async fn relay(
    sink: SocketSink,
    mut source: SplitStream<WebSocket>,
    stream: DuplexStream,
) -> Result<(), ()> {
    let (activity, mut last_activity) = watch::channel(Instant::now());
    let incoming_activity = activity.clone();
    let (mut reader, mut writer) = tokio::io::split(stream);
    let inbound_sink = Arc::clone(&sink);
    let incoming = async move {
        loop {
            let message = source.next().await.ok_or(())?.map_err(|_| ())?;
            incoming_activity.send_replace(Instant::now());
            match message {
                Message::Binary(bytes) => {
                    timeout(WRITE_TIMEOUT, writer.write_all(&bytes))
                        .await
                        .map_err(|_| ())?
                        .map_err(|_| ())?;
                }
                Message::Ping(_) | Message::Pong(_) => {
                    // Tungstenite queues Pong automatically; flush even if mail is idle.
                    timeout(WRITE_TIMEOUT, async {
                        inbound_sink.lock().await.flush().await
                    })
                    .await
                    .map_err(|_| ())?
                    .map_err(|_| ())?;
                }
                Message::Close(_) => return Ok(()),
                Message::Text(_) => return Err(()),
            }
        }
    };
    let outgoing = async move {
        let mut buf = vec![0; CHUNK];
        loop {
            let count = reader.read(&mut buf).await.map_err(|_| ())?;
            activity.send_replace(Instant::now());
            if count == 0 {
                let _ = timeout(WRITE_TIMEOUT, async { sink.lock().await.close().await }).await;
                return Ok::<(), ()>(());
            }
            timeout(WRITE_TIMEOUT, async {
                sink.lock()
                    .await
                    .send(Message::Binary(buf[..count].to_vec().into()))
                    .await
            })
            .await
            .map_err(|_| ())?
            .map_err(|_| ())?;
        }
    };
    let idle = async move {
        loop {
            let deadline = *last_activity.borrow_and_update() + IDLE_TIMEOUT;
            if !matches!(
                timeout_at(deadline, last_activity.changed()).await,
                Ok(Ok(()))
            ) {
                return Err(());
            }
        }
    };
    tokio::select! {
        result = incoming => result,
        result = outgoing => result,
        result = idle => result,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn bridge_default_and_db_override_apply_before_upgrade() {
        use tower::ServiceExt;
        for (configured, stored, expected) in [
            (false, None, StatusCode::NOT_FOUND),
            (true, None, StatusCode::SERVICE_UNAVAILABLE),
            (true, Some("false"), StatusCode::NOT_FOUND),
            (false, Some("true"), StatusCode::SERVICE_UNAVAILABLE),
        ] {
            let pool = chatmail_db::init_memory_db().await.unwrap();
            if let Some(value) = stored {
                chatmail_db::set_setting(&pool, settings_keys::WEBSOCKET_BRIDGE_ENABLED, value)
                    .await
                    .unwrap();
            }
            let dir = tempfile::tempdir().unwrap();
            let app = Arc::new(chatmail_state::AppState::new(dir.path(), pool.clone()));
            let config = chatmail_config::AppConfig {
                websocket_bridge_enabled: configured,
                ..Default::default()
            };
            let router = crate::www_router(WwwState::new(pool, app, config, dir.path()));
            for path in ["/transport/imap", "/transport/smtp"] {
                let mut req = axum::http::Request::builder()
                    .uri(path)
                    .header("X-Forwarded-Proto", "https")
                    .body(axum::body::Body::empty())
                    .unwrap();
                req.extensions_mut().insert(SecureHttp);
                assert_eq!(
                    router.clone().oneshot(req).await.unwrap().status(),
                    expected
                );
            }
        }
    }

    #[tokio::test]
    async fn authentication_releases_pre_auth_budget_before_session_ends() {
        let budget = Semaphore::new(1);
        let permit = budget.try_acquire().unwrap();
        let authenticated = Arc::new(AtomicBool::new(false));
        let deadline = authentication_deadline(Arc::clone(&authenticated), permit);
        tokio::pin!(deadline);
        assert!(timeout(Duration::from_millis(1), &mut deadline)
            .await
            .is_err());
        assert!(budget.try_acquire().is_err());
        authenticated.store(true, Ordering::Release);
        assert!(timeout(Duration::from_millis(100), &mut deadline)
            .await
            .is_err());
        assert!(budget.try_acquire().is_ok());
    }

    #[tokio::test]
    async fn plaintext_never_exposes_bridge_status() {
        use tower::ServiceExt;
        let pool = chatmail_db::init_memory_db().await.unwrap();
        let dir = tempfile::tempdir().unwrap();
        let app = Arc::new(chatmail_state::AppState::new(dir.path(), pool.clone()));
        let router = crate::www_router(WwwState::new(
            pool.clone(),
            app,
            Default::default(),
            dir.path(),
        ));
        for enabled in ["false", "true"] {
            chatmail_db::set_setting(&pool, settings_keys::WEBSOCKET_BRIDGE_ENABLED, enabled)
                .await
                .unwrap();
            for path in ["/transport/imap", "/transport/smtp"] {
                let req = axum::http::Request::builder()
                    .uri(path)
                    .header("X-Forwarded-Proto", "https")
                    .body(axum::body::Body::empty())
                    .unwrap();
                assert_eq!(
                    router.clone().oneshot(req).await.unwrap().status(),
                    StatusCode::NOT_FOUND
                );
            }
        }
    }

    #[test]
    fn origins_require_exact_allowlist_and_reject_multiple_headers() {
        let mut headers = HeaderMap::new();
        assert!(bridge_origin_allowed(&headers, &[]));
        headers.insert(header::ORIGIN, "https://client.example".parse().unwrap());
        assert!(!bridge_origin_allowed(&headers, &[]));
        assert!(!bridge_origin_allowed(&headers, &["*".into()]));
        assert!(bridge_origin_allowed(
            &headers,
            &["https://client.example".into()]
        ));
        headers.append(header::ORIGIN, "https://evil.example".parse().unwrap());
        assert!(!bridge_origin_allowed(
            &headers,
            &["https://client.example".into()]
        ));
    }
}
