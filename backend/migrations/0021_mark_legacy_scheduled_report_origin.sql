ALTER TABLE briefing_runs
    DROP CONSTRAINT briefing_runs_origin_kind_check;

UPDATE briefing_runs
SET origin_kind = 'legacy_scheduled'
WHERE origin_kind IN ('morning', 'evening');

ALTER TABLE briefing_runs
    ADD CONSTRAINT briefing_runs_origin_kind_check
        CHECK (origin_kind IN ('manual', 'legacy_scheduled', 'report'));
