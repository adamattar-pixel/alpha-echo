CREATE TABLE research_run_events (
    research_run_id UUID NOT NULL REFERENCES research_runs(id) ON DELETE CASCADE,
    event_key TEXT NOT NULL,
    market TEXT NOT NULL,
    event_type TEXT NOT NULL,
    is_scheduled BOOLEAN NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,
    headline TEXT NOT NULL,
    facts JSONB NOT NULL,
    evidence_keys JSONB NOT NULL,
    PRIMARY KEY (research_run_id, event_key)
);
