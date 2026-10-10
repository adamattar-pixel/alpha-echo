CREATE TABLE meme_token_blacklist (
    instrument_id UUID PRIMARY KEY REFERENCES instruments(id) ON DELETE CASCADE,
    reason TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

COMMENT ON TABLE meme_token_blacklist IS
    'User-controlled Meme research exclusions. Historical observations remain immutable.';
