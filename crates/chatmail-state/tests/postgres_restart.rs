//! Invoked by tests/postgres-restart-docker.sh against a disposable database.
use chatmail_config::{DatabaseConfig, DbDriver};
use chatmail_db::init_db_from_config;
use chatmail_state::{flush_federation_stats, FederationTracker};

#[tokio::test]
#[ignore = "requires the disposable PostgreSQL database from the Docker runner"]
async fn postgres_runtime_stats_survive_restart() {
    let config = DatabaseConfig {
        driver: DbDriver::Postgres,
        dsn: std::env::var("MADMAIL_POSTGRES_TEST_DSN").expect("Docker test DSN"),
    };
    let pool = init_db_from_config(&config).await.unwrap();
    let tracker = FederationTracker::new();
    tracker.hydrate(&pool).await.unwrap();
    assert!(tracker.snapshot().is_empty());
    let domain = "runtime.example.invalid";
    tracker.increment_queue(domain);
    for transport in ["HTTP", "HTTPS", "SMTP"] {
        tracker.record_failure(domain, transport);
        tracker.record_success(domain, i64::from(i32::MAX), transport);
    }
    tracker.record_success(domain, 42, "");
    flush_federation_stats(&pool, &tracker).await.unwrap();
    let last_active = tracker.snapshot()[0].last_active;
    drop(tracker);
    if let chatmail_db::DbPool::Postgres(p) = &pool {
        p.close().await;
    }
    // Re-run migration and startup hydration through a new connection.
    let pool = init_db_from_config(&config).await.unwrap();
    let restored = FederationTracker::new();
    restored.hydrate(&pool).await.unwrap();
    let rows = restored.snapshot();
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!(row.domain, domain);
    assert_eq!(row.queued_messages, 1);
    assert_eq!(
        (row.failed_http, row.failed_https, row.failed_smtp),
        (1, 1, 1)
    );
    assert_eq!(
        (row.success_http, row.success_https, row.success_smtp),
        (1, 1, 1)
    );
    assert_eq!(row.inbound_deliveries, 1);
    assert_eq!(row.successful_deliveries, 4);
    assert_eq!(row.total_latency_ms, i64::from(i32::MAX) * 3 + 42);
    assert_eq!(row.last_active, last_active);
}
