-- FederationTracker and its flusher use i64. Widen existing counters without
-- changing the checksum of the original migration. Also used by the legacy
-- ensure path, including Go's failed_http_s / success_http_s column names.
DO $$
DECLARE
    counter_name TEXT;
BEGIN
    FOR counter_name IN
        SELECT column_name FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'federation_server_stats'
          AND data_type IN ('smallint', 'integer')
          AND column_name IN (
              'queued_messages', 'failed_http', 'failed_https', 'failed_http_s',
              'failed_smtp', 'success_http', 'success_https', 'success_http_s',
              'success_smtp', 'inbound_deliveries', 'successful_deliveries',
              'total_latency_ms', 'last_active'
          )
    LOOP
        EXECUTE format(
            'ALTER TABLE public.federation_server_stats ALTER COLUMN %I TYPE BIGINT',
            counter_name
        );
    END LOOP;
END $$;
