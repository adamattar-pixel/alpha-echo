CREATE EXTENSION IF NOT EXISTS pgcrypto;

CREATE TABLE accounts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    market TEXT NOT NULL CHECK (market IN ('a_share', 'hk', 'us', 'crypto', 'meme')),
    currency TEXT NOT NULL,
    broker TEXT,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE wallets (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    chain TEXT NOT NULL,
    address TEXT NOT NULL,
    is_own BOOLEAN NOT NULL DEFAULT TRUE,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (chain, address)
);

CREATE TABLE tracked_wallets (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    wallet_id UUID NOT NULL REFERENCES wallets(id) ON DELETE CASCADE,
    label TEXT NOT NULL,
    category TEXT NOT NULL CHECK (category IN ('smart_money', 'kol', 'dev', 'other')),
    notes TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (wallet_id, category)
);

CREATE TABLE instruments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    market TEXT NOT NULL CHECK (market IN ('a_share', 'hk', 'us', 'crypto', 'meme')),
    symbol TEXT NOT NULL,
    name TEXT NOT NULL,
    currency TEXT,
    exchange TEXT,
    chain TEXT,
    contract_address TEXT,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((market = 'meme' AND chain IS NOT NULL AND contract_address IS NOT NULL) OR market <> 'meme')
);

CREATE UNIQUE INDEX instruments_listed_identity
    ON instruments (market, exchange, symbol)
    WHERE contract_address IS NULL;
CREATE UNIQUE INDEX instruments_chain_contract_identity
    ON instruments (chain, lower(contract_address))
    WHERE contract_address IS NOT NULL;

CREATE TABLE instrument_aliases (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    instrument_id UUID NOT NULL REFERENCES instruments(id) ON DELETE CASCADE,
    source TEXT NOT NULL,
    alias TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (source, alias)
);

CREATE TABLE trades (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id UUID REFERENCES accounts(id),
    wallet_id UUID REFERENCES wallets(id),
    instrument_id UUID NOT NULL REFERENCES instruments(id),
    side TEXT NOT NULL CHECK (side IN ('buy', 'sell', 'transfer_in', 'transfer_out', 'airdrop')),
    quantity NUMERIC(38, 18) NOT NULL CHECK (quantity > 0),
    price NUMERIC(38, 18),
    fees NUMERIC(38, 18),
    currency TEXT,
    executed_at TIMESTAMPTZ NOT NULL,
    external_id TEXT,
    source TEXT NOT NULL,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((account_id IS NOT NULL)::int + (wallet_id IS NOT NULL)::int = 1)
);

CREATE TABLE position_snapshots (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id UUID REFERENCES accounts(id),
    wallet_id UUID REFERENCES wallets(id),
    as_of TIMESTAMPTZ NOT NULL,
    source TEXT NOT NULL,
    source_snapshot_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((account_id IS NOT NULL)::int + (wallet_id IS NOT NULL)::int = 1),
    UNIQUE (account_id, wallet_id, as_of)
);

CREATE TABLE position_snapshot_items (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    snapshot_id UUID NOT NULL REFERENCES position_snapshots(id) ON DELETE CASCADE,
    instrument_id UUID NOT NULL REFERENCES instruments(id),
    quantity NUMERIC(38, 18) NOT NULL,
    average_cost NUMERIC(38, 18),
    market_price NUMERIC(38, 18),
    market_value NUMERIC(38, 18),
    unrealized_pnl NUMERIC(38, 18),
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    UNIQUE (snapshot_id, instrument_id)
);

CREATE INDEX position_snapshots_owner_as_of_idx
    ON position_snapshots (account_id, wallet_id, as_of DESC);

