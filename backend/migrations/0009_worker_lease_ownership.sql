ALTER TABLE briefing_runs
    ADD COLUMN lease_token UUID;

CREATE INDEX briefing_runs_running_lease_idx
    ON briefing_runs (id, lease_token)
    WHERE status = 'running';

ALTER TABLE research_findings
    ADD COLUMN related_event_keys JSONB NOT NULL DEFAULT '[]'::jsonb,
    ADD COLUMN related_narrative_keys JSONB NOT NULL DEFAULT '[]'::jsonb,
    ADD COLUMN related_transmission_path_keys JSONB NOT NULL DEFAULT '[]'::jsonb;
