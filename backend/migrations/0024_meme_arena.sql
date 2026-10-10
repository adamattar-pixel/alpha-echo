CREATE TABLE meme_report_runs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'running', 'complete', 'partial', 'failed')),
    as_of TIMESTAMPTZ NOT NULL,
    candidate_window_start TIMESTAMPTZ NOT NULL,
    input_snapshot JSONB NOT NULL DEFAULT '{}'::jsonb,
    output JSONB,
    data_gaps JSONB NOT NULL DEFAULT '[]'::jsonb,
    error TEXT,
    model TEXT,
    skill_name TEXT,
    skill_version TEXT,
    validation_status TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ
);

CREATE UNIQUE INDEX meme_report_runs_one_active
    ON meme_report_runs ((true))
    WHERE status IN ('pending', 'running');

CREATE TABLE meme_token_observations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    report_id UUID NOT NULL REFERENCES meme_report_runs(id) ON DELETE CASCADE,
    instrument_id UUID NOT NULL REFERENCES instruments(id),
    source_snapshot_id UUID REFERENCES source_snapshots(id),
    chain TEXT NOT NULL CHECK (chain IN ('sol', 'bsc', 'robinhood')),
    token_created_at TIMESTAMPTZ,
    market_cap_usd NUMERIC(38, 6),
    survival_status TEXT NOT NULL
        CHECK (survival_status IN ('alive', 'dormant', 'eliminated', 'revived', 'unknown')),
    previous_status TEXT,
    nomination_reasons JSONB NOT NULL DEFAULT '[]'::jsonb,
    links JSONB NOT NULL DEFAULT '{}'::jsonb,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    observed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (report_id, instrument_id)
);

CREATE INDEX meme_token_observations_latest
    ON meme_token_observations (instrument_id, observed_at DESC);

INSERT INTO app_settings (key, value)
VALUES ('meme_report_schedule', '{"timezone":"Asia/Shanghai","time":"20:30","enabled":false}'::jsonb)
ON CONFLICT (key) DO NOTHING;
