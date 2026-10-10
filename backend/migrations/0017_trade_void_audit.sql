ALTER TABLE trades
    ADD COLUMN voided_at TIMESTAMPTZ,
    ADD COLUMN void_reason TEXT;

CREATE INDEX trades_latest_active_manual_idx
    ON trades (account_id, executed_at DESC, created_at DESC, id DESC)
    WHERE account_id IS NOT NULL
      AND source = 'web_manual'
      AND side IN ('buy', 'sell')
      AND voided_at IS NULL;
