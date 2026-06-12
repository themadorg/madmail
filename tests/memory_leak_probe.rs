//! Empirical probes for suspected long-lived memory growth in madmail.
//!
//! These tests do not fix anything — they exercise realistic workloads and measure
//! observable growth (RSS + public/internal map sizes where accessible).

mod support;

use std::io::Cursor;
use std::sync::Arc;

use chatmail_auth::AuthContext;
use chatmail_config::CredentialPolicy;
use chatmail_state::AppState;
use chatmail_storage::{
    delivery_batch::{DeliveryBatcher, PendingDelivery},
    list_mailbox_messages, never_delivery_batcher_coordinator_count, write_blob,
    write_blob_mailbox_stream, FsyncMode, MailboxStore, StoragePolicy,
};
use support::{create_user, deliver_message, spawn_mail_servers};

/// Resident set size in KiB (Linux `/proc/self/statm`, page 2 × page size).
fn rss_kib() -> usize {
    let page_size = 4096usize;
    std::fs::read_to_string("/proc/self/statm")
        .ok()
        .and_then(|s| s.split_whitespace().nth(1)?.parse::<usize>().ok())
        .map(|pages| pages * page_size / 1024)
        .unwrap_or(0)
}

fn rss_delta_kib(before: usize, after: usize) -> isize {
    after as isize - before as isize
}

/// Suspect 1: `MaildirListCache` retains a full listing per (user, mailbox) ever read.
#[tokio::test]
async fn probe_maildir_list_cache_rss_grows_per_mailbox() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = MailboxStore::with_policy(
        dir.path(),
        StoragePolicy {
            fsync_mode: FsyncMode::Always,
            cas_enabled: false,
            ..StoragePolicy::default()
        },
    );

    const USERS: usize = 200;
    const MSGS_PER_USER: usize = 50;
    let body = vec![b'x'; 512];

    let rss_before = rss_kib();
    for i in 0..USERS {
        let user = format!("user{i}@test");
        store.init_user_dir(&user).await.expect("init");
        for m in 0..MSGS_PER_USER {
            write_blob(&store, &user, &format!("msg-{m}"), &body)
                .await
                .expect("write");
        }
        // Triggers cache store() with full `Vec<StoredMessage>`.
        let _ = list_mailbox_messages(&store, &user, "INBOX")
            .await
            .expect("list");
    }
    let rss_after = rss_kib();
    let delta = rss_delta_kib(rss_before, rss_after);

    eprintln!(
        "maildir_list_cache: {USERS} users × {MSGS_PER_USER} msgs, RSS +{delta} KiB \
         ({:.1} KiB/user)",
        delta as f64 / USERS as f64
    );

    // ~5 KiB/user metadata is plausible; sustained >20 KiB/user indicates unbounded cache retention.
    assert!(
        delta > 0,
        "expected RSS growth after listing many mailboxes (cache should retain data)"
    );
    let per_user = delta as f64 / USERS as f64;
    assert!(
        per_user > 1.0,
        "per-user RSS growth too small to attribute to listing cache: {per_user} KiB/user"
    );
}

/// Suspect 2: `jit_flights` DashMap never removes per-user mutexes after JIT login.
#[tokio::test]
async fn probe_jit_flights_map_grows_without_cleanup() {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = chatmail_db::init_memory_db().await.expect("db");
    chatmail_db::set_setting(
        &pool,
        chatmail_db::settings_keys::JIT_REGISTRATION_ENABLED,
        "true",
    )
    .await
    .expect("jit on");
    chatmail_db::set_setting(&pool, chatmail_db::settings_keys::REGISTRATION_OPEN, "true")
        .await
        .expect("reg open");

    let ctx = Arc::new(AppState::new(dir.path(), pool.clone()));
    ctx.auth.hydrate(&pool).await.expect("hydrate");

    let auth = AuthContext {
        pool: pool.clone(),
        state: Arc::clone(&ctx),
        primary_domain: "test".into(),
        jit_domain: Some("test".into()),
        credential_policy: CredentialPolicy::default(),
    };

    const N: usize = 100;
    for i in 0..N {
        let user = format!("jituser{i:04}@test");
        chatmail_auth::authenticate(&auth, &user, "longpassword-here")
            .await
            .expect("jit login");
    }

    let flights = ctx.jit_flights.len();
    eprintln!("jit_flights: {flights} entries after {N} unique JIT logins");

    assert_eq!(
        flights, N,
        "jit_flights retained every user mutex — confirmed unbounded growth vector"
    );
}

