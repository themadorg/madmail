//! Live Docker E2E: IMAP GETMETADATA + TURN Allocate against a running container.
//!
//! Not part of default `cargo test` (`#[ignore]`). Run manually after starting a
//! Docker madmail with TURN, with `DOCKER_TURN_*` env vars set:
//! `cargo test -p chatmail-integration --test docker_turn_e2e -- --ignored`.

mod support;

use std::net::SocketAddr;

use chatmail_turn::{parse_turn_metadata, turn_allocate};
use support::ImapClient;

fn extract_turn_metadata_value(imap_response: &str) -> String {
    let marker = "/shared/vendor/deltachat/turn";
    let idx = imap_response
        .find(marker)
        .expect("turn key in METADATA response");
    let after = &imap_response[idx + marker.len()..];
    let start = after.find('"').expect("opening quote") + 1;
    let rest = &after[start..];
    let end = rest.find('"').expect("closing quote");
    rest[..end].to_string()
}

fn env_required(key: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| panic!("{key} must be set for docker_turn_live_allocate"))
}

fn imap_quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

#[tokio::test]
#[ignore = "live docker e2e — run via scripts/docker-turn-e2e.sh"]
async fn docker_turn_live_allocate() {
    let imap_addr: SocketAddr = env_required("DOCKER_TURN_IMAP_ADDR")
        .parse()
        .expect("DOCKER_TURN_IMAP_ADDR");
    let control_addr: SocketAddr = env_required("DOCKER_TURN_CONTROL_ADDR")
        .parse()
        .expect("DOCKER_TURN_CONTROL_ADDR");
    let user = env_required("DOCKER_TURN_USER");
    let pass = env_required("DOCKER_TURN_PASS");
    let secret = env_required("DOCKER_TURN_SECRET");
    let relay_min: u16 = env_required("DOCKER_TURN_RELAY_MIN")
        .parse()
        .expect("DOCKER_TURN_RELAY_MIN");
    let relay_max: u16 = env_required("DOCKER_TURN_RELAY_MAX")
        .parse()
        .expect("DOCKER_TURN_RELAY_MAX");

    let mut c = ImapClient::connect_tls_insecure(imap_addr).await;
    let login = c
        .command(&format!(
            "a001 LOGIN {} {}",
            imap_quote(&user),
            imap_quote(&pass)
        ))
        .await;
    assert!(login.contains("OK LOGIN"), "LOGIN failed: {login}");
    let r = c
        .command("a002 GETMETADATA \"\" (/shared/vendor/deltachat/turn)")
        .await;
    assert!(
        r.contains("/shared/vendor/deltachat/turn"),
        "GETMETADATA: {r}"
    );

    let line = extract_turn_metadata_value(&r);
    let parsed = parse_turn_metadata(&line).expect("parse metadata");
    assert_eq!(parsed.port, control_addr.port());

    let username = parsed.expiration_timestamp.to_string();
    let realm = parsed.hostname.clone();
    let relay = turn_allocate(control_addr, &secret, &realm, &username)
        .await
        .expect("TURN allocate through Docker UDP mapping");

    assert_ne!(
        relay.port(),
        control_addr.port(),
        "relay port must differ from control port"
    );
    assert!(
        relay.port() >= relay_min && relay.port() <= relay_max,
        "relay port {} outside configured range {}-{}",
        relay.port(),
        relay_min,
        relay_max
    );
}
