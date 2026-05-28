//! Delta Chat P2P flow against in-process chatmail (register → Secure Join → chat).
//!
//! For **Delta Chat Core** against a real `chatmail` binary, run:
//! `scripts/core-e2e.sh` (or `cargo test chatmail_rs_register_securejoin_and_chat` in `desktop/core`).

mod support;

use support::{run_p2p_chat_flow, run_p2p_chat_flow_via_smtp, spawn_mail_servers};

#[tokio::test]
async fn deltachat_p2p_register_securejoin_and_chat() {
    let dir = tempfile::tempdir().expect("tempdir");
    let servers = spawn_mail_servers(dir.path()).await;
    run_p2p_chat_flow(&servers).await;
}

#[tokio::test]
async fn deltachat_p2p_securejoin_and_chat_via_smtp() {
    let dir = tempfile::tempdir().expect("tempdir");
    let servers = spawn_mail_servers(dir.path()).await;
    run_p2p_chat_flow_via_smtp(&servers).await;
}
