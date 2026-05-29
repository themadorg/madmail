//! End-to-end IMAP tests (TDD `03-imap-server.md`, Delta Chat client expectations).
//!
//! Covers every command implemented in `chatmail-imap` session: CAPABILITY, NOOP, LOGIN,
//! LIST, SELECT, EXAMINE, STATUS, FETCH, UID FETCH, APPEND, IDLE, GETQUOTA, GETMETADATA, LOGOUT.

mod support;

use std::sync::Arc;
use std::time::Duration;

use support::{
    create_user, deliver_message, pgp_mime_for_user, smtp_submit, spawn_mail_servers, ImapClient,
    PGP_MIME_BODY,
};

const USER: &str = "u@test";
const PASS: &str = "imap-secret";
const PEER: &str = "peer@test";

// --- Connection & capabilities (RFC 3501 + Chatmail extensions) ---

#[tokio::test]
async fn imap_e2e_greeting_and_capability() {
    let dir = tempfile::tempdir().expect("tempdir");
    let srv = spawn_mail_servers(dir.path()).await;
    create_user(&srv.ctx, &srv.pool, USER, PASS).await;

    let mut c = ImapClient::connect(srv.imap_addr).await;
    assert!(c.transcript().contains("IMAP4rev1 ready"));

    let r = c.command("c001 CAPABILITY").await;
    for cap in [
        "IMAP4rev1",
        "IDLE",
        "QUOTA",
        "MOVE",
        "AUTH=PLAIN",
        "XCHATMAIL",
    ] {
        assert!(r.contains(cap), "missing capability {cap}: {r}");
    }
}

#[tokio::test]
async fn imap_e2e_noop_and_logout() {
    let dir = tempfile::tempdir().expect("tempdir");
    let srv = spawn_mail_servers(dir.path()).await;
    create_user(&srv.ctx, &srv.pool, USER, PASS).await;

    let mut c = ImapClient::connect(srv.imap_addr).await;
    assert!(c.command("n001 NOOP").await.contains("OK NOOP"));
    let r = c.command("n002 LOGOUT").await;
    assert!(r.contains("BYE") || c.transcript().contains("BYE"));
}

// --- Authentication (TDD `05-authentication.md`) ---

#[tokio::test]
async fn imap_e2e_login_success_and_failure() {
    let dir = tempfile::tempdir().expect("tempdir");
    let srv = spawn_mail_servers(dir.path()).await;
    create_user(&srv.ctx, &srv.pool, USER, PASS).await;

    let mut ok = ImapClient::connect(srv.imap_addr).await;
    assert!(ok
        .command(&format!("a001 LOGIN {USER} {PASS}"))
        .await
        .contains("OK LOGIN"));

    let mut bad = ImapClient::connect(srv.imap_addr).await;
    let r = bad.command(&format!("b001 LOGIN {USER} wrong-pass")).await;
    assert!(
        !r.contains("OK LOGIN"),
        "bad password must not succeed: {r}"
    );
}

#[tokio::test]
async fn imap_e2e_select_requires_login() {
    let dir = tempfile::tempdir().expect("tempdir");
    let srv = spawn_mail_servers(dir.path()).await;

    let mut c = ImapClient::connect(srv.imap_addr).await;
    let r = c.command("s001 SELECT INBOX").await;
    assert!(
        r.contains("BAD") || r.contains("NO ") || !r.contains("OK [SELECT]"),
        "SELECT before LOGIN should fail: {r}"
    );
}

// --- Mailbox management: LIST, SELECT, EXAMINE, STATUS ---

