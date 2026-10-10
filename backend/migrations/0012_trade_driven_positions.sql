ALTER TABLE trades
    ADD COLUMN realized_pnl NUMERIC(38, 18),
    ADD COLUMN position_quantity_after NUMERIC(38, 18),
    ADD COLUMN average_cost_after NUMERIC(38, 18);

CREATE INDEX trades_owner_executed_at_idx
    ON trades (account_id, wallet_id, executed_at DESC, created_at DESC);