/// Suspect 3: `EventBus.inbox_versions` grows one entry per user ever notified.
#[tokio::test]
async fn probe_eventbus_inbox_versions_grow_per_user() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = Arc::new(AppState::new(
        dir.path(),
        chatmail_db::init_memory_db().await.expect("db"),
    ));

    const N: usize = 500;
    for i in 0..N {
        let user = format!("notify{i}@test");
        ctx.events.notify_new_message(&user, "msg-1");
    }

    let versions = ctx.events.inbox_version_snapshot().len();
    eprintln!("eventbus inbox_versions: {versions} entries after {N} notifications");

    assert_eq!(
        versions, N,
        "inbox_versions map grows monotonically per notified user"
    );
}

/// Suspect 4: `DeliveryBatcher` spawns a permanent worker per (user, mailbox) under Never mode.
#[tokio::test]
async fn probe_delivery_batcher_spawns_per_mailbox_workers() {
    let batcher = DeliveryBatcher::new();
    const N: usize = 150;

    for i in 0..N {
        let user = format!("u{i}@test");
        batcher
            .submit_for_never(
                &user,
                "INBOX",
                PendingDelivery {
                    msg_id: format!("m{i}"),
                    final_path: format!("/tmp/fake-{i}").into(),
                    size: 100,
                    internal_secs: 0,
                },
            )
            .await;
    }

    // Let workers settle.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let coordinators = batcher.coordinator_count();
    let rss = rss_kib();
    eprintln!(
        "delivery_batcher: submitted {N} unique mailboxes, coordinators={coordinators}, \
         RSS now {rss} KiB (each first submit spawns an infinite-loop tokio task)"
    );

    assert_eq!(
        coordinators, N,
        "each first submit should create one permanent coordinator + worker"
    );
}

/// Suspect 5: production `never_batcher()` path (`mail_fsync=never` + CAS, first blob per user).
#[tokio::test]
async fn probe_never_cas_blob_path_uses_global_batcher() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = MailboxStore::with_policy(
        dir.path(),
        StoragePolicy {
            fsync_mode: FsyncMode::Never,
            cas_enabled: true,
            ..StoragePolicy::default()
        },
    );

    const N: usize = 120;
    let coordinators_before = never_delivery_batcher_coordinator_count();
    let rss_before = rss_kib();
    for i in 0..N {
        let user = format!("never{i}@test");
        store.init_user_dir(&user).await.expect("init");
        // Never-mode streaming commit → `finalize_from_tmp` → `never_batcher().submit_for_never`.
        // (`write_blob` uses `put_if_absent` + `install_maildir_entry` and skips the batcher.)
        let body = format!("unique-body-{i}-{}", "x".repeat(256));
        let body_len = body.len() as u64;
        let mut reader = Cursor::new(body.into_bytes());
        write_blob_mailbox_stream(
            &store,
            &user,
            "INBOX",
            &format!("msg-{i}"),
            &mut reader,
            body_len,
        )
        .await
        .expect("write");
    }
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let coordinators_after = never_delivery_batcher_coordinator_count();
    let rss_after = rss_kib();
    let coordinator_delta = coordinators_after.saturating_sub(coordinators_before);
    let rss_delta = rss_delta_kib(rss_before, rss_after);
    eprintln!(
        "never_cas_blob: {N} distinct first-writes, coordinators +{coordinator_delta}, \
         RSS +{rss_delta} KiB (global DeliveryBatcher spawns one infinite worker per mailbox)"
    );
    assert_eq!(
        coordinator_delta, N,
        "each distinct (user, INBOX) first-write should register one batcher coordinator"
    );
}

/// Suspect 6: sustained IMAP + delivery workload on a live mini-server.
#[tokio::test]
async fn probe_live_server_rss_under_repeated_delivery() {
    let dir = tempfile::tempdir().expect("tempdir");
    let srv = spawn_mail_servers(dir.path()).await;

    const USERS: usize = 80;
    const ROUNDS: usize = 5;
    let body = b"From: a@test\r\nTo: b@test\r\n\r\nbody\r\n";

    let rss_start = rss_kib();
    for round in 0..ROUNDS {
        for i in 0..USERS {
            let user = format!("live{i}@test");
            create_user(&srv.ctx, &srv.pool, &user, "secret-pass").await;
            deliver_message(&srv.ctx, &user, &format!("r{round}-m{i}"), body).await;
        }
    }
    let rss_end = rss_kib();
    let delta = rss_delta_kib(rss_start, rss_end);

    let jit_flights = srv.ctx.jit_flights.len();
    let inbox_versions = srv.ctx.events.inbox_version_snapshot().len();

    eprintln!(
        "live_server: {ROUNDS} rounds × {USERS} users, RSS +{delta} KiB, \
         jit_flights={jit_flights}, inbox_versions={inbox_versions}"
    );

    assert!(
        inbox_versions >= USERS,
        "event bus retained per-user version counters"
    );
    assert!(delta >= 0, "RSS should not shrink mid-test");
}