#[tokio::test]
async fn imap_e2e_list_select_examine_status_empty_inbox() {
    let dir = tempfile::tempdir().expect("tempdir");
    let srv = spawn_mail_servers(dir.path()).await;
    create_user(&srv.ctx, &srv.pool, USER, PASS).await;

    let mut c = ImapClient::connect(srv.imap_addr).await;
    c.command(&format!("a001 LOGIN {USER} {PASS}")).await;

    let list = c.command("a002 LIST \"\" \"INBOX\"").await;
    assert!(list.contains("INBOX"), "LIST: {list}");

    let sel = c.command("a003 SELECT INBOX").await;
    assert!(sel.contains("EXISTS"), "SELECT: {sel}");
    assert!(sel.contains("UIDNEXT"), "SELECT UIDNEXT: {sel}");
    assert!(sel.contains("UIDVALIDITY"), "SELECT: {sel}");
    assert!(sel.contains("OK [SELECT]"), "SELECT: {sel}");

    let exa = c.command("a004 EXAMINE INBOX").await;
    assert!(exa.contains("EXISTS"), "EXAMINE: {exa}");

    let st = c
        .command("a005 STATUS INBOX (MESSAGES UIDNEXT UIDVALIDITY UNSEEN)")
        .await;
    assert!(st.contains("STATUS"), "STATUS: {st}");
    assert!(st.contains("UIDNEXT"), "STATUS: {st}");
}

// --- FETCH / UID FETCH (Delta Chat prefetch + download) ---

#[tokio::test]
async fn imap_e2e_fetch_header_fields_and_body() {
    let dir = tempfile::tempdir().expect("tempdir");
    let srv = spawn_mail_servers(dir.path()).await;
    create_user(&srv.ctx, &srv.pool, USER, PASS).await;
    deliver_message(&srv.ctx, USER, "m1", PGP_MIME_BODY).await;

    let mut c = ImapClient::connect(srv.imap_addr).await;
    c.command(&format!("f001 LOGIN {USER} {PASS}")).await;
    c.command("f002 SELECT INBOX").await;

    let hdr = c
        .command("f003 UID FETCH 1 (UID RFC822.SIZE BODY.PEEK[HEADER.FIELDS (MESSAGE-ID FROM)])")
        .await;
    assert!(hdr.contains("RFC822.SIZE"), "header fetch: {hdr}");
    assert!(
        hdr.contains("MESSAGE-ID") || hdr.contains("From:"),
        "headers: {hdr}"
    );

    let body = c.command("f004 UID FETCH 1 (BODY.PEEK[])").await;
    assert!(
        body.contains("application/pgp-encrypted") || body.contains("multipart/encrypted"),
        "body fetch: {body}"
    );
}

#[tokio::test]
async fn imap_e2e_fetch_sequence_set() {
    let dir = tempfile::tempdir().expect("tempdir");
    let srv = spawn_mail_servers(dir.path()).await;
    create_user(&srv.ctx, &srv.pool, USER, PASS).await;
    deliver_message(&srv.ctx, USER, "m1", PGP_MIME_BODY).await;
    deliver_message(&srv.ctx, USER, "m2", PGP_MIME_BODY).await;

    let mut c = ImapClient::connect(srv.imap_addr).await;
    c.command(&format!("u001 LOGIN {USER} {PASS}")).await;
    let sel = c.command("u002 SELECT INBOX").await;
    assert!(sel.contains("* 2 EXISTS"), "two messages: {sel}");

    let fetch = c.command("u003 UID FETCH 1:2 (UID RFC822.SIZE)").await;
    assert!(
        fetch.contains("UID 1") || fetch.contains("UID 2"),
        "{fetch}"
    );
    assert!(fetch.contains("RFC822.SIZE"), "{fetch}");
}

// --- APPEND + PGP enforcement (TDD `03-imap-server.md`, `12-security.md`) ---

#[tokio::test]
async fn imap_e2e_append_encrypted_visible_after_select() {
    let dir = tempfile::tempdir().expect("tempdir");
    let srv = spawn_mail_servers(dir.path()).await;
    create_user(&srv.ctx, &srv.pool, USER, PASS).await;

    let body = pgp_mime_for_user(USER);
    let mut c = ImapClient::connect(srv.imap_addr).await;
    c.command(&format!("p001 LOGIN {USER} {PASS}")).await;
    let append = c
        .append_literal(&format!("p002 APPEND INBOX {{{}}}", body.len()), &body)
        .await;
    assert!(append.contains("OK APPEND"), "append: {append}");

    let sel = c.command("p003 SELECT INBOX").await;
    assert!(sel.contains("* 1 EXISTS"), "after append: {sel}");
}

