DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM position_snapshots
        GROUP BY account_id, wallet_id, as_of
        HAVING count(*) > 1
    ) THEN
        RAISE EXCEPTION USING
            MESSAGE = 'position snapshot identity is ambiguous: duplicate owner/as_of rows exist',
            HINT = 'Inspect duplicate position_snapshots grouped by account_id, wallet_id and as_of; preserve one canonical immutable snapshot before restarting Alpha Echo.';
    END IF;
END $$;

CREATE UNIQUE INDEX briefing_runs_single_successor
    ON briefing_runs (supersedes_id)
    WHERE supersedes_id IS NOT NULL;

-- Before status was versioned, the application exposed no endpoint that could
-- mutate an intent status after creation. The migration-0002 parent backfill is
-- therefore the historical status for all application-created legacy rows.