CREATE TABLE trade_intents (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title TEXT NOT NULL,
    strategy_type TEXT NOT NULL,
    time_horizon TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('draft', 'active', 'paused', 'invalidated', 'closed')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE trade_intent_versions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    trade_intent_id UUID NOT NULL REFERENCES trade_intents(id) ON DELETE CASCADE,
    version INTEGER NOT NULL CHECK (version > 0),
    thesis TEXT NOT NULL,
    risk_budget NUMERIC(20, 8),
    target_position NUMERIC(20, 8),
    catalysts JSONB NOT NULL DEFAULT '[]'::jsonb,
    invalidation_conditions JSONB NOT NULL DEFAULT '[]'::jsonb,
    exit_principles JSONB NOT NULL DEFAULT '[]'::jsonb,
    notes TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (trade_intent_id, version)
);

CREATE TABLE position_intent_links (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    trade_intent_id UUID NOT NULL REFERENCES trade_intents(id) ON DELETE CASCADE,
    instrument_id UUID NOT NULL REFERENCES instruments(id),
    account_id UUID REFERENCES accounts(id),
    wallet_id UUID REFERENCES wallets(id),
    position_slice NUMERIC(20, 8),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((account_id IS NOT NULL)::int + (wallet_id IS NOT NULL)::int <= 1),
    UNIQUE NULLS NOT DISTINCT (trade_intent_id, instrument_id, account_id, wallet_id)
);

CREATE TABLE source_snapshots (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    provider TEXT NOT NULL,
    source_type TEXT NOT NULL,
    captured_at TIMESTAMPTZ NOT NULL,
    as_of TIMESTAMPTZ NOT NULL,
    request JSONB NOT NULL DEFAULT '{}'::jsonb,
    payload JSONB NOT NULL,
    checksum TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE position_snapshots
    ADD CONSTRAINT position_snapshots_source_snapshot_fk
    FOREIGN KEY (source_snapshot_id) REFERENCES source_snapshots(id);

CREATE TABLE evidence_sources (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    source_snapshot_id UUID REFERENCES source_snapshots(id),
    source_type TEXT NOT NULL,
    title TEXT NOT NULL,
    url TEXT,
    publisher TEXT,
    published_at TIMESTAMPTZ,
    observed_at TIMESTAMPTZ NOT NULL,
    excerpt TEXT,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE market_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    event_key TEXT NOT NULL UNIQUE,
    market TEXT NOT NULL,
    event_type TEXT NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,
    headline TEXT NOT NULL,
    facts JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE briefing_runs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    kind TEXT NOT NULL CHECK (kind IN ('morning', 'evening', 'manual')),
    status TEXT NOT NULL CHECK (status IN ('pending', 'running', 'complete', 'partial', 'failed')),
    scheduled_for TIMESTAMPTZ,
    as_of TIMESTAMPTZ NOT NULL,
    research_window_start TIMESTAMPTZ NOT NULL,
    revision INTEGER NOT NULL DEFAULT 1 CHECK (revision > 0),
    supersedes_id UUID REFERENCES briefing_runs(id),
    input_snapshot JSONB NOT NULL,
    rendered_markdown TEXT,
    schema_version TEXT NOT NULL DEFAULT 'v1',
    model TEXT,
    validation_status TEXT,
    error TEXT,
    data_gaps JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    UNIQUE NULLS NOT DISTINCT (kind, scheduled_for, revision)
);

CREATE INDEX briefing_runs_status_created_idx ON briefing_runs (status, created_at);

CREATE TABLE research_runs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    briefing_run_id UUID NOT NULL REFERENCES briefing_runs(id) ON DELETE CASCADE,
    market TEXT NOT NULL,
    run_type TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('pending', 'running', 'complete', 'partial', 'failed')),
    as_of TIMESTAMPTZ NOT NULL,
    research_window_start TIMESTAMPTZ NOT NULL,
    input_snapshot JSONB NOT NULL,
    skill_name TEXT NOT NULL,
    skill_version TEXT NOT NULL,
    model TEXT,
    schema_version TEXT NOT NULL,
    raw_output JSONB,
    validation_status TEXT,
    error TEXT,
    data_gaps JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ
);

