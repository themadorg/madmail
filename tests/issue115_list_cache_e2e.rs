//! Issue #115: concurrent local delivery + IMAP listing must not lose messages.

use std::sync::Arc;
use std::time::Duration;

use chatmail_storage::list_mailbox_messages;

mod support;
use support::{create_user, deliver_message, spawn_mail_servers, ImapClient, PGP_MIME_BODY};

const BOB: &str = "bobsmith@test";
const PASS: &str = "longpassword1";

#[tokio::test]
async fn issue115_concurrent_deliver_and_list_sees_all() {
    let dir = tempfile::tempdir().expect("tempdir");
    let srv = spawn_mail_servers(dir.path()).await;
    create_user(&srv.ctx, &srv.pool, "alice123@test", PASS).await;
    create_user(&srv.ctx, &srv.pool, BOB, PASS).await;

    let n = 40usize;
    let ctx = Arc::clone(&srv.ctx);

    let writer = {
        let ctx = Arc::clone(&ctx);
        tokio::spawn(async move {
            for i in 0..n {
                deliver_message(&ctx, BOB, &format!("m{i}"), PGP_MIME_BODY).await;
                if i % 3 == 0 {
                    tokio::task::yield_now().await;
                }
            }
        })
    };

    let readers: Vec<_> = (0..6)
        .map(|_| {
            let ctx = Arc::clone(&ctx);
            tokio::spawn(async move {
                for _ in 0..80 {
                    let _ = list_mailbox_messages(&ctx.mailbox_store, BOB, "INBOX").await;
                    tokio::task::yield_now().await;
                }
            })
        })
        .collect();

    writer.await.expect("writer");
    for r in readers {
        r.await.expect("reader");
    }

    let listed = list_mailbox_messages(&srv.ctx.mailbox_store, BOB, "INBOX")
        .await
        .expect("list");
    assert_eq!(
        listed.len(),
        n,
        "all delivered messages must be listed; got {:?}",
        listed
            .iter()
            .map(|m| m.base_id.as_str())
            .collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn issue115_imap_search_sees_all_after_concurrent_deliver() {
    let dir = tempfile::tempdir().expect("tempdir");
    let srv = spawn_mail_servers(dir.path()).await;
    create_user(&srv.ctx, &srv.pool, "alice123@test", PASS).await;
    create_user(&srv.ctx, &srv.pool, BOB, PASS).await;

    let n = 25usize;
    let ctx = Arc::clone(&srv.ctx);
    let writer = {
        let ctx = Arc::clone(&ctx);
        tokio::spawn(async move {
            for i in 0..n {
                deliver_message(&ctx, BOB, &format!("imap-{i}"), PGP_MIME_BODY).await;
            }
        })
    };
    let reader = {
        let addr = srv.imap_addr;
        tokio::spawn(async move {
            for _ in 0..30 {
                let mut c = ImapClient::connect(addr).await;
                let _ = c.command(&format!("a LOGIN {BOB} {PASS}")).await;
                let _ = c.command("a SELECT INBOX").await;
                let _ = c.command("a SEARCH ALL").await;
                let _ = c.command("a LOGOUT").await;
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        })
    };
    writer.await.expect("writer");
    let _ = reader.await;

    let mut client = ImapClient::connect(srv.imap_addr).await;
    let login = client.command(&format!("a LOGIN {BOB} {PASS}")).await;
    assert!(login.contains("OK"), "login: {login}");
    let select = client.command("a SELECT INBOX").await;
    assert!(select.contains("OK"), "select: {select}");
    // * N EXISTS
    let exists = select
        .lines()
        .find_map(|l| {
            let parts: Vec<_> = l.split_whitespace().collect();
            if parts.len() == 3 && parts[0] == "*" && parts[2] == "EXISTS" {
                parts[1].parse::<usize>().ok()
            } else {
                None
            }
        })
        .unwrap_or(0);
    assert_eq!(
        exists, n,
        "IMAP SELECT EXISTS must match delivered count; response:\n{select}"
    );
}
