ALTER TABLE position_snapshots
    DROP CONSTRAINT position_snapshots_account_id_wallet_id_as_of_key;

ALTER TABLE position_snapshots
    ADD CONSTRAINT position_snapshots_source_not_empty CHECK (source <> '');

CREATE UNIQUE INDEX position_snapshots_account_as_of_unique
    ON position_snapshots (account_id, as_of)
    WHERE account_id IS NOT NULL;

CREATE UNIQUE INDEX position_snapshots_wallet_as_of_unique
    ON position_snapshots (wallet_id, as_of)
    WHERE wallet_id IS NOT NULL;

ALTER TABLE trade_intent_versions
    ADD COLUMN status TEXT;

UPDATE trade_intent_versions tiv
SET status = ti.status
FROM trade_intents ti
WHERE ti.id = tiv.trade_intent_id;

ALTER TABLE trade_intent_versions
    ALTER COLUMN status SET NOT NULL,
    ADD CONSTRAINT trade_intent_versions_status_check
        CHECK (status IN ('draft', 'active', 'paused', 'invalidated', 'closed'));

ALTER TABLE briefing_runs
    DROP CONSTRAINT briefing_runs_kind_scheduled_for_revision_key,
    ADD COLUMN skill_name TEXT,
    ADD COLUMN skill_version TEXT;

CREATE UNIQUE INDEX briefing_runs_scheduled_revision_unique
    ON briefing_runs (kind, scheduled_for, revision)
    WHERE scheduled_for IS NOT NULL;