#[tokio::test]
async fn imap_e2e_append_plaintext_rejected() {
    let dir = tempfile::tempdir().expect("tempdir");
    let srv = spawn_mail_servers(dir.path()).await;
    create_user(&srv.ctx, &srv.pool, USER, PASS).await;

    let plain = b"From: u@test\r\nSubject: x\r\nContent-Type: text/plain\r\n\r\nn";
    let mut c = ImapClient::connect(srv.imap_addr).await;
    c.command(&format!("e001 LOGIN {USER} {PASS}")).await;
    let r = c
        .append_literal(&format!("e002 APPEND INBOX {{{}}}", plain.len()), plain)
        .await;
    assert!(r.contains("NO [ENCRYPTED]"), "plaintext append: {r}");
}

// --- QUOTA (RFC 2087, Madmail GETQUOTA only) ---

#[tokio::test]
async fn imap_e2e_getquota_and_getquotaroot() {
    let dir = tempfile::tempdir().expect("tempdir");
    let srv = spawn_mail_servers(dir.path()).await;
    create_user(&srv.ctx, &srv.pool, USER, PASS).await;

    let mut c = ImapClient::connect(srv.imap_addr).await;
    c.command(&format!("q001 LOGIN {USER} {PASS}")).await;

    let quota = c.command("q002 GETQUOTA \"ROOT\"").await;
    assert!(quota.contains("QUOTA"), "GETQUOTA: {quota}");
    assert!(quota.contains("STORAGE"), "GETQUOTA storage: {quota}");

    let root = c.command("q003 GETQUOTAROOT INBOX").await;
    assert!(
        root.contains("QUOTA") || root.contains("OK GETQUOTA"),
        "GETQUOTAROOT: {root}"
    );
}

// --- METADATA TURN: see tests/turn_e2e.rs (Phase 9) ---

// --- IDLE (RFC 2177, primary Delta Chat push) ---

#[tokio::test]
async fn imap_e2e_idle_unsolicited_exists_on_event() {
    let dir = tempfile::tempdir().expect("tempdir");
    let srv = spawn_mail_servers(dir.path()).await;
    create_user(&srv.ctx, &srv.pool, USER, PASS).await;
    deliver_message(&srv.ctx, USER, "m1", PGP_MIME_BODY).await;

    let ctx = Arc::clone(&srv.ctx);
    let mut c = ImapClient::connect(srv.imap_addr).await;
    c.command(&format!("i001 LOGIN {USER} {PASS}")).await;
    c.command("i002 SELECT INBOX").await;
    assert!(c.idle_start("i003").await.contains("+ idling"));

    deliver_message(&ctx, USER, "m2", PGP_MIME_BODY).await;

    let push = c.read_until("* 2 EXISTS", Duration::from_secs(2)).await;
    assert!(push.contains("* 2 EXISTS"), "IDLE EXISTS: {push}");
    assert!(push.contains("RECENT"), "IDLE RECENT: {push}");

    let end = c.idle_done("i003").await;
    assert!(end.contains("IDLE terminated"), "IDLE done: {end}");
}

#[tokio::test]
async fn imap_e2e_idle_requires_selected_mailbox() {
    let dir = tempfile::tempdir().expect("tempdir");
    let srv = spawn_mail_servers(dir.path()).await;
    create_user(&srv.ctx, &srv.pool, USER, PASS).await;

    let mut c = ImapClient::connect(srv.imap_addr).await;
    c.command(&format!("d001 LOGIN {USER} {PASS}")).await;
    let r = c.command("d002 IDLE").await;
    assert!(r.contains("BAD"), "IDLE without SELECT: {r}");
}

