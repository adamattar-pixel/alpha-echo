ALTER TABLE briefing_runs
    ADD COLUMN origin_kind TEXT;

UPDATE briefing_runs
SET origin_kind = CASE
    WHEN scheduled_for IS NULL THEN 'manual'
    WHEN (scheduled_for AT TIME ZONE 'Asia/Shanghai')::time < TIME '12:00' THEN 'morning'
    ELSE 'evening'
END;

ALTER TABLE briefing_runs
    ALTER COLUMN origin_kind SET DEFAULT 'report',
    ALTER COLUMN origin_kind SET NOT NULL,
    ADD CONSTRAINT briefing_runs_origin_kind_check
        CHECK (origin_kind IN ('morning', 'evening', 'manual', 'report'));