CREATE TABLE research_findings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    research_run_id UUID NOT NULL REFERENCES research_runs(id) ON DELETE CASCADE,
    finding_type TEXT NOT NULL,
    market TEXT NOT NULL,
    window_start TIMESTAMPTZ NOT NULL,
    window_end TIMESTAMPTZ NOT NULL,
    affected_object_type TEXT,
    affected_object_id UUID,
    fact_summary TEXT NOT NULL,
    inference TEXT,
    impact_direction TEXT CHECK (impact_direction IN ('strengthened', 'weakened', 'invalidated', 'unchanged')),
    impact_strength SMALLINT CHECK (impact_strength BETWEEN 0 AND 5),
    time_horizon TEXT,
    confidence NUMERIC(4, 3) NOT NULL CHECK (confidence BETWEEN 0 AND 1),
    contrary_evidence JSONB NOT NULL DEFAULT '[]'::jsonb,
    confirmation_conditions JSONB NOT NULL DEFAULT '[]'::jsonb,
    invalidation_conditions JSONB NOT NULL DEFAULT '[]'::jsonb,
    conditional_action TEXT,
    data_gaps JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE finding_evidence (
    finding_id UUID NOT NULL REFERENCES research_findings(id) ON DELETE CASCADE,
    evidence_source_id UUID NOT NULL REFERENCES evidence_sources(id),
    relevance TEXT NOT NULL,
    PRIMARY KEY (finding_id, evidence_source_id)
);

CREATE TABLE market_narratives (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    market TEXT NOT NULL,
    name TEXT NOT NULL,
    narrative_type TEXT NOT NULL CHECK (narrative_type IN ('explicit', 'latent')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (market, name)
);

CREATE TABLE narrative_snapshots (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    narrative_id UUID NOT NULL REFERENCES market_narratives(id) ON DELETE CASCADE,
    research_run_id UUID NOT NULL REFERENCES research_runs(id) ON DELETE CASCADE,
    as_of TIMESTAMPTZ NOT NULL,
    stage TEXT NOT NULL,
    summary TEXT NOT NULL,
    confidence NUMERIC(4, 3) NOT NULL CHECK (confidence BETWEEN 0 AND 1),
    capital_flow_inference TEXT,
    evidence JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE narrative_instruments (
    narrative_id UUID NOT NULL REFERENCES market_narratives(id) ON DELETE CASCADE,
    instrument_id UUID NOT NULL REFERENCES instruments(id) ON DELETE CASCADE,
    role TEXT NOT NULL,
    PRIMARY KEY (narrative_id, instrument_id)
);

CREATE TABLE transmission_paths (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    research_run_id UUID NOT NULL REFERENCES research_runs(id) ON DELETE CASCADE,
    source_driver TEXT NOT NULL,
    steps JSONB NOT NULL,
    target_market TEXT NOT NULL,
    target_instrument_id UUID REFERENCES instruments(id),
    direction TEXT NOT NULL,
    confidence NUMERIC(4, 3) NOT NULL CHECK (confidence BETWEEN 0 AND 1),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE briefing_items (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    briefing_run_id UUID NOT NULL REFERENCES briefing_runs(id) ON DELETE CASCADE,
    section TEXT NOT NULL,
    sort_order INTEGER NOT NULL,
    title TEXT NOT NULL,
    body TEXT NOT NULL,
    severity TEXT,
    source_finding_ids JSONB NOT NULL DEFAULT '[]'::jsonb,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (briefing_run_id, section, sort_order)
);

CREATE TABLE app_settings (
    key TEXT PRIMARY KEY,
    value JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

INSERT INTO app_settings (key, value) VALUES
    ('briefing_schedule', '{"timezone":"Asia/Shanghai","morning":"07:30","evening":"19:00"}'::jsonb),
    ('research_runner', '{"kind":"not_configured"}'::jsonb);