#[tokio::test]
async fn imap_e2e_idle_tagged_done_ends_session() {
    let dir = tempfile::tempdir().expect("tempdir");
    let srv = spawn_mail_servers(dir.path()).await;
    create_user(&srv.ctx, &srv.pool, USER, PASS).await;
    deliver_message(&srv.ctx, USER, "m1", PGP_MIME_BODY).await;

    let mut c = ImapClient::connect(srv.imap_addr).await;
    c.command(&format!("t001 LOGIN {USER} {PASS}")).await;
    c.command("t002 SELECT INBOX").await;
    c.idle_start("t003").await;
    c.send_line("t003 DONE").await;
    let end = c.read_until("t003 OK", Duration::from_secs(2)).await;
    assert!(end.contains("IDLE terminated"), "tagged DONE: {end}");
}

// --- Cross-protocol: SMTP delivery wakes IMAP IDLE (TDD `02` + `03`) ---

#[tokio::test]
async fn imap_e2e_idle_after_smtp_local_delivery() {
    let dir = tempfile::tempdir().expect("tempdir");
    let srv = spawn_mail_servers(dir.path()).await;
    create_user(&srv.ctx, &srv.pool, USER, PASS).await;
    create_user(&srv.ctx, &srv.pool, PEER, PASS).await;

    let body = String::from_utf8_lossy(&pgp_mime_for_user(PEER))
        .replace("From: u@test", &format!("From: {PEER}"))
        .replace("To: u@test", &format!("To: {USER}"));
    let mut c = ImapClient::connect(srv.imap_addr).await;
    c.command(&format!("x001 LOGIN {USER} {PASS}")).await;
    c.command("x002 SELECT INBOX").await;
    c.idle_start("x003").await;

    let smtp_log = smtp_submit(srv.smtp_addr, PEER, USER, PEER, PASS, &body).await;
    assert!(smtp_log.contains("250 2.0.0 OK"), "smtp: {smtp_log}");

    let push = c.read_until("EXISTS", Duration::from_secs(3)).await;
    assert!(push.contains("EXISTS"), "IDLE after SMTP: {push}");
    c.idle_done("x003").await;
}

// --- UID STORE / MOVE / COPY (Delta Chat sync cleanup) ---

#[tokio::test]
async fn imap_e2e_uid_store_seen() {
    let dir = tempfile::tempdir().expect("tempdir");
    let srv = spawn_mail_servers(dir.path()).await;
    create_user(&srv.ctx, &srv.pool, USER, PASS).await;
    deliver_message(&srv.ctx, USER, "m1", PGP_MIME_BODY).await;

    let mut c = ImapClient::connect(srv.imap_addr).await;
    c.command(&format!("s001 LOGIN {USER} {PASS}")).await;
    c.command("s002 SELECT INBOX").await;
    let r = c.command("s003 UID STORE 1 +FLAGS (\\Seen)").await;
    assert!(r.contains("OK UID STORE"), "STORE: {r}");
    assert!(r.contains("\\Seen"), "STORE flags: {r}");

    let fetch = c.command("s004 UID FETCH 1 (FLAGS)").await;
    assert!(fetch.contains("\\Seen"), "seen after STORE: {fetch}");
}

#[tokio::test]
async fn imap_e2e_uid_store_deleted_and_close_expunge() {
    let dir = tempfile::tempdir().expect("tempdir");
    let srv = spawn_mail_servers(dir.path()).await;
    create_user(&srv.ctx, &srv.pool, USER, PASS).await;
    deliver_message(&srv.ctx, USER, "m1", PGP_MIME_BODY).await;

    let mut c = ImapClient::connect(srv.imap_addr).await;
    c.command(&format!("d001 LOGIN {USER} {PASS}")).await;
    c.command("d002 SELECT INBOX").await;
    let r = c.command("d003 UID STORE 1 +FLAGS (\\Deleted)").await;
    assert!(r.contains("OK UID STORE"), "delete flag: {r}");
    c.command("d004 CLOSE").await;
    let sel = c.command("d005 SELECT INBOX").await;
    assert!(sel.contains("* 0 EXISTS"), "expunged: {sel}");
}

