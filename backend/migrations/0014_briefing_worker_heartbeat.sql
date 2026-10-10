ALTER TABLE briefing_runs
    ADD COLUMN heartbeat_at TIMESTAMPTZ;

UPDATE briefing_runs
SET heartbeat_at = started_at
WHERE status = 'running';

CREATE INDEX briefing_runs_running_heartbeat_idx
    ON briefing_runs (heartbeat_at)
    WHERE status = 'running';