#[tokio::test]
async fn imap_e2e_uid_move_to_deltachat() {
    let dir = tempfile::tempdir().expect("tempdir");
    let srv = spawn_mail_servers(dir.path()).await;
    create_user(&srv.ctx, &srv.pool, USER, PASS).await;
    deliver_message(&srv.ctx, USER, "m1", PGP_MIME_BODY).await;

    let mut c = ImapClient::connect(srv.imap_addr).await;
    c.command(&format!("v001 LOGIN {USER} {PASS}")).await;
    c.command("v002 SELECT INBOX").await;
    let r = c.command("v003 UID MOVE 1 DeltaChat").await;
    assert!(r.contains("OK UID MOVE"), "MOVE: {r}");

    let inbox = c.command("v004 SELECT INBOX").await;
    assert!(inbox.contains("* 0 EXISTS"), "inbox empty: {inbox}");

    let mv = c.command("v005 SELECT DeltaChat").await;
    assert!(mv.contains("* 1 EXISTS"), "mvbox has message: {mv}");
}

#[tokio::test]
async fn imap_e2e_uid_copy_to_deltachat() {
    let dir = tempfile::tempdir().expect("tempdir");
    let srv = spawn_mail_servers(dir.path()).await;
    create_user(&srv.ctx, &srv.pool, USER, PASS).await;
    deliver_message(&srv.ctx, USER, "m1", PGP_MIME_BODY).await;

    let mut c = ImapClient::connect(srv.imap_addr).await;
    c.command(&format!("c001 LOGIN {USER} {PASS}")).await;
    c.command("c002 SELECT INBOX").await;
    let r = c.command("c003 UID COPY 1 DeltaChat").await;
    assert!(r.contains("OK UID COPY"), "COPY: {r}");

    let inbox = c.command("c004 SELECT INBOX").await;
    assert!(inbox.contains("* 1 EXISTS"), "inbox kept: {inbox}");

    let mv = c.command("c005 SELECT DeltaChat").await;
    assert!(mv.contains("* 1 EXISTS"), "copy landed: {mv}");
}

// --- Full Delta Chat-style session (TDD `03-imap-server.md` client sequence) ---

#[tokio::test]
async fn imap_e2e_delta_chat_sync_session() {
    let dir = tempfile::tempdir().expect("tempdir");
    let srv = spawn_mail_servers(dir.path()).await;
    create_user(&srv.ctx, &srv.pool, USER, PASS).await;
    deliver_message(&srv.ctx, USER, "sync-1", PGP_MIME_BODY).await;

    let mut c = ImapClient::connect(srv.imap_addr).await;
    let caps = c.command("dc01 CAPABILITY").await;
    assert!(caps.contains("IDLE") && caps.contains("XCHATMAIL"));

    assert!(c
        .command(&format!("dc02 LOGIN {USER} {PASS}"))
        .await
        .contains("OK LOGIN"));

    assert!(c.command("dc03 LIST \"\" \"*\"").await.contains("INBOX"));

    let sel = c.command("dc04 SELECT INBOX").await;
    assert!(sel.contains("UIDNEXT") && sel.contains("EXISTS"));

    assert!(c
        .command("dc05 STATUS INBOX (UIDNEXT MESSAGES)")
        .await
        .contains("STATUS"));

    let fetch = c
        .command(
            "dc06 UID FETCH 1 (UID INTERNALDATE RFC822.SIZE \
             BODY.PEEK[HEADER.FIELDS (MESSAGE-ID FROM DATE)])",
        )
        .await;
    assert!(fetch.contains("RFC822.SIZE"), "prefetch: {fetch}");

    assert!(c.command("dc07 LOGOUT").await.contains("OK") || c.transcript().contains("BYE"));
}
